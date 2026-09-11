//! Render Markdown to formatted plain-text lines for a read-only preview pane.
//!
//! Parses `CommonMark` with `pulldown-cmark` and flattens it to readable lines:
//! headings get an underline rule, list items a `• ` bullet, block quotes a
//! `│ ` rail, fenced code its raw lines, and thematic breaks a `───` rule.
//! Inline emphasis/strong/code keep their text (the markup is dropped) and links
//! render as `text (url)`. The host shows the [`Panel`]'s lines, scrollable.
//!
//! T206 adds two things on top of that flattened text: each display line's
//! originating source line (so opening the preview can scroll-sync to where
//! the cursor was), and a table of contents of the document's headings
//! (`Panel::toc`) for a jump list -- reusing `vix_outline_panel::Entry`/
//! `Outline` exactly as the source-file outline does, just pointed at
//! *preview* lines instead of source lines.

#![warn(clippy::pedantic)]

use std::fmt::Write;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use vix_outline_panel::Entry;

/// A rendered Markdown preview: display lines, each line's originating
/// 1-based source line, and a heading table of contents.
#[derive(Default)]
pub struct Rendered {
    /// Rendered display lines.
    pub lines: Vec<String>,
    /// Each line's originating 1-based source line (parallel to `lines`).
    pub source_lines: Vec<usize>,
    /// Heading rows: `kind` is `"#"` repeated per level (`"##"` is an H2),
    /// `name` is the heading text, `line` is its 1-based row in `lines`.
    pub toc: Vec<Entry>,
}

/// Render `markdown` into display lines for the preview pane, discarding the
/// source-line map and table of contents ([`render_full`] keeps both).
#[must_use]
pub fn render(markdown: &str) -> Vec<String> {
    render_full(markdown).lines
}

/// Render `markdown`, keeping the per-line source-line map (for scroll-sync)
/// and a heading table of contents (for the TOC jump list) alongside the
/// display lines.
#[must_use]
pub fn render_full(markdown: &str) -> Rendered {
    let line_starts = line_start_offsets(markdown);
    let mut st = RenderState::default();
    for (ev, range) in Parser::new(markdown).into_offset_iter() {
        st.source_line = line_for_offset(&line_starts, range.start);
        st.handle(ev);
    }
    st.flush();
    // Drop a trailing run of blank lines.
    while st.out.last().is_some_and(String::is_empty) {
        st.out.pop();
        st.source_lines.pop();
    }
    Rendered {
        lines: st.out,
        source_lines: st.source_lines,
        toc: st.toc,
    }
}

/// Byte offset of the start of each 1-based source line (`starts[0]` is line
/// 1's own start, always `0`).
fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

/// The 1-based source line containing byte `offset`.
fn line_for_offset(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&s| s <= offset)
}

/// How many `#`s a heading level renders as in [`Entry::kind`].
fn heading_marker(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "#",
        HeadingLevel::H2 => "##",
        HeadingLevel::H3 => "###",
        HeadingLevel::H4 => "####",
        HeadingLevel::H5 => "#####",
        HeadingLevel::H6 => "######",
    }
}

/// Accumulated state while folding Markdown events into display lines. Split out
/// of [`render_full`] so the per-event match stays within the pedantic line limit.
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
    /// The source line of whichever event is currently being handled; every
    /// pushed display line is tagged with this.
    source_line: usize,
    /// Each pushed display line's source line, parallel to `out`.
    source_lines: Vec<usize>,
    /// Heading rows collected as their `TagEnd::Heading` closes.
    toc: Vec<Entry>,
}

impl RenderState {
    /// Push a finished display line, tagging it with the current source line.
    fn push_line(&mut self, line: String) {
        self.out.push(line);
        self.source_lines.push(self.source_line);
    }

