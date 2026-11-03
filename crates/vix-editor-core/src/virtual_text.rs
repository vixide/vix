//! End-of-line virtual text (T614): text drawn after a line's content that is
//! not part of the buffer.
//!
//! A host hands the widget one list of [`EolText`] per [`VirtualTextLayer`];
//! the renderers draw every layer's text for a line, dimmed and truncated at
//! the view's right edge. Inline diagnostics (T614) are the first producer;
//! inline debugger values (T617) and color swatches (T620) add layers of their
//! own, so no producer ever clobbers another's text.
//!
//! Storage is `Editor::eol_texts`: a `Vec<(VirtualTextLayer, Vec<EolText>)>`
//! kept sorted by layer (the derived `Ord` is declaration order), so drawing
//! it front to back yields the documented left-to-right layer order. The text
//! is never part of the rope, so it cannot be selected, searched or saved; the
//! cursor never lands on it. Drawing happens from the line renderers in
//! [`crate::render`]. See `crates/vix-editor-core/spec/index.md`.

use crate::code::{RopeGraphemes, grapheme_width_and_chars_len};
use crate::editor::Editor;
use ratatui_core::buffer::Buffer;
use ratatui_core::style::{Color, Modifier};
use unicode_width::UnicodeWidthChar;

/// Columns left blank between a line's content and its virtual text, and
/// between two virtual texts on one line.
const GAP: usize = 2;

/// Which producer a set of end-of-line virtual text belongs to. Layers are
/// drawn in declaration order, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum VirtualTextLayer {
    /// LSP diagnostics for the line (T614).
    Diagnostics,
    /// Problems a task's problem matcher found in its output (T632).
    TaskProblems,
    /// The debugger's variable values on the stopped line (T617).
    DebugValues,
    /// A `■` in each document color's own color (T621).
    ColorSwatches,
}

/// One piece of end-of-line virtual text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EolText {
    /// Zero-based logical line it is drawn after.
    pub line: usize,
    /// The text; only its first line is drawn.
    pub text: String,
    /// Foreground color, or `None` for the dimmed whitespace style alone.
    pub color: Option<Color>,
}

impl EolText {
    /// Virtual text `text` after zero-based `line`, in the dimmed default color.
    #[must_use]
    pub fn new(line: usize, text: impl Into<String>) -> Self {
        EolText {
            line,
            text: text.into(),
            color: None,
        }
    }

