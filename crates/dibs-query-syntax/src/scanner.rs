use snark::parser::{
    ExternalScanRequest, ExternalScanResult, ExternalScannerHost, ParserExecutionError,
};

#[derive(Debug, Default)]
pub(crate) struct DibsExternalScanner;

impl ExternalScannerHost for DibsExternalScanner {
    fn scan(
        &self,
        request: ExternalScanRequest<'_>,
    ) -> Result<Option<ExternalScanResult>, ParserExecutionError> {
        if request.external_name() != Some("_dollar_quoted_literal") {
            return Ok(None);
        }
        Ok(
            scan_dollar_quote(request.input(), request.byte_position())
                .map(ExternalScanResult::new),
        )
    }
}

fn scan_dollar_quote(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    let tag_end = rest.get(1..)?.find('$')? + 1;
    let delimiter = rest.get(..=tag_end)?;
    let tag = delimiter.strip_prefix('$')?.strip_suffix('$')?;
    if !is_valid_tag(tag) {
        return None;
    }
    let body_start = start + delimiter.len();
    let body = source.get(body_start..)?;
    let body_end = body.find(delimiter)?;
    Some(body_start + body_end + delimiter.len())
}

fn is_valid_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return true;
    }
    let mut chars = tag.chars();
    chars.next().is_some_and(is_identifier_start) && chars.all(is_identifier_continue)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic() || !ch.is_ascii()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() || !ch.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::scan_dollar_quote;

    #[test]
    fn scans_empty_and_tagged_dollar_quotes() {
        assert_eq!(scan_dollar_quote("$$:x$$", 0), Some(6));
        assert_eq!(scan_dollar_quote("$tag$:x$tag$", 0), Some(12));
        assert_eq!(scan_dollar_quote("$é$:x$é$", 0), Some(10));
        assert_eq!(scan_dollar_quote("$bad-tag$x$bad-tag$", 0), None);
    }
}
