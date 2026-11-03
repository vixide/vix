//! Convert Markdown into HTML (Tools → Convert → Markdown → HTML).
//!
//! Parses `CommonMark` with `pulldown-cmark` and renders the HTML fragment.
//!
//! Output is a bare fragment (no `<html>`/`<body>` wrapper) and is not
//! sanitized: raw HTML in the input passes through verbatim, which is what a
//! text-conversion tool should do. The return type reuses
//! [`vix_convert_tabular::ConvertError`] so every Convert tool shares one
//! signature. Spec: `crates/vix-convert-from-markdown-into-html-tool/spec/index.md`.

use pulldown_cmark::{Parser, html};

/// Convert Markdown `input` to an HTML fragment.
///
/// # Examples
/// ```
/// let html = vix_convert_from_markdown_into_html_tool::convert("# Title").unwrap();
/// assert_eq!(html, "<h1>Title</h1>\n");
/// ```
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    let parser = Parser::new(input);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_heading_and_emphasis() {
        assert_eq!(convert("# Title").unwrap(), "<h1>Title</h1>\n");
        assert_eq!(convert("a *b* c").unwrap(), "<p>a <em>b</em> c</p>\n");
    }

    #[test]
    fn converts_list() {
        let html = convert("- one\n- two\n").unwrap();
        assert!(html.contains("<ul>"), "got: {html}");
        assert!(html.contains("<li>one</li>"), "got: {html}");
    }

    proptest::proptest! {
        // Converting arbitrary (possibly malformed) Markdown never panics.
        #[test]
        fn convert_never_panics(s in ".*") {
            let _ = convert(&s);
        }
    }
}
