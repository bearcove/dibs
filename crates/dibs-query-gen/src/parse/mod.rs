//! Parse styx into query schema types.
//!
//! Uses facet-styx for parsing.

use crate::error::{ErrorKind, QueryGenError};
use crate::schema;
use camino::Utf8Path;
use dibs_query_schema::Span;

/// Parse a styx source string into a QueryFile.
pub fn parse_query_file(
    source_path: &Utf8Path,
    source: &str,
) -> Result<schema::QueryFile, QueryGenError> {
    facet_styx::from_str(source).map_err(|e| {
        let span = e.span.unwrap_or(Span { offset: 0, len: 0 });
        QueryGenError {
            source: source.to_string(),
            source_path: source_path.to_owned(),
            span,
            kind: ErrorKind::Parse {
                message: e.kind.to_string(),
            },
        }
    })
}
