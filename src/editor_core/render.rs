#![warn(clippy::pedantic)]
use ratatui_core::{widgets::Widget};
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};
use crate::editor_core::editor::Editor;
use crate::editor_core::code::{
    RopeGraphemes, grapheme_width_and_chars_len, grapheme_width_and_bytes_len
};

/// Draws the main editor view in the provided area using the ratatui rendering buffer.
///
/// Renders the main editor view in four distinct layers:
/// 1. Line numbers and text content are drawn in the visible viewport.
/// 2. Syntax highlighting is overlaid on top of the text.
/// 3. The selection highlight is rendered above the syntax layer.
/// 4. User marks are rendered as the final uppermost overlay.
///
/// # Arguments
///
/// * `self` - The `Editor` instance (as reference) to render.
/// * `area` - The rectangular area on the terminal to draw within.
/// * `buf` - The ratatui `Buffer` that represents the screen cells to draw to.
/// 
impl Widget for &Editor {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.soft_wrap {
            self.render_wrapped(area, buf);
        } else {
            self.render_nowrap(area, buf);
        }
    }
}

impl Editor {
    /// Classic render: one logical line per screen row, horizontal scrolling.
    fn render_nowrap(&self, area: Rect, buf: &mut Buffer) {
        let code = self.code_ref();
        let total_chars = code.len_chars();

        let total_lines = code.len_lines();
        let max_line_number = total_lines.max(1);
        let line_number_digits = max_line_number.to_string().len().max(5);
        let line_number_width = self.get_line_number_width();
        let line_number_width_u16 = u16::try_from(line_number_width).unwrap_or(u16::MAX);

        // draw line numbers and text
        self.draw_text_lines(
            area, buf, line_number_width, line_number_width_u16,
            line_number_digits, total_lines,
        );

        // draw syntax highlighting
        if code.is_highlight() {
            self.draw_syntax_layer(
                area, buf, line_number_width, line_number_width_u16,
                total_lines, total_chars,
            );
        }

        // draw selection(s) — one range per caret (multiple-cursor aware)
        for (start, end) in self.caret_selections() {
            self.highlight_char_range(
                area, buf, line_number_width,
                start, end, self.selection_style,
            );
        }

        // draw marks
        if let Some(ref marks) = self.marks {
            for &(start, end, _color) in marks {
                if start >= end || end > total_chars { continue }
                self.highlight_char_range(
                    area, buf, line_number_width,
                    start, end, Style::default().add_modifier(Modifier::UNDERLINED),
                );
            }
        }

        // draw spell-check marks: a red underline under misspelled words
        if let Some(ref spell) = self.spell_marks {
            let spell_style = Style {
                fg: Some(Color::Red),
                ..Style::default().add_modifier(Modifier::UNDERLINED)
            };
            for &(start, end) in spell {
                if start >= end || end > total_chars { continue }
                self.highlight_char_range(
                    area, buf, line_number_width,
                    start, end, spell_style,
                );
            }
        }

        // draw LSP diagnostic marks: a colored underline per severity, on a
        // channel separate from spell marks so the two coexist.
        if let Some(ref diags) = self.diagnostic_marks {
            for &(start, end, color) in diags {
                if start >= end || end > total_chars { continue }
                let diag_style = Style {
                    fg: Some(color),
                    ..Style::default().add_modifier(Modifier::UNDERLINED)
                };
                self.highlight_char_range(
                    area, buf, line_number_width,
                    start, end, diag_style,
                );
            }
        }

        // draw cursor(s) (topmost): a one-cell block at each caret, themed by the
        // host (multiple-cursor aware).
        if let Some(cursor_style) = self.cursor_style {
            for cur in self.caret_positions() {
                let cur = cur.min(total_chars);
                self.draw_caret_block(
                    area, buf, line_number_width,
                    cur, false, cursor_style,
                );
            }
        }

        // highlight the bracket matching the one at the cursor
        if let Some(bpos) = self.matching_bracket() {
            self.draw_caret_block(
                area, buf, line_number_width,
                bpos, true, self.bracket_style,
            );
        }
    }

