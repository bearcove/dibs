#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Snark parser, source AST, and recovery session for Dibs query declarations.

mod diagnostic;
mod scanner;
mod support;

#[allow(dead_code, missing_docs, unused_imports)]
mod generated_ast {
    include!(concat!(env!("OUT_DIR"), "/dibs_query_ast.rs"));
}

/// Typed source AST for strict Dibs query compilation.
pub mod ast {
    pub use crate::generated_ast::{PgTypeName, Statement, StatementNode, TypedAstLowerError};

    /// Raw generated source file before public contract normalization.
    pub(crate) type GeneratedSourceFile = crate::generated_ast::SourceFile;
    /// Raw generated query declaration before public contract normalization.
    pub(crate) type GeneratedQueryDecl = crate::generated_ast::QueryDecl;

    /// Public source file for strict compilation.
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct SourceFile {
        /// Complete document span.
        pub span: crate::Span,
        /// Query declarations in source order.
        pub queries: Vec<QueryDecl>,
    }

    /// Public query declaration.
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct QueryDecl {
        /// Complete declaration span.
        pub span: crate::Span,
        /// Retained `///` documentation tokens.
        pub documentations: Vec<crate::Spanned<String>>,
        /// Declaration name.
        pub name: crate::Identifier,
        /// Ordered parameter declarations.
        pub parameters: Vec<ParameterDecl>,
        /// Declared runtime result contract.
        pub result_mode: crate::ResultMode,
        /// Exactly one statement body.
        pub statement: Statement,
    }

    /// Public ordered parameter declaration.
    #[derive(Debug, Clone, PartialEq, facet::Facet)]
    pub struct ParameterDecl {
        /// Complete parameter span.
        pub span: crate::Span,
        /// Parameter name.
        pub name: crate::Identifier,
        /// PostgreSQL catalog type spelling.
        pub type_name: PgTypeName,
        /// Whether the generated API accepts SQL `NULL` for this bind.
        pub nullable: bool,
    }
}

use scanner::DibsExternalScanner;
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        RecoveringDocument, WeavyParsePlan, WeavyParseSession,
        parse_prepared_weavy_recovering_with_report_and_scanner,
    },
    parser::{ParseTable, ParserGrammar, ResolvedCstTree},
    validated::ValidatedGrammar,
};

pub use ast::{ParameterDecl, PgTypeName, QueryDecl, SourceFile, Statement, StatementNode};
pub use diagnostic::{Diagnostic, DiagnosticCode, Repair, to_margin_diagnostics};
/// Tree-sitter-style byte edit descriptor for incremental reparsing.
pub use snark::parser::ParserInputEdit;
pub use support::{SourceId, Span, Spanned};
/// Source identifier spelling preserved with its byte range.
pub type Identifier = Spanned<String>;

/// Runtime result contract declared by a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum ResultMode {
    /// Return every row.
    Many,
    /// Accept zero or one row.
    Optional,
    /// Require exactly one row.
    One,
    /// Require a rowless statement and return affected-row count.
    Exec,
}

impl ResultMode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "many" => Some(Self::Many),
            "optional" => Some(Self::Optional),
            "one" => Some(Self::One),
            "exec" => Some(Self::Exec),
            _ => None,
        }
    }
}

const GRAMMAR_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/dibs_query_grammar.json"));

/// Version of the Dibs source language and PostgreSQL lexical contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageVersion {
    /// Dibs query grammar revision.
    pub grammar: u16,
    /// PostgreSQL major version whose lexical policy is accepted.
    pub postgres_major: u16,
}

impl LanguageVersion {
    /// Initial declaration grammar targeting PostgreSQL 16 lexical behavior.
    pub const POSTGRES_16: Self = Self {
        grammar: 1,
        postgres_major: 16,
    };
}

/// Prepared Dibs query parser artifacts reusable across documents and edits.
pub struct DibsParser {
    parser: ParserGrammar,
    table: ParseTable,
    plan: WeavyParsePlan,
    language_version: LanguageVersion,
    scanner: DibsExternalScanner,
}

impl DibsParser {
    /// Builds and prepares the embedded Dibs query grammar.
    #[must_use]
    pub fn new() -> Self {
        Self::try_new()
            .unwrap_or_else(|error| panic!("invalid embedded Dibs query grammar: {error}"))
    }

    fn try_new() -> Result<Self, String> {
        let raw = RawGrammarJson::from_tree_sitter_json_str(GRAMMAR_JSON)
            .map_err(|error| error.to_string())?;
        let validated = ValidatedGrammar::from_raw(&raw).map_err(|error| error.to_string())?;
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .map_err(|error| error.to_string())?
            .prepare_productions_for_items()
            .map_err(|error| error.to_string())?;
        let table = ParseTable::from_grammar(&parser).map_err(|error| error.to_string())?;
        let plan =
            WeavyParsePlan::new(&validated, &parser, &table).map_err(|error| error.to_string())?;
        Ok(Self {
            parser,
            table,
            plan,
            language_version: LanguageVersion::POSTGRES_16,
            scanner: DibsExternalScanner,
        })
    }

    /// Returns the language and PostgreSQL lexical version prepared by this parser.
    pub const fn language_version(&self) -> LanguageVersion {
        self.language_version
    }

    /// Strictly parses and lowers one source document.
    ///
    /// Recovery/error/missing facts are rejected before the fallible generated
    /// AST lowering API is called.
    pub fn parse_strict(
        &self,
        source_id: SourceId,
        source: &str,
    ) -> Result<ast::SourceFile, Vec<Diagnostic>> {
        let report = snark::lower::weavy::parse_prepared_weavy_with_report_and_scanner(
            &self.plan,
            &self.parser,
            &self.table,
            source,
            Some(&self.scanner),
        )
        .map_err(|error| vec![Diagnostic::parse_error(source_id, source.len(), &error)])?;
        let tree = report
            .accepted_resolved_cst(&self.parser, source)
            .ok_or_else(|| {
                vec![Diagnostic::parse_failure(
                    source_id,
                    source.len(),
                    "accepted parse did not produce a resolved CST".to_owned(),
                )]
            })?;
        strict_lower(source_id, &tree)
    }

