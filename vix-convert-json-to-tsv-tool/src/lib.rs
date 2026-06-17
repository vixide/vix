//! Convert a JSON array of objects into TSV (Tools → Convert → JSON → TSV).
//!
//! The TSV header is the union of all object keys (first-seen order); each
//! object becomes one row. See [`vix_convert_tabular`] for the shared logic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert JSON `input` (an array of objects) to TSV.
///
/// # Errors
/// Returns an error when the input is not valid JSON, not an array, or contains
/// a non-object element.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(vix_convert_tabular::write_tsv(&vix_convert_tabular::json_to_rows(input)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_objects_to_rows() {
        let tsv = convert(r#"[{"a":"1","b":"2"}]"#).unwrap();
        assert_eq!(tsv, "a\tb\n1\t2\n");
    }

    #[test]
    fn rejects_non_array() {
        assert!(convert("nope").is_err());
    }
}
