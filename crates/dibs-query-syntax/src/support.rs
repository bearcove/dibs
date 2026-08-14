use std::fmt;

/// Stable identity for one Dibs query source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct SourceId(u32);

impl SourceId {
    /// Creates a source identity from its stable numeric value.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Creates the conventional source identity used by parser tests.
    pub const fn test() -> Self {
        Self(0)
    }

    /// Returns the stable numeric value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Half-open byte range inside a source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct Span {
    /// First byte included in the range.
    pub start: u32,
    /// First byte after the range.
    pub end: u32,
}

impl Span {
    /// Creates a half-open source range.
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns an empty range at `offset`.
    pub const fn empty(offset: u32) -> Self {
        Self::new(offset, offset)
    }
}

/// A half-open byte range paired with its source document identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct SourceSpan {
    /// Source document containing the range.
    pub source_id: SourceId,
    /// Half-open byte range inside the source document.
    pub span: Span,
}

impl SourceSpan {
    /// Creates a source-qualified half-open byte range.
    pub const fn new(source_id: SourceId, span: Span) -> Self {
        Self { source_id, span }
    }
}

/// A value paired with the source bytes that produced it.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct Spanned<T> {
    /// Decoded value.
    pub value: T,
    /// Source byte range.
    pub span: Span,
}
