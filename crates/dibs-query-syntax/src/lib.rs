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
use snark::parser::{ParserGrammar, ResolvedCstTree};

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
    module: snark::module::BorrowedSnarkModule<'static>,
    language_version: LanguageVersion,
    scanner: DibsExternalScanner,
}

impl DibsParser {
    /// Loads the embedded precompiled Dibs parser module.
    #[must_use]
    pub fn new() -> Self {
        const MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dibs_query_parser.weavy"));
        const PARSER_DECODE_CEILING: usize = 320 * 1024 * 1024;
        let limits = snark::module::SnarkModuleLoadLimits::default()
            .with_max_decoded_bytes(PARSER_DECODE_CEILING)
            .with_max_retained_bytes(PARSER_DECODE_CEILING);
        let module = snark::module::SnarkModule::load_borrowed_with_limits(MODULE, limits)
            .unwrap_or_else(|error| panic!("invalid embedded Dibs parser module: {error}"));
        Self {
            module,
            language_version: LanguageVersion::POSTGRES_18,
            scanner: DibsExternalScanner,
        }
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
        strict_lower(source_id, source, &tree)
    }

    /// Parses one source document with skip-invalid recovery.
    pub fn parse_recovering(
        &self,
        source_id: SourceId,
        source: &str,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let report = self
            .module
            .parse_recovering(source, Some(&self.scanner))
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
            parser: self.module.parser_grammar(),
            session: self.module.session().with_external_scanner(&self.scanner),
            last_input_len: 0,
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
    source: &str,
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
    lower_public_ast(source_id, source, tree, generated)
}

fn lower_public_ast(
    source_id: SourceId,
    source: &str,
    _tree: &ResolvedCstTree,
    generated: ast::GeneratedSourceFile,
) -> Result<ast::SourceFile, Vec<Diagnostic>> {
    let binds_by_query = generated
        .queries
        .iter()
        .map(|query| {
            collect_query_binds(source, query.span.start as usize, query.span.end as usize)
        })
        .collect::<Vec<_>>();
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
    parser: &'a ParserGrammar,
    session: snark::module::BorrowedSnarkSession<'a>,
    last_input_len: usize,
}

impl DibsDocumentSession<'_> {
    /// Parses a complete source and installs it as the incremental baseline.
    pub fn parse_recovering(
        &mut self,
        source: impl Into<String>,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let source = source.into();
        self.last_input_len = source.len();
        let report = self
            .session
            .parse_recovering(source.clone())
            .map_err(|error| {
                vec![Diagnostic::parse_error(
                    self.source_id,
                    self.last_input_len,
                    &error,
                )]
            })?;
        recovering_parse(self.source_id, self.parser, report, &source)
    }

    /// Reparses a localized edit against the installed source baseline.
    pub fn reparse_recovering(
        &mut self,
        edit: ParserInputEdit,
        source: impl Into<String>,
    ) -> Result<RecoveringParse, Vec<Diagnostic>> {
        let source = source.into();
        self.last_input_len = source.len();
        let report = self
            .session
            .reparse_recovering(edit, source.clone())
            .map_err(|error| {
                vec![Diagnostic::parse_error(
                    self.source_id,
                    self.last_input_len,
                    &error,
                )]
            })?;
        recovering_parse(self.source_id, self.parser, report, &source)
    }
}

fn recovering_parse(
    source_id: SourceId,
    parser: &ParserGrammar,
    report: &snark::lower::weavy::WeavyParseReport,
    source: &str,
) -> Result<RecoveringParse, Vec<Diagnostic>> {
    report
        .accepted_resolved_cst(parser, source)
        .map(|tree| RecoveringParse::new(source_id, tree))
        .ok_or_else(|| {
            vec![Diagnostic::parse_failure(
                source_id,
                source.len(),
                "recovering parse did not produce a resolved CST".to_owned(),
            )]
        })
}

impl ast::QueryDecl {
    /// Iterates named bind tokens in statement source order.
    pub fn bind_occurrences(&self) -> impl Iterator<Item = &Spanned<String>> {
        self.binds.iter()
    }
}

