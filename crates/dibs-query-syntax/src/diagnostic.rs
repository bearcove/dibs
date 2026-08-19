use snark::parser::{ParseDiagnostic, ParseDiagnosticCode, ParseRepair};

use crate::{SourceId, Span, ast::TypedAstLowerError};

/// Stable syntax diagnostic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// The strict parser could not accept the document.
    ParseFailed,
    /// Recovery skipped unexpected authored bytes.
    UnexpectedToken,
    /// Recovery inserted a grammar symbol that was absent from the source.
    MissingToken,
    /// The accepted CST did not match the generated typed-AST contract.
    AstLoweringFailed,
}

/// Recovery operation recorded for a syntax diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    /// Unexpected input was skipped.
    SkipUnexpected,
    /// A missing grammar symbol was inserted.
    InsertMissing,
}

/// Structured Dibs query syntax diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable diagnostic category.
    pub code: DiagnosticCode,
    /// Source document identity.
    pub source_id: SourceId,
    /// Primary half-open byte range.
    pub primary: Span,
    /// Exact unexpected source text, when available.
    pub unexpected: Option<String>,
    /// Expected terminal or node name, when available.
    pub expected: Option<String>,
    /// Recovery operation, when the parser recovered.
    pub repair: Option<Repair>,
    /// Parser-provided recovery cost, when available.
    pub cost: Option<u32>,
    /// Human-readable diagnostic text.
    pub message: String,
    /// Actionable hints for the author.
    pub hints: Vec<String>,
}

impl Diagnostic {
    pub(crate) fn parse_failure(source_id: SourceId, source_len: usize, message: String) -> Self {
        Self {
            code: DiagnosticCode::ParseFailed,
            source_id,
            primary: Span::empty(source_len.min(u32::MAX as usize) as u32),
            unexpected: None,
            expected: None,
            repair: None,
            cost: None,
            message,
            hints: Vec::new(),
        }
    }

    pub(crate) fn parse_error(
        source_id: SourceId,
        source_len: usize,
        error: &snark::lower::weavy::WeavyParseError,
    ) -> Self {
        let offset = match error {
            snark::lower::weavy::WeavyParseError::NoToken { byte_position, .. }
            | snark::lower::weavy::WeavyParseError::NoAction { byte_position, .. }
            | snark::lower::weavy::WeavyParseError::TrailingInput { byte_position } => {
                *byte_position
            }
            _ => source_len,
        };
        Self::parse_failure(source_id, offset, error.to_string())
    }

    pub(crate) fn recovered(source_id: SourceId, diagnostic: &ParseDiagnostic) -> Self {
        let code = match diagnostic.code {
            ParseDiagnosticCode::UnexpectedToken => DiagnosticCode::UnexpectedToken,
            ParseDiagnosticCode::MissingToken => DiagnosticCode::MissingToken,
        };
        let repair = match diagnostic.repair {
            ParseRepair::SkipUnexpected => Repair::SkipUnexpected,
            ParseRepair::InsertMissing => Repair::InsertMissing,
        };
        let start = diagnostic.bytes.start().get();
        let end = diagnostic.bytes.end().get();
        let message = match diagnostic.code {
            ParseDiagnosticCode::UnexpectedToken => {
                let unexpected = diagnostic.unexpected.as_deref().unwrap_or("input");
                format!("unexpected {unexpected:?}")
            }
            ParseDiagnosticCode::MissingToken => {
                let expected = diagnostic.expected.as_deref().unwrap_or("syntax");
                format!("expected {expected}")
            }
        };

        Self {
            code,
            source_id,
            primary: Span::new(start, end),
            unexpected: diagnostic.unexpected.clone(),
            expected: diagnostic.expected.clone(),
            repair: Some(repair),
            cost: diagnostic.cost,
            message,
            hints: Vec::new(),
        }
    }

    pub(crate) fn lowering(source_id: SourceId, error: TypedAstLowerError) -> Self {
        Self {
            code: DiagnosticCode::AstLoweringFailed,
            source_id,
            primary: Span::empty(0),
            unexpected: None,
            expected: None,
            repair: None,
            cost: None,
            message: format!("typed AST lowering failed: {error:?}"),
            hints: Vec::new(),
        }
    }
}
