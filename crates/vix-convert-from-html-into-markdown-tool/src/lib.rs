//! Convert HTML into Markdown (Tools → Convert → HTML → Markdown).
//!
//! Uses `htmd` (a Turndown-inspired converter) to turn an HTML fragment into
//! Markdown text. Spec: `crates/vix-convert-from-html-into-markdown-tool/spec/index.md`.

/// Convert HTML `input` to Markdown.
///
/// # Examples
/// ```
/// let md = vix_convert_from_html_into_markdown_tool::convert("<h1>Title</h1>").unwrap();
/// assert_eq!(md, "# Title");
/// ```
///
/// # Errors
/// Returns a [`vix_convert_tabular::ConvertError`] (labelled "HTML") when
/// `htmd` cannot convert the input.
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    htmd::convert(input).map_err(|e| vix_convert_tabular::ConvertError::parse("HTML", e))
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

    proptest::proptest! {
        // Converting arbitrary (possibly malformed) HTML never panics.
        #[test]
        fn convert_never_panics(s in ".*") {
            let _ = convert(&s);
        }
    }
}
