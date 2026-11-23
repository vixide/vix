//! Painting the editor into a ratatui [`Buffer`].
//!
//! `impl Widget for &Editor` is the entry point: it clips to the buffer, then
//! dispatches to the no-wrap renderer here or the soft-wrap renderer in the
//! private `wrap` module. Both draw in layers, back to front: gutter + text,
//! indent guides, syntax colours, rainbow brackets, then selection / marks /
//! diagnostics / carets and bracket matching. The renderer is read-only over
//! the [`Editor`] (state such as the highlight cache lives behind `RefCell`s).
//! The App shell (`src/ui.rs`) draws everything *around* the editor; the editor
//! itself is drawn only here. See `crates/vix-editor-core/spec/index.md`.

use crate::code::{RopeGraphemes, grapheme_width_and_bytes_len, grapheme_width_and_chars_len};
use crate::editor::{CommentMark, Editor, Flags};
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};
use ratatui_core::widgets::Widget;

/// Checked `+` and `-` on the screen and text offsets drawn here: a sum past
/// the top of the type is the top (off-screen or past the text), and a
/// difference below zero is zero.
trait Geo: Sized {
    /// `self + other`, or the type's top when it does not fit.
    fn plus(self, other: Self) -> Self;
    /// `self - other`, or zero when it would go below zero.
    fn minus(self, other: Self) -> Self;
}

impl Geo for usize {
    fn plus(self, other: Self) -> Self {
        add_or(self, other, Self::MAX)
    }
    fn minus(self, other: Self) -> Self {
        sub_or(self, other, 0)
    }
}

impl Geo for u16 {
    fn plus(self, other: Self) -> Self {
        add_u16_or(self, other, Self::MAX)
    }
    fn minus(self, other: Self) -> Self {
        sub_u16_or(self, other, 0)
    }
}

/// `a + b`, or the declared `fallback` when the sum does not fit.
fn add_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_add(b).unwrap_or(fallback)
}

