//! Query generation errors.

use camino::Utf8PathBuf;
use dibs_query_schema::Span;
use std::fmt;

// ============================================================================
// Error Handling Types
// ============================================================================

/// Error during code generation.
/// Carries span information for proper error reporting.
#[derive(Clone)]
pub struct QueryGenError {
    /// The original source code (for rendering diagnostics)
    pub source: String,

    /// Path to the source file
    pub source_path: Utf8PathBuf,

    /// Location in the source .styx file
    pub span: Span,

    /// Error classification and details
    pub kind: ErrorKind,
}

/// Error classification for query generation.
#[derive(Debug, Clone)]
pub enum ErrorKind {
    /// A column referenced in the query does not exist in the table.
    ColumnNotFound {
        /// The table that was searched.
        table: String,
        /// The column that was not found.
        column: String,
    },

    /// A table referenced in the query does not exist in the schema.
    TableNotFound {
        /// The table that was not found.
        table: String,
    },

    /// The query references a column that exists but has incompatible properties.
    SchemaMismatch {
        /// The table containing the column.
        table: String,
        /// The column with the mismatch.
        column: String,
        /// Description of the mismatch.
        reason: String,
    },

    /// The query planner failed to produce a plan.
    PlanMissing {
        /// Why the plan could not be generated.
        reason: String,
    },

    /// Failed to parse the styx source file.
    Parse {
        /// The parse error message.
        message: String,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::ColumnNotFound { table, column } => {
                write!(f, "column '{}' not found in table '{}'", column, table)
            }
            ErrorKind::TableNotFound { table } => {
                write!(f, "table '{}' not found", table)
            }
            ErrorKind::SchemaMismatch {
                table,
                column,
                reason,
            } => {
                write!(f, "schema mismatch for '{}.{}': {}", table, column, reason)
            }
            ErrorKind::PlanMissing { reason } => {
                write!(f, "query plan missing: {}", reason)
            }
            ErrorKind::Parse { message } => {
                write!(f, "{}", message)
            }
        }
    }
}

impl fmt::Debug for QueryGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for QueryGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ariadne::{Config, Label, Report, ReportKind, Source};

        let message = self.kind.to_string();

        let start = self.span.start as usize;
        let end = self.span.end as usize;

        let mut output = Vec::new();

        let report = Report::build(ReportKind::Error, (&self.source_path, start..end))
            .with_message(&message)
            .with_config(Config::default().with_color(false))
            .with_label(Label::new((&self.source_path, start..end)).with_message(&message))
            .finish();

        report
            .write((&self.source_path, Source::from(&self.source)), &mut output)
            .ok();

        write!(f, "{}", String::from_utf8_lossy(&output))
    }
}

impl std::error::Error for QueryGenError {}
