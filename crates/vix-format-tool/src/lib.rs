//! Reformat the selection (or buffer) within one data format: pretty-print or
//! minify JSON, and canonicalize YAML and TOML.
//!
//! Each function parses the text into a generic value and re-serializes it, so
//! the document is normalized (consistent indentation, key handling, quoting)
//! without changing its meaning. Used by the Tools → Format menu via
//! `App::transform_selection_or_buffer_try`.

pub mod prettier;
#[cfg(feature = "process")]
mod run;

/// Why a document could not be reformatted.
///
/// `Display` is the parser's or serializer's own message (unchanged from the
/// earlier `String` error).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatError {
    /// The input is not valid text of the format.
    Parse {
        /// The format, e.g. `"JSON"`.
        format: &'static str,
        /// The parser's own message.
        message: String,
    },
    /// The parsed value could not be written back out.
    Render {
        /// The format, e.g. `"TOML"`.
        format: &'static str,
        /// The serializer's own message.
        message: String,
    },
}

impl FormatError {
    /// Wrap a parser's error, keeping only its message (the library error types
    /// are not part of this crate's API).
    fn parse(format: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Parse {
            format,
            message: e.to_string(),
        }
    }

    /// Wrap a serializer's error; see `parse`.
    fn render(format: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Render {
            format,
            message: e.to_string(),
        }
    }
}

/// Displays just the underlying message (the format name is available in the
/// variant for callers that want to prefix it).
impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { message, .. } | Self::Render { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for FormatError {}

/// Pretty-print JSON with two-space indentation.
///
/// # Examples
/// ```
/// let out = vix_format_tool::json_pretty(r#"{"a":1}"#).unwrap();
/// assert_eq!(out, "{\n  \"a\": 1\n}");
/// ```
///
/// # Errors
/// Returns a [`FormatError`] when the input is not valid JSON.
pub fn json_pretty(input: &str) -> Result<String, FormatError> {
    let v: serde_json::Value =
        serde_json::from_str(input).map_err(|e| FormatError::parse("JSON", e))?;
    serde_json::to_string_pretty(&v).map_err(|e| FormatError::render("JSON", e))
}

/// Minify JSON to its most compact single-line form.
///
/// # Examples
/// ```
/// assert_eq!(vix_format_tool::json_minify("{ \"a\" : 1 }").unwrap(), "{\"a\":1}");
/// ```
///
/// # Errors
/// Returns a [`FormatError`] when the input is not valid JSON.
pub fn json_minify(input: &str) -> Result<String, FormatError> {
    let v: serde_json::Value =
        serde_json::from_str(input).map_err(|e| FormatError::parse("JSON", e))?;
    serde_json::to_string(&v).map_err(|e| FormatError::render("JSON", e))
}

/// Canonicalize YAML (re-emit with consistent formatting).
///
/// # Errors
/// Returns a [`FormatError`] when the input is not valid YAML.
pub fn yaml_format(input: &str) -> Result<String, FormatError> {
    let v: serde_norway::Value =
        serde_norway::from_str(input).map_err(|e| FormatError::parse("YAML", e))?;
    serde_norway::to_string(&v).map_err(|e| FormatError::render("YAML", e))
}

/// Canonicalize TOML (re-emit pretty-printed).
///
/// # Errors
/// Returns a [`FormatError`] when the input is not valid TOML, or is not a table.
pub fn toml_format(input: &str) -> Result<String, FormatError> {
    let v: toml::Value = toml::from_str(input).map_err(|e| FormatError::parse("TOML", e))?;
    toml::to_string_pretty(&v).map_err(|e| FormatError::render("TOML", e))
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
    fn errors_are_typed_per_format() {
        assert!(matches!(
            json_pretty("{nope}"),
            Err(FormatError::Parse { format: "JSON", .. })
        ));
        assert!(matches!(
            yaml_format("a: [unclosed"),
            Err(FormatError::Parse { format: "YAML", .. })
        ));
        let e = toml_format("= bad").unwrap_err();
        assert!(matches!(e, FormatError::Parse { format: "TOML", .. }));
        assert_ne!(e.to_string(), "");
        let r = FormatError::render("TOML", "boom");
        assert_eq!(r.to_string(), "boom");
        assert!(matches!(r, FormatError::Render { format: "TOML", .. }));
        let _: &dyn std::error::Error = &r;
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
