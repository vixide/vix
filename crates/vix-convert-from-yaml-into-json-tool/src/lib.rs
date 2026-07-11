//! Convert YAML into JSON (Tools → Convert → YAML → JSON).
//!
//! The YAML document is parsed into a generic value and re-serialized as
//! pretty-printed JSON, so scalars, sequences and mappings all carry over.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert YAML `input` to pretty-printed JSON.
///
/// # Errors
/// Returns an error when the input is not valid YAML.
pub fn convert(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_yaml::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_mapping() {
        let json = convert("name: Vix\ncount: 3\n").unwrap();
        assert!(json.contains("\"name\": \"Vix\""), "got: {json}");
        assert!(json.contains("\"count\": 3"), "got: {json}");
    }

    #[test]
    fn converts_sequence() {
        assert_eq!(convert("- 1\n- 2\n").unwrap(), "[\n  1,\n  2\n]");
    }

    #[test]
    fn rejects_invalid_yaml() {
        assert!(convert("a: [unclosed").is_err());
    }
}
