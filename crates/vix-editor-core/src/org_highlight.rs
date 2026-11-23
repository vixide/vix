//! Highlighting for Org files, which have no Tree-sitter grammar here.
//!
//! [`line_spans`] reads one line and returns styled byte ranges named after
//! the theme entries [`crate::editor::Editor`] derives for Org
//! (`org.heading`, `org.todo`, `org.done`, `org.link`, `org.markup`): the
//! headline, its TODO keyword, each link's visible text and the brackets and
//! target around it, and `#+` keyword and comment lines. The renderer uses
//! the first span covering a character, so the more specific spans come first.

/// Keywords drawn as an open task.
const TODO_WORDS: &[&str] = &["TODO", "NEXT", "WAIT", "WAITING", "HOLD"];
/// Keywords drawn as a closed task.
const DONE_WORDS: &[&str] = &["DONE", "CANCELED", "CANCELLED"];

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// `line` from byte `i` on; empty when `i` is past the end or inside a character.
fn tail(line: &str, i: usize) -> &str {
    line.get(i..).unwrap_or("")
}

/// The styled byte ranges in one Org `line` (without its line break), most
/// specific first: `(start, end, theme entry)`.
#[must_use]
pub fn line_spans(line: &str) -> Vec<(usize, usize, &'static str)> {
    let mut spans = Vec::new();
    link_spans(line, &mut spans);
    let trimmed = line.trim_start();
    let indent = sub_or(line.len(), trimmed.len(), 0);
    let stars = sub_or(line.len(), line.trim_start_matches('*').len(), 0);
    if stars > 0 && tail(line, stars).starts_with(' ') {
        // A headline: its TODO keyword, then the whole line.
        let after = tail(line, add_or(stars, 1, usize::MAX));
        let word = after.split(' ').next().unwrap_or(after);
        let kind = if TODO_WORDS.contains(&word) {
            Some("org.todo")
        } else if DONE_WORDS.contains(&word) {
            Some("org.done")
        } else {
            None
        };
        if let Some(kind) = kind {
            let at = add_or(stars, 1, usize::MAX);
            spans.push((at, add_or(at, word.len(), usize::MAX), kind));
        }
        spans.push((0, line.len(), "org.heading"));
    } else if trimmed.starts_with("#+") || trimmed.starts_with("# ") || trimmed == "#" {
        spans.push((indent, line.len(), "org.markup"));
    }
    spans
}

/// The spans of every `[[target][text]]` and `[[target]]` link in `line`:
/// the visible text as `org.link`, the brackets and a described link's
/// target as `org.markup`.
fn link_spans(line: &str, spans: &mut Vec<(usize, usize, &'static str)>) {
    let mut from = 0;
    while let Some(open) = tail(line, from)
        .find("[[")
        .map(|i| add_or(from, i, usize::MAX))
    {
        let inner_start = add_or(open, 2, usize::MAX);
        let Some(close) = tail(line, inner_start)
            .find("]]")
            .map(|i| add_or(inner_start, i, usize::MAX))
        else {
            break;
        };
        let inner = line.get(inner_start..close).unwrap_or("");
        if let Some(mid) = inner.find("][") {
            // `[[target][text]]`: the text is the link, the rest is markup.
            let text_start = add_or(add_or(inner_start, mid, usize::MAX), 2, usize::MAX);
            spans.push((text_start, close, "org.link"));
            spans.push((open, text_start, "org.markup"));
        } else {
            // `[[target]]`: the target is what shows.
            spans.push((inner_start, close, "org.link"));
            spans.push((open, inner_start, "org.markup"));
        }
        let after_close = add_or(close, 2, usize::MAX);
        spans.push((close, after_close, "org.markup"));
        from = after_close;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named<'a>(
        line: &'a str,
        spans: &[(usize, usize, &'static str)],
    ) -> Vec<(&'a str, &'static str)> {
        spans.iter().map(|&(s, e, n)| (&line[s..e], n)).collect()
    }

    #[test]
    fn headline_with_keyword_and_link() {
        let line = "** TODO Read [[https://orgmode.org][the manual]]";
        assert_eq!(
            named(line, &line_spans(line)),
            vec![
                ("the manual", "org.link"),
                ("[[https://orgmode.org][", "org.markup"),
                ("]]", "org.markup"),
                ("TODO", "org.todo"),
                (line, "org.heading"),
            ]
        );
    }

    #[test]
    fn done_keywords_and_plain_headlines() {
        let line = "* DONE Ship it";
        assert_eq!(named(line, &line_spans(line))[0], ("DONE", "org.done"));
        let line = "* TODOS are not a keyword";
        assert_eq!(named(line, &line_spans(line)), vec![(line, "org.heading")]);
        assert_eq!(line_spans("*bold* is not a headline"), []);
    }

    #[test]
    fn bare_links_and_several_per_line() {
        let line = "see [[file:a.org]] and [[id:x][X]]";
        assert_eq!(
            named(line, &line_spans(line)),
            vec![
                ("file:a.org", "org.link"),
                ("[[", "org.markup"),
                ("]]", "org.markup"),
                ("X", "org.link"),
                ("[[id:x][", "org.markup"),
                ("]]", "org.markup"),
            ]
        );
        assert_eq!(line_spans("an unclosed [[link"), []);
    }

    #[test]
    fn keyword_and_comment_lines() {
        assert_eq!(line_spans("#+title: Notes"), vec![(0, 14, "org.markup")]);
        assert_eq!(
            line_spans("  #+begin_src rust"),
            vec![(2, 18, "org.markup")]
        );
        assert_eq!(line_spans("# a comment"), vec![(0, 11, "org.markup")]);
        assert_eq!(line_spans("#hashtag"), []);
        assert_eq!(line_spans("plain text"), []);
    }

    /// The highlighter reaches the screen: an Org buffer (no Tree-sitter
    /// grammar) still gets a bold headline and an underlined link when drawn,
    /// with no color theme at all.
    #[test]
    fn an_org_buffer_renders_styled() {
        use crate::editor::Editor;
        use ratatui_core::buffer::Buffer;
        use ratatui_core::layout::Rect;
        use ratatui_core::style::Modifier;
        use ratatui_core::widgets::Widget;
        let mut ed = Editor::new("org", "* Head\nsee [[x][link]] here\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_syntax_theme(&[]);
        let area = Rect::new(0, 0, 30, 3);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let row = |y: u16| (0..30).map(|x| buf[(x, y)].symbol()).collect::<String>();
        let col = |y: u16, text: &str| u16::try_from(row(y).find(text).unwrap()).unwrap();
        let head = col(0, "Head");
        assert!(
            buf[(head, 0)].modifier.contains(Modifier::BOLD),
            "headline bold"
        );
        let link = col(1, "link");
        assert!(
            buf[(link, 1)].modifier.contains(Modifier::UNDERLINED),
            "link underlined"
        );
        let here = col(1, "here");
        assert!(!buf[(here, 1)].modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn multibyte_text_keeps_byte_ranges_on_char_boundaries() {
        let line = "* Café [[x][né]]";
        for (s, e, _) in line_spans(line) {
            assert!(line.is_char_boundary(s) && line.is_char_boundary(e));
        }
    }
}
