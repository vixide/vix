#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Convert CSV text into a JSON array of objects (Tools → Convert → CSV → JSON).
//!
//! The first CSV row supplies the object keys; each later row becomes one object
//! with string values. See [`crate::convert_tabular`] for the shared logic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert CSV `input` to pretty-printed JSON.
///
/// # Errors
/// Never fails today (CSV parsing is lenient); returns `Result` for a uniform
/// tool interface with the directions that can fail.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(crate::convert_tabular::rows_to_json(&crate::convert_tabular::parse_csv(input)))
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
