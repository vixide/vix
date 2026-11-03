//! Convert TSV text into a JSON array of objects (Tools → Convert → TSV → JSON).
//!
//! The first TSV row supplies the object keys; each later row becomes one object
//! with string values. See [`vix_convert_tabular`] for the shared logic.
//!
//! This crate is a thin adapter: it only wires `parse_tsv` to `rows_to_json`
//! so the Tools menu has one `convert` entry point per format pair, all with
//! the same `fn(&str) -> Result<String, ConvertError>` shape. Spec:
//! `crates/vix-convert-from-tsv-into-json-tool/spec/index.md`.

/// Convert TSV `input` to pretty-printed JSON.
///
/// The first row is the header (object keys); every later row is one object
/// whose values are all strings (no type inference).
///
/// # Examples
/// ```
/// let json = vix_convert_from_tsv_into_json_tool::convert("a\tb\n1\t2\n").unwrap();
/// assert!(json.contains("\"a\": \"1\""));
/// ```
///
/// # Errors
/// Never fails today; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    Ok(vix_convert_tabular::rows_to_json(
        &vix_convert_tabular::parse_tsv(input),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_header_and_rows() {
        let json = convert("a\tb\n1\t2\n").unwrap();
        assert!(json.contains("\"a\": \"1\""), "got: {json}");
        assert!(json.contains("\"b\": \"2\""), "got: {json}");
    }
}
