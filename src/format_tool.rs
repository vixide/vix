//! Reformat the selection (or buffer) within one data format: pretty-print or
//! minify JSON, and canonicalize YAML and TOML.
//!
//! Each function parses the text into a generic value and re-serializes it, so
//! the document is normalized (consistent indentation, key handling, quoting)
//! without changing its meaning. Used by the Tools → Format menu via
//! `App::transform_selection_or_buffer_try`.

/// Pretty-print JSON with two-space indentation.
///
/// # Errors
/// Returns an error when the input is not valid JSON.
pub fn json_pretty(input: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Minify JSON to its most compact single-line form.
///
/// # Errors
/// Returns an error when the input is not valid JSON.
pub fn json_minify(input: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

/// Canonicalize YAML (re-emit with consistent formatting).
///
/// # Errors
/// Returns an error when the input is not valid YAML.
pub fn yaml_format(input: &str) -> Result<String, String> {
    let v: serde_yaml::Value = serde_yaml::from_str(input).map_err(|e| e.to_string())?;
    serde_yaml::to_string(&v).map_err(|e| e.to_string())
}

/// Canonicalize TOML (re-emit pretty-printed).
///
/// # Errors
/// Returns an error when the input is not valid TOML, or is not a table.
pub fn toml_format(input: &str) -> Result<String, String> {
    let v: toml::Value = toml::from_str(input).map_err(|e| e.to_string())?;
    toml::to_string_pretty(&v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_pretty_indents() {
        assert_eq!(json_pretty(r#"{"a":1}"#).unwrap(), "{\n  \"a\": 1\n}");
    }

    #[test]
    fn json_minify_compacts() {
        assert_eq!(json_minify("{\n  \"a\": 1\n}").unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn json_rejects_bad_input() {
        assert!(json_pretty("{nope}").is_err());
        assert!(json_minify("{nope}").is_err());
    }

    #[test]
    fn yaml_roundtrips() {
        let out = yaml_format("a:    1\nb: 2\n").unwrap();
        assert!(out.contains("a: 1"), "got: {out}");
        assert!(yaml_format("a: [unclosed").is_err());
    }

    #[test]
    fn toml_roundtrips() {
        let out = toml_format("a=1\nb=2\n").unwrap();
        assert!(out.contains("a = 1"), "got: {out}");
        assert!(toml_format("= bad").is_err());
    }
}