    /// Draw line numbers, the git-diff gutter, and the (optionally
    /// whitespace-annotated) text for every visible line.
    fn draw_text_lines(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width: usize,
        line_number_width_u16: u16,
        line_number_digits: usize,
        total_lines: usize,
    ) {
        let code = self.code_ref();
        let line_number_style = self.line_number_style;
        let default_text_style = self.text_style;

        for line_idx in self.offset_y..total_lines {
            let draw_y = area.top() + u16::try_from(line_idx - self.offset_y).unwrap_or(u16::MAX);
            if draw_y >= area.bottom() { break }
            if self.show_line_numbers {
                let line_number = format!("{:>width$}", line_idx + 1, width = line_number_digits);
                buf.set_string(area.left(), draw_y, &line_number, line_number_style);
            }
            // git diff gutter: a colored bar in the gutter gap (just before the
            // text), or column 0 when line numbers are hidden.
            if let Some(ref gmarks) = self.gutter_marks
                && let Some(&(_, color)) = gmarks.iter().find(|&&(l, _)| l == line_idx) {
                    let sign_x = if self.show_line_numbers {
                        area.left() + u16::try_from(line_number_digits).unwrap_or(u16::MAX)
                    } else {
                        area.left()
                    };
                    if sign_x < area.right() {
                        buf[(sign_x, draw_y)]
                            .set_symbol("\u{258e}")
                            .set_style(Style::default().fg(color));
                    }
                }
            let line_len = code.line_len(line_idx);
            let max_x = (area.width as usize).saturating_sub(line_number_width);

            let start_col = self.offset_x.min(line_len);
            let end_col = (start_col + max_x).min(line_len);

            let line_start_char = code.line_to_char(line_idx);
            let char_start = line_start_char + start_col;
            let char_end = line_start_char + end_col;

            let visible_chars = code.char_slice(char_start, char_end);

            let text_x = area.left() + line_number_width_u16;
            let right_edge = area.left() + area.width;

            if self.show_whitespace {
                // Substitute a visible glyph for each space / tab / carriage
                // return. The syntax layer below only restyles cells (it never
                // rewrites them), so these glyphs survive.
                let displayed_line: String = visible_chars
                    .chars()
                    .map(|c| match c {
                        '\t' => '\u{2192}', // → tab
                        ' ' => '\u{00B7}',  // · space
                        '\r' => '\u{240D}', // ␍ carriage return
                        other => other,
                    })
                    .collect();
                if text_x < right_edge && draw_y < area.top() + area.height {
                    buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
                    // Dim the glyph cells, tracking display columns so wide
                    // characters elsewhere on the line don't shift the marks.
                    let mut x = 0usize;
                    for g in RopeGraphemes::new(&visible_chars) {
                        let (g_width, _g_chars) = grapheme_width_and_chars_len(g);
                        if matches!(g.chars().next(), Some('\t' | ' ' | '\r')) {
                            let gx = text_x + u16::try_from(x).unwrap_or(u16::MAX);
                            if gx < right_edge {
                                buf[(gx, draw_y)].set_style(self.whitespace_style);
                            }
                        }
                        x = x.saturating_add(g_width);
                    }
                    // A line-ending glyph after the content, when the line's end
                    // is in view and it is not the final (newline-less) line.
                    if end_col == line_len && line_idx + 1 < total_lines {
                        let nl_x = text_x + u16::try_from(x).unwrap_or(u16::MAX);
                        if nl_x < right_edge {
                            buf.set_string(nl_x, draw_y, "\u{00B6}", self.whitespace_style); // ¶
                        }
                    }
                }
            } else {
                let displayed_line = visible_chars.to_string().replace('\t', " ");
                if text_x < right_edge && draw_y < area.top() + area.height {
                    buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
                }
            }
        }
    }

