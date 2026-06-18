//! Convert TSV text into a JSON array of objects (Tools → Convert → TSV → JSON).
//!
//! The first TSV row supplies the object keys; each later row becomes one object
//! with string values. See [`crate::convert_tabular`] for the shared logic.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert TSV `input` to pretty-printed JSON.
///
/// # Errors
/// Never fails today; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(crate::convert_tabular::rows_to_json(&crate::convert_tabular::parse_tsv(input)))
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