    /// The same text, drawn in `color` (still dimmed).
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// `text`'s first line with tabs as spaces and other control characters
/// dropped, cut to at most `avail` display columns. A cut text ends in `…`.
///
/// Widths are terminal display columns, not chars: a wide glyph that would
/// straddle the cut is dropped whole, and the `…` itself reserves one column.
/// `avail == 0` yields an empty string.
///
/// # Examples
///
/// ```
/// use vix_editor_core::virtual_text::truncate_to_width;
///
/// assert_eq!(truncate_to_width("abc", 5), "abc");
/// assert_eq!(truncate_to_width("abcdef", 4), "abc\u{2026}");
/// assert_eq!(truncate_to_width("one\ntwo", 10), "one");
/// ```
#[must_use]
pub fn truncate_to_width(text: &str, avail: usize) -> String {
    let clean: Vec<char> = text
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let width: usize = clean.iter().map(|c| c.width().unwrap_or(0)).sum();
    if width <= avail {
        return clean.into_iter().collect();
    }
    if avail == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in clean {
        let w = c.width().unwrap_or(0);
        if used + w > avail - 1 {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('\u{2026}');
    out
}

impl Editor {
    /// Set `layer`'s end-of-line virtual text, replacing what it had; an empty
    /// list removes the layer. Other layers are untouched.
    ///
    /// The layer vector stays sorted by [`VirtualTextLayer`] so the binary
    /// search below is valid and draw order is deterministic regardless of
    /// which producer reports first.
    pub fn set_eol_texts(&mut self, layer: VirtualTextLayer, texts: Vec<EolText>) {
        // Empty lists never get stored: `has_eol_texts` and the draw loop can
        // then treat "layer present" and "has text" alike.
        let at = self.eol_texts.binary_search_by_key(&layer, |(l, _)| *l);
        match (at, texts.is_empty()) {
            (Ok(i), true) => {
                self.eol_texts.remove(i);
            }
            (Ok(i), false) => self.eol_texts[i].1 = texts,
            (Err(_), true) => {}
            (Err(i), false) => self.eol_texts.insert(i, (layer, texts)),
        }
    }

    /// Remove `layer`'s end-of-line virtual text.
    pub fn clear_eol_texts(&mut self, layer: VirtualTextLayer) {
        self.set_eol_texts(layer, Vec::new());
    }

    /// `layer`'s current end-of-line virtual text (empty when none).
    #[must_use]
    pub fn eol_texts(&self, layer: VirtualTextLayer) -> &[EolText] {
        self.eol_texts
            .iter()
            .find(|(l, _)| *l == layer)
            .map_or(&[], |(_, t)| t.as_slice())
    }

    /// Whether any layer has virtual text for `line_idx`.
    pub(crate) fn has_eol_texts(&self, line_idx: usize) -> bool {
        self.eol_texts
            .iter()
            .any(|(_, t)| t.iter().any(|e| e.line == line_idx))
    }

    /// Display width of `line_idx`'s text from char column `from_col` to its
    /// end (tabs count as drawn, one column).
    pub(crate) fn line_width_from(&self, line_idx: usize, from_col: usize) -> usize {
        let code = self.code_ref();
        // `from_col` is clamped so a stale scroll offset past the line end
        // yields width 0 instead of an out-of-range slice.
        let start = code.line_to_char(line_idx);
        let len = code.line_len(line_idx);
        let slice = code.char_slice(start + from_col.min(len), start + len);
        RopeGraphemes::new(&slice)
            .map(|g| grapheme_width_and_chars_len(g).0)
            .sum()
    }

    /// Draw every layer's virtual text for `line_idx`, starting [`GAP`] columns
    /// after `content_end` (the absolute column just past the line's drawn
    /// content) and truncated at `right_edge`.
    pub(crate) fn draw_eol_texts(
        &self,
        buf: &mut Buffer,
        line_idx: usize,
        content_end: usize,
        right_edge: u16,
        draw_y: u16,
    ) {
        let right = usize::from(right_edge);
        let mut x = content_end;
        let texts = self
            .eol_texts
            .iter()
            .flat_map(|(_, t)| t.iter())
            .filter(|t| t.line == line_idx);
        for t in texts {
            x += GAP;
            // Out of room: later texts on this line start even further right,
            // so stop rather than skip.
            if x >= right {
                return;
            }
            let shown = truncate_to_width(&t.text, right - x);
            if shown.is_empty() {
                // Nothing drawable (e.g. all control chars): the gap is kept
                // but no column is consumed by text.
                continue;
            }
            let mut style = self.whitespace_style.add_modifier(Modifier::DIM);
            if let Some(color) = t.color {
                style = style.fg(color);
            }
            // Terminal columns are `u16`; a larger absolute column means the
            // view is off-screen anyway.
            let Ok(x16) = u16::try_from(x) else {
                return;
            };
            buf.set_string(x16, draw_y, &shown, style);
            x += unicode_width::UnicodeWidthStr::width(shown.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    fn row(buf: &Buffer, y: u16) -> String {
        (buf.area.left()..buf.area.right())
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn editor(text: &str) -> Editor {
        let mut ed = Editor::new("text", text, Vec::new()).expect("editor");
        ed.show_line_numbers(false);
        ed.set_left_code_padding(0);
        ed
    }

    fn render(ed: &Editor, width: u16) -> Buffer {
        let area = Rect::new(0, 0, width, 3);
        let mut buf = Buffer::empty(area);
        ed.render(area, &mut buf);
        buf
    }

    #[test]
    fn color_swatches_draw_after_diagnostics_and_are_independent() {
        let mut ed = editor("abc\n");
        ed.set_eol_texts(
            VirtualTextLayer::ColorSwatches,
            vec![EolText::new(0, "\u{25a0}")],
        );
        ed.set_eol_texts(VirtualTextLayer::Diagnostics, vec![EolText::new(0, "err")]);
        assert_eq!(row(&render(&ed, 20), 0), "abc  err  \u{25a0}");
        ed.clear_eol_texts(VirtualTextLayer::Diagnostics);
        assert_eq!(ed.eol_texts(VirtualTextLayer::ColorSwatches).len(), 1);
    }

    #[test]
    fn truncate_keeps_short_text_and_cuts_long_text_with_an_ellipsis() {
        assert_eq!(truncate_to_width("abc", 5), "abc");
        assert_eq!(truncate_to_width("abcdef", 4), "abc\u{2026}");
        assert_eq!(truncate_to_width("abc", 0), "");
        assert_eq!(truncate_to_width("one\ntwo", 10), "one");
        assert_eq!(truncate_to_width("a\tb", 10), "a b");
        // A wide glyph that would straddle the edge is dropped whole.
        assert_eq!(truncate_to_width("a\u{4e2d}b", 3), "a\u{2026}");
    }

    #[test]
    fn text_is_drawn_two_columns_after_the_line_dimmed_and_colored() {
        let mut ed = editor("let x\nok\n");
        ed.set_eol_texts(
            VirtualTextLayer::Diagnostics,
            vec![EolText::new(0, "unused").with_color(Color::Red)],
        );
        let buf = render(&ed, 30);
        assert_eq!(row(&buf, 0), "let x  unused");
        assert_eq!(row(&buf, 1), "ok");
        assert_eq!(buf[(7, 0)].fg, Color::Red);
        assert!(buf[(7, 0)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn text_is_truncated_at_the_right_edge() {
        let mut ed = editor("abc\n");
        ed.set_eol_texts(
            VirtualTextLayer::Diagnostics,
            vec![EolText::new(0, "a very long message")],
        );
        let buf = render(&ed, 12);
        assert_eq!(row(&buf, 0), "abc  a very\u{2026}");
        // Too narrow for even the gap: nothing drawn, no panic.
        let buf = render(&ed, 4);
        assert_eq!(row(&buf, 0), "abc");
    }

    #[test]
    fn soft_wrap_draws_the_text_after_the_lines_last_row() {
        let mut ed = editor("abcdefgh\n");
        ed.set_soft_wrap(true);
        ed.set_eol_texts(VirtualTextLayer::Diagnostics, vec![EolText::new(0, "e")]);
        let buf = render(&ed, 6);
        assert_eq!(row(&buf, 0), "abcdef");
        assert_eq!(row(&buf, 1), "gh  e");
    }

    #[test]
    fn horizontal_scroll_keeps_the_text_after_the_visible_content() {
        let mut ed = editor("abcdefgh\n");
        ed.set_eol_texts(VirtualTextLayer::Diagnostics, vec![EolText::new(0, "e")]);
        ed.set_offset_x(3);
        let buf = render(&ed, 20);
        assert_eq!(row(&buf, 0), "defgh  e");
    }

    #[test]
    fn layers_are_replaced_and_cleared_independently() {
        let mut ed = editor("x\n");
        ed.set_eol_texts(VirtualTextLayer::Diagnostics, vec![EolText::new(0, "a")]);
        ed.set_eol_texts(VirtualTextLayer::Diagnostics, vec![EolText::new(0, "b")]);
        assert_eq!(ed.eol_texts(VirtualTextLayer::Diagnostics).len(), 1);
        assert_eq!(ed.eol_texts(VirtualTextLayer::Diagnostics)[0].text, "b");
        ed.clear_eol_texts(VirtualTextLayer::Diagnostics);
        assert_eq!(
            ed.eol_texts(VirtualTextLayer::Diagnostics),
            &[] as &[EolText]
        );
        assert_eq!(row(&render(&ed, 10), 0), "x");
    }
}