fn is_postgresql_identifier_continue(character: char) -> bool {
    matches!(character, '\u{200C}' | '\u{200D}')
}

/// Failure while splitting a source file into complete top-level declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationSplitError {
    /// A `query` declaration opened a body but did not close it.
    UnclosedDeclaration,
}

/// Returns complete top-level query declarations without parsing them together.
///
/// Query-like text inside comments and quoted PostgreSQL regions is ignored.
pub fn declaration_sources(source: &str) -> Result<Vec<&str>, DeclarationSplitError> {
    let bytes = source.as_bytes();
    let mut declarations = Vec::new();
    let mut index = 0usize;
    while let Some(start) = next_query_keyword(bytes, index) {
        let end = declaration_end(bytes, start)?;
        declarations.push(source[start..end].trim_end());
        index = end;
    }
    Ok(declarations)
}

fn next_query_keyword(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        match bytes[index] {
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index)?;
            }
            b'\'' | b'"' => index = skip_quoted(bytes, index, bytes[index])?,
            b'$' => {
                if let Some(end) = skip_dollar_quoted(bytes, index) {
                    index = end;
                } else {
                    index += 1;
                }
            }
            b'q' if bytes[index..].starts_with(b"query")
                && !bytes
                    .get(index.wrapping_sub(1))
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                && !bytes
                    .get(index + 5)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_') =>
            {
                return Some(index);
            }
            _ => index += 1,
        }
    }
    None
}

fn declaration_end(bytes: &[u8], start: usize) -> Result<usize, DeclarationSplitError> {
    let mut index = start + "query".len();
    let mut depth = 0usize;
    let mut opened = false;
    while index < bytes.len() {
        match bytes[index] {
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index)
                    .ok_or(DeclarationSplitError::UnclosedDeclaration)?;
            }
            b'\'' | b'"' => {
                index = skip_quoted(bytes, index, bytes[index])
                    .ok_or(DeclarationSplitError::UnclosedDeclaration)?;
            }
            b'$' => {
                if let Some(end) = skip_dollar_quoted(bytes, index) {
                    index = end;
                } else {
                    index += 1;
                }
            }
            b'{' => {
                opened = true;
                depth += 1;
                index += 1;
            }
            b'}' if opened => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Ok(index);
                }
            }
            _ => index += 1,
        }
    }
    Err(DeclarationSplitError::UnclosedDeclaration)
}

fn skip_quoted(bytes: &[u8], mut index: usize, quote: u8) -> Option<usize> {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
            } else {
                return Some(index + 1);
            }
        } else {
            index += 1;
        }
    }
    None
}

fn skip_block_comment(bytes: &[u8], mut index: usize) -> Option<usize> {
    index += 2;
    let mut depth = 1usize;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return Some(index);
            }
        } else {
            index += 1;
        }
    }
    None
}

fn skip_dollar_quoted(bytes: &[u8], start: usize) -> Option<usize> {
    let mut delimiter_end = start + 1;
    while delimiter_end < bytes.len()
        && (bytes[delimiter_end].is_ascii_alphanumeric() || bytes[delimiter_end] == b'_')
    {
        delimiter_end += 1;
    }
    if bytes.get(delimiter_end) != Some(&b'$') {
        return None;
    }
    delimiter_end += 1;
    let delimiter = &bytes[start..delimiter_end];
    bytes[delimiter_end..]
        .windows(delimiter.len())
        .position(|window| window == delimiter)
        .map(|offset| delimiter_end + offset + delimiter.len())
}

fn collect_query_binds(source: &str, start: usize, end: usize) -> Vec<Spanned<String>> {
    let body_start = source[start..end]
        .find("->")
        .map(|offset| start + offset + 2)
        .and_then(|offset| {
            source[offset..end]
                .find('{')
                .map(|brace| offset + brace + 1)
        })
        .unwrap_or(start);
    collect_named_binds(source, body_start, end)
}