/// `a - b`, or the declared `fallback` when it would go below zero.
fn sub_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a * b`, or the declared `fallback` when the product does not fit.
fn mul_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_mul(b).unwrap_or(fallback)
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// Draws the main editor view in the provided area using the ratatui rendering buffer.
///
/// Renders the main editor view in four distinct layers:
/// 1. Line numbers and text content are drawn in the visible viewport.
/// 2. Syntax highlighting is overlaid on top of the text.
/// 3. The selection highlight is rendered above the syntax layer.
/// 4. User marks are rendered as the final uppermost overlay.
///
/// The text-only layers are the same in both modes; with
/// the soft-wrap flag set the work is delegated to the soft-wrap renderer
/// (wrapped rows, no horizontal scroll), otherwise to the classic one-row-per-
/// line renderer.
///
/// # Arguments
///
/// * `self` - The `Editor` instance (as reference) to render.
/// * `area` - The rectangular area on the terminal to draw within.
/// * `buf` - The ratatui `Buffer` that represents the screen cells to draw to.
///
impl Widget for &Editor {
    /// Draw the editor into `buf` inside `area` (clipped to `buf.area`; an empty
    /// intersection draws nothing).
    fn render(self, area: Rect, buf: &mut Buffer) {
        // A caller's rect can overhang a tiny terminal; never paint outside the buffer.
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        if self.flags.contains(Flags::SOFT_WRAP) {
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
            area,
            buf,
            line_number_width,
            line_number_width_u16,
            line_number_digits,
            total_lines,
        );

        // draw indentation guides (faint vertical bars at each indent level)
        self.draw_indent_guides(area, buf, line_number_width_u16, total_lines);

        // draw syntax highlighting -- or the LSP semantic-token overlay
        // (T123a) alone, for a file whose language has a server but no
        // bundled Tree-sitter grammar (`is_highlight()` false: no query).
        if code.has_highlighter() || !self.semantic_tokens.is_empty() {
            self.draw_syntax_layer(
                area,
                buf,
                line_number_width,
                line_number_width_u16,
                total_lines,
                total_chars,
            );
        }

        // recolor brackets by nesting depth (over the syntax colors)
        if self.flags.contains(Flags::RAINBOW_BRACKETS) {
            self.draw_rainbow_brackets(area, buf, line_number_width_u16, total_lines);
        }

        // draw selection(s) — one range per caret (multiple-cursor aware)
        for (start, end) in self.caret_selections() {
            // A selection left pointing past a buffer that has since shrunk (the
            // text was replaced under it) is drawn as far as the text goes,
            // never indexed out of range (T575).
            let end = end.min(total_chars);
            let start = start.min(end);
            self.highlight_char_range(
                area,
                buf,
                line_number_width,
                start,
                end,
                self.selection_style,
            );
        }

        // Cowork: other participants' carets, beneath the local cursor
        self.draw_remote_carets(area, buf, line_number_width, total_chars);

        // draw marks
        if let Some(ref marks) = self.marks {
            for &(start, end, _color) in marks {
                if start >= end || end > total_chars {
                    continue;
                }
                self.highlight_char_range(
                    area,
                    buf,
                    line_number_width,
                    start,
                    end,
                    Style::default().add_modifier(Modifier::UNDERLINED),
                );
            }
        }

        // draw passive word-occurrence marks: a subtle reversed/dim background so
        // every occurrence of the word under the cursor stands out.
        if let Some(ref words) = self.word_marks {
            let word_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::DIM);
            for &(start, end) in words {
                if start >= end || end > total_chars {
                    continue;
                }
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
                if start >= end || end > total_chars {
                    continue;
                }
                self.highlight_char_range(area, buf, line_number_width, start, end, spell_style);
            }
        }

        // draw LSP diagnostic marks: a colored underline per severity, on a
        // channel separate from spell marks so the two coexist.
        if let Some(ref diags) = self.diagnostic_marks {
            for &(start, end, color) in diags {
                if start >= end || end > total_chars {
                    continue;
                }
                let diag_style = Style {
                    fg: Some(color),
                    ..Style::default().add_modifier(Modifier::UNDERLINED)
                };
                self.highlight_char_range(area, buf, line_number_width, start, end, diag_style);
            }
        }

        // draw cursor(s) (topmost): a one-cell block at each caret, themed by the
        // host (multiple-cursor aware).
        if let Some(cursor_style) = self.cursor_style {
            for cur in self.caret_positions() {
                let cur = cur.min(total_chars);
                self.draw_caret_block(area, buf, line_number_width, cur, false, cursor_style);
            }
        }

        // highlight the bracket matching the one at the cursor
        if let Some(bpos) = self.matching_bracket() {
            self.draw_caret_block(area, buf, line_number_width, bpos, true, self.bracket_style);
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
            .fold(0, |total: usize, w| total.plus(w));
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

    /// Screen row (absolute `y`) for logical `line`, accounting for folds and
    /// code-lens rows (T726), or `None` if the line is hidden, above the
    /// viewport, or below it. With no folds or lenses this is exactly
    /// `area.top() + (line - offset_y)`.
    fn screen_row(&self, line: usize, area: Rect) -> Option<u16> {
        let row = self.rows_to_text(line)?;
        let draw_y = area
            .top()
            .checked_add(u16::try_from(row).unwrap_or(u16::MAX))?;
        (draw_y < area.bottom()).then_some(draw_y)
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
        let default_text_style = self.text_style;

        let text_x0 = area.left().plus(line_number_width_u16);
        for (row, view_row) in self
            .view_rows(usize::from(area.height))
            .into_iter()
            .enumerate()
        {
            let draw_y = area.top().plus(u16::try_from(row).unwrap_or(u16::MAX));
            if draw_y >= area.bottom() {
                break;
            }
            let line_idx = match view_row {
                crate::lens::ViewRow::Lens(line) => {
                    self.draw_lens_row(buf, line, text_x0, area.right(), draw_y);
                    continue;
                }
                crate::lens::ViewRow::Filler(_) => {
                    self.draw_filler_row(area, buf, draw_y);
                    continue;
                }
                crate::lens::ViewRow::Text(line) => line,
            };
            if line_idx >= total_lines {
                break;
            }
            self.draw_line_gutter(buf, area, draw_y, line_idx, line_number_digits);
            let line_len = code.line_len(line_idx);
            let max_x = usize::from(area.width).minus(line_number_width);

            let start_col = self.offset_x.min(line_len);
            let end_col = start_col.plus(max_x).min(line_len);

            let line_start_char = code.line_to_char(line_idx);
            let char_start = line_start_char.plus(start_col);
            let char_end = line_start_char.plus(end_col);

            let visible_chars = code.char_slice(char_start, char_end);

            let text_x = area.left().plus(line_number_width_u16);
            let right_edge = area.left().plus(area.width);

            self.draw_fold_marker(buf, line_idx, area.left(), text_x, right_edge, draw_y);

            if self.flags.contains(Flags::SHOW_WHITESPACE) {
                self.draw_whitespace_line(
                    buf,
                    &visible_chars,
                    area,
                    text_x,
                    right_edge,
                    draw_y,
                    end_col,
                    line_len,
                    line_idx,
                    total_lines,
                    default_text_style,
                );
            } else {
                let hints = self.line_inlays(line_idx);
                if !(text_x < right_edge && draw_y < area.top().plus(area.height)) {
                    // off the right edge / below the view
                } else if hints.is_empty() {
                    let displayed_line = visible_chars.to_string().replace('\t', " ");
                    buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
                } else {
                    self.draw_line_with_inlays(
                        buf,
                        text_x,
                        right_edge,
                        draw_y,
                        &visible_chars,
                        &hints,
                        default_text_style,
                    );
                }
            }

            self.draw_eol_note(
                buf,
                line_idx,
                line_start_char,
                line_len,
                text_x,
                right_edge,
                draw_y,
            );
            self.draw_fold_ellipsis(
                buf,
                line_idx,
                line_start_char,
                line_len,
                text_x,
                right_edge,
                draw_y,
                default_text_style,
            );
        }
    }

    /// Draw one filler row (T671) at screen row `y`: a hatch across the text
    /// area, blank gutter.
    fn draw_filler_row(&self, area: Rect, buf: &mut Buffer, y: u16) {
        let text_x = area
            .left()
            .plus(u16::try_from(self.get_line_number_width()).unwrap_or(u16::MAX));
        for x in text_x..area.right() {
            buf[(x, y)]
                .set_symbol("\u{2571}")
                .set_style(self.line_number_style);
        }
    }

    /// Draw `line_idx`'s line number (absolute, or hybrid-relative to the
    /// cursor's line) and its gutter sign at screen row `draw_y`.
    fn draw_line_gutter(
        &self,
        buf: &mut Buffer,
        area: Rect,
        draw_y: u16,
        line_idx: usize,
        line_number_digits: usize,
    ) {
        if self.flags.contains(Flags::SHOW_LINE_NUMBERS) {
            // Hybrid relative: the cursor line shows its absolute number;
            // others show their distance from it.
            let cursor_line = self.code_ref().char_to_line(self.cursor);
            let value =
                if self.flags.contains(Flags::RELATIVE_LINE_NUMBERS) && line_idx != cursor_line {
                    line_idx.abs_diff(cursor_line)
                } else {
                    line_idx.plus(1)
                };
            let line_number = format!("{value:>line_number_digits$}");
            buf.set_string(area.left(), draw_y, &line_number, self.line_number_style);
        }
        // Gutter sign column (just before the text, or column 0 when line
        // numbers are hidden).
        let sign_x = if self.flags.contains(Flags::SHOW_LINE_NUMBERS) {
            area.left()
                .plus(u16::try_from(line_number_digits).unwrap_or(u16::MAX))
        } else {
            area.left()
        };
        self.draw_gutter_sign(buf, sign_x, draw_y, line_idx, area.right());
    }

    /// Draw the fold marker (▸ folded, ▾ open) in the gutter padding column just
    /// before a foldable line's text.
    pub(crate) fn draw_fold_marker(
        &self,
        buf: &mut Buffer,
        line_idx: usize,
        area_left: u16,
        text_x: u16,
        right_edge: u16,
        draw_y: u16,
    ) {
        if let Some(folded) = self.fold_marker(line_idx) {
            let fx = text_x.minus(1);
            if fx >= area_left && fx < right_edge {
                let sym = if folded { "\u{25b8}" } else { "\u{25be}" }; // ▸ folded, ▾ open
                buf[(fx, draw_y)]
                    .set_symbol(sym)
                    .set_style(self.line_number_style);
            }
        }
    }

    /// Render a trailing `...` right after a folded start line's text, so a
    /// collapsed fold (e.g. an Org drawer `:PROPERTIES:...`) shows that content
    /// is hidden below it, mirroring how Org and outline editors mark a fold.
    #[allow(
        clippy::too_many_arguments,
        reason = "a draw helper split out of draw_text_lines; its inputs are the frame state it was carved from"
    )]
    fn draw_fold_ellipsis(
        &self,
        buf: &mut Buffer,
        line_idx: usize,
        line_start_char: usize,
        line_len: usize,
        text_x: u16,
        right_edge: u16,
        draw_y: u16,
        text_style: Style,
    ) {
        if self.offset_x != 0 || self.fold_marker(line_idx) != Some(true) {
            return;
        }
        let full = self
            .code_ref()
            .char_slice(line_start_char, line_start_char.plus(line_len));
        let lw: usize = RopeGraphemes::new(&full)
            .map(|g| grapheme_width_and_chars_len(g).0)
            .fold(0, |total: usize, w| total.plus(w));
        let ex = text_x.plus(u16::try_from(lw).unwrap_or(u16::MAX));
        if ex < right_edge {
            let avail = usize::from(right_edge.minus(ex));
            let shown: String = "...".chars().take(avail).collect();
            buf.set_string(ex, draw_y, &shown, text_style);
        }
    }

    /// Render `visible` with visible whitespace glyphs (→ tab, · space, ␍ CR, and
    /// a trailing ¶ at a line's end), dimming the glyph cells. Split out of
    /// [`Editor::draw_text_lines`] to keep it within the line limit.
    #[allow(
        clippy::too_many_arguments,
        reason = "a draw helper split out of draw_text_lines; its inputs are the frame state it was carved from"
    )]
    fn draw_whitespace_line(
        &self,
        buf: &mut Buffer,
        visible: &ropey::RopeSlice,
        area: Rect,
        text_x: u16,
        right_edge: u16,
        draw_y: u16,
        end_col: usize,
        line_len: usize,
        line_idx: usize,
        total_lines: usize,
        default_text_style: Style,
    ) {
        // Substitute a visible glyph for each space / tab / carriage return. The
        // syntax layer below only restyles cells (it never rewrites them), so
        // these glyphs survive.
        let displayed_line: String = visible
            .chars()
            .map(|c| match c {
                '\t' => '\u{2192}', // → tab
                ' ' => '\u{00B7}',  // · space
                '\r' => '\u{240D}', // ␍ carriage return
                other => other,
            })
            .collect();
        if text_x < right_edge && draw_y < area.top().plus(area.height) {
            buf.set_string(text_x, draw_y, &displayed_line, default_text_style);
            // Dim the glyph cells, tracking display columns so wide characters
            // elsewhere on the line don't shift the marks.
            let mut x = 0usize;
            for g in RopeGraphemes::new(visible) {
                let (g_width, _g_chars) = grapheme_width_and_chars_len(g);
                if matches!(g.chars().next(), Some('\t' | ' ' | '\r')) {
                    let gx = text_x.plus(u16::try_from(x).unwrap_or(u16::MAX));
                    if gx < right_edge {
                        buf[(gx, draw_y)].set_style(self.whitespace_style);
                    }
                }
                x = x.plus(g_width);
            }
            // A line-ending glyph after the content, when the line's end is in
            // view and it is not the final (newline-less) line.
            if end_col == line_len && line_idx.plus(1) < total_lines {
                let nl_x = text_x.plus(u16::try_from(x).unwrap_or(u16::MAX));
                if nl_x < right_edge {
                    buf.set_string(nl_x, draw_y, "\u{00B6}", self.whitespace_style); // ¶
                }
            }
        }
    }

    /// Draw the gutter sign for `line_idx` at `sign_x`, one glyph per line in
    /// priority order: ▶ the debugger's stopped line, ● a breakpoint, ◆/◇ an
    /// open/resolved comment thread, else ▎ the diff or coverage bar (see
    /// [`Editor::set_comment_marks`]). No-op when `sign_x` is past
    /// `area_right`. Split out of [`Editor::draw_text_lines`] to keep it
    /// within the line limit; the soft-wrap renderer calls it too, for a
    /// logical line's first row.
    pub(crate) fn draw_gutter_sign(
        &self,
        buf: &mut Buffer,
        sign_x: u16,
        draw_y: u16,
        line_idx: usize,
        area_right: u16,
    ) {
        if sign_x >= area_right {
            return;
        }
        if self.debug_line == Some(line_idx) {
            buf[(sign_x, draw_y)]
                .set_symbol("\u{25b6}")
                .set_style(Style::default().fg(Color::Yellow));
        } else if self.breakpoints.contains(&line_idx) {
            buf[(sign_x, draw_y)]
                .set_symbol("\u{25cf}")
                .set_style(Style::default().fg(Color::Red));
        } else if let Some(mark) = self.comment_mark(line_idx) {
            let (symbol, style) = match mark {
                CommentMark::Resolved => ("\u{25c7}", self.line_number_style),
                _ => ("\u{25c6}", Style::default()),
            };
            buf[(sign_x, draw_y)].set_symbol(symbol).set_style(style);
        } else if let Some(ref gmarks) = self.gutter_marks
            && let Some(&(_, color)) = gmarks.iter().find(|&&(l, _)| l == line_idx)
        {
            buf[(sign_x, draw_y)]
                .set_symbol("\u{258e}")
                .set_style(Style::default().fg(color));
        }
    }

    /// Draw the optional end-of-line virtual note (e.g. inline git blame) for
    /// `line_idx`, dimmed after the line content, then every layer's
    /// end-of-line virtual text (T614) after that. The note is drawn only when
    /// not horizontally scrolled and the note is for this line.
    #[allow(
        clippy::too_many_arguments,
        reason = "a draw helper split out of draw_text_lines; its inputs are the frame state it was carved from"
    )]
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
        let mut note_end = None;
        if self.offset_x == 0
            && let Some((nl, note)) = self.eol_note.as_ref()
            && *nl == line_idx
            && !note.is_empty()
        {
            let full = self
                .code_ref()
                .char_slice(line_start_char, line_start_char.plus(line_len));
            let lw: usize = RopeGraphemes::new(&full)
                .map(|g| grapheme_width_and_chars_len(g).0)
                .fold(0, |total: usize, w| total.plus(w));
            let nx = text_x.plus(u16::try_from(lw.plus(2)).unwrap_or(u16::MAX));
            if nx < right_edge {
                let avail = usize::from(right_edge.minus(nx));
                let shown: String = note.chars().take(avail).collect();
                let style = self.whitespace_style.add_modifier(Modifier::ITALIC);
                buf.set_string(nx, draw_y, &shown, style);
                note_end = Some(
                    usize::from(nx).plus(unicode_width::UnicodeWidthStr::width(shown.as_str())),
                );
            }
        }
        // Skip measuring the line on the (usual) frame with no virtual text.
        if !self.has_eol_texts(line_idx) {
            return;
        }
        let content_end = note_end.unwrap_or_else(|| {
            let mut end = usize::from(text_x)
                .plus(self.line_width_from(line_idx, self.offset_x))
                .plus(usize::from(self.inlay_shift(line_idx, usize::MAX)));
            if self.offset_x == 0 && self.fold_marker(line_idx) == Some(true) {
                end = end.plus(3); // the folded line's `...`
            }
            end
        });
        self.draw_eol_texts(buf, line_idx, content_end, right_edge, draw_y);
    }

    /// Draw a line's text with inline hints inserted at their columns (the real
    /// glyphs after each hint shift right); the hint cells are dimmed.
    #[allow(
        clippy::too_many_arguments,
        reason = "a draw helper split out of draw_text_lines; its inputs are the frame state it was carved from"
    )]
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
            while let Some(&(at, label)) = hints.get(hi)
                && at == i
            {
                let w = UnicodeWidthStr::width(label);
                hint_spans.push((disp_col, w));
                displayed.push_str(label);
                disp_col = disp_col.plus(w);
                hi = hi.plus(1);
            }
            let c = if ch == '\t' { ' ' } else { ch };
            displayed.push(c);
            disp_col = disp_col.plus(UnicodeWidthChar::width(c).unwrap_or(1));
        }
        // Hints anchored at or past the end of the visible text.
        while let Some(&(_, label)) = hints.get(hi) {
            let w = UnicodeWidthStr::width(label);
            hint_spans.push((disp_col, w));
            displayed.push_str(label);
            disp_col = disp_col.plus(w);
            hi = hi.plus(1);
        }
        buf.set_string(text_x, draw_y, &displayed, text_style);
        for (start, w) in hint_spans {
            for dx in 0..w {
                let x = text_x.plus(u16::try_from(start.plus(dx)).unwrap_or(u16::MAX));
                if x < right_edge {
                    buf[(x, draw_y)].set_style(self.whitespace_style);
                }
            }
        }
    }

    /// Draw faint vertical indentation guides at each indent level on every
    /// visible line. Space-indented buffers only (the common case); tab- or
    /// mixed-indented buffers are skipped to avoid column-mapping ambiguity.
    fn draw_indent_guides(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width_u16: u16,
        total_lines: usize,
    ) {
        let code = self.code_ref();
        let unit = code.indent();
        if unit.is_empty() || unit.contains('\t') {
            return;
        }
        let step = unit.chars().count();
        let max_x = usize::from(area.width).minus(usize::from(line_number_width_u16));
        for (screen_y, line_idx) in self.text_rows(usize::from(area.height)) {
            if line_idx >= total_lines {
                break;
            }
            let line_start = code.line_to_char(line_idx);
            let len = code.line_len(line_idx);
            let leading = code
                .char_slice(line_start, line_start.plus(len))
                .chars()
                .take_while(|c| *c == ' ')
                .count();
            let levels = leading.checked_div(step).unwrap_or(0);
            for col in (0..levels).map(|k| mul_or(k, step, usize::MAX)) {
                if col < self.offset_x {
                    continue;
                }
                let x = col.minus(self.offset_x);
                if x >= max_x {
                    break;
                }
                let draw_x = area
                    .left()
                    .plus(line_number_width_u16)
                    .plus(u16::try_from(x).unwrap_or(u16::MAX));
                let draw_y = area.top().plus(u16::try_from(screen_y).unwrap_or(u16::MAX));
                if draw_x < area.right() && draw_y < area.bottom() {
                    buf[(draw_x, draw_y)]
                        .set_symbol("\u{2502}")
                        .set_style(self.whitespace_style);
                }
            }
        }
    }

    /// Recolor `()[]{}` by nesting depth across the visible lines (rainbow
    /// brackets). Depth is seeded by counting brackets from the start of the
    /// document to the first visible line, then carried line to line. Skipped for
    /// very large documents to keep the per-frame cost bounded.
    fn draw_rainbow_brackets(
        &self,
        area: Rect,
        buf: &mut Buffer,
        line_number_width_u16: u16,
        total_lines: usize,
    ) {
        // Cycled by nesting depth modulo 6; closers re-use their opener's colour.
        const COLORS: [Color; 6] = [
            Color::Rgb(0xff, 0xd7, 0x00), // gold
            Color::Rgb(0xda, 0x70, 0xd6), // orchid
            Color::Rgb(0x6a, 0x9e, 0xff), // blue
            Color::Rgb(0x8b, 0xc3, 0x4a), // green
            Color::Rgb(0xff, 0x8c, 0x42), // orange
            Color::Rgb(0x4d, 0xd0, 0xe1), // cyan
        ];
        // The colour for a nesting depth (the depth wraps around the six colours).
        let color_at = |depth: usize| {
            depth
                .checked_rem(COLORS.len())
                .and_then(|i| COLORS.get(i))
                .copied()
        };
        let code = self.code_ref();
        let rows = self.text_rows(usize::from(area.height));
        let Some(&(_, first)) = rows.first() else {
            return;
        };
        let base = code.line_to_char(first);
        // Past ~200k chars the from-start depth scan would cost too much per frame.
        if base > 200_000 {
            return; // bound the from-start scan on very large files
        }
        // Seed nesting depth from the document start to the first visible line.
        // Seeding is by char count, not tokens, so a bracket inside a string or
        // comment before the viewport shifts the colours (cosmetic only).
        let mut depth: usize = code
            .char_slice(0, base)
            .chars()
            .fold(0usize, |d, c| match c {
                '(' | '[' | '{' => d.plus(1),
                ')' | ']' | '}' => d.minus(1),
                _ => d,
            });
        let max_x = usize::from(area.width).minus(usize::from(line_number_width_u16));
        for (screen_y, line_idx) in rows {
            if line_idx >= total_lines {
                break;
            }
            let ls = code.line_to_char(line_idx);
            let len = code.line_len(line_idx);
            let slice = code.char_slice(ls, ls.plus(len));
            let mut vcol = 0usize;
            for g in RopeGraphemes::new(&slice) {
                let w = grapheme_width_and_bytes_len(g).0;
                if g.len_chars() == 1 {
                    let c = g.char(0);
                    let color = match c {
                        '(' | '[' | '{' => {
                            let col = color_at(depth);
                            depth = depth.plus(1);
                            col
                        }
                        ')' | ']' | '}' => {
                            depth = depth.minus(1);
                            color_at(depth)
                        }
                        _ => None,
                    };
                    if let Some(color) = color
                        && vcol >= self.offset_x
                    {
                        let x = vcol.minus(self.offset_x);
                        if x < max_x {
                            let dx = area
                                .left()
                                .plus(line_number_width_u16)
                                .plus(u16::try_from(x).unwrap_or(u16::MAX));
                            let dy = area.top().plus(u16::try_from(screen_y).unwrap_or(u16::MAX));
                            if dx < area.right() && dy < area.bottom() {
                                buf[(dx, dy)].set_fg(color);
                            }
                        }
                    }
                }
                vcol = vcol.plus(w);
            }
        }
    }

    /// Overlay Tree-sitter syntax styles on the visible portion of each line.
    /// Highlighting is limited to the visible columns so long off-screen lines are
    /// not processed. Semantic tokens from the language server are merged into
    /// the same layer when the theme maps their type.
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
        let mut region: Option<(usize, usize)> = None;
        let rows = self.text_rows(usize::from(area.height));
        for &(_, line_idx) in &rows {
            if line_idx >= total_lines {
                break;
            }
            let ls = code.line_to_char(line_idx);
            let s = code.char_to_byte(ls);
            let e = code.char_to_byte(ls.plus(code.line_len(line_idx)));
            region = Some(region.map_or((s, e), |(rs, re)| (rs.min(s), re.max(e))));
        }
        let treesitter_highlights =
            region.map_or_default(|(s, e)| self.highlight_interval(s, e, &self.theme));
        // T123a: LSP semantic tokens, a second highlight layer over
        // Tree-sitter's purely syntactic one -- collected *before*
        // `treesitter_highlights` below (not after) so the per-glyph scan's
        // first-match-wins rule (see below) prefers the LSP's judgment over
        // Tree-sitter's wherever the two disagree, which is the whole point
        // of adding this layer; everywhere semantic tokens say nothing,
        // Tree-sitter's own results still apply unchanged. Each span was
        // computed against the buffer state at the moment it arrived (like
        // `inlay_hints`/`fold_ranges`) and can go stale after an
        // intervening edit; skip anything now out of bounds rather than let
        // `char_to_byte` panic on it.
        let mut highlights = Vec::new();
        if let Some((region_start, region_end)) = region {
            let total = code.len();
            for (start_char, end_char, capture) in &self.semantic_tokens {
                if *start_char >= *end_char || *end_char > total {
                    continue;
                }
                let Some(style) = self.theme.get(capture) else {
                    continue;
                };
                let start_byte = code.char_to_byte(*start_char);
                let end_byte = code.char_to_byte(*end_char);
                if end_byte <= region_start || start_byte >= region_end {
                    continue;
                }
                highlights.push((
                    start_byte.max(region_start),
                    end_byte.min(region_end),
                    *style,
                ));
            }
        }
        highlights.extend(treesitter_highlights);
        for (screen_y, line_idx) in rows {
            if line_idx >= total_lines {
                break;
            }

            let line_len = code.line_len(line_idx);
            let max_x = usize::from(area.width).minus(line_number_width);

            let line_start_char = code.line_to_char(line_idx);
            let start_char = line_start_char.plus(self.offset_x);
            let visible_len = line_len.minus(self.offset_x);
            let end = max_x.min(visible_len);
            let end_char = start_char.plus(end);

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

                if x >= max_x {
                    break;
                }

                let shift = self.inlay_shift(line_idx, char_col);
                char_col = char_col.plus(g.chars().count());
                let start_x = area
                    .left()
                    .plus(line_number_width_u16)
                    .plus(shift)
                    .plus(u16::try_from(x).unwrap_or(u16::MAX));
                let draw_y = area.top().plus(u16::try_from(screen_y).unwrap_or(u16::MAX));

                for dx in 0..g_width {
                    if x.plus(dx) >= max_x {
                        break;
                    }
                    let draw_x = start_x.plus(u16::try_from(dx).unwrap_or(u16::MAX));
                    for &(start, end, s) in &highlights {
                        if start <= byte_idx_in_rope && byte_idx_in_rope < end {
                            buf[(draw_x, draw_y)].set_style(s);
                            break;
                        }
                    }
                }

                x = x.plus(g_width);
                byte_idx_in_rope = byte_idx_in_rope.plus(g_bytes);
            }
        }
    }

    /// Overlay `style` on the cells covering the character range `[start, end)`
    /// across the visible lines, accounting for grapheme widths and horizontal
    /// scroll. Used by selection, mark, spell-check, and diagnostic layers.
    pub(crate) fn highlight_char_range(
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
            let Some(draw_y) = self.screen_row(line_idx, area) else {
                continue;
            };

            let line_start_char = code.line_to_char(line_idx);
            let line_len = code.line_len(line_idx);
            let line_end_char = line_start_char.plus(line_len);

            let highlight_start = start.max(line_start_char);
            let highlight_end = end.min(line_end_char);

            let rel_start = highlight_start.minus(line_start_char);
            let rel_end = highlight_end.minus(line_start_char);

            let start_col = self.offset_x.min(line_len);
            let max_text_width = usize::from(area.width).minus(line_number_width);
            let end_col = start_col.plus(max_text_width).min(line_len);

            let char_slice_start = line_start_char.plus(start_col);
            let char_slice_end = line_start_char.plus(end_col);

            let visible_chars = code.char_slice(char_slice_start, char_slice_end);

            let mut visual_x: u16 = 0;
            let mut char_col = start_col;

            for g in RopeGraphemes::new(&visible_chars) {
                let (g_width, g_chars) = grapheme_width_and_chars_len(g);

                if char_col < rel_end && char_col.plus(g_chars) > rel_start {
                    let shift = self.inlay_shift(line_idx, char_col);
                    let start_x = area
                        .left()
                        .plus(line_number_width_u16)
                        .plus(shift)
                        .plus(visual_x);
                    for dx in 0..u16::try_from(g_width).unwrap_or(u16::MAX) {
                        let draw_x = start_x.plus(dx);
                        if draw_x < area.right() && draw_y < area.bottom() {
                            buf[(draw_x, draw_y)].set_style(style);
                        }
                    }
                }

                visual_x = visual_x.plus(u16::try_from(g_width).unwrap_or(u16::MAX));
                char_col = char_col.plus(g_chars);
            }
        }
    }

    /// Draw a one-cell block at the character offset `pos` with `style`. When
    /// `exclusive_end` is set the caret must lie strictly before the last visible
    /// column (used for bracket matching); otherwise the end column is allowed.
    pub(crate) fn draw_caret_block(
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
        let Some(draw_y) = self.screen_row(line_idx, area) else {
            return;
        };
        let line_start_char = code.line_to_char(line_idx);
        let line_len = code.line_len(line_idx);
        let rel = pos.minus(line_start_char);
        let start_col = self.offset_x.min(line_len);
        let max_text_width = usize::from(area.width).minus(line_number_width);
        let end_col = start_col.plus(max_text_width).min(line_len);

        let in_view = if exclusive_end {
            rel >= start_col && rel < end_col
        } else {
            rel >= start_col && rel <= end_col
        };
        if !in_view {
            return;
        }

        let visible = code.char_slice(
            line_start_char.plus(start_col),
            line_start_char.plus(end_col),
        );
        let mut visual_x: u16 = 0;
        let mut char_col = start_col;
        for g in RopeGraphemes::new(&visible) {
            if char_col >= rel {
                break;
            }
            let (g_width, g_chars) = grapheme_width_and_chars_len(g);
            visual_x = visual_x.plus(u16::try_from(g_width).unwrap_or(u16::MAX));
            char_col = char_col.plus(g_chars);
        }
        let draw_x = area
            .left()
            .plus(line_number_width_u16)
            .plus(self.inlay_shift(line_idx, rel))
            .plus(visual_x);
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
        (x0..x1)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn viewport_highlight_reaches_lines_beyond_the_first() {
        use ratatui_core::style::Color;
        // The whole visible region is highlighted in one query; ensure a token on
        // the SECOND visible line is still styled (not just the first line).
        let mut ed = Editor::new("rust", "let a = 1;\nfn main() {}\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_syntax_theme(&[
            ("keyword", "ff0000"),
            ("keyword.function", "ff0000"),
            ("function", "ff0000"),
        ]);
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let line1_styled = (0..40).any(|x| buf[(x, 1)].fg == Color::Rgb(255, 0, 0));
        assert!(
            line1_styled,
            "second visible line (fn main) is highlighted via the single viewport query"
        );
    }

    #[test]
    fn semantic_tokens_are_merged_into_the_syntax_layer() {
        use ratatui_core::style::Color;
        // Plain "text" grammar -- Tree-sitter contributes nothing here, so
        // any highlighting on screen must come from the semantic-token
        // overlay (T123a), not accidentally from the syntax layer too.
        let mut ed = Editor::new("text", "myVariable\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_left_code_padding(0); // no gutter offset -- column 0 is char 0
        ed.set_syntax_theme(&[("keyword", "ff0000")]);
        // Chars 0..10 span the whole word "myVariable".
        ed.set_semantic_tokens(vec![(0, 10, "keyword".to_string())]);
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert!(
            (0..10).all(|x| buf[(x, 0)].fg == Color::Rgb(255, 0, 0)),
            "the whole semantic-token span is styled"
        );
        assert_ne!(
            buf[(10, 0)].fg,
            Color::Rgb(255, 0, 0),
            "styling doesn't bleed past the token's own end"
        );
    }

    #[test]
    fn semantic_tokens_with_no_theme_mapping_are_silently_skipped() {
        use ratatui_core::style::Color;
        // "function" has no capture in the theme passed here -- must not
        // panic, and must not paint anything either.
        let mut ed = Editor::new("text", "myFunction\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_left_code_padding(0);
        ed.set_syntax_theme(&[("keyword", "ff0000")]);
        ed.set_semantic_tokens(vec![(0, 10, "function".to_string())]);
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert!((0..10).all(|x| buf[(x, 0)].fg != Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn stale_semantic_tokens_past_the_buffer_end_do_not_panic() {
        // The buffer shrank since these were computed (T123a's accepted
        // staleness risk, same as inlay_hints/fold_ranges) -- must be
        // skipped, not panic `char_to_byte` on an out-of-range index.
        let mut ed = Editor::new("text", "hi\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_syntax_theme(&[("keyword", "ff0000")]);
        ed.set_semantic_tokens(vec![(0, 500, "keyword".to_string())]);
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf); // must not panic
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
        assert_eq!(
            buf[(2, 0)].fg,
            Color::Rgb(0xff, 0xd7, 0x00),
            "outer ( is depth-0 gold"
        );
        assert_eq!(
            buf[(4, 0)].fg,
            Color::Rgb(0xda, 0x70, 0xd6),
            "inner [ is depth-1 orchid"
        );
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
        assert!(
            row0.contains('\u{2502}'),
            "indented line shows a guide: {row0:?}"
        );
        assert!(
            !row1.contains('\u{2502}'),
            "flush line shows no guide: {row1:?}"
        );
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
        assert_eq!(
            off, 6,
            "click on wrapped row 1 col 2 maps to char index 6 ('g')"
        );
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

#[cfg(test)]
mod overhang_tests {
    use crate::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    /// Chaos-monkey regression: a rect taller or wider than the buffer (a 12x1
    /// terminal) used to index outside it and panic.
    #[test]
    fn a_rect_overhanging_the_buffer_is_clipped_not_a_panic() {
        for soft_wrap in [false, true] {
            let mut ed = Editor::new("text", "one\ntwo\nthree\n", Vec::new()).unwrap();
            ed.set_soft_wrap(soft_wrap);
            let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
            (&ed).render(Rect::new(0, 0, 40, 9), &mut buf);
            (&ed).render(Rect::new(30, 5, 10, 3), &mut buf);
        }
    }
}

#[cfg(test)]
mod gutter_sign_tests {
    use crate::editor::{CommentMark, Editor};
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    /// The glyphs of row `y`, so a test can look for a gutter sign in it.
    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    /// T753: one glyph per line in the sign column -- debugger, then a
    /// comment thread, then the diff bar -- in both renderers.
    #[test]
    fn comment_marks_sit_between_debugger_and_diff_bar() {
        for wrap in [false, true] {
            let mut ed = Editor::new("text", "a\nb\nc\nd\n", Vec::new()).unwrap();
            ed.set_soft_wrap(wrap);
            ed.set_gutter_marks(vec![(0, "#3fb950"), (1, "#3fb950"), (3, "#3fb950")]);
            ed.set_comment_marks(vec![
                (0, CommentMark::Open),
                (2, CommentMark::Resolved),
                (3, CommentMark::Open),
            ]);
            ed.set_breakpoints(vec![3]);
            let area = Rect::new(0, 0, 20, 4);
            let mut buf = Buffer::empty(area);
            (&ed).render(area, &mut buf);
            let rows: Vec<String> = (0..4).map(|y| row(&buf, y)).collect();
            assert!(
                rows[0].contains('◆') && !rows[0].contains('▎'),
                "{wrap}: {rows:?}"
            );
            assert!(rows[1].contains('▎'), "a plain changed line keeps its bar");
            assert!(rows[2].contains('◇'), "a resolved thread");
            assert!(
                rows[3].contains('●') && !rows[3].contains('◆'),
                "{wrap}: {rows:?}"
            );
        }
    }

    #[test]
    fn one_mark_per_line_and_open_wins() {
        let mut ed = Editor::new("text", "a\nb\n", Vec::new()).unwrap();
        ed.set_comment_marks(vec![
            (1, CommentMark::Resolved),
            (0, CommentMark::Resolved),
            (1, CommentMark::Open),
        ]);
        assert_eq!(
            ed.comment_marks(),
            &[(0, CommentMark::Resolved), (1, CommentMark::Open)]
        );
        assert_eq!(ed.comment_mark(1), Some(CommentMark::Open));
        assert_eq!(ed.comment_mark(5), None);
        ed.clear_comment_marks();
        assert_eq!(ed.comment_marks(), []);
    }
}

#[cfg(test)]
mod filler_tests {
    use super::*;

    fn pad(ed: &Editor) -> u16 {
        u16::try_from(ed.get_line_number_width()).unwrap()
    }

    fn row(buf: &Buffer, y: u16, x0: u16, x1: u16) -> String {
        (x0..x1)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    /// A 6-line editor, no gutter, with 2 filler rows before line 2 (T671).
    fn with_fillers() -> Editor {
        let mut ed = Editor::new("text", "l0\nl1\nl2\nl3\nl4\nl5", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_filler_rows(vec![(2, 2)]);
        ed
    }

    #[test]
    fn filler_rows_push_later_lines_down() {
        let ed = with_fillers();
        let area = Rect::new(0, 0, 10, 8);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let text: Vec<String> = (0..7)
            .map(|y| row(&buf, y, pad(&ed), pad(&ed) + 2))
            .collect();
        assert_eq!(
            text,
            [
                "l0",
                "l1",
                "\u{2571}\u{2571}",
                "\u{2571}\u{2571}",
                "l2",
                "l3",
                "l4"
            ]
        );
    }

    #[test]
    fn filler_rows_after_the_last_line_are_drawn() {
        let mut ed = Editor::new("text", "a\nb", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_filler_rows(vec![(2, 1)]);
        let area = Rect::new(0, 0, 4, 4);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert_eq!(row(&buf, 2, pad(&ed), pad(&ed) + 1), "\u{2571}");
    }

    #[test]
    fn a_click_on_a_filler_row_lands_on_the_next_real_line() {
        let ed = with_fillers();
        let area = Rect::new(0, 0, 10, 8);
        let pos = |y| ed.cursor_from_mouse(pad(&ed), y, &area).unwrap();
        // Rows 2 and 3 are fillers; row 4 is line 2 ("l2" starts at char 6).
        assert_eq!(pos(2), 6);
        assert_eq!(pos(3), 6);
        assert_eq!(pos(4), 6);
        assert_eq!(pos(5), 9, "row 5 is line 3");
        assert_eq!(pos(1), 3, "rows above the filler are unchanged");
    }

    #[test]
    fn the_cursor_is_never_drawn_on_a_filler_row() {
        let mut ed = with_fillers();
        ed.set_cursor_style(Some(Style::default().bg(Color::Red)));
        for c in 0..=17 {
            ed.set_cursor(c);
            let area = Rect::new(0, 0, 10, 8);
            let mut buf = Buffer::empty(area);
            (&ed).render(area, &mut buf);
            for y in [2, 3] {
                assert_ne!(
                    buf[(pad(&ed), y)].bg,
                    Color::Red,
                    "cursor {c} on filler row {y}"
                );
            }
        }
    }

    #[test]
    fn display_row_and_focus_count_filler_rows() {
        let mut ed = with_fillers();
        assert_eq!(ed.display_row(1), 1);
        assert_eq!(ed.display_row(2), 4);
        assert_eq!(ed.display_row(5), 7);
        // A 5-row view: line 5 sits on row 7, so focusing it scrolls.
        ed.set_cursor(15);
        ed.focus(&Rect::new(0, 0, 10, 5));
        assert!(ed.get_offset_y() > 0);
        assert!(ed.display_row(5) < 5, "the cursor row fits the view");
    }

    #[test]
    fn filler_rows_merge_and_ignore_soft_wrap() {
        let mut ed = with_fillers();
        ed.set_filler_rows(vec![(3, 1), (3, 2), (1, 0)]);
        assert_eq!(ed.filler_rows(), [(3, 3)]);
        ed.set_soft_wrap(true);
        assert_eq!(ed.filler_before(3), 0, "unwrapped views only");
    }
}

#[cfg(test)]
mod layer_tests {
    //! What each decoration layer paints, cell by cell, in the no-wrap and
    //! soft-wrap renderers.

    use crate::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::style::{Color, Modifier, Style};
    use ratatui_core::widgets::Widget;

    /// An editor with no gutter at all, so screen column == character column.
    fn bare(lang: &str, text: &str) -> Editor {
        let mut e = Editor::new(lang, text, Vec::new()).unwrap();
        e.show_line_numbers(false);
        e.set_left_code_padding(0);
        e
    }

    fn draw(e: &Editor, w: u16, h: u16) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        e.render(area, &mut buf);
        buf
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    fn has(buf: &Buffer, x: u16, y: u16, m: Modifier) -> bool {
        buf[(x, y)].modifier.contains(m)
    }

    #[test]
    fn mark_layers_paint_their_own_styles() {
        let mut e = bare("text", "abcdefghijkl");
        e.set_marks(vec![(0, 2, "#ff0000"), (5, 3, "#00ff00")]);
        e.set_word_marks(vec![(2, 4)]);
        e.set_spell_marks(vec![(4, 6)]);
        e.set_diagnostic_marks(vec![(6, 8, Color::Blue), (9, 999, Color::Blue)]);
        let b = draw(&e, 20, 2);
        let ul = Modifier::UNDERLINED;
        // Highlight marks: underlined (the inverted range is skipped).
        assert!(has(&b, 0, 0, ul) && has(&b, 1, 0, ul) && !has(&b, 2, 0, ul));
        // Word-occurrence marks: reversed and dim.
        assert!(has(&b, 2, 0, Modifier::REVERSED | Modifier::DIM) && has(&b, 3, 0, Modifier::DIM));
        assert!(!has(&b, 4, 0, Modifier::REVERSED));
        // Spell marks: red underline.
        assert_eq!(b[(4, 0)].fg, Color::Red);
        assert!(has(&b, 4, 0, ul) && has(&b, 5, 0, ul) && !has(&b, 3, 0, ul));
        // Diagnostics: underline in the diagnostic's colour; a range past the
        // end of the buffer is ignored rather than clipped.
        assert_eq!(b[(6, 0)].fg, Color::Blue);
        assert!(has(&b, 7, 0, ul) && !has(&b, 8, 0, ul));
        assert!(!has(&b, 10, 0, ul), "out-of-range diagnostic skipped");
        // The text itself is unchanged underneath.
        assert_eq!(row(&b, 0).trim_end(), "abcdefghijkl");
    }

    #[test]
    fn soft_wrap_paints_marks_selection_and_spell_underlines() {
        let mut e = bare("text", "abcdefghij");
        e.set_soft_wrap(true);
        e.set_marks(vec![(1, 3, "#ff0000")]);
        e.set_spell_marks(vec![(7, 9)]);
        e.set_selection_range(4, 6);
        e.set_selection_style(Style::default().bg(Color::Rgb(1, 2, 3)));
        // Five columns per row: "abcde" / "fghij".
        let b = draw(&e, 5, 3);
        assert_eq!(row(&b, 0), "abcde");
        assert_eq!(row(&b, 1), "fghij");
        assert!(has(&b, 1, 0, Modifier::UNDERLINED) && has(&b, 2, 0, Modifier::UNDERLINED));
        assert!(!has(&b, 0, 0, Modifier::UNDERLINED));
        assert_eq!(b[(4, 0)].bg, Color::Rgb(1, 2, 3), "selection on 'e'");
        assert_eq!(b[(0, 1)].bg, Color::Rgb(1, 2, 3), "and on 'f'");
        assert_eq!(b[(1, 1)].bg, Color::Reset);
        assert_eq!(b[(2, 1)].fg, Color::Red);
        assert!(has(&b, 3, 1, Modifier::UNDERLINED));
    }

    #[test]
    fn soft_wrap_draws_the_bracket_match_whitespace_glyphs_and_extra_carets() {
        let mut e = bare("text", "(a b)\tz\n");
        e.set_soft_wrap(true);
        e.show_whitespace(true);
        e.set_whitespace_style(Style::default().fg(Color::Green));
        e.set_bracket_style(Style::default().bg(Color::Magenta));
        e.set_cursor_style(Some(Style::default().bg(Color::Yellow)));
        e.set_cursor(0);
        e.add_caret_at(3);
        let b = draw(&e, 12, 2);
        assert_eq!(row(&b, 0).trim_end(), "(a\u{b7}b)\u{2192}z");
        // Whitespace glyphs use the whitespace style.
        assert_eq!(b[(2, 0)].fg, Color::Green);
        assert_eq!(b[(5, 0)].fg, Color::Green);
        // The cursor was moved by add_caret_at, so the primary is at 3 and the
        // other caret at 0; both draw as cursor blocks; the match of the
        // primary's neighbour is highlighted.
        assert_eq!(b[(3, 0)].bg, Color::Yellow);
        assert_eq!(b[(0, 0)].bg, Color::Yellow);
    }

    #[test]
    fn inlay_hints_are_inserted_dimmed_and_shift_what_follows() {
        let mut e = bare("text", "ab");
        e.set_whitespace_style(Style::default().fg(Color::Cyan));
        e.set_inlay_hints(vec![(0, 1, ":T".to_string()), (0, 2, "!".to_string())]);
        let b = draw(&e, 10, 1);
        assert_eq!(row(&b, 0).trim_end(), "a:Tb!");
        assert_eq!(b[(1, 0)].fg, Color::Cyan);
        assert_eq!(b[(2, 0)].fg, Color::Cyan);
        assert_eq!(b[(4, 0)].fg, Color::Cyan, "a hint at the line end");
        assert_ne!(b[(3, 0)].fg, Color::Cyan, "the real glyph after the hint");
        // A caret after the hint lands after its width; selections shift too.
        e.set_cursor_style(Some(Style::default().bg(Color::Red)));
        e.set_selection_range(2, 1);
        e.set_selection_style(Style::default().bg(Color::Blue));
        let b = draw(&e, 10, 1);
        assert_eq!(
            b[(3, 0)].bg,
            Color::Red,
            "cursor on 'b' shifted by the hint"
        );
        // Scrolled horizontally, hints are not drawn.
        e.set_offset_x(1);
        let b = draw(&e, 10, 1);
        assert_eq!(row(&b, 0).trim_end(), "b");
    }

    #[test]
    fn inlay_hints_shift_syntax_highlighting() {
        let mut e = bare("rust", "let a = 1;");
        e.set_syntax_theme(&[("keyword", "ff0000")]);
        e.set_inlay_hints(vec![(0, 3, "XX".to_string())]);
        let b = draw(&e, 20, 1);
        // `let` stays red; the hint (inserted at column 3) is not.
        assert_eq!(b[(0, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(b[(2, 0)].fg, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn eol_notes_draw_dim_italic_after_the_line_and_truncate() {
        let mut e = bare("text", "abc\ndef");
        e.set_eol_note(Some((0, "blame here".to_string())));
        let b = draw(&e, 20, 2);
        assert_eq!(row(&b, 0).trim_end(), "abc  blame here");
        assert!(has(&b, 5, 0, Modifier::ITALIC));
        assert_eq!(row(&b, 1).trim_end(), "def", "only the noted line");
        let b = draw(&e, 9, 2);
        assert_eq!(row(&b, 0), "abc  blam");
        // An empty note draws nothing, and a note for a missing line neither.
        e.set_eol_note(Some((0, String::new())));
        assert_eq!(row(&draw(&e, 20, 1), 0).trim_end(), "abc");
        e.set_eol_note(Some((9, "x".to_string())));
        assert_eq!(row(&draw(&e, 20, 1), 0).trim_end(), "abc");
        // Not drawn when scrolled horizontally.
        e.set_eol_note(Some((0, "note".to_string())));
        e.set_offset_x(1);
        assert_eq!(row(&draw(&e, 20, 1), 0).trim_end(), "bc");
    }

    #[test]
    fn folded_lines_show_a_marker_and_an_ellipsis() {
        let mut e = Editor::new("text", "head\nbody\nmore\ntail\n", Vec::new()).unwrap();
        e.show_line_numbers(false);
        e.set_fold_ranges(vec![(0, 2)]);
        let b = draw(&e, 20, 3);
        assert!(
            row(&b, 0).trim_start().starts_with("\u{25be}head"),
            "open: {:?}",
            row(&b, 0)
        );
        assert!(e.fold(0));
        let b = draw(&e, 20, 3);
        assert!(
            row(&b, 0).trim_start().starts_with("\u{25b8}head..."),
            "{:?}",
            row(&b, 0)
        );
        assert!(row(&b, 1).trim_start().starts_with("tail"));
        // The ellipsis is not drawn when the line is scrolled sideways.
        e.set_offset_x(1);
        assert!(!row(&draw(&e, 20, 3), 0).contains("..."));
    }

    #[test]
    fn rainbow_brackets_recolour_by_depth() {
        let mut e = bare("text", "([{}])");
        e.set_rainbow_brackets(true);
        e.set_cursor(3); // keep the bracket-match highlight off the checks
        let b = draw(&e, 10, 1);
        let gold = Color::Rgb(0xff, 0xd7, 0x00);
        let orchid = Color::Rgb(0xda, 0x70, 0xd6);
        let blue = Color::Rgb(0x6a, 0x9e, 0xff);
        assert_eq!(b[(0, 0)].fg, gold);
        assert_eq!(b[(1, 0)].fg, orchid);
        assert_eq!(b[(2, 0)].fg, blue);
        assert_eq!(b[(3, 0)].fg, blue, "a closer takes its opener's colour");
        assert_eq!(b[(4, 0)].fg, orchid);
        assert_eq!(b[(5, 0)].fg, gold);
        // Depth is seeded from the text above the first visible line.
        let mut e = bare("text", "(\n(\n)\n)");
        e.set_rainbow_brackets(true);
        e.set_offset_y(1);
        e.set_cursor(7);
        let b = draw(&e, 10, 3);
        assert_eq!(b[(0, 0)].fg, orchid, "second-level opener");
        assert_eq!(b[(0, 1)].fg, orchid);
        assert_eq!(b[(0, 2)].fg, gold);
        // The depth palette wraps after six levels.
        let mut e = bare("text", "((((((()");
        e.set_rainbow_brackets(true);
        e.set_cursor(0);
        e.set_cursor(8);
        let b = draw(&e, 10, 1);
        assert_eq!(b[(6, 0)].fg, gold, "seventh level reuses the first colour");
        // Horizontally scrolled: brackets left of the view are skipped.
        e.set_offset_x(7);
        let b = draw(&e, 10, 1);
        assert_eq!(row(&b, 0).trim_end(), ")");
    }

    #[test]
    fn indent_guides_mark_each_space_indent_level() {
        let mut e = bare("rust", "        x\n    y\nz");
        e.set_whitespace_style(Style::default().fg(Color::Cyan));
        let b = draw(&e, 20, 3);
        assert_eq!(b[(0, 0)].symbol(), "\u{2502}");
        assert_eq!(b[(4, 0)].symbol(), "\u{2502}");
        assert_eq!(b[(0, 1)].symbol(), "\u{2502}");
        assert_ne!(b[(4, 1)].symbol(), "\u{2502}");
        assert_eq!(b[(0, 0)].fg, Color::Cyan);
        assert_ne!(b[(0, 2)].symbol(), "\u{2502}");
        // Tab-indented buffers get no guides; a scroll hides the left ones.
        let mut e = bare("go", "\t\tx");
        e.set_whitespace_style(Style::default());
        assert!(!row(&draw(&e, 10, 1), 0).contains('\u{2502}'));
        let mut e = bare("rust", "        x");
        e.set_offset_x(1);
        let b = draw(&e, 10, 1);
        assert_eq!(b[(3, 0)].symbol(), "\u{2502}", "the level at column 4");
        // An indent unit that is empty draws no guides.
        let mut e = bare("rust", "    x");
        e.set_indent(Some(String::new()));
        assert!(!row(&draw(&e, 10, 1), 0).contains('\u{2502}'));
    }

    #[test]
    fn gutter_signs_follow_priority_and_relative_numbers() {
        let mut e = Editor::new("text", "a\nb\nc\nd\ne\n", Vec::new()).unwrap();
        e.set_debug_line(Some(0));
        e.set_breakpoints(vec![0, 1]);
        e.set_gutter_marks(vec![(2, "#00ff00")]);
        let b = draw(&e, 20, 5);
        let sign = |y: u16| b[(5, y)].symbol().to_string();
        assert_eq!(sign(0), "\u{25b6}", "the stopped line beats a breakpoint");
        assert_eq!(sign(1), "\u{25cf}");
        assert_eq!(sign(2), "\u{258e}");
        assert_eq!(b[(5, 0)].fg, Color::Yellow);
        assert_eq!(b[(5, 1)].fg, Color::Red);
        assert_eq!(b[(5, 2)].fg, Color::Rgb(0, 255, 0));
        // Relative numbers: the cursor line keeps its absolute number.
        e.set_relative_line_numbers(true);
        e.set_cursor_line(2);
        let b = draw(&e, 20, 5);
        let num = |y: u16| {
            row(&b, y)
                .chars()
                .take(5)
                .collect::<String>()
                .trim()
                .to_string()
        };
        assert_eq!(
            (num(0), num(1), num(2), num(3)),
            ("2".into(), "1".into(), "3".into(), "1".into())
        );
        // A sign column past the right edge is not drawn.
        let b = draw(&e, 5, 3);
        assert_eq!(row(&b, 0).chars().count(), 5);
    }

    #[test]
    fn stale_ranges_and_far_selections_never_panic() {
        let mut e = bare("text", "abc");
        e.set_marks(vec![(0, 99, "#ff0000"), (2, 1, "#ff0000")]);
        e.set_spell_marks(vec![(0, 99)]);
        e.set_word_marks(vec![(0, 99)]);
        e.set_selection(Some(crate::selection::Selection { start: 1, end: 99 }));
        e.set_semantic_tokens(vec![(0, 99, "keyword".into()), (2, 1, "keyword".into())]);
        for wrap in [false, true] {
            e.set_soft_wrap(wrap);
            let b = draw(&e, 10, 2);
            assert_eq!(row(&b, 0).trim_end(), "abc");
        }
    }

    #[test]
    fn highlight_range_clips_to_the_view_and_scrolls_with_offsets() {
        let mut e = bare("text", "abcdefghijklmnopqrstuvwxyz");
        e.set_selection_range(2, 24);
        e.set_selection_style(Style::default().bg(Color::Blue));
        e.set_offset_x(10);
        let b = draw(&e, 8, 1);
        // Columns 10..18 of the line are visible; the whole of them selected.
        assert_eq!(row(&b, 0), "klmnopqr");
        assert!((0..8).all(|x| b[(x, 0)].bg == Color::Blue));
        // A caret scrolled out of view is not drawn.
        e.set_cursor_style(Some(Style::default().bg(Color::Red)));
        e.set_cursor(1);
        let b = draw(&e, 8, 1);
        assert!((0..8).all(|x| b[(x, 0)].bg != Color::Red));
    }

    #[test]
    fn bracket_partner_is_highlighted_beside_the_cursor() {
        let mut e = bare("text", "a(b)c");
        e.set_bracket_style(Style::default().bg(Color::Magenta));
        e.set_cursor(1);
        let b = draw(&e, 10, 1);
        assert_eq!(b[(3, 0)].bg, Color::Magenta);
        assert_ne!(b[(1, 0)].bg, Color::Magenta);
    }

    #[test]
    fn whitespace_mode_marks_tabs_spaces_and_line_ends() {
        let mut e = bare("text", "a b\tc\nd\r\ne");
        e.show_whitespace(true);
        e.set_whitespace_style(Style::default().fg(Color::Green));
        let b = draw(&e, 12, 3);
        assert_eq!(row(&b, 0).trim_end(), "a\u{b7}b\u{2192}c\u{b6}");
        assert_eq!(b[(1, 0)].fg, Color::Green);
        assert_eq!(b[(3, 0)].fg, Color::Green);
        assert_eq!(b[(5, 0)].symbol(), "\u{b6}");
        assert_eq!(row(&b, 1).trim_end(), "d\u{240d}\u{b6}");
        assert_eq!(row(&b, 2).trim_end(), "e", "no ¶ after the last line");
    }

    #[test]
    fn every_layer_clips_at_the_right_edge_of_any_width() {
        // Rendering with all layers on into views of every width must clip, not
        // overflow: each narrow row is a prefix of the same row drawn wide.
        let src = "    fn f(a: (u8, [u8; 2])) { // note\n\t\tx = \"hi\"; \u{4e2d}\u{6587}\n";
        let build = || {
            let mut e = Editor::new("rust", src, Vec::new()).unwrap();
            e.set_syntax_theme(&[("keyword", "ff0000"), ("string", "00ff00")]);
            e.show_whitespace(true);
            e.set_rainbow_brackets(true);
            e.set_eol_note(Some((0, "blame".to_string())));
            e.set_marks(vec![(4, 8, "#ff0000")]);
            e.set_word_marks(vec![(9, 10)]);
            e.set_spell_marks(vec![(12, 15)]);
            e.set_diagnostic_marks(vec![(16, 20, Color::Red)]);
            e.set_selection_range(2, 40);
            e.set_cursor_style(Some(Style::default().bg(Color::Yellow)));
            e.set_cursor(6);
            e.set_gutter_marks(vec![(0, "#00ff00")]);
            e.set_breakpoints(vec![1]);
            e.set_fold_ranges(vec![(0, 1)]);
            e
        };
        let wide_ed = build();
        let wide = draw(&wide_ed, 80, 3);
        for w in 1..40_u16 {
            let narrow = draw(&build(), w, 3);
            for y in 0..2 {
                // A two-column glyph that does not fit in the last cell is
                // left out rather than half drawn, so compare all but it.
                let keep = usize::from(w).saturating_sub(1);
                let a: String = row(&narrow, y).chars().take(keep).collect();
                let b: String = row(&wide, y).chars().take(keep).collect();
                assert_eq!(a, b, "width {w}, row {y}");
            }
        }
        // And with the first line folded and horizontal scroll applied.
        let mut e = build();
        assert!(e.fold(0));
        e.set_offset_x(3);
        for w in 1..30_u16 {
            let b = draw(&e, w, 3);
            assert_eq!(row(&b, 0).chars().count(), usize::from(w));
        }
    }

    #[test]
    fn zero_height_and_one_cell_areas_draw_nothing_and_do_not_panic() {
        let mut e = Editor::new("text", "abc", Vec::new()).unwrap();
        e.set_selection_range(0, 2);
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 0),
            Rect::new(0, 0, 1, 1),
        ] {
            let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
            e.render(area, &mut buf);
        }
    }
}
