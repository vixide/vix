use ratatui_core::{widgets::Widget};
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Modifier, Style};
use crate::editor::Editor;
use crate::code::{
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

        let mut draw_y = area.top();
        
        let line_number_style = self.line_number_style;
        let default_text_style = self.text_style;

        // draw line numbers and text
        for line_idx in self.offset_y..total_lines {
            if draw_y >= area.bottom() { break }
            if self.show_line_numbers {
                let line_number = format!("{:>width$}", line_idx + 1, width = line_number_digits);
                buf.set_string(area.left(), draw_y, &line_number, line_number_style);
            }
            let line_len = code.line_len(line_idx);
            let max_x = (area.width as usize).saturating_sub(line_number_width);
        
            let start_col = self.offset_x.min(line_len);
            let end_col = (start_col + max_x).min(line_len);
        
            let line_start_char = code.line_to_char(line_idx);
            let char_start = line_start_char + start_col;
            let char_end = line_start_char + end_col;
        
            let visible_chars = code.char_slice(char_start, char_end);

            let text_x = area.left() + line_number_width as u16;
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
                            let gx = text_x + x as u16;
                            if gx < right_edge {
                                buf[(gx, draw_y)].set_style(self.whitespace_style);
                            }
                        }
                        x = x.saturating_add(g_width);
                    }
                    // A line-ending glyph after the content, when the line's end
                    // is in view and it is not the final (newline-less) line.
                    if end_col == line_len && line_idx + 1 < total_lines {
                        let nl_x = text_x + x as u16;
                        if nl_x < right_edge {
                            buf.set_string(nl_x, draw_y, "\u{00B6}", self.whitespace_style); // ¶
                        }
                    }
                }
            } else {
                let displayed_line = visible_chars.to_string().replace("\t", " ");
                if text_x < right_edge && draw_y < area.top() + area.height {
                    buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
                }
            }

            draw_y += 1;
        }

        // draw syntax highlighting
        if code.is_highlight() {
            
            // Render syntax highlighting for the visible portion of the text buffer.
            // For each visible line within the viewport, limit the highlighting to the
            // visible columns to avoid expensive processing of long lines outside the view.
            // This improves performance by only querying Tree-sitter for the visible slice,
            // then applying styles per character based on byte ranges returned by the syntax query.

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
                
                    let start_x = area.left() + line_number_width as u16 + x as u16;
                    let draw_y = area.top() + screen_y as u16;
                
                    for dx in 0..g_width {
                        if x + dx >= max_x { break; }
                        let draw_x = start_x + dx as u16;
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

        // draw selection
        if let Some(selection) = self.selection && !selection.is_empty() {
            let start = selection.start.min(selection.end);
            let end = selection.start.max(selection.end);
        
            let start_line = code.char_to_line(start);
            let end_line = code.char_to_line(end);
        
            for line_idx in start_line..=end_line {
                if line_idx < self.offset_y { continue }
                if line_idx >= self.offset_y + area.height as usize { break }
        
                let line_start_char = code.line_to_char(line_idx);
                let line_len = code.line_len(line_idx);
                let line_end_char = line_start_char + line_len;
        
                let sel_start = start.max(line_start_char);
                let sel_end = end.min(line_end_char);
        
                let rel_start = sel_start - line_start_char;
                let rel_end = sel_end - line_start_char;
        
                let start_col = self.offset_x.min(line_len);
                let max_text_width = (area.width as usize).saturating_sub(line_number_width);
                let end_col = (start_col + max_text_width).min(line_len);
        
                let char_slice_start = line_start_char + start_col;
                let char_slice_end = line_start_char + end_col;
        
                let visible_chars = code.char_slice(char_slice_start, char_slice_end);

                let draw_y = area.top() + (line_idx - self.offset_y) as u16;
                let mut visual_x: u16 = 0;
                let mut char_col = start_col;

                for g in RopeGraphemes::new(&visible_chars) {
                    let (g_width, g_chars) = grapheme_width_and_chars_len(g);
                
                    if char_col < rel_end && char_col + g_chars > rel_start {
                        let start_x = area.left() + line_number_width as u16 + visual_x;
                        for dx in 0..g_width as u16 {
                            let draw_x = start_x + dx;
                            if draw_x < area.right() && draw_y < area.bottom() {
                                buf[(draw_x, draw_y)].set_style(self.selection_style);
                            }
                        }
                    }
                
                    visual_x = visual_x.saturating_add(g_width as u16);
                    char_col += g_chars;
                }
            }
        }

        // draw marks
        if let Some(ref marks) = self.marks {
            for &(start, end, _color) in marks {
                if start >= end || end > total_chars { continue }

                let start_line = code.char_to_line(start);
                let end_line = code.char_to_line(end);

                for line_idx in start_line..=end_line {
                    if line_idx < self.offset_y || line_idx >= self.offset_y + area.height as usize {
                        continue;
                    }

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

                    let draw_y = area.top() + (line_idx - self.offset_y) as u16;
                    let mut visual_x: u16 = 0;
                    let mut char_col = start_col;

                    for g in RopeGraphemes::new(&visible_chars) {
                        let (g_width, g_chars) = grapheme_width_and_chars_len(g);
                    
                        if char_col < rel_end && char_col + g_chars > rel_start {
                            let start_x = area.left() + line_number_width as u16 + visual_x;
                            for dx in 0..g_width as u16 {
                                let draw_x = start_x + dx;
                                if draw_x < area.right() && draw_y < area.bottom() {
                                    buf[(draw_x, draw_y)].set_style(
                                        Style::default().add_modifier(Modifier::UNDERLINED),
                                    );
                                }
                            }
                        }
                    
                        visual_x = visual_x.saturating_add(g_width as u16);
                        char_col += g_chars;
                    }
                }
            }
        }

        // draw cursor (topmost): a one-cell block at the caret, themed by the host
        if let Some(cursor_style) = self.cursor_style {
            let cur = self.cursor.min(total_chars);
            let line_idx = code.char_to_line(cur);
            if line_idx >= self.offset_y && line_idx < self.offset_y + area.height as usize {
                let line_start_char = code.line_to_char(line_idx);
                let line_len = code.line_len(line_idx);
                let rel = cur - line_start_char; // cursor's character column
                let start_col = self.offset_x.min(line_len);
                let max_text_width = (area.width as usize).saturating_sub(line_number_width);
                let end_col = (start_col + max_text_width).min(line_len);

                // Only draw when the caret column is within the horizontal viewport.
                if rel >= start_col && rel <= end_col {
                    // Walk graphemes from the first visible column to accumulate the
                    // visual x of the caret (so wide glyphs are accounted for).
                    let visible = code.char_slice(line_start_char + start_col, line_start_char + end_col);
                    let mut visual_x: u16 = 0;
                    let mut char_col = start_col;
                    for g in RopeGraphemes::new(&visible) {
                        if char_col >= rel { break; }
                        let (g_width, g_chars) = grapheme_width_and_chars_len(g);
                        visual_x = visual_x.saturating_add(g_width as u16);
                        char_col += g_chars;
                    }
                    let draw_x = area.left() + line_number_width as u16 + visual_x;
                    let draw_y = area.top() + (line_idx - self.offset_y) as u16;
                    if draw_x < area.right() && draw_y < area.bottom() {
                        buf[(draw_x, draw_y)].set_style(cursor_style);
                    }
                }
            }
        }

        // highlight the bracket matching the one at the cursor
        if let Some(bpos) = self.matching_bracket() {
            let line_idx = code.char_to_line(bpos);
            if line_idx >= self.offset_y && line_idx < self.offset_y + area.height as usize {
                let line_start_char = code.line_to_char(line_idx);
                let line_len = code.line_len(line_idx);
                let rel = bpos - line_start_char;
                let start_col = self.offset_x.min(line_len);
                let max_text_width = (area.width as usize).saturating_sub(line_number_width);
                let end_col = (start_col + max_text_width).min(line_len);
                if rel >= start_col && rel < end_col {
                    let visible = code.char_slice(line_start_char + start_col, line_start_char + end_col);
                    let mut visual_x: u16 = 0;
                    let mut char_col = start_col;
                    for g in RopeGraphemes::new(&visible) {
                        if char_col >= rel { break; }
                        let (g_width, g_chars) = grapheme_width_and_chars_len(g);
                        visual_x = visual_x.saturating_add(g_width as u16);
                        char_col += g_chars;
                    }
                    let draw_x = area.left() + line_number_width as u16 + visual_x;
                    let draw_y = area.top() + (line_idx - self.offset_y) as u16;
                    if draw_x < area.right() && draw_y < area.bottom() {
                        buf[(draw_x, draw_y)].set_style(self.bracket_style);
                    }
                }
            }
        }
    }

}

#[cfg(test)]
mod whitespace_tests {
    use crate::editor::Editor;
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
