#![warn(clippy::pedantic)]
use ratatui_core::{widgets::Widget};
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};
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
        let line_number_width_u16 = u16::try_from(line_number_width).unwrap_or(u16::MAX);

        // draw line numbers and text
        self.draw_text_lines(
            area, buf, line_number_width, line_number_width_u16,
            line_number_digits, total_lines,
        );

        // draw indentation guides (faint vertical bars at each indent level)
        self.draw_indent_guides(area, buf, line_number_width_u16, total_lines);

        // draw syntax highlighting
        if code.is_highlight() {
            self.draw_syntax_layer(
                area, buf, line_number_width, line_number_width_u16,
                total_lines, total_chars,
            );
        }

        // recolor brackets by nesting depth (over the syntax colors)
        if self.rainbow_brackets {
            self.draw_rainbow_brackets(area, buf, line_number_width_u16, total_lines);
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

        // draw passive word-occurrence marks: a subtle reversed/dim background so
        // every occurrence of the word under the cursor stands out.
        if let Some(ref words) = self.word_marks {
            let word_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::DIM);
            for &(start, end) in words {
                if start >= end || end > total_chars { continue }
                self.highlight_char_range(area, buf, line_number_width, start, end, word_style);
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

    /// Display columns that inline hints add before the real glyph at char
    /// column `col` on `line` (sum of widths of hints with `hint_col <= col`).
    /// Returns 0 when there are no hints or the view is horizontally scrolled, so
    /// non-hinted rendering is unchanged.
    fn inlay_shift(&self, line: usize, col: usize) -> u16 {
        if self.offset_x != 0 || self.inlay_hints.is_empty() {
            return 0;
        }
        let w: usize = self
            .inlay_hints
            .iter()
            .filter(|(l, c, _)| *l == line && *c <= col)
            .map(|(_, _, label)| unicode_width::UnicodeWidthStr::width(label.as_str()))
            .sum();
        u16::try_from(w).unwrap_or(u16::MAX)
    }

    /// The inline hints on `line` (column, label), sorted by column, when the
    /// view is not horizontally scrolled.
    fn line_inlays(&self, line: usize) -> Vec<(usize, &str)> {
        if self.offset_x != 0 {
            return Vec::new();
        }
        let mut hints: Vec<(usize, &str)> = self
            .inlay_hints
            .iter()
            .filter(|(l, _, _)| *l == line)
            .map(|(_, c, label)| (*c, label.as_str()))
            .collect();
        hints.sort_by_key(|&(c, _)| c);
        hints
    }

    /// Screen row (absolute `y`) for logical `line`, accounting for folds, or
    /// `None` if the line is hidden, above the viewport, or below it. With no
    /// active folds this is exactly `area.top() + (line - offset_y)`.
    fn screen_row(&self, line: usize, area: Rect) -> Option<u16> {
        if line < self.offset_y {
            return None;
        }
        let row = if self.has_folds() {
            if self.is_line_hidden(line) {
                return None;
            }
            (self.offset_y..line).filter(|&l| !self.is_line_hidden(l)).count()
        } else {
            line - self.offset_y
        };
        let draw_y = area.top().checked_add(u16::try_from(row).unwrap_or(u16::MAX))?;
        (draw_y < area.bottom()).then_some(draw_y)
    }

    /// The logical line drawn at screen row `screen_y` (0-based within the view),
    /// accounting for folds. With no active folds this is `offset_y + screen_y`.
    fn line_at_row(&self, screen_y: usize) -> Option<usize> {
        let total = self.code_ref().len_lines();
        if !self.has_folds() {
            let line = self.offset_y + screen_y;
            return (line < total).then_some(line);
        }
        let mut count = 0;
        for line in self.offset_y..total {
            if self.is_line_hidden(line) {
                continue;
            }
            if count == screen_y {
                return Some(line);
            }
            count += 1;
        }
        None
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
        // For relative numbering, the cursor's line is the reference point.
        let cursor_line = if self.relative_line_numbers { code.char_to_line(self.cursor) } else { 0 };

        let mut row: u16 = 0;
        for line_idx in self.offset_y..total_lines {
            if self.is_line_hidden(line_idx) { continue }
            let draw_y = area.top() + row;
            if draw_y >= area.bottom() { break }
            row = row.saturating_add(1);
            if self.show_line_numbers {
                // Hybrid relative: the cursor line shows its absolute number;
                // others show their distance from it.
                let value = if self.relative_line_numbers && line_idx != cursor_line {
                    line_idx.abs_diff(cursor_line)
                } else {
                    line_idx + 1
                };
                let line_number = format!("{value:>line_number_digits$}");
                buf.set_string(area.left(), draw_y, &line_number, line_number_style);
            }
            // Gutter sign column (just before the text, or column 0 when line
            // numbers are hidden). Debugger markers take precedence over the git
            // diff bar: ▶ for the stopped line, ● for a breakpoint, else ▎ diff.
            let sign_x = if self.show_line_numbers {
                area.left() + u16::try_from(line_number_digits).unwrap_or(u16::MAX)
            } else {
                area.left()
            };
            if sign_x < area.right() {
                if self.debug_line == Some(line_idx) {
                    buf[(sign_x, draw_y)].set_symbol("\u{25b6}").set_style(Style::default().fg(Color::Yellow));
                } else if self.breakpoints.contains(&line_idx) {
                    buf[(sign_x, draw_y)].set_symbol("\u{25cf}").set_style(Style::default().fg(Color::Red));
                } else if let Some(ref gmarks) = self.gutter_marks
                    && let Some(&(_, color)) = gmarks.iter().find(|&&(l, _)| l == line_idx)
                {
                    buf[(sign_x, draw_y)].set_symbol("\u{258e}").set_style(Style::default().fg(color));
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

            // Fold marker in the gutter padding column just before the text.
            if let Some(folded) = self.fold_marker(line_idx) {
                let fx = text_x.saturating_sub(1);
                if fx >= area.left() && fx < right_edge {
                    let sym = if folded { "\u{25b8}" } else { "\u{25be}" }; // ▸ folded, ▾ open
                    buf[(fx, draw_y)].set_symbol(sym).set_style(line_number_style);
                }
            }

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
                let hints = self.line_inlays(line_idx);
                if !(text_x < right_edge && draw_y < area.top() + area.height) {
                    // off the right edge / below the view
                } else if hints.is_empty() {
                    let displayed_line = visible_chars.to_string().replace('\t', " ");
                    buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
                } else {
                    self.draw_line_with_inlays(
                        buf, text_x, right_edge, draw_y, &visible_chars, &hints, default_text_style,
                    );
                }
            }

            self.draw_eol_note(buf, line_idx, line_start_char, line_len, text_x, right_edge, draw_y);
        }
    }

    /// Draw the optional end-of-line virtual note (e.g. inline git blame) for
    /// `line_idx`, dimmed after the line content. Only when not horizontally
    /// scrolled and the note is for this line.
    #[allow(clippy::too_many_arguments)]
    fn draw_eol_note(
        &self,
        buf: &mut Buffer,
        line_idx: usize,
        line_start_char: usize,
        line_len: usize,
        text_x: u16,
        right_edge: u16,
        draw_y: u16,
    ) {
        if self.offset_x != 0 {
            return;
        }
        let Some((nl, note)) = self.eol_note.as_ref() else { return };
        if *nl != line_idx || note.is_empty() {
            return;
        }
        let full = self.code_ref().char_slice(line_start_char, line_start_char + line_len);
        let lw: usize = RopeGraphemes::new(&full).map(|g| grapheme_width_and_chars_len(g).0).sum();
        let nx = text_x + u16::try_from(lw + 2).unwrap_or(u16::MAX);
        if nx < right_edge {
            let avail = (right_edge - nx) as usize;
            let shown: String = note.chars().take(avail).collect();
            let style = self.whitespace_style.add_modifier(Modifier::ITALIC);
            buf.set_string(nx, draw_y, &shown, style);
        }
    }

    /// Draw a line's text with inline hints inserted at their columns (the real
    /// glyphs after each hint shift right); the hint cells are dimmed.
    #[allow(clippy::too_many_arguments)]
    fn draw_line_with_inlays(
        &self,
        buf: &mut Buffer,
        text_x: u16,
        right_edge: u16,
        draw_y: u16,
        visible_chars: &ropey::RopeSlice,
        hints: &[(usize, &str)],
        text_style: Style,
    ) {
        use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
        let mut displayed = String::new();
        let mut hint_spans: Vec<(usize, usize)> = Vec::new(); // (display col, width)
        let mut disp_col = 0usize;
        let mut hi = 0usize;
        for (i, ch) in visible_chars.chars().enumerate() {
            while hi < hints.len() && hints[hi].0 == i {
                let w = UnicodeWidthStr::width(hints[hi].1);
                hint_spans.push((disp_col, w));
                displayed.push_str(hints[hi].1);
                disp_col += w;
                hi += 1;
            }
            let c = if ch == '\t' { ' ' } else { ch };
            displayed.push(c);
            disp_col += UnicodeWidthChar::width(c).unwrap_or(1);
        }
        // Hints anchored at or past the end of the visible text.
        while hi < hints.len() {
            let w = UnicodeWidthStr::width(hints[hi].1);
            hint_spans.push((disp_col, w));
            displayed.push_str(hints[hi].1);
            disp_col += w;
            hi += 1;
        }
        buf.set_string(text_x, draw_y, &displayed, text_style);
        for (start, w) in hint_spans {
            for dx in 0..w {
                let x = text_x + u16::try_from(start + dx).unwrap_or(u16::MAX);
                if x < right_edge {
                    buf[(x, draw_y)].set_style(self.whitespace_style);
                }
            }
        }
    }

    /// Overlay Tree-sitter syntax styles on the visible portion of each line.
    /// Highlighting is limited to the visible columns so long off-screen lines are
    /// not processed.
    /// Draw faint vertical indentation guides at each indent level on every
    /// visible line. Space-indented buffers only (the common case); tab- or
    /// mixed-indented buffers are skipped to avoid column-mapping ambiguity.
    fn draw_indent_guides(&self, area: Rect, buf: &mut Buffer, line_number_width_u16: u16, total_lines: usize) {
        let code = self.code_ref();
        let unit = code.indent();
        if unit.is_empty() || unit.contains('\t') {
            return;
        }
        let step = unit.chars().count();
        let max_x = (area.width as usize).saturating_sub(line_number_width_u16 as usize);
        for screen_y in 0..(area.height as usize) {
            let Some(line_idx) = self.line_at_row(screen_y) else { break };
            if line_idx >= total_lines {
                break;
            }
            let line_start = code.line_to_char(line_idx);
            let len = code.line_len(line_idx);
            let leading = code.char_slice(line_start, line_start + len).chars().take_while(|c| *c == ' ').count();
            let levels = leading / step;
            for col in (0..levels).map(|k| k * step) {
                if col < self.offset_x {
                    continue;
                }
                let x = col - self.offset_x;
                if x >= max_x {
                    break;
                }
                let draw_x = area.left() + line_number_width_u16 + u16::try_from(x).unwrap_or(u16::MAX);
                let draw_y = area.top() + u16::try_from(screen_y).unwrap_or(u16::MAX);
                if draw_x < area.right() && draw_y < area.bottom() {
                    buf[(draw_x, draw_y)].set_symbol("\u{2502}").set_style(self.whitespace_style);
                }
            }
        }
    }

    /// Recolor `()[]{}` by nesting depth across the visible lines (rainbow
    /// brackets). Depth is seeded by counting brackets from the start of the
    /// document to the first visible line, then carried line to line. Skipped for
    /// very large documents to keep the per-frame cost bounded.
    fn draw_rainbow_brackets(&self, area: Rect, buf: &mut Buffer, line_number_width_u16: u16, total_lines: usize) {
        const COLORS: [Color; 6] = [
            Color::Rgb(0xff, 0xd7, 0x00), // gold
            Color::Rgb(0xda, 0x70, 0xd6), // orchid
            Color::Rgb(0x6a, 0x9e, 0xff), // blue
            Color::Rgb(0x8b, 0xc3, 0x4a), // green
            Color::Rgb(0xff, 0x8c, 0x42), // orange
            Color::Rgb(0x4d, 0xd0, 0xe1), // cyan
        ];
        let code = self.code_ref();
        let Some(first) = self.line_at_row(0) else { return };
        let base = code.line_to_char(first);
        if base > 200_000 {
            return; // bound the from-start scan on very large files
        }
        // Seed nesting depth from the document start to the first visible line.
        let mut depth: usize = code.char_slice(0, base).chars().fold(0usize, |d, c| match c {
            '(' | '[' | '{' => d + 1,
            ')' | ']' | '}' => d.saturating_sub(1),
            _ => d,
        });
        let max_x = (area.width as usize).saturating_sub(line_number_width_u16 as usize);
        for screen_y in 0..(area.height as usize) {
            let Some(line_idx) = self.line_at_row(screen_y) else { break };
            if line_idx >= total_lines {
                break;
            }
            let ls = code.line_to_char(line_idx);
            let len = code.line_len(line_idx);
            let slice = code.char_slice(ls, ls + len);
            let mut vcol = 0usize;
            for g in RopeGraphemes::new(&slice) {
                let w = grapheme_width_and_bytes_len(g).0;
                if g.len_chars() == 1 {
                    let c = g.char(0);
                    let color = match c {
                        '(' | '[' | '{' => {
                            let col = COLORS[depth % COLORS.len()];
                            depth += 1;
                            Some(col)
                        }
                        ')' | ']' | '}' => {
                            depth = depth.saturating_sub(1);
                            Some(COLORS[depth % COLORS.len()])
                        }
                        _ => None,
                    };
                    if let Some(color) = color
                        && vcol >= self.offset_x
                    {
                        let x = vcol - self.offset_x;
                        if x < max_x {
                            let dx = area.left() + line_number_width_u16 + u16::try_from(x).unwrap_or(u16::MAX);
                            let dy = area.top() + u16::try_from(screen_y).unwrap_or(u16::MAX);
                            if dx < area.right() && dy < area.bottom() {
                                buf[(dx, dy)].set_fg(color);
                            }
                        }
                    }
                }
                vcol += w;
            }
        }
    }

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
        // Highlight the whole visible region in a single Tree-sitter query rather
        // than one query per line: cheaper while typing, and the (start, end) cache
        // now memoizes one entry instead of one per visible line.
        let highlights = {
            let mut region: Option<(usize, usize)> = None;
            for screen_y in 0..(area.height as usize) {
                let Some(line_idx) = self.line_at_row(screen_y) else { break };
                if line_idx >= total_lines { break }
                let ls = code.line_to_char(line_idx);
                let s = code.char_to_byte(ls);
                let e = code.char_to_byte(ls + code.line_len(line_idx));
                region = Some(region.map_or((s, e), |(rs, re)| (rs.min(s), re.max(e))));
            }
            region.map(|(s, e)| self.highlight_interval(s, e, &self.theme)).unwrap_or_default()
        };
        for screen_y in 0..(area.height as usize) {
            let Some(line_idx) = self.line_at_row(screen_y) else { break };
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

            let mut x = 0;
            let mut byte_idx_in_rope = start_byte;
            let mut char_col = self.offset_x;

            for g in RopeGraphemes::new(&chars) {
                let (g_width, g_bytes) = grapheme_width_and_bytes_len(g);

                if x >= max_x { break; }

                let shift = self.inlay_shift(line_idx, char_col);
                char_col += g.chars().count();
                let start_x = area.left() + line_number_width_u16 + shift + u16::try_from(x).unwrap_or(u16::MAX);
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
            let Some(draw_y) = self.screen_row(line_idx, area) else { continue };

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

            let mut visual_x: u16 = 0;
            let mut char_col = start_col;

            for g in RopeGraphemes::new(&visible_chars) {
                let (g_width, g_chars) = grapheme_width_and_chars_len(g);

                if char_col < rel_end && char_col + g_chars > rel_start {
                    let shift = self.inlay_shift(line_idx, char_col);
                    let start_x = area.left() + line_number_width_u16 + shift + visual_x;
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
        let Some(draw_y) = self.screen_row(line_idx, area) else { return };
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
        let draw_x = area.left() + line_number_width_u16 + self.inlay_shift(line_idx, rel) + visual_x;
        if draw_x < area.right() && draw_y < area.bottom() {
            buf[(draw_x, draw_y)].set_style(style);
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

    #[cfg(feature = "lang-rust")]
    #[test]
    fn viewport_highlight_reaches_lines_beyond_the_first() {
        use ratatui_core::style::Color;
        // The whole visible region is highlighted in one query; ensure a token on
        // the SECOND visible line is still styled (not just the first line).
        let mut ed = Editor::new("rust", "let a = 1;\nfn main() {}\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_syntax_theme(&[("keyword", "ff0000"), ("keyword.function", "ff0000"), ("function", "ff0000")]);
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let line1_styled = (0..40).any(|x| buf[(x, 1)].fg == Color::Rgb(255, 0, 0));
        assert!(line1_styled, "second visible line (fn main) is highlighted via the single viewport query");
    }

    #[test]
    fn relative_line_numbers_show_distance_from_the_cursor() {
        let mut ed = Editor::new("text", "l0\nl1\nl2\nl3", Vec::new()).unwrap();
        ed.set_relative_line_numbers(true);
        ed.set_cursor(6); // char 6 is on line 2 ("l2")
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        // Gutter is 5 wide (min). Line 0 → distance 2, line 2 → absolute 3, line 3 → 1.
        let gutter = |y: u16| row(&buf, y, 0, 5);
        assert_eq!(gutter(0).trim(), "2", "distance from cursor line");
        assert_eq!(gutter(2).trim(), "3", "cursor line shows absolute number");
        assert_eq!(gutter(3).trim(), "1");
    }

    #[test]
    fn rainbow_brackets_color_by_nesting_depth() {
        use ratatui_core::style::Color;
        let mut ed = Editor::new("text", "(a[b])", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_rainbow_brackets(true);
        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        // 2-cell left padding: '(' at x=2 (depth 0), '[' at x=4 (depth 1).
        assert_eq!(buf[(2, 0)].fg, Color::Rgb(0xff, 0xd7, 0x00), "outer ( is depth-0 gold");
        assert_eq!(buf[(4, 0)].fg, Color::Rgb(0xda, 0x70, 0xd6), "inner [ is depth-1 orchid");
    }

    #[test]
    fn indent_guides_drawn_on_indented_lines_only() {
        let mut ed = Editor::new("text", "    code\nflush\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_indent(Some("  ".to_string())); // 2-space indent unit → guide step 2
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let row0 = row(&buf, 0, 0, 20);
        let row1 = row(&buf, 1, 0, 20);
        assert!(row0.contains('\u{2502}'), "indented line shows a guide: {row0:?}");
        assert!(!row1.contains('\u{2502}'), "flush line shows no guide: {row1:?}");
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
