//! Soft-wrap layout and rendering for the editor widget (Vix's own code, held to
//! the crate's `clippy::pedantic`). Long logical lines wrap across several screen
//! rows; a shared [`Editor::visual_rows`] layout drives the renderer here as well
//! as the cursor scroll (`Editor::focus`) and mouse hit-testing
//! (`cursor_from_mouse`).

#![warn(clippy::pedantic)]

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};
use ropey::RopeSlice;

use crate::editor_core::code::{grapheme_width_and_bytes_len, grapheme_width_and_chars_len, RopeGraphemes};
use crate::editor_core::editor::Editor;

/// One on-screen row in soft-wrap mode: a `[start, end)` character-offset slice
/// of a logical line (`line`).
#[derive(Clone, Copy)]
pub(crate) struct VRow {
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Shared per-render context for drawing one visual row. Bundles the values that
/// are constant across a single `render_wrapped` call so the per-row helper takes
/// a manageable number of arguments.
#[derive(Clone, Copy)]
struct RowCtx<'a> {
    area: Rect,
    text_x0: u16,
    right: u16,
    draw_y: u16,
    line_number_digits: usize,
    default_text_style: Style,
    caret_sels: &'a [(usize, usize)],
    bracket: Option<usize>,
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
        let text_x0 = area.left() + u16::try_from(line_number_width).unwrap_or(u16::MAX);
        let right = area.right();
        let text_width = (area.width as usize).saturating_sub(line_number_width);
        let height = area.height as usize;
        let default_text_style = self.text_style;

        let rows = self.visual_rows(text_width, height);
        let bracket = self.matching_bracket();

        // Sorted selection range as char offsets, if non-empty.
        // Every caret's selection range (multiple-cursor aware).
        let caret_sels = self.caret_selections();

        for (i, vr) in rows.iter().enumerate() {
            let draw_y = area.top() + u16::try_from(i).unwrap_or(u16::MAX);
            if draw_y >= area.bottom() {
                break;
            }
            let ctx = RowCtx {
                area,
                text_x0,
                right,
                draw_y,
                line_number_digits,
                default_text_style,
                caret_sels: &caret_sels,
                bracket,
            };
            self.render_visual_row(buf, *vr, &ctx);
        }
    }

    /// Render one visual (soft-wrap) row: line number, text, whitespace glyphs,
    /// syntax/selection/mark overlays, and the caret.
    fn render_visual_row(&self, buf: &mut Buffer, vr: VRow, ctx: &RowCtx) {
        let code = self.code_ref();
        let RowCtx { area, text_x0, right, draw_y, line_number_digits, default_text_style, .. } = *ctx;

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

        // git diff gutter: a colored bar on the first visual row of a changed
        // logical line (mirrors the non-wrapped renderer; continuation rows have
        // no bar so a wrapped change is marked once, at its start).
        if is_first
            && let Some(ref gmarks) = self.gutter_marks
            && let Some(&(_, color)) = gmarks.iter().find(|&&(l, _)| l == vr.line)
        {
            let sign_x = if self.show_line_numbers {
                area.left() + u16::try_from(line_number_digits).unwrap_or(u16::MAX)
            } else {
                area.left()
            };
            if sign_x < right {
                buf[(sign_x, draw_y)].set_symbol("\u{258e}").set_style(Style::default().fg(color));
            }
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

        let (mut caret, vx) =
            self.paint_row_graphemes(buf, &seg, ctx, &highlights, vr.start, start_byte);

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

    /// Style one visual row's graphemes (whitespace, syntax, selection, marks,
    /// spell, bracket, inline extra carets) starting at character offset
    /// `start_ch` / byte offset `start_byte`. Returns the primary caret's visual
    /// column (if it falls within the segment) and the segment's final width.
    fn paint_row_graphemes(
        &self,
        buf: &mut Buffer,
        seg: &RopeSlice,
        ctx: &RowCtx,
        highlights: &[(usize, usize, Style)],
        start_ch: usize,
        start_byte: usize,
    ) -> (Option<u16>, u16) {
        let RowCtx { area, text_x0, right, draw_y, caret_sels, bracket, .. } = *ctx;
        let mut vx: u16 = 0;
        let mut ch = start_ch;
        let mut byte = start_byte;
        let mut caret: Option<u16> = None;
        for g in RopeGraphemes::new(seg) {
            let (w, g_chars) = grapheme_width_and_chars_len(g);
            let (_, g_bytes) = grapheme_width_and_bytes_len(g);
            let gw = if g.chars().next() == Some('\t') { 1 } else { u16::try_from(w).unwrap_or(u16::MAX) };
            let cell_x = text_x0 + vx;

            if self.show_whitespace
                && matches!(g.chars().next(), Some('\t' | ' ' | '\r'))
                && cell_x < right
            {
                buf[(cell_x, draw_y)].set_style(self.whitespace_style);
            }
            for &(hs, he, st) in highlights {
                if hs <= byte && byte < he {
                    paint(buf, cell_x, gw, right, draw_y, st);
                    break;
                }
            }
            if caret_sels.iter().any(|&(ss, se)| ss <= ch && ch < se) {
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
            if let Some(spell) = self.spell_marks.as_ref()
                && spell.iter().any(|&(s, e)| s <= ch && ch < e)
            {
                paint(
                    buf,
                    cell_x,
                    gw,
                    right,
                    draw_y,
                    Style {
                        fg: Some(Color::Red),
                        ..Style::default().add_modifier(Modifier::UNDERLINED)
                    },
                );
            }
            if bracket == Some(ch) {
                paint(buf, cell_x, gw, right, draw_y, self.bracket_style);
            }
            if self.cursor == ch {
                caret = Some(vx);
            } else if let Some(cs) = self.cursor_style {
                // Extra carets are painted inline (only the primary uses the
                // end-of-line caret handling below).
                if self.carets.iter().any(|c| c.pos == ch) {
                    let cx = text_x0 + vx;
                    if cx < right && draw_y < area.bottom() {
                        buf[(cx, draw_y)].set_style(cs);
                    }
                }
            }

            vx = vx.saturating_add(gw);
            ch += g_chars;
            byte += g_bytes;
        }
        (caret, vx)
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
