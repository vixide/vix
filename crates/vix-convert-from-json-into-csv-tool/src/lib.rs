//! Convert a JSON array of objects into CSV (Tools → Convert → JSON → CSV).
//!
//! The CSV header is the union of all object keys (first-seen order); each
//! object becomes one row. See [`vix_convert_tabular`] for the shared logic;
//! this crate is only the thin JSON -> CSV pairing exposed to the tool menu.
//!
//! Owning spec: `crates/vix-convert-from-json-into-csv-tool/spec/index.md`.

/// Convert JSON `input` (an array of objects) to CSV.
///
/// # Errors
/// Returns an error when the input is not valid JSON, not an array, or contains
/// a non-object element.
///
/// # Examples
///
/// ```
/// let csv = vix_convert_from_json_into_csv_tool::convert(r#"[{"a":"1","b":"2"}]"#).unwrap();
/// assert_eq!(csv, "a,b\n1,2\n");
/// ```
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    Ok(vix_convert_tabular::write_csv(
        &vix_convert_tabular::json_to_rows(input)?,
    ))
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