fn collect_named_binds(source: &str, start: usize, end: usize) -> Vec<Spanned<String>> {
    let bytes = source.as_bytes();
    let mut binds = Vec::new();
    let mut index = start;
    let mut dollar_delimiter: Option<Vec<u8>> = None;
    while index < end {
        if let Some(delimiter) = dollar_delimiter.as_deref() {
            if bytes[index..end].starts_with(delimiter) {
                index += delimiter.len();
                dollar_delimiter = None;
            } else {
                index += 1;
            }
            continue;
        }
        match bytes[index] {
            b'-' if index + 1 < end && bytes[index + 1] == b'-' => {
                index += 2;
                while index < end && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if index + 1 < end && bytes[index + 1] == b'*' => {
                index += 2;
                let mut depth = 1usize;
                while index < end && depth > 0 {
                    if index + 1 < end && bytes[index] == b'/' && bytes[index + 1] == b'*' {
                        depth += 1;
                        index += 2;
                    } else if index + 1 < end && bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            b'\'' => {
                index += 1;
                while index < end {
                    if bytes[index] == b'\'' {
                        if index + 1 < end && bytes[index + 1] == b'\'' {
                            index += 2;
                        } else {
                            index += 1;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
            }
            b'"' => {
                index += 1;
                while index < end {
                    if bytes[index] == b'"' {
                        if index + 1 < end && bytes[index + 1] == b'"' {
                            index += 2;
                        } else {
                            index += 1;
                            break;
                        }
                    } else {
                        index += 1;
                    }
                }
            }
            b'$' => {
                let mut delimiter_end = index + 1;
                while delimiter_end < end
                    && (bytes[delimiter_end].is_ascii_alphanumeric()
                        || bytes[delimiter_end] == b'_')
                {
                    delimiter_end += 1;
                }
                if delimiter_end < end && bytes[delimiter_end] == b'$' {
                    delimiter_end += 1;
                    dollar_delimiter = Some(bytes[index..delimiter_end].to_vec());
                    index = delimiter_end;
                } else {
                    index += 1;
                }
            }
            b':' if index + 1 < end
                && bytes[index + 1] != b':'
                && (index == start || bytes[index - 1] != b':') =>
            {
                let bind_start = index;
                index += 1;
                if index < end
                    && source[index..end]
                        .chars()
                        .next()
                        .is_some_and(|character| character == '_' || character.is_alphabetic())
                {
                    index += source[index..end]
                        .chars()
                        .next()
                        .expect("character")
                        .len_utf8();
                    while index < end {
                        let Some(character) = source[index..end].chars().next() else {
                            break;
                        };
                        if character == '_'
                            || character == '$'
                            || character.is_alphanumeric()
                            || is_postgresql_identifier_continue(character)
                        {
                            index += character.len_utf8();
                        } else {
                            break;
                        }
                    }
                    binds.push(Spanned {
                        span: Span::new(bind_start as u32, index as u32),
                        value: source[bind_start..index].to_owned(),
                    });
                }
            }
            _ => index += 1,
        }
    }
    binds
}

#[cfg(test)]
mod tests {
    use super::collect_named_binds;

    #[test]
    fn named_bind_scan_skips_quotes_dollar_quotes_and_casts() {
        let source = "select ':x', $$:x$$, :x::text, /* :block /* :nested */ */ 1 -- :line\n";
        assert_eq!(
            collect_named_binds(source, 0, source.len())
                .into_iter()
                .map(|bind| bind.value)
                .collect::<Vec<_>>(),
            [":x"]
        );
    }

    #[test]
    fn named_bind_scan_accepts_postgresql_unicode_identifiers() {
        let source = "select :étiquette";
        let binds = collect_named_binds(source, 0, source.len());
        assert_eq!(binds.len(), 1);
        assert_eq!(binds[0].value, ":étiquette");
    }

    #[test]
    fn declaration_sources_ignore_query_text_inside_quoted_regions_and_comments() {
        let source = r#"
/// query Documentation() -> one { select 0 }
query Alpha() -> one { select 'query NotADeclaration()' }
/* query BlockComment() -> one { select 0 } */
query Beta(value: text) -> one { select $$query DollarQuoted()$$, :value }
"#;
        let declarations = super::declaration_sources(source).expect("split declarations");
        assert_eq!(declarations.len(), 2);
        assert!(declarations[0].starts_with("query Alpha"));
        assert!(declarations[1].starts_with("query Beta"));
    }
}
