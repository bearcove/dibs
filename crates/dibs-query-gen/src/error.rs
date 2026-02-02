//! Query generation errors.

use dibs_query_schema::Span;
use std::fmt;

// ============================================================================
// Error Handling Types
// ============================================================================

/// Error during code generation.
/// Carries span information for proper error reporting.
#[derive(Clone)]
pub struct QueryGenError {
    /// Location in the source .styx file
    pub span: Span,
    /// The original source code (for rendering diagnostics)
    pub source: String,
    /// Error classification and details
    pub kind: ErrorKind,
}

/// Error classification for query generation.
#[derive(Debug, Clone)]
pub enum ErrorKind {
    ColumnNotFound {
        table: String,
        column: String,
    },
    TableNotFound {
        table: String,
    },
    SchemaMismatch {
        table: String,
        column: String,
        reason: String,
    },
    PlanMissing {
        reason: String,
    },
}

impl fmt::Display for QueryGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

impl std::error::Error for QueryGenError {}
