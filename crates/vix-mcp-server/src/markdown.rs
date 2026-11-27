//! Markdown headings for the `outline` tool (T860).
//!
//! One pass over `pulldown-cmark`'s events collects every ATX and setext
//! heading as its level, its plain title and the 1-based source line it
//! starts on. This used to borrow `vix-markdown-preview`'s table of contents
//! and map preview rows back to source lines; the headless server now parses
//! the headings itself, so it depends on no preview or panel crate
//! (`tui/tests/crate_layering.rs`, rule 5).

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// One Markdown heading.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    /// 1 for `#` (or a `===` underline) through 6 for `######`.
    pub(crate) level: usize,
    /// The heading's text: its text, code spans and link and image text, with
    /// a line break inside it as one space, trimmed. Markup is dropped.
    pub(crate) title: String,
    /// The 1-based source line the heading starts on.
    pub(crate) line: usize,
}

/// The level number of a `pulldown-cmark` heading level.
fn level_number(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Every heading in `markdown`, in document order. A `#` line inside a code
/// block is not a heading; neither is a `#` with no space after it.
pub(crate) fn headings(markdown: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    // Line counting is incremental: heading starts come in increasing order,
    // so each newline is counted once (`scanned` is the byte offset reached,
    // `line` the 1-based line holding it).
    let mut scanned = 0;
    let mut line: usize = 1;
    let mut open: Option<Heading> = None;
    for (event, range) in Parser::new(markdown).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let newlines = markdown
                    .get(scanned..range.start)
                    .map_or(0, |s| s.matches('\n').count());
                // MCP-F1: a line count past usize::MAX cannot happen for text
                // in memory; the line stays put.
                line = line.checked_add(newlines).unwrap_or(line);
                scanned = scanned.max(range.start);
                open = Some(Heading {
                    level: level_number(level),
                    title: String::new(),
                    line,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(h) = open.take() {
                    out.push(Heading {
                        title: h.title.trim().to_owned(),
                        ..h
                    });
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(h) = open.as_mut() {
                    h.title.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(h) = open.as_mut() {
                    h.title.push(' ');
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Heading, headings};

    fn h(level: usize, title: &str, line: usize) -> Heading {
        Heading {
            level,
            title: title.to_owned(),
            line,
        }
    }

    #[test]
    fn atx_headings_carry_level_title_and_source_line() {
        let text = "# Title\n\nIntro.\n\n## Usage\n\n###### Deep ##\n";
        assert_eq!(
            headings(text),
            [h(1, "Title", 1), h(2, "Usage", 5), h(6, "Deep", 7)]
        );
    }

    #[test]
    fn setext_headings_start_on_their_text_line() {
        let text = "Top\n===\n\nTwo\nlines\n---\n";
        assert_eq!(headings(text), [h(1, "Top", 1), h(2, "Two lines", 4)]);
    }

    #[test]
    fn code_blocks_and_hashes_without_a_space_are_not_headings() {
        let text = "```sh\n# a comment\n```\n#hashtag\n\n    # indented code\n\n# Real\n";
        assert_eq!(headings(text), [h(1, "Real", 8)]);
    }

    #[test]
    fn inline_markup_keeps_only_its_text() {
        let text = "# The `run` [command](https://example.com) is **fast**\n";
        assert_eq!(headings(text), [h(1, "The run command is fast", 1)]);
    }

    #[test]
    fn quoted_and_crlf_headings_keep_their_lines() {
        let text = "intro\r\n\r\n> ## Quoted\r\n\r\n# Last\r\n";
        assert_eq!(headings(text), [h(2, "Quoted", 3), h(1, "Last", 5)]);
    }

    #[test]
    fn no_headings_is_empty() {
        assert_eq!(headings(""), []);
        assert_eq!(headings("plain text\n"), []);
    }
}
