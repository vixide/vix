//! Convert a JSON array of objects into CSV (Tools → Convert → JSON → CSV).
//!
//! The CSV header is the union of all object keys (first-seen order); each
//! object becomes one row. See [`crate::convert_tabular`] for the shared logic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert JSON `input` (an array of objects) to CSV.
///
/// # Errors
/// Returns an error when the input is not valid JSON, not an array, or contains
/// a non-object element.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(crate::convert_tabular::write_csv(&crate::convert_tabular::json_to_rows(input)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_objects_to_rows() {
        let csv = convert(r#"[{"a":"1","b":"2"}]"#).unwrap();
        assert_eq!(csv, "a,b\n1,2\n");
    }

    #[test]
    fn rejects_non_array() {
        assert!(convert(r#"{"a":1}"#).is_err());
    }
}
