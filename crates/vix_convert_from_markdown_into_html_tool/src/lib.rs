//! Convert Markdown into HTML (Tools → Convert → Markdown → HTML).
//!
//! Parses `CommonMark` with `pulldown-cmark` and renders the HTML fragment.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use pulldown_cmark::{html, Parser};

/// Convert Markdown `input` to an HTML fragment.
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, String> {
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
}
