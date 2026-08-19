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

impl DiagnosticCode {
    /// Returns the stable machine-readable code used by rendered diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseFailed => "DIBS-SYNTAX-PARSE",
            Self::UnexpectedToken => "DIBS-SYNTAX-UNEXPECTED",
            Self::MissingToken => "DIBS-SYNTAX-MISSING",
            Self::AstLoweringFailed => "DIBS-SYNTAX-AST-LOWERING",
        }
    }
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

/// Converts Dibs syntax diagnostics into Margin's renderer-neutral envelope.
///
/// `source_name` and `source_text` describe the source document referenced by
/// every diagnostic. Dibs-specific recovery, repair, and cost fields remain on
/// the input diagnostics and are not replaced by Margin types.
pub fn to_margin_diagnostics<'a>(
    source_name: impl Into<String>,
    source_text: impl Into<String>,
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
) -> margin::Diagnostics {
    let mut diagnostics = diagnostics.into_iter().peekable();
    let source_id = diagnostics
        .peek()
        .map(|diagnostic| diagnostic.source_id)
        .unwrap_or_else(SourceId::test);
    let reports = diagnostics
        .map(|diagnostic| margin::Report {
            code: Some(diagnostic.code.as_str().to_string()),
            severity: margin::Severity::Error,
            title: diagnostic.message.clone(),
            annotations: vec![margin::Annotation {
                spans: vec![margin::Span {
                    source_id: margin::SourceId(diagnostic.source_id.to_string()),
                    start: diagnostic.primary.start as usize,
                    end: diagnostic.primary.end as usize,
                }],
                role: margin::AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some(diagnostic.message.clone()),
                priority: 100,
            }],
            notes: diagnostic
                .hints
                .iter()
                .cloned()
                .map(|text| margin::Note {
                    kind: margin::NoteKind::Help,
                    text,
                })
                .collect(),
            sections: Vec::new(),
        })
        .collect();

    margin::Diagnostics {
        sources: vec![margin::Source {
            id: margin::SourceId(source_id.to_string()),
            name: source_name.into(),
            hyperlink: None,
            text: source_text.into(),
        }],
        reports,
    }
}
#[cfg(test)]
mod tests {
    use margin::{AnnotationRole, NoteKind};

    use super::{Diagnostic, DiagnosticCode, Repair};
    use crate::{SourceId, Span};

    #[test]
    fn margin_conversion_preserves_typed_diagnostic_facts() {
        let source_id = SourceId::new(7);
        let source = "query Café() -> one { select § }";

        let start = source.find('§').unwrap();
        let diagnostic = Diagnostic {
            code: DiagnosticCode::UnexpectedToken,
            source_id,
            primary: Span::new(start as u32, (start + '§'.len_utf8()) as u32),
            unexpected: Some("§".to_string()),
            expected: None,
            repair: Some(Repair::SkipUnexpected),
            cost: Some(1),
            message: "unexpected \"§\"".to_string(),
            hints: vec!["remove the unexpected token".to_string()],
        };

        let rendered = super::to_margin_diagnostics("queries/example.dibs", source, [&diagnostic]);

        assert_eq!(diagnostic.repair, Some(Repair::SkipUnexpected));
        assert_eq!(diagnostic.cost, Some(1));
        assert_eq!(rendered.sources.len(), 1);
        assert_eq!(rendered.sources[0].id.0, "7");
        assert_eq!(rendered.sources[0].name, "queries/example.dibs");
        assert_eq!(rendered.sources[0].text, source);
        let report = &rendered.reports[0];
        assert_eq!(report.code.as_deref(), Some("DIBS-SYNTAX-UNEXPECTED"));
        assert_eq!(report.title, "unexpected \"§\"");
        assert_eq!(report.annotations[0].role, AnnotationRole::PrimaryLabel);
        assert_eq!(report.annotations[0].spans[0].start, start);
        assert_eq!(report.annotations[0].spans[0].end, start + '§'.len_utf8());
        assert_eq!(
            report.annotations[0].message.as_deref(),
            Some("unexpected \"§\"")
        );
        assert_eq!(report.notes[0].kind, NoteKind::Help);
        assert_eq!(report.notes[0].text, "remove the unexpected token");
    }

    #[test]
    fn margin_conversion_uses_missing_token_code_and_label() {
        let diagnostic = Diagnostic {
            code: DiagnosticCode::MissingToken,
            source_id: SourceId::test(),
            primary: Span::empty(6),
            unexpected: None,
            expected: Some("identifier".to_string()),
            repair: Some(Repair::InsertMissing),
            cost: Some(1),
            message: "expected identifier".to_string(),
            hints: Vec::new(),
        };

        let rendered = super::to_margin_diagnostics("query.dibs", "query ", [&diagnostic]);
        let report = &rendered.reports[0];

        assert_eq!(report.code.as_deref(), Some("DIBS-SYNTAX-MISSING"));
        assert_eq!(
            report.annotations[0].message.as_deref(),
            Some("expected identifier")
        );
        assert_eq!(report.annotations[0].spans[0].start, 6);
        assert_eq!(report.annotations[0].spans[0].end, 6);
    }
}
