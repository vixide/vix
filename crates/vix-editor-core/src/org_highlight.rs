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

/// The styled byte ranges in one Org `line` (without its line break), most
/// specific first: `(start, end, theme entry)`.
#[must_use]
pub fn line_spans(line: &str) -> Vec<(usize, usize, &'static str)> {
    let mut spans = Vec::new();
    link_spans(line, &mut spans);
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let stars = line.len() - line.trim_start_matches('*').len();
    if stars > 0 && line[stars..].starts_with(' ') {
        // A headline: its TODO keyword, then the whole line.
        let after = &line[stars + 1..];
        let word_end = after.find(' ').unwrap_or(after.len());
        let word = &after[..word_end];
        let kind = if TODO_WORDS.contains(&word) {
            Some("org.todo")
        } else if DONE_WORDS.contains(&word) {
            Some("org.done")
        } else {
            None
        };
        if let Some(kind) = kind {
            let at = stars + 1;
            spans.push((at, at + word.len(), kind));
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
    while let Some(open) = line[from..].find("[[").map(|i| from + i) {
        let Some(close) = line[open + 2..].find("]]").map(|i| open + 2 + i) else {
            break;
        };
        let inner = &line[open + 2..close];
        if let Some(mid) = inner.find("][") {
            // `[[target][text]]`: the text is the link, the rest is markup.
            let text_start = open + 2 + mid + 2;
            spans.push((text_start, close, "org.link"));
            spans.push((open, text_start, "org.markup"));
        } else {
            // `[[target]]`: the target is what shows.
            spans.push((open + 2, close, "org.link"));
            spans.push((open, open + 2, "org.markup"));
        }
        spans.push((close, close + 2, "org.markup"));
        from = close + 2;
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

    #[test]
    fn multibyte_text_keeps_byte_ranges_on_char_boundaries() {
        let line = "* Café [[x][né]]";
        for (s, e, _) in line_spans(line) {
            assert!(line.is_char_boundary(s) && line.is_char_boundary(e));
        }
    }
}
