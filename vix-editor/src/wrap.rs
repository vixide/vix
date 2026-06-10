//! Soft-wrap layout and rendering for the editor widget (Vix's own code, held to
//! `clippy::pedantic`). Long logical lines wrap across several screen rows; a
//! shared [`Editor::visual_rows`] layout drives the renderer here as well as the
//! cursor scroll (`Editor::focus`) and mouse hit-testing (`cursor_from_mouse`).

#![warn(clippy::pedantic)]
// TUI layout math casts small `usize` counts to `u16` cell coordinates (always in
// range), matching the rest of Vix.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Modifier, Style};
use ropey::RopeSlice;

use crate::code::{grapheme_width_and_bytes_len, grapheme_width_and_chars_len, RopeGraphemes};
use crate::editor::Editor;

/// One on-screen row in soft-wrap mode: a `[start, end)` character-offset slice
/// of a logical line (`line`).
#[derive(Clone, Copy)]
pub(crate) struct VRow {
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Display width of one grapheme for soft-wrap layout. Tabs render as a single
/// cell, so they count as width 1.
fn gwidth(g: RopeSlice) -> usize {
    if g.chars().next() == Some('\t') {
        1
    } else {
        grapheme_width_and_chars_len(g).0
    }
}

impl Editor {
    /// Soft-wrap render: long logical lines wrap across several screen rows. All
    /// layers (text, whitespace glyphs, syntax, selection, marks, cursor) are
    /// drawn per visual row using the shared [`Editor::visual_rows`] layout.
    pub(crate) fn render_wrapped(&self, area: Rect, buf: &mut Buffer) {
        let code = self.code_ref();
        let total_lines = code.len_lines();
        let line_number_digits = total_lines.max(1).to_string().len().max(5);
        let line_number_width = self.get_line_number_width();
        let text_x0 = area.left() + line_number_width as u16;
        let right = area.right();
        let text_width = (area.width as usize).saturating_sub(line_number_width);
        let height = area.height as usize;
        let default_text_style = self.text_style;

        let rows = self.visual_rows(text_width, height);
        let bracket = self.matching_bracket();

        // Sorted selection range as char offsets, if non-empty.
        let sel = self
            .selection
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| (s.start.min(s.end), s.start.max(s.end)));

