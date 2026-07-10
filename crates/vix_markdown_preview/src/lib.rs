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
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut list_stack: Vec<Option<u64>> = Vec::new(); // None = bullet, Some(n) = ordered
    let mut in_code = false;
    let mut heading: Option<HeadingLevel> = None;
    let mut quote = false;
    let mut link_url: Option<String> = None;

    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };

    for ev in Parser::new(markdown) {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut cur, &mut out);
                heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                let text = std::mem::take(&mut cur);
                let rule_ch = if matches!(heading, Some(HeadingLevel::H1)) { '=' } else { '-' };
                let width = text.chars().count().max(1);
                out.push(text);
                out.push(rule_ch.to_string().repeat(width));
                out.push(String::new());
                heading = None;
            }
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Item) => flush(&mut cur, &mut out),
            Event::End(TagEnd::Paragraph) => {
                flush(&mut cur, &mut out);
                out.push(String::new());
            }
            Event::Start(Tag::List(start)) => list_stack.push(start),
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                if list_stack.is_empty() {
                    out.push(String::new());
                }
            }
            Event::Start(Tag::Item) => {
                flush(&mut cur, &mut out);
                let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                match list_stack.last_mut() {
                    Some(Some(n)) => {
                        write!(cur, "{indent}{n}. ").unwrap();
                        *n += 1;
                    }
                    _ => write!(cur, "{indent}• ").unwrap(),
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut cur, &mut out);
                quote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote = false;
                out.push(String::new());
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut cur, &mut out);
                in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                out.push(String::new());
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                // The link text arrives as Text; append " (url)" when it closes.
                link_url = Some(dest_url.to_string());
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = link_url.take() {
                    write!(cur, " ({url})").unwrap();
                }
            }
            Event::Text(t) => {
                if in_code {
                    for line in t.lines() {
                        out.push(format!("    {line}"));
                    }
                } else if quote {
                    write!(cur, "│ {t}").unwrap();
                } else {
                    cur.push_str(&t);
                }
            }
            Event::Code(t) => cur.push_str(&t),
            Event::SoftBreak | Event::HardBreak if !in_code => flush(&mut cur, &mut out),
            Event::Rule => {
                flush(&mut cur, &mut out);
                out.push("───".to_string());
                out.push(String::new());
            }
            _ => {}
        }
    }
    flush(&mut cur, &mut out);
    // Drop a trailing run of blank lines.
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
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
        Panel { lines: render(markdown), scroll: 0 }
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
