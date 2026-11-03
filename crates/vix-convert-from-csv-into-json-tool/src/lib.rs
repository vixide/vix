//! Convert CSV text into a JSON array of objects (Tools → Convert → CSV → JSON).
//!
//! The first CSV row supplies the object keys; each later row becomes one object
//! with string values. See [`vix_convert_tabular`] for the shared logic.
//!
//! This crate is a deliberately thin adapter: the parsing and JSON rendering
//! live in `vix-convert-tabular`; this crate only fixes the CSV-to-JSON
//! direction behind the uniform `convert(&str) -> Result<String, _>` shape that
//! every convert tool exposes to the Tools menu. Spec:
//! `crates/vix-convert-from-csv-into-json-tool/spec/index.md`.

/// Convert CSV `input` to pretty-printed JSON.
///
/// The first row is the header; every later row becomes one JSON object whose
/// values are all strings (no type inference).
///
/// # Examples
/// ```
/// let json = vix_convert_from_csv_into_json_tool::convert("a,b\n1,2\n").unwrap();
/// assert!(json.contains("\"a\": \"1\""));
/// ```
///
/// # Errors
/// Never fails today (CSV parsing is lenient); returns `Result` for a uniform
/// tool interface with the directions that can fail.
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    Ok(vix_convert_tabular::rows_to_json(
        &vix_convert_tabular::parse_csv(input),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_header_and_rows() {
        let json = convert("a,b\n1,2\n").unwrap();
        assert!(json.contains("\"a\": \"1\""), "got: {json}");
        assert!(json.contains("\"b\": \"2\""), "got: {json}");
    }
}