        for (i, vr) in rows.iter().enumerate() {
            let draw_y = area.top() + i as u16;
            if draw_y >= area.bottom() {
                break;
            }

            let line_start = code.line_to_char(vr.line);
            let line_len = code.line_len(vr.line);
            let is_first = vr.start == line_start;

            if self.show_line_numbers {
                let s = if is_first {
                    format!("{:>line_number_digits$}", vr.line + 1)
                } else {
                    " ".repeat(line_number_digits)
                };
                buf.set_string(area.left(), draw_y, &s, self.line_number_style);
            }

            // Base text for the segment (with whitespace glyphs if enabled).
            let seg = code.char_slice(vr.start, vr.end);
            let displayed: String = if self.show_whitespace {
                seg.chars()
                    .map(|c| match c {
                        '\t' => '\u{2192}',
                        ' ' => '\u{00B7}',
                        '\r' => '\u{240D}',
                        other => other,
                    })
                    .collect()
            } else {
                seg.to_string().replace('\t', " ")
            };
            buf.set_string(text_x0, draw_y, &displayed, default_text_style);

            // Single grapheme pass for styling + caret position.
            let start_byte = code.char_to_byte(vr.start);
            let highlights = if code.is_highlight() {
                let end_byte = code.char_to_byte(vr.end);
                self.highlight_interval(start_byte, end_byte, &self.theme)
            } else {
                Vec::new()
            };

            let mut vx: u16 = 0;
            let mut ch = vr.start;
            let mut byte = start_byte;
            let mut caret: Option<u16> = None;
            for g in RopeGraphemes::new(&seg) {
                let (w, g_chars) = grapheme_width_and_chars_len(g);
                let (_, g_bytes) = grapheme_width_and_bytes_len(g);
                let gw = if g.chars().next() == Some('\t') { 1 } else { w as u16 };
                let cell_x = text_x0 + vx;

                if self.show_whitespace
                    && matches!(g.chars().next(), Some('\t' | ' ' | '\r'))
                    && cell_x < right
                {
                    buf[(cell_x, draw_y)].set_style(self.whitespace_style);
                }
                for &(hs, he, st) in &highlights {
                    if hs <= byte && byte < he {
                        paint(buf, cell_x, gw, right, draw_y, st);
                        break;
                    }
                }
                if let Some((ss, se)) = sel
                    && ss <= ch
                    && ch < se
                {
                    paint(buf, cell_x, gw, right, draw_y, self.selection_style);
                }
                if let Some(marks) = self.marks.as_ref()
                    && marks.iter().any(|&(s, e, _)| s <= ch && ch < e)
                {
                    paint(
                        buf,
                        cell_x,
                        gw,
                        right,
                        draw_y,
                        Style::default().add_modifier(Modifier::UNDERLINED),
                    );
                }
                if bracket == Some(ch) {
                    paint(buf, cell_x, gw, right, draw_y, self.bracket_style);
                }
                if self.cursor == ch {
                    caret = Some(vx);
                }

                vx = vx.saturating_add(gw);
                ch += g_chars;
                byte += g_bytes;
            }

            // Caret at the segment end is shown only at the logical line's end;
            // at a wrap boundary it belongs to the next visual row's column 0.
            if caret.is_none() && self.cursor == vr.end && vr.end == line_start + line_len {
                caret = Some(vx);
            }
            if caret.is_none() && vr.start == vr.end && self.cursor == vr.start {
                caret = Some(0);
            }
            if let (Some(cvx), Some(cursor_style)) = (caret, self.cursor_style) {
                let cx = text_x0 + cvx;
                if cx < right && draw_y < area.bottom() {
                    buf[(cx, draw_y)].set_style(cursor_style);
                }
            }
        }
    }

    /// Visible visual rows (soft-wrap segments) starting at `offset_y`, up to
    /// `max_rows`. Each row holds at most `text_width` display columns of one
    /// logical line.
    pub(crate) fn visual_rows(&self, text_width: usize, max_rows: usize) -> Vec<VRow> {
        let code = self.code_ref();
        let total_lines = code.len_lines();
        let tw = text_width.max(1);
        let mut rows = Vec::new();
        let mut line = self.offset_y;
        while line < total_lines && rows.len() < max_rows {
            let line_start = code.line_to_char(line);
            let line_len = code.line_len(line);
            if line_len == 0 {
                rows.push(VRow { line, start: line_start, end: line_start });
                line += 1;
                continue;
            }
            let mut col = 0usize;
            while col < line_len && rows.len() < max_rows {
                let slice = code.char_slice(line_start + col, line_start + line_len);
                let mut width = 0usize;
                let mut seg_chars = 0usize;
                for g in RopeGraphemes::new(&slice) {
                    let g_chars = grapheme_width_and_chars_len(g).1;
                    let gw = gwidth(g);
                    if width + gw > tw && seg_chars > 0 {
                        break;
                    }
                    width += gw;
                    seg_chars += g_chars;
                    if width >= tw {
                        break;
                    }
                }
                if seg_chars == 0 {
                    seg_chars = 1;
                }
                let seg_end = (col + seg_chars).min(line_len);
                rows.push(VRow { line, start: line_start + col, end: line_start + seg_end });
                col = seg_end;
            }
            line += 1;
        }
        rows
    }
}

/// Apply `style` to the `width` cells starting at `x` on row `y`, clipped to
/// `right`.
fn paint(buf: &mut Buffer, x: u16, width: u16, right: u16, y: u16, style: Style) {
    for dx in 0..width {
        let cx = x + dx;
        if cx < right {
            buf[(cx, y)].set_style(style);
        }
    }
}
