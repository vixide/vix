//! Convert HTML into Markdown (Tools → Convert → HTML → Markdown).
//!
//! Uses `htmd` (a Turndown-inspired converter) to turn an HTML fragment into
//! Markdown text.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert HTML `input` to Markdown.
///
/// # Errors
/// Returns an error when the HTML cannot be converted.
pub fn convert(input: &str) -> Result<String, String> {
    htmd::convert(input).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_heading_and_emphasis() {
        assert_eq!(convert("<h1>Title</h1>").unwrap(), "# Title");
        assert_eq!(convert("<p>a <em>b</em> c</p>").unwrap(), "a *b* c");
    }

    #[test]
    fn converts_link() {
        let md = convert("<a href=\"https://x.test\">link</a>").unwrap();
        assert!(md.contains("[link](https://x.test)"), "got: {md}");
    }
}
