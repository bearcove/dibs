#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Snark parser, source AST, and recovery session for Dibs query declarations.

mod diagnostic;
mod scanner;
mod support;

#[allow(
    clippy::needless_question_mark,
    dead_code,
    missing_docs,
    unused_imports
)]
mod generated_ast {
    include!(concat!(env!("OUT_DIR"), "/dibs_query_ast.rs"));
}

/// Typed source AST for strict Dibs query compilation.
pub mod ast {
    pub use crate::generated_ast::*;

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
        /// Named bind tokens in authored source order.
        pub(crate) binds: Vec<crate::Spanned<String>>,
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
    lower::weavy::{
        RecoveringDocument, WeavyParseSession,
        parse_prepared_weavy_recovering_with_report_and_scanner,
    },
    module::{BorrowedSnarkModule, SnarkModule},
    parser::ResolvedCstTree,
};

pub use ast::{Expression, ParameterDecl, PgTypeName, QueryDecl, Relation, SourceFile, Statement};
pub use diagnostic::{
    Diagnostic, DiagnosticCode, MarginDiagnosticConversionError, Repair, to_margin_diagnostics,
};
/// Tree-sitter-style byte edit descriptor for incremental reparsing.
pub use snark::parser::ParserInputEdit;
pub use support::{SourceId, SourceSpan, Span, Spanned};
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

const PARSER_MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dibs_query_parser.weavy"));

/// Version of the Dibs source language and PostgreSQL lexical contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageVersion {
    /// Dibs query grammar revision.
    pub grammar: u16,
    /// PostgreSQL major version whose lexical policy is accepted.
    pub postgres_major: u16,
}

impl LanguageVersion {
    /// Initial declaration grammar targeting PostgreSQL 18 lexical behavior.
    pub const POSTGRES_18: Self = Self {
        grammar: 1,
        postgres_major: 18,
    };
}

/// Prepared parser-machine size facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserFacts {
    /// LR/GLR parse states.
    pub states: usize,
    /// Retained parser-table conflicts.
    pub conflicts: usize,
}

/// Prepared Dibs query parser artifacts reusable across documents and edits.
pub struct DibsParser {
    module: BorrowedSnarkModule<'static>,
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
        Ok(Self {
            module: SnarkModule::load_borrowed(PARSER_MODULE).map_err(|error| error.to_string())?,
            language_version: LanguageVersion::POSTGRES_18,
            scanner: DibsExternalScanner,
        })
    }

    /// Returns the language and PostgreSQL lexical version prepared by this parser.
    pub const fn language_version(&self) -> LanguageVersion {
        self.language_version
    }

    /// Returns prepared parser-machine size facts for qualification budgets.
    #[must_use]
    pub fn parser_facts(&self) -> ParserFacts {
        ParserFacts {
            states: self.module.runtime_state_count(),
            conflicts: self.module.runtime_conflict_count(),
        }
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
        let report = self
            .module
            .parse(source, Some(&self.scanner))
            .map_err(|error| vec![Diagnostic::parse_error(source_id, source.len(), &error)])?;
        let tree = report
            .accepted_resolved_cst(self.module.parser_grammar(), source)
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
            self.module.plan(),
            self.module.parser_grammar(),
            self.module.parse_table(),
            source,
            Some(&self.scanner),
        )
        .map_err(|error| vec![Diagnostic::parse_error(source_id, source.len(), &error)])?;
        let tree = report
            .accepted_resolved_cst(self.module.parser_grammar(), source)
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
            session: WeavyParseSession::new(
                self.module.plan(),
                self.module.parser_grammar(),
                self.module.parse_table(),
            )
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
    lower_public_ast(source_id, tree, generated)
}

fn lower_public_ast(
    source_id: SourceId,
    tree: &ResolvedCstTree,
    generated: ast::GeneratedSourceFile,
) -> Result<ast::SourceFile, Vec<Diagnostic>> {
    let binds_by_query = tree
        .root()
        .map(|root| {
            root.children()
                .filter(|child| child.kind() == "query_decl")
                .map(collect_query_binds)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    generated
        .queries
        .into_iter()
        .zip(binds_by_query)
        .map(|(query, binds)| lower_query(source_id, query, binds))
        .collect::<Result<Vec<_>, _>>()
        .map(|queries| ast::SourceFile {
            span: generated.span,
            queries,
        })
}

fn lower_query(
    source_id: SourceId,
    query: ast::GeneratedQueryDecl,
    binds: Vec<Spanned<String>>,
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
        binds,
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
        self.binds.iter()
    }
}

fn collect_query_binds(node: snark::parser::ResolvedCstTreeNode<'_>) -> Vec<Spanned<String>> {
    let mut binds = Vec::new();
    collect_bind_descendants(node, &mut binds);
    binds.sort_by_key(|bind| bind.span.start);
    binds
}

fn collect_bind_descendants(
    node: snark::parser::ResolvedCstTreeNode<'_>,
    output: &mut Vec<Spanned<String>>,
) {
    if node.kind() == "named_bind"
        && let Some(value) = node.text()
    {
        let bytes = node.bytes();
        output.push(Spanned {
            value: value.to_owned(),
            span: Span::new(bytes.start().get(), bytes.end().get()),
        });
    }
    for child in node.children() {
        collect_bind_descendants(child, output);
    }
}
