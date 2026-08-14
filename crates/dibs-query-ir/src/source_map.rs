use dibs_query_syntax::SourceSpan;

use crate::{SqlNodeId, TypedNodeId};

/// Source provenance carried by HIR and typed IR nodes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct SourceOrigin {
    /// Primary authored source location, absent only for compiler-generated nodes.
    pub primary: Option<SourceSpan>,
    /// Additional authored locations contributing to the node, in semantic order.
    pub related: Vec<SourceSpan>,
    /// Why the node has no exact authored token when it is generated.
    pub generated: Option<GeneratedOrigin>,
}

impl SourceOrigin {
    /// Creates an origin for one exact authored source location.
    #[must_use]
    pub fn authored(primary: SourceSpan) -> Self {
        Self {
            primary: Some(primary),
            related: Vec::new(),
            generated: None,
        }
    }

    /// Creates an origin for a compiler-generated node.
    #[must_use]
    pub fn generated(reason: GeneratedOrigin, related: Vec<SourceSpan>) -> Self {
        Self {
            primary: None,
            related,
            generated: Some(reason),
        }
    }

    /// Returns the primary source span.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self.primary {
            Some(span) => span,
            None => panic!("generated origin has no primary span"),
        }
    }
}

/// Reason a semantic node was synthesized by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum GeneratedOrigin {
    /// Required PostgreSQL syntax with no authored token.
    RequiredSyntax,
    /// Normalized equivalent semantic form.
    Normalization,
    /// Generated API-contract connection.
    Contract,
    /// Compiler-internal structural node.
    Structural,
}

/// Half-open byte range inside deterministic rendered SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct SqlByteRange {
    /// First rendered byte included in the range.
    pub start: u32,
    /// First rendered byte after the range.
    pub end: u32,
}

impl SqlByteRange {
    /// Creates a half-open rendered SQL byte range.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns whether the byte offset is contained in this half-open range.
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns whether two non-empty half-open ranges overlap.
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Provenance for one rendered SQL fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum SqlProvenance {
    /// Token or fragment directly represents authored semantics.
    Authored,
    /// Whitespace chosen by the deterministic renderer.
    GeneratedWhitespace,
    /// Punctuation required by PostgreSQL syntax.
    GeneratedPunctuation,
    /// Keyword introduced by lowering or normalization.
    GeneratedKeyword,
    /// Positional bind token generated from a named parameter.
    GeneratedBind,
}

/// One exact link among source, typed IR, and rendered SQL.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
pub struct SourceMapEntry {
    /// Generated SQL fragment identity.
    pub sql_node_id: SqlNodeId,
    /// Typed node that caused the fragment, when semantic.
    pub typed_node: Option<TypedNodeId>,
    /// Original source origin, absent for purely generated punctuation/spacing.
    pub source: Option<SourceOrigin>,
    /// Exact half-open rendered SQL byte range.
    pub sql_range: SqlByteRange,
    /// Authored or generated provenance class.
    pub provenance: SqlProvenance,
}

/// Bidirectional exact source-to-rendered-SQL mapping.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct SourceMap {
    /// Entries in deterministic rendered SQL order. Repeated fragments remain distinct.
    pub entries: Vec<SourceMapEntry>,
}

impl SourceMap {
    /// Creates a source map and normalizes entries by rendered position and node identity.
    #[must_use]
    pub fn new(mut entries: Vec<SourceMapEntry>) -> Self {
        entries.sort_by_key(|entry| {
            (
                entry.sql_range.start,
                entry.sql_range.end,
                entry.sql_node_id,
            )
        });
        Self { entries }
    }

    /// Returns every entry whose primary source span exactly equals `source`.
    ///
    /// Repeated rendered fragments are returned separately in rendered order.
    #[must_use]
    pub fn entries_for_source(&self, source: SourceSpan) -> Vec<&SourceMapEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .source
                    .as_ref()
                    .and_then(|origin| origin.primary)
                    .is_some_and(|candidate| candidate == source)
            })
            .collect()
    }

    /// Returns every entry containing `offset` under half-open interval semantics.
    #[must_use]
    pub fn entries_at_sql_offset(&self, offset: u32) -> Vec<&SourceMapEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.sql_range.contains(offset))
            .collect()
    }

    /// Returns every entry whose rendered range overlaps `range`.
    #[must_use]
    pub fn entries_overlapping_sql(&self, range: SqlByteRange) -> Vec<&SourceMapEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.sql_range.overlaps(range))
            .collect()
    }
}
