//! Parse styx into query schema types.
//!
//! Uses facet-styx for parsing.

use crate::filter_spec::validate_query_file;
use crate::{QError, QErrorKind, QSource};
use camino::Utf8Path;
use dibs_query_schema::{QueryFile, Span};
use std::sync::Arc;

/// Parse a styx source string into a QueryFile.
///
/// This function parses the styx source and validates all filter arguments
/// according to their specifications. If any filter has invalid arguments
/// (wrong count or wrong type), an error with proper span information is returned.
pub fn parse_query_file(source_path: &Utf8Path, source: &str) -> Result<QueryFile, QError> {
    let qsource = Arc::new(QSource {
        source: source.to_string(),
        source_path: source_path.to_owned(),
    });

    let query_file: QueryFile = facet_styx::from_str(source).map_err(|e| {
        let span = e.span.unwrap_or(Span { offset: 0, len: 0 });
        QError {
            source: qsource.clone(),
            span,
            kind: QErrorKind::Parse {
                message: e.kind.to_string(),
            },
        }
    })?;

    // Validate all filter arguments after parsing
    validate_query_file(qsource, &query_file)?;

    Ok(query_file)
}