    /// Parses one source document with skip-invalid recovery.
    pub fn parse_recovering(
        &self,
        source_id: SourceId,
        source: &str,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let report = parse_prepared_weavy_recovering_with_report_and_scanner(
            &self.plan,
            &self.parser,
            &self.table,
            source,
            Some(&self.scanner),
        )
        .map_err(|error| vec![Diagnostic::parse_error(source_id, source.len(), &error)])?;
        let tree = report
            .accepted_resolved_cst(&self.parser, source)
            .ok_or_else(|| {
                vec![Diagnostic::parse_failure(
                    source_id,
                    source.len(),
                    "recovering parse did not produce a resolved CST".to_owned(),
                )]
            })?;
        Ok(RecoveringParse::new(source_id, tree))
    }

    /// Creates a persistent recovering session for one source identity.
    pub fn session(&self, source_id: SourceId) -> DibsDocumentSession<'_> {
        DibsDocumentSession {
            source_id,
            session: WeavyParseSession::new(&self.plan, &self.parser, &self.table)
                .with_external_scanner(&self.scanner),
        }
    }
}

impl Default for DibsParser {
    fn default() -> Self {
        Self::new()
    }
}

fn strict_lower(
    source_id: SourceId,
    tree: &ResolvedCstTree,
) -> Result<ast::SourceFile, Vec<Diagnostic>> {
    if tree.contains_error() {
        return Err(tree
            .diagnostics()
            .iter()
            .map(|diagnostic| Diagnostic::recovered(source_id, diagnostic))
            .collect());
    }
    let Some(root) = tree.root() else {
        return Err(vec![Diagnostic::parse_failure(
            source_id,
            0,
            "parse produced no source root".to_owned(),
        )]);
    };
    let generated = generated_ast::try_lower_source_file(&root)
        .map_err(|error| vec![Diagnostic::lowering(source_id, error)])?;
    lower_public_ast(source_id, generated)
}

fn lower_public_ast(
    source_id: SourceId,
    generated: ast::GeneratedSourceFile,
) -> Result<ast::SourceFile, Vec<Diagnostic>> {
    generated
        .queries
        .into_iter()
        .map(|query| lower_query(source_id, query))
        .collect::<Result<Vec<_>, _>>()
        .map(|queries| ast::SourceFile {
            span: generated.span,
            queries,
        })
}

fn lower_query(
    source_id: SourceId,
    query: ast::GeneratedQueryDecl,
) -> Result<ast::QueryDecl, Vec<Diagnostic>> {
    let result_mode = ResultMode::parse(&query.result_mode.value).ok_or_else(|| {
        vec![Diagnostic::parse_failure(
            source_id,
            query.result_mode.span.end as usize,
            format!("unknown result mode {:?}", query.result_mode.value),
        )]
    })?;
    Ok(ast::QueryDecl {
        span: query.span,
        documentations: query.documentations,
        name: query.name,
        parameters: query
            .parameters
            .into_iter()
            .map(|parameter| ast::ParameterDecl {
                span: parameter.span,
                name: parameter.name,
                type_name: parameter.type_name,
                nullable: parameter.nullable.is_some(),
            })
            .collect(),
        result_mode,
        statement: query.statement,
    })
}

/// Recovering CST and normalized diagnostics for one parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveringParse {
    /// Arena-backed recovery CST.
    pub tree: ResolvedCstTree,
    /// Structured syntax diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl RecoveringParse {
    fn new(source_id: SourceId, tree: ResolvedCstTree) -> Self {
        let diagnostics = tree
            .diagnostics()
            .iter()
            .map(|diagnostic| Diagnostic::recovered(source_id, diagnostic))
            .collect();
        Self { tree, diagnostics }
    }
}

/// Persistent recovering parser session for an edited Dibs query document.
pub struct DibsDocumentSession<'a> {
    source_id: SourceId,
    session: WeavyParseSession<'a>,
}

impl DibsDocumentSession<'_> {
    /// Parses a complete source and installs it as the incremental baseline.
    pub fn parse_recovering(
        &mut self,
        source: impl Into<String>,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let document = self
            .session
            .parse_recovering_document(source)
            .map_err(|error| {
                vec![Diagnostic::parse_error(
                    self.source_id,
                    self.session.last_input().map_or(0, str::len),
                    &error,
                )]
            })?;
        Ok(self.recovering_parse(document))
    }

    /// Reparses a localized edit against the installed source baseline.
    pub fn reparse_recovering(
        &mut self,
        edit: ParserInputEdit,
        source: impl Into<String>,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let document = self
            .session
            .reparse_recovering_document(edit, source)
            .map_err(|error| {
                vec![Diagnostic::parse_error(
                    self.source_id,
                    self.session.last_input().map_or(0, str::len),
                    &error,
                )]
            })?;
        Ok(self.recovering_parse(document))
    }

    fn recovering_parse(&self, document: RecoveringDocument) -> RecoveringParse {
        RecoveringParse::new(self.source_id, document.tree)
    }
}

impl ast::QueryDecl {
    /// Iterates named bind tokens in statement source order.
    pub fn bind_occurrences(&self) -> impl Iterator<Item = &Spanned<String>> {
        self.statement.items.iter().filter_map(|item| match item {
            ast::StatementNode::NamedBind(bind) => Some(bind),
            _ => None,
        })
    }
}
