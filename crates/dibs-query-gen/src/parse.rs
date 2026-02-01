//! Parse styx into query schema types.
//!
//! Uses facet-styx for parsing.

use crate::schema;
use facet_format::DeserializeError;
use thiserror::Error;

/// Errors that can occur when parsing a query file.
///
/// Stores the filename and source so that `Debug` can render a pretty ariadne report
/// for deserialization errors.
///
/// The `Deserialize` variant wraps deserialization errors from facet-styx, which have
/// full source location information.
///
/// The `Validation` variant is for errors that occur after parsing succeeds.
/// These currently lack source location information - see:
/// <https://github.com/bearcove/styx/issues/45>
pub struct ParseError {
    /// The filename being parsed (for error reporting).
    pub filename: String,
    /// The source text (for error reporting).
    pub source: String,
    /// The kind of error.
    pub kind: ParseErrorKind,
}

/// The specific kind of parse error.
#[derive(Debug, Error)]
pub enum ParseErrorKind {
    /// Deserialization error from facet-styx (has source location).
    #[error("{0}")]
    Deserialize(#[from] DeserializeError),
}

impl std::fmt::Debug for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ParseErrorKind::Deserialize(e) => {
                let report = e.to_pretty(&self.filename, &self.source);
                write!(f, "{}", report)
            }
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.kind)
    }
}

impl ParseError {
    /// Create a new parse error with context.
    pub fn new(
        filename: impl Into<String>,
        source: impl Into<String>,
        kind: ParseErrorKind,
    ) -> Self {
        Self {
            filename: filename.into(),
            source: source.into(),
            kind,
        }
    }

    /// Create a deserialization error.
    pub fn deserialize(
        filename: impl Into<String>,
        source: impl Into<String>,
        err: DeserializeError,
    ) -> Self {
        Self::new(filename, source, ParseErrorKind::Deserialize(err))
    }
}

/// Parse a styx source string into a QueryFile.
pub fn parse_query_file(
    filename: &str,
    source: &str,
) -> Result<schema::QueryFile, Box<ParseError>> {
    // Use facet-styx for parsing, return the schema directly
    facet_styx::from_str(source).map_err(|e| Box::new(ParseError::deserialize(filename, source, e)))
}