    /// Overlay Tree-sitter syntax styles on the visible portion of each line.
    /// Highlighting is limited to the visible columns so long off-screen lines are
    /// not processed.
    fn draw_syntax_layer(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width: usize,
        line_number_width_u16: u16,
        total_lines: usize,
        total_chars: usize,
    ) {
        let code = self.code_ref();
        for screen_y in 0..(area.height as usize) {
            let line_idx = self.offset_y + screen_y;
            if line_idx >= total_lines { break }

            let line_len = code.line_len(line_idx);
            let max_x = (area.width as usize).saturating_sub(line_number_width);

            let line_start_char = code.line_to_char(line_idx);
            let start_char = line_start_char + self.offset_x;
            let visible_len = line_len.saturating_sub(self.offset_x);
            let end = max_x.min(visible_len);
            let end_char = start_char + end;

            if start_char > total_chars || end_char > total_chars {
                continue; // last line offset case
            }

            let chars = code.char_slice(start_char, end_char);

            let start_byte = code.char_to_byte(start_char);
            let end_byte = code.char_to_byte(end_char);

            let highlights = self.highlight_interval(
                start_byte, end_byte, &self.theme
            );

            let mut x = 0;
            let mut byte_idx_in_rope = start_byte;

            for g in RopeGraphemes::new(&chars) {
                let (g_width, g_bytes) = grapheme_width_and_bytes_len(g);

                if x >= max_x { break; }

                let start_x = area.left() + line_number_width_u16 + u16::try_from(x).unwrap_or(u16::MAX);
                let draw_y = area.top() + u16::try_from(screen_y).unwrap_or(u16::MAX);

                for dx in 0..g_width {
                    if x + dx >= max_x { break; }
                    let draw_x = start_x + u16::try_from(dx).unwrap_or(u16::MAX);
                    for &(start, end, s) in &highlights {
                        if start <= byte_idx_in_rope && byte_idx_in_rope < end {
                            buf[(draw_x, draw_y)].set_style(s);
                            break;
                        }
                    }
                }

                x = x.saturating_add(g_width);
                byte_idx_in_rope += g_bytes;
            }
        }
    }

    /// Overlay `style` on the cells covering the character range `[start, end)`
    /// across the visible lines, accounting for grapheme widths and horizontal
    /// scroll. Used by selection, mark, spell-check, and diagnostic layers.
    fn highlight_char_range(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width: usize,
        start: usize,
        end: usize,
        style: Style,
    ) {
        let line_number_width_u16 = u16::try_from(line_number_width).unwrap_or(u16::MAX);
        let code = self.code_ref();
        let start_line = code.char_to_line(start);
        let end_line = code.char_to_line(end);

        for line_idx in start_line..=end_line {
            if line_idx < self.offset_y { continue }
            if line_idx >= self.offset_y + area.height as usize { break }

            let line_start_char = code.line_to_char(line_idx);
            let line_len = code.line_len(line_idx);
            let line_end_char = line_start_char + line_len;

            let highlight_start = start.max(line_start_char);
            let highlight_end = end.min(line_end_char);

            let rel_start = highlight_start - line_start_char;
            let rel_end = highlight_end - line_start_char;

            let start_col = self.offset_x.min(line_len);
            let max_text_width = (area.width as usize).saturating_sub(line_number_width);
            let end_col = (start_col + max_text_width).min(line_len);

            let char_slice_start = line_start_char + start_col;
            let char_slice_end = line_start_char + end_col;

            let visible_chars = code.char_slice(char_slice_start, char_slice_end);

            let draw_y = area.top() + u16::try_from(line_idx - self.offset_y).unwrap_or(u16::MAX);
            let mut visual_x: u16 = 0;
            let mut char_col = start_col;

            for g in RopeGraphemes::new(&visible_chars) {
                let (g_width, g_chars) = grapheme_width_and_chars_len(g);

                if char_col < rel_end && char_col + g_chars > rel_start {
                    let start_x = area.left() + line_number_width_u16 + visual_x;
                    for dx in 0..u16::try_from(g_width).unwrap_or(u16::MAX) {
                        let draw_x = start_x + dx;
                        if draw_x < area.right() && draw_y < area.bottom() {
                            buf[(draw_x, draw_y)].set_style(style);
                        }
                    }
                }

                visual_x = visual_x.saturating_add(u16::try_from(g_width).unwrap_or(u16::MAX));
                char_col += g_chars;
            }
        }
    }

