//! Convert TOML into JSON (Tools → Convert → TOML → JSON).
//!
//! The TOML document is parsed into a generic value and re-serialized as
//! pretty-printed JSON.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert TOML `input` to pretty-printed JSON.
///
/// # Errors
/// Returns an error when the input is not valid TOML.
pub fn convert(input: &str) -> Result<String, String> {
    let value: serde_json::Value = toml::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_table() {
        let json = convert("name = \"Vix\"\ncount = 3\n").unwrap();
        assert!(json.contains("\"name\": \"Vix\""), "got: {json}");
        assert!(json.contains("\"count\": 3"), "got: {json}");
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(convert("= bad").is_err());
    }

    proptest::proptest! {
        #[test]
        fn convert_never_panics(s in ".*") {
            let _ = convert(&s);
        }
    }
}
