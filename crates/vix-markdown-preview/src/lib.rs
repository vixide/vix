//! Render Markdown to formatted plain-text lines for a read-only preview pane.
//!
//! Parses `CommonMark` with `pulldown-cmark` and flattens it to readable lines:
//! headings get an underline rule, list items a `• ` bullet, block quotes a
//! `│ ` rail, fenced code its raw lines, and thematic breaks a `───` rule.
//! Inline emphasis/strong/code keep their text (the markup is dropped) and links
//! render as `text (url)`. The host shows the [`Panel`]'s lines, scrollable.

#![warn(clippy::pedantic)]

use std::fmt::Write;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// Render `markdown` into display lines for the preview pane.
#[must_use]
pub fn render(markdown: &str) -> Vec<String> {
    let mut st = RenderState::default();
    for ev in Parser::new(markdown) {
        st.handle(ev);
    }
    st.flush();
    // Drop a trailing run of blank lines.
    while st.out.last().is_some_and(String::is_empty) {
        st.out.pop();
    }
    st.out
}

/// Accumulated state while folding Markdown events into display lines. Split out
/// of [`render`] so the per-event match stays within the pedantic line limit.
#[derive(Default)]
struct RenderState {
    /// Finished display lines.
    out: Vec<String>,
    /// The line currently being built.
    cur: String,
    /// Open list nesting: `None` = bullet, `Some(n)` = ordered at index `n`.
    list_stack: Vec<Option<u64>>,
    /// Inside a fenced/indented code block.
    in_code: bool,
    /// The heading level currently open, if any.
    heading: Option<HeadingLevel>,
    /// Inside a block quote.
    quote: bool,
    /// A link's URL, pending until its text closes.
    link_url: Option<String>,
}

impl RenderState {
    /// Push the in-progress line to `out` (when non-empty) and clear it.
    fn flush(&mut self) {
        if !self.cur.is_empty() {
            self.out.push(std::mem::take(&mut self.cur));
        }
    }

    /// Fold a single Markdown event into the accumulated lines.
    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush();
                self.heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                let text = std::mem::take(&mut self.cur);
                let rule_ch = if matches!(self.heading, Some(HeadingLevel::H1)) {
                    '='
                } else {
                    '-'
                };
                let width = text.chars().count().max(1);
                self.out.push(text);
                self.out.push(rule_ch.to_string().repeat(width));
                self.out.push(String::new());
                self.heading = None;
            }
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Item) => self.flush(),
            Event::End(TagEnd::Paragraph) => {
                self.flush();
                self.out.push(String::new());
            }
            Event::Start(Tag::List(start)) => self.list_stack.push(start),
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.out.push(String::new());
                }
            }
            Event::Start(Tag::Item) => {
                self.flush();
                let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                match self.list_stack.last_mut() {
                    Some(Some(n)) => {
                        write!(self.cur, "{indent}{n}. ").unwrap();
                        *n += 1;
                    }
                    _ => write!(self.cur, "{indent}• ").unwrap(),
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush();
                self.quote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.quote = false;
                self.out.push(String::new());
            }
            Event::Start(Tag::CodeBlock(_)) => {
                self.flush();
                self.in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                self.in_code = false;
                self.out.push(String::new());
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                // The link text arrives as Text; append " (url)" when it closes.
                self.link_url = Some(dest_url.to_string());
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = self.link_url.take() {
                    write!(self.cur, " ({url})").unwrap();
                }
            }
            Event::Text(t) => {
                if self.in_code {
                    for line in t.lines() {
                        self.out.push(format!("    {line}"));
                    }
                } else if self.quote {
                    write!(self.cur, "│ {t}").unwrap();
                } else {
                    self.cur.push_str(&t);
                }
            }
            Event::Code(t) => self.cur.push_str(&t),
            Event::SoftBreak | Event::HardBreak if !self.in_code => self.flush(),
            Event::Rule => {
                self.flush();
                self.out.push("───".to_string());
                self.out.push(String::new());
            }
            _ => {}
        }
    }
}

/// Scroll state for the Markdown preview overlay.
#[derive(Default)]
pub struct Panel {
    /// Rendered display lines.
    pub lines: Vec<String>,
    /// First visible line.
    pub scroll: usize,
}

impl Panel {
    /// Build a preview panel for `markdown`.
    #[must_use]
    pub fn open(markdown: &str) -> Self {
        Panel {
            lines: render(markdown),
            scroll: 0,
        }
    }

    /// Scroll up by `n` lines.
    pub fn up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Scroll down by `n` lines, keeping at least one line visible.
    pub fn down(&mut self, n: usize) {
        let max = self.lines.len().saturating_sub(1);
        self.scroll = (self.scroll + n).min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_gets_an_underline_rule() {
        let out = render("# Title\n");
        assert_eq!(out[0], "Title");
        assert_eq!(out[1], "=====");
    }

    #[test]
    fn lists_get_bullets_and_numbers() {
        let out = render("- a\n- b\n");
        assert!(out.contains(&"• a".to_string()), "{out:?}");
        assert!(out.contains(&"• b".to_string()), "{out:?}");
        let ord = render("1. one\n2. two\n");
        assert!(ord.contains(&"1. one".to_string()), "{ord:?}");
        assert!(ord.contains(&"2. two".to_string()), "{ord:?}");
    }

    #[test]
    fn paragraph_text_is_plain() {
        let out = render("a *b* c\n");
        assert_eq!(out[0], "a b c");
    }

    #[test]
    fn code_block_lines_are_indented() {
        let out = render("```\nlet x = 1;\n```\n");
        assert!(out.iter().any(|l| l == "    let x = 1;"), "{out:?}");
    }

    #[test]
    fn rule_renders_a_divider() {
        let out = render("a\n\n---\n\nb\n");
        assert!(out.contains(&"───".to_string()), "{out:?}");
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(render("").is_empty());
    }

    #[test]
    fn panel_scrolls_within_bounds() {
        let mut p = Panel::open("a\n\nb\n\nc\n");
        p.up(5);
        assert_eq!(p.scroll, 0);
        p.down(100);
        assert!(p.scroll <= p.lines.len().saturating_sub(1));
    }
}