    /// Draw a one-cell block at the character offset `pos` with `style`. When
    /// `exclusive_end` is set the caret must lie strictly before the last visible
    /// column (used for bracket matching); otherwise the end column is allowed.
    fn draw_caret_block(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width: usize,
        pos: usize,
        exclusive_end: bool,
        style: Style,
    ) {
        let line_number_width_u16 = u16::try_from(line_number_width).unwrap_or(u16::MAX);
        let code = self.code_ref();
        let line_idx = code.char_to_line(pos);
        if line_idx < self.offset_y || line_idx >= self.offset_y + area.height as usize {
            return;
        }
        let line_start_char = code.line_to_char(line_idx);
        let line_len = code.line_len(line_idx);
        let rel = pos - line_start_char;
        let start_col = self.offset_x.min(line_len);
        let max_text_width = (area.width as usize).saturating_sub(line_number_width);
        let end_col = (start_col + max_text_width).min(line_len);

        let in_view = if exclusive_end {
            rel >= start_col && rel < end_col
        } else {
            rel >= start_col && rel <= end_col
        };
        if !in_view {
            return;
        }

        let visible = code.char_slice(line_start_char + start_col, line_start_char + end_col);
        let mut visual_x: u16 = 0;
        let mut char_col = start_col;
        for g in RopeGraphemes::new(&visible) {
            if char_col >= rel { break; }
            let (g_width, g_chars) = grapheme_width_and_chars_len(g);
            visual_x = visual_x.saturating_add(u16::try_from(g_width).unwrap_or(u16::MAX));
            char_col += g_chars;
        }
        let draw_x = area.left() + line_number_width_u16 + visual_x;
        let draw_y = area.top() + u16::try_from(line_idx - self.offset_y).unwrap_or(u16::MAX);
        if draw_x < area.right() && draw_y < area.bottom() {
            buf[(draw_x, draw_y)].set_style(style);
        }
    }

}

#[cfg(test)]
mod whitespace_tests {
    use crate::editor_core::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    /// Collect the glyphs of one rendered row into a String.
    fn row(buf: &Buffer, y: u16, x0: u16, x1: u16) -> String {
        (x0..x1).map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' ')).collect()
    }

    #[test]
    fn soft_wrap_splits_long_line_across_rows() {
        let mut ed = Editor::new("text", "abcdefghij", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_soft_wrap(true);
        // The editor keeps a 2-cell left padding, so a width-6 area has a width-4
        // text column starting at x=2. "abcdefghij" wraps into abcd / efgh / ij.
        let area = Rect::new(0, 0, 6, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert_eq!(row(&buf, 0, 2, 6), "abcd");
        assert_eq!(row(&buf, 1, 2, 6), "efgh");
        assert_eq!(row(&buf, 2, 2, 6), "ij  ");
    }

    #[test]
    fn soft_wrap_mouse_maps_to_wrapped_row() {
        let mut ed = Editor::new("text", "abcdefghij", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_soft_wrap(true);
        let area = Rect::new(0, 0, 6, 5);
        // Click the 2nd visual row ("efgh") at text column 2 (x = 2 padding + 2).
        let off = ed.cursor_from_mouse(4, 1, &area).unwrap();
        assert_eq!(off, 6, "click on wrapped row 1 col 2 maps to char index 6 ('g')");
    }

    #[test]
    fn shows_glyphs_for_space_tab_and_newline() {
        // Two lines so the first line has a trailing newline glyph.
        let mut ed = Editor::new("text", "a b\tc\nx", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.show_whitespace(true);
        let area = Rect::new(0, 0, 12, 3);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let line0 = row(&buf, 0, 0, 12);
        assert!(line0.contains('\u{00B7}'), "space → middot: {line0:?}");
        assert!(line0.contains('\u{2192}'), "tab → arrow: {line0:?}");
        assert!(line0.contains('\u{00B6}'), "line end → pilcrow: {line0:?}");
    }

    #[test]
    fn hidden_by_default() {
        let mut ed = Editor::new("text", "a b\tc", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        let area = Rect::new(0, 0, 12, 2);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let line0 = row(&buf, 0, 0, 12);
        assert!(!line0.contains('\u{00B7}'), "no glyphs when off: {line0:?}");
        assert!(!line0.contains('\u{2192}'));
    }
}
