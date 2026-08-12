use dibs_query_syntax::{Diagnostic as SyntaxDiagnostic, SourceSpan};

/// Stable semantic compiler diagnostic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum CompileDiagnosticCode {
    /// Strict parsing failed before semantic lowering.
    Syntax,
    /// The source declares the same parameter more than once.
    DuplicateParameter,
    /// A named bind does not have a matching declaration.
    UnknownParameter,
    /// A declared parameter is never referenced by the statement.
    UnusedParameter,
    /// No catalog relation matches the authored name.
    UnknownRelation,
    /// More than one catalog relation matches the authored unqualified name.
    AmbiguousRelation,
    /// No visible relation exposes the authored field.
    UnknownField,
    /// More than one visible relation exposes an unqualified field.
    AmbiguousField,
    /// No catalog callable matches the authored function call.
    UnknownCallable,
    /// More than one catalog callable remains viable after semantic checking.
    AmbiguousCallable,
    /// A relation alias is repeated in one scope.
    DuplicateRelationBinding,
    /// A projection output label is repeated.
    DuplicateOutputLabel,
    /// A computed projection has no explicit output alias.
    MissingOutputAlias,
    /// The clause is structurally valid SQL but outside the ordinary SELECT compiler path.
    UnsupportedClause,
    /// A LIMIT or OFFSET value is invalid for the checked artifact.
    InvalidLimit,
    /// Semantic PostgreSQL type checking failed.
    TypeMismatch,
    /// Declared result mode is incompatible with the statement result.
    ResultModeMismatch,
    /// Target-language API naming or codec policy cannot be represented losslessly.
    InvalidApiContract,
    /// The completed immutable artifact failed its own validation.
    InvalidArtifact,
}

/// One structured compiler failure with exact source location.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct CompileDiagnostic {
    /// Stable diagnostic category.
    pub code: CompileDiagnosticCode,
    /// Exact primary source span.
    pub span: SourceSpan,
    /// Related source locations such as competing bindings.
    pub related: Vec<SourceSpan>,
    /// Human-readable explanation.
    pub message: String,
}

impl CompileDiagnostic {
    pub(crate) fn new(
        code: CompileDiagnosticCode,
        span: SourceSpan,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            span,
            related: Vec::new(),
            message: message.into(),
        }
    }

    pub(crate) fn with_related(mut self, related: Vec<SourceSpan>) -> Self {
        self.related = related;
        self
    }

    pub(crate) fn from_syntax(diagnostic: SyntaxDiagnostic) -> Self {
        Self::new(
            CompileDiagnosticCode::Syntax,
            SourceSpan::new(diagnostic.source_id, diagnostic.primary),
            diagnostic.message,
        )
    }
}

/// Non-empty semantic diagnostic collection returned by strict compilation.
pub type DiagnosticSet = Vec<CompileDiagnostic>;
