//! Convert a JSON array of objects into TSV (Tools → Convert → JSON → TSV).
//!
//! The TSV header is the union of all object keys (first-seen order); each
//! object becomes one row. See [`vix_convert_tabular`] for the shared logic.
//!
//! This crate is a deliberately thin wrapper: parsing (`json_to_rows`) and
//! rendering (`write_tsv`) both live in `vix-convert-tabular`, which is shared
//! with the other tabular converters so quoting/escaping rules stay in one
//! place. The host calls [`convert`] from the Tools -> Convert menu action and
//! replaces the buffer (or opens a new one) with the result.
//!
//! Spec: `crates/vix-convert-from-json-into-tsv-tool/spec/index.md`.

/// Convert JSON `input` (an array of objects) to TSV.
///
/// The header row is the union of all object keys in first-seen order; every
/// object then becomes one row.
///
/// # Errors
/// Returns an error when the input is not valid JSON, not an array, or contains
/// a non-object element.
///
/// # Examples
/// ```
/// let tsv = vix_convert_from_json_into_tsv_tool::convert(r#"[{"a":"1","b":"2"}]"#).unwrap();
/// assert_eq!(tsv, "a\tb\n1\t2\n");
/// assert!(vix_convert_from_json_into_tsv_tool::convert("not json").is_err());
/// ```
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    Ok(vix_convert_tabular::write_tsv(
        &vix_convert_tabular::json_to_rows(input)?,
    ))
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
