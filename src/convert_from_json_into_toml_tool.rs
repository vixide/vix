#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Convert JSON into TOML (Tools → Convert → JSON → TOML).
//!
//! The JSON document is parsed into a generic value and re-serialized as TOML.
//! TOML documents must be a table at the top level, so a JSON array or scalar
//! input is reported as an error.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert JSON `input` to TOML.
///
/// # Errors
/// Returns an error when the input is not valid JSON, or when it is not a
/// top-level object (TOML cannot represent a top-level array or scalar).
pub fn convert(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    toml::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_object() {
        let toml = convert(r#"{"name":"Vix","count":3}"#).unwrap();
        assert!(toml.contains("name = \"Vix\""), "got: {toml}");
        assert!(toml.contains("count = 3"), "got: {toml}");
    }

    #[test]
    fn rejects_top_level_array() {
        // TOML has no top-level array form.
        assert!(convert("[1, 2, 3]").is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(convert("{nope}").is_err());
    }
}