    /// Push the in-progress line to `out` (when non-empty) and clear it.
    fn flush(&mut self) {
        if !self.cur.is_empty() {
            let line = std::mem::take(&mut self.cur);
            self.push_line(line);
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
                let level = self.heading.unwrap_or(HeadingLevel::H1);
                let rule_ch = if matches!(level, HeadingLevel::H1) {
                    '='
                } else {
                    '-'
                };
                let width = text.chars().count().max(1);
                self.toc.push(Entry {
                    kind: heading_marker(level).to_string(),
                    name: text.clone(),
                    line: self.out.len() + 1,
                });
                self.push_line(text);
                self.push_line(rule_ch.to_string().repeat(width));
                self.push_line(String::new());
                self.heading = None;
            }
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Item) => self.flush(),
            Event::End(TagEnd::Paragraph) => {
                self.flush();
                self.push_line(String::new());
            }
            Event::Start(Tag::List(start)) => self.list_stack.push(start),
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.push_line(String::new());
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
                self.push_line(String::new());
            }
            Event::Start(Tag::CodeBlock(_)) => {
                self.flush();
                self.in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                self.in_code = false;
                self.push_line(String::new());
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
                        self.push_line(format!("    {line}"));
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
                self.push_line("───".to_string());
                self.push_line(String::new());
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
    /// Each line's originating 1-based source line (parallel to `lines`),
    /// for [`Panel::sync_to_source_line`].
    pub source_lines: Vec<usize>,
    /// First visible line.
    pub scroll: usize,
    /// Heading table of contents (T206's TOC jump list), in document order.
    pub toc: Vec<Entry>,
}

impl Panel {
    /// Build a preview panel for `markdown`, scrolled to the top.
    #[must_use]
    pub fn open(markdown: &str) -> Self {
        let r = render_full(markdown);
        Panel {
            lines: r.lines,
            source_lines: r.source_lines,
            scroll: 0,
            toc: r.toc,
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

    /// Scroll directly to preview line `line` (1-based) -- for a TOC jump,
    /// which already names the exact preview line rather than a source line
    /// (see [`Panel::sync_to_source_line`] for that direction). No-op when
    /// the preview has no lines at all.
    pub fn scroll_to_line(&mut self, line: usize) {
        if self.lines.is_empty() {
            return;
        }
        self.scroll = line.saturating_sub(1).min(self.lines.len() - 1);
    }

    /// Scroll to the preview line rendered from `source_line` (1-based) --
    /// the last line at or before it, so the sync still lands somewhere
    /// sensible inside a paragraph or list rather than only on exact matches.
    /// No-op when the preview has no lines at all.
    pub fn sync_to_source_line(&mut self, source_line: usize) {
        if self.source_lines.is_empty() {
            return;
        }
        // The greatest mapped source line at or before `source_line` (several
        // display lines -- a heading's text, its underline, the blank line
        // after -- can share one source line); land on the *first* of them,
        // not the last, so a paragraph's own text wins over its trailing
        // blank separator.
        let target = self
            .source_lines
            .iter()
            .copied()
            .filter(|&l| l <= source_line)
            .max();
        let idx = match target {
            Some(t) => self.source_lines.iter().position(|&l| l == t).unwrap_or(0),
            None => 0,
        };
        self.scroll = idx.min(self.lines.len().saturating_sub(1));
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

    #[test]
    fn toc_lists_headings_with_their_preview_line() {
        let r = render_full("# Title\n\nintro\n\n## Section\n\nbody\n");
        assert_eq!(r.toc.len(), 2);
        assert_eq!(r.toc[0].kind, "#");
        assert_eq!(r.toc[0].name, "Title");
        assert_eq!(r.lines[r.toc[0].line - 1], "Title");
        assert_eq!(r.toc[1].kind, "##");
        assert_eq!(r.toc[1].name, "Section");
        assert_eq!(r.lines[r.toc[1].line - 1], "Section");
    }

    #[test]
    fn source_lines_track_each_displayed_line() {
        let r = render_full("# Title\n\npara one\n\npara two\n");
        // "Title" (source line 1) and "para one" (source line 3) both appear
        // among the mapped lines with their real source line.
        let title_idx = r.lines.iter().position(|l| l == "Title").unwrap();
        assert_eq!(r.source_lines[title_idx], 1);
        let para_idx = r.lines.iter().position(|l| l == "para one").unwrap();
        assert_eq!(r.source_lines[para_idx], 3);
    }

    #[test]
    fn sync_to_source_line_scrolls_to_the_nearest_preceding_line() {
        let mut p = Panel::open("# Title\n\npara one\n\npara two\n");
        let para_two_idx = p.lines.iter().position(|l| l == "para two").unwrap();
        // Source line 5 is "para two"; a click a couple of lines later (still
        // before anything else) should land on the same preview line.
        p.sync_to_source_line(5);
        assert_eq!(p.scroll, para_two_idx);
    }

    #[test]
    fn scroll_to_line_jumps_directly_to_a_known_preview_line() {
        let mut p = Panel::open("# Title\n\n## Section\n\nbody\n");
        assert_eq!(p.toc.len(), 2);
        let section_line = p.toc[1].line;
        p.scroll_to_line(section_line);
        assert_eq!(p.lines[p.scroll], "Section");
    }
}
