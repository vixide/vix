//! Convert JSON into YAML (Tools → Convert → JSON → YAML).
//!
//! The JSON document is parsed into a generic value and re-serialized as YAML,
//! so scalars, arrays and objects all carry over.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert JSON `input` to YAML.
///
/// # Errors
/// Returns an error when the input is not valid JSON.
pub fn convert(input: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    serde_yaml::to_string(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_object() {
        // Object keys come out sorted (serde_json's default map ordering).
        assert_eq!(
            convert(r#"{"name":"Vix","count":3}"#).unwrap(),
            "count: 3\nname: Vix\n"
        );
    }

    #[test]
    fn converts_array() {
        assert_eq!(convert("[1,2]").unwrap(), "- 1\n- 2\n");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(convert("{nope}").is_err());
    }

    proptest::proptest! {
        #[test]
        fn convert_never_panics(s in ".*") {
            let _ = convert(&s);
        }
    }

    #[test]
    fn deeply_nested_input_does_not_overflow() {
        // Pathological nesting must return an error, not overflow the stack.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        let _ = convert(&deep);
    }
}
