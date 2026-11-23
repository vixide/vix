//! Soft-wrap layout and rendering for the editor widget (Vix's own code, held to
//! the crate's `clippy::pedantic`). Long logical lines wrap across several screen
//! rows; a shared [`Editor::visual_rows`] layout drives the renderer here as well
//! as the cursor scroll (`Editor::focus`) and mouse hit-testing
//! (`cursor_from_mouse`).
//!
//! Wrapping is by **display columns** at grapheme boundaries (never splitting
//! a grapheme cluster), with tabs counted as one cell, matching how the
//! renderer draws them. It is a hard wrap at the width: there is no
//! word-boundary breaking. `offset_y` stays a *logical line* index in this
//! mode, so the view always starts at the top of a line (it cannot begin
//! mid-wrap). Enabled by [`Editor::set_soft_wrap`](crate::editor::Editor::set_soft_wrap);
//! the non-wrapped renderer lives in [`crate::render`].
//! See `crates/vix-editor-core/spec/index.md`.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};
use ropey::RopeSlice;

use crate::code::{RopeGraphemes, grapheme_width_and_bytes_len, grapheme_width_and_chars_len};
use crate::editor::{Editor, Flags};

/// One on-screen row in soft-wrap mode: a `[start, end)` character-offset slice
/// of a logical line (`line`), or (`lens`) the virtual code-lens row drawn
/// above that line (T726), which holds no text (`start == end ==` the line's
/// first char).
#[derive(Clone, Copy)]
pub(crate) struct VRow {
    /// Zero-based logical line this row belongs to.
    pub(crate) line: usize,
    /// First character offset (absolute, in the whole buffer) of the segment.
    pub(crate) start: usize,
    /// One past the last character offset of the segment (exclusive).
    pub(crate) end: usize,
    /// Whether this is the virtual code-lens row above `line`, not text.
    pub(crate) lens: bool,
}

/// Shared per-render context for drawing one visual row. Bundles the values that
/// are constant across a single `render_wrapped` call so the per-row helper takes
/// a manageable number of arguments.
#[derive(Clone, Copy)]
struct RowCtx<'a> {
    /// The whole widget rectangle being rendered into.
    area: Rect,
    /// Absolute x of the first text cell (just right of the gutter).
    text_x0: u16,
    /// Absolute x one past the last drawable cell (`area.right()`).
    right: u16,
    /// Absolute y of the row being drawn.
    draw_y: u16,
    /// Width of the line-number column in digits (at least 5, so the gutter
    /// does not jitter as the line count crosses a power of ten).
    line_number_digits: usize,
    /// Base style for plain text.
    default_text_style: Style,
    /// Every caret's non-empty selection as `(start, end)` char offsets.
    caret_sels: &'a [(usize, usize)],
    /// Char offset of the bracket matching the one under the cursor, if any.
    bracket: Option<usize>,
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

/// `a + b` for screen columns; a column past the top of `u16` is past the right edge.
fn add_u16(a: u16, b: u16) -> u16 {
    add_u16_or(a, b, u16::MAX)
}

/// `a + b`, or the declared `fallback` when the sum does not fit.
fn add_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_add(b).unwrap_or(fallback)
}

/// `a - b` for screen columns; below zero is zero columns.
fn sub_u16(a: u16, b: u16) -> u16 {
    sub_u16_or(a, b, 0)
}

/// `a - b`, or the declared `fallback` when it would go below zero.
fn sub_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_sub(b).unwrap_or(fallback)
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
        let text_x0 = add_u16(
            area.left(),
            u16::try_from(line_number_width).unwrap_or(u16::MAX),
        );
        let right = area.right();
        let text_width = sub_or(usize::from(area.width), line_number_width, 0);
        let height = usize::from(area.height);
        let default_text_style = self.text_style;

        // One layout pass shared by every row; rows beyond `height` are never
        // computed.
        let rows = self.visual_rows(text_width, height);
        let bracket = self.matching_bracket();

        // Every caret's selection range (multiple-cursor aware).
        let caret_sels = self.caret_selections();

        for (i, vr) in rows.iter().enumerate() {
            let draw_y = add_u16(area.top(), u16::try_from(i).unwrap_or(u16::MAX));
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
        let RowCtx {
            area,
            text_x0,
            right,
            draw_y,
            line_number_digits,
            default_text_style,
            ..
        } = *ctx;

        if vr.lens {
            self.draw_lens_row(buf, vr.line, text_x0, right, draw_y);
            return;
        }
        let line_start = code.line_to_char(vr.line);
        let line_len = code.line_len(vr.line);
        let is_first = vr.start == line_start;

        if self.flags.contains(Flags::SHOW_LINE_NUMBERS) {
            let s = if is_first {
                format!("{:>line_number_digits$}", add_or(vr.line, 1, usize::MAX))
            } else {
                " ".repeat(line_number_digits)
            };
            buf.set_string(area.left(), draw_y, &s, self.line_number_style);
        }

        // The gutter sign (debugger, comment thread, diff/coverage bar) on the
        // first visual row of a logical line, by the same renderer and
        // priority as the non-wrapped view; continuation rows have none, so a
        // wrapped line is marked once, at its start.
        if is_first {
            let sign_x = if self.flags.contains(Flags::SHOW_LINE_NUMBERS) {
                add_u16(
                    area.left(),
                    u16::try_from(line_number_digits).unwrap_or(u16::MAX),
                )
            } else {
                area.left()
            };
            self.draw_gutter_sign(buf, sign_x, draw_y, vr.line, right);
            // The fold chevron (▸/▾), in the same column as unwrapped.
            self.draw_fold_marker(buf, vr.line, area.left(), text_x0, right, draw_y);
        }

        // Base text for the segment (with whitespace glyphs if enabled). Tabs
        // are drawn as one cell either way, matching `gwidth` in the layout.
        let seg = code.char_slice(vr.start, vr.end);
        let displayed: String = if self.flags.contains(Flags::SHOW_WHITESPACE) {
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
        let highlights = if code.has_highlighter() {
            let end_byte = code.char_to_byte(vr.end);
            self.highlight_interval(start_byte, end_byte, &self.theme)
        } else {
            Vec::new()
        };

        let (mut caret, vx) =
            self.paint_row_graphemes(buf, &seg, ctx, &highlights, vr.start, start_byte);

        // Caret at the segment end is shown only at the logical line's end;
        // at a wrap boundary it belongs to the next visual row's column 0.
        let line_end = add_or(line_start, line_len, usize::MAX);
        if caret.is_none() && self.cursor == vr.end && vr.end == line_end {
            caret = Some(vx);
        }
        if caret.is_none() && vr.start == vr.end && self.cursor == vr.start {
            caret = Some(0);
        }
        // End-of-line virtual text (T614) after the line's last visual row,
        // and a folded line's `...` before it.
        if vr.end == line_end {
            let mut end = add_or(usize::from(text_x0), usize::from(vx), usize::MAX);
            if self.fold_marker(vr.line) == Some(true) {
                let x = u16::try_from(end).unwrap_or(u16::MAX);
                if x < right {
                    let shown: String =
                        "...".chars().take(usize::from(sub_u16(right, x))).collect();
                    buf.set_string(x, draw_y, &shown, default_text_style);
                }
                end = add_or(end, 3, usize::MAX);
            }
            self.draw_eol_texts(buf, vr.line, end, right, draw_y);
        }
        if let (Some(cvx), Some(cursor_style)) = (caret, self.cursor_style) {
            let cx = add_u16(text_x0, cvx);
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
        let RowCtx {
            area,
            text_x0,
            right,
            draw_y,
            caret_sels,
            bracket,
            ..
        } = *ctx;
        let mut vx: u16 = 0;
        let mut ch = start_ch;
        let mut byte = start_byte;
        let mut caret: Option<u16> = None;
        for g in RopeGraphemes::new(seg) {
            let (w, g_chars) = grapheme_width_and_chars_len(g);
            let (_, g_bytes) = grapheme_width_and_bytes_len(g);
            let gw = if g.chars().next() == Some('\t') {
                1
            } else {
                u16::try_from(w).unwrap_or(u16::MAX)
            };
            let cell_x = add_u16(text_x0, vx);

            if self.flags.contains(Flags::SHOW_WHITESPACE)
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
                    let cx = add_u16(text_x0, vx);
                    if cx < right && draw_y < area.bottom() {
                        buf[(cx, draw_y)].set_style(cs);
                    }
                }
            }

            vx = add_u16(vx, gw);
            ch = add_or(ch, g_chars, usize::MAX);
            byte = add_or(byte, g_bytes, usize::MAX);
        }
        (caret, vx)
    }

    /// Visible visual rows (soft-wrap segments) starting at `offset_y`, up to
    /// `max_rows`. Each row holds at most `text_width` display columns of one
    /// logical line; a line with a code lens (T726) is preceded by its lens row.
    pub(crate) fn visual_rows(&self, text_width: usize, max_rows: usize) -> Vec<VRow> {
        let code = self.code_ref();
        let total_lines = code.len_lines();
        // A zero-width view would never make progress below; wrap at 1 column.
        let tw = text_width.max(1);
        let mut rows = Vec::new();
        let mut line = self.offset_y;
        while line < total_lines && rows.len() < max_rows {
            // A folded range's body takes no rows, as in the unwrapped view.
            if self.is_line_hidden(line) {
                line = add_or(line, 1, usize::MAX);
                continue;
            }
            let line_start = code.line_to_char(line);
            let line_len = code.line_len(line);
            if self.lens_row_above(line) {
                rows.push(VRow {
                    line,
                    start: line_start,
                    end: line_start,
                    lens: true,
                });
                if rows.len() >= max_rows {
                    break;
                }
            }
            // An empty line still occupies one row (so its number and caret show).
            if line_len == 0 {
                rows.push(VRow {
                    line,
                    start: line_start,
                    end: line_start,
                    lens: false,
                });
                line = add_or(line, 1, usize::MAX);
                continue;
            }
            let mut col = 0usize;
            while col < line_len && rows.len() < max_rows {
                let slice = code.char_slice(
                    add_or(line_start, col, usize::MAX),
                    add_or(line_start, line_len, usize::MAX),
                );
                let mut width = 0usize;
                let mut seg_chars = 0usize;
                for g in RopeGraphemes::new(&slice) {
                    let g_chars = grapheme_width_and_chars_len(g).1;
                    let gw = gwidth(g);
                    // `seg_chars > 0` guarantees progress: a single grapheme
                    // wider than the view still gets a row of its own.
                    if add_or(width, gw, usize::MAX) > tw && seg_chars > 0 {
                        break;
                    }
                    width = add_or(width, gw, usize::MAX);
                    seg_chars = add_or(seg_chars, g_chars, usize::MAX);
                    if width >= tw {
                        break;
                    }
                }
                // Defensive: never emit an empty segment (infinite loop).
                if seg_chars == 0 {
                    seg_chars = 1;
                }
                let seg_end = add_or(col, seg_chars, usize::MAX).min(line_len);
                rows.push(VRow {
                    line,
                    start: add_or(line_start, col, usize::MAX),
                    end: add_or(line_start, seg_end, usize::MAX),
                    lens: false,
                });
                col = seg_end;
            }
            line = add_or(line, 1, usize::MAX);
        }
        rows
    }
}

/// Apply `style` to the `width` cells starting at `x` on row `y`, clipped to
/// `right` (so a wide grapheme at the edge is styled only as far as it fits).
fn paint(buf: &mut Buffer, x: u16, width: u16, right: u16, y: u16, style: Style) {
    for dx in 0..width {
        let cx = add_u16(x, dx);
        if cx < right {
            buf[(cx, y)].set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    /// Up and Down step over a folded range's hidden lines instead of
    /// putting the caret inside it.
    #[test]
    fn vertical_moves_skip_folded_lines() {
        use crate::actions::{MoveDown, MoveUp};
        let mut ed = Editor::new("text", "head\nbody 1\nbody 2\nafter\n", Vec::new()).unwrap();
        ed.set_fold_ranges(vec![(0, 2)]);
        assert!(ed.fold(0));
        ed.set_cursor(0);
        ed.apply(MoveDown { shift: false });
        assert_eq!(ed.cursor_line(), 3, "Down lands on the line after the fold");
        ed.apply(MoveUp { shift: false });
        assert_eq!(ed.cursor_line(), 0, "Up lands on the fold's header");
    }

    /// With soft wrap on, a folded range's body takes no rows and its header
    /// shows `...`, as in the unwrapped view.
    #[test]
    fn soft_wrap_hides_folded_lines_and_marks_the_fold() {
        let mut ed = Editor::new("text", "head\nbody 1\nbody 2\nafter\n", Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_soft_wrap(true);
        ed.set_fold_ranges(vec![(0, 2)]);
        assert!(ed.fold(0));
        let lines: Vec<usize> = ed.visual_rows(20, 10).iter().map(|r| r.line).collect();
        assert_eq!(lines, vec![0, 3, 4], "lines 1 and 2 are folded away");
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        let row = |y: u16| (0..20).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(
            row(0).trim_start().starts_with("\u{25b8}head..."),
            "{:?}",
            row(0)
        );
        assert!(row(1).trim_start().starts_with("after"), "{:?}", row(1));

        // Unfolded, the header shows ▾, on its first visual row only, with
        // line numbers on too.
        let mut ed_narrow = Editor::new(
            "text",
            "head line that wraps\nbody 1\nbody 2\nafter\n",
            Vec::new(),
        )
        .unwrap();
        ed_narrow.set_soft_wrap(true);
        ed_narrow.set_fold_ranges(vec![(0, 2)]);
        let area = Rect::new(0, 0, 14, 4);
        let mut buf = Buffer::empty(area);
        (&ed_narrow).render(area, &mut buf);
        let row = |y: u16| (0..14).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(row(0).contains("\u{25be}head"), "{:?}", row(0));
        assert!(!row(1).contains('\u{25be}'), "{:?}", row(1));
    }
}

#[cfg(test)]
mod wrap_layer_tests {
    //! Soft-wrap layout and per-row painting at the edges.

    use crate::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::style::{Color, Style};
    use ratatui_core::widgets::Widget;

    fn bare(lang: &str, text: &str) -> Editor {
        let mut e = Editor::new(lang, text, Vec::new()).unwrap();
        e.show_line_numbers(false);
        e.set_left_code_padding(0);
        e.set_soft_wrap(true);
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

    #[test]
    fn syntax_colours_follow_the_text_across_wrapped_rows() {
        let mut e = bare("rust", "let a = 1; let b = 2;");
        e.set_syntax_theme(&[("keyword", "ff0000")]);
        // Ten columns: "let a = 1;" / " let b = 2" / ";".
        let b = draw(&e, 10, 3);
        assert_eq!(row(&b, 0), "let a = 1;");
        assert_eq!(b[(0, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(b[(2, 0)].fg, Color::Rgb(255, 0, 0));
        assert_ne!(b[(4, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(b[(1, 1)].fg, Color::Rgb(255, 0, 0), "second `let`");
    }

    #[test]
    fn the_caret_block_follows_the_cursor_at_row_ends_and_on_empty_lines() {
        let red = Style::default().bg(Color::Red);
        let mut e = bare("text", "abcd\n\nef");
        e.set_cursor_style(Some(red));
        // Cursor at the end of a line that exactly fills the row: the block
        // sits just past the text only at the logical line's end...
        e.set_cursor(4);
        let b = draw(&e, 4, 4);
        assert_eq!(row(&b, 0), "abcd");
        assert!(
            (0..4).all(|x| b[(x, 0)].bg != Color::Red),
            "no cell fits it"
        );
        let b = draw(&e, 6, 4);
        assert_eq!(b[(4, 0)].bg, Color::Red, "end of line");
        // ...and an empty line shows it in column 0.
        e.set_cursor(5);
        let b = draw(&e, 6, 4);
        assert_eq!(b[(0, 1)].bg, Color::Red);
        // At a wrap boundary the caret belongs to the next row's column 0.
        let mut e = bare("text", "abcdef");
        e.set_cursor_style(Some(red));
        e.set_cursor(3);
        let b = draw(&e, 3, 3);
        assert_eq!(b[(0, 1)].bg, Color::Red);
        assert_ne!(b[(2, 0)].bg, Color::Red);
        // Extra carets paint inline, clipped to the view.
        e.add_caret_at(1);
        let b = draw(&e, 3, 3);
        assert_eq!(b[(1, 0)].bg, Color::Red);
        assert_eq!(b[(0, 1)].bg, Color::Red);
    }

    #[test]
    fn a_folded_line_ellipsis_is_clipped_at_the_right_edge() {
        let mut e = bare("text", "head\nbody\nmore\ntail\n");
        e.set_fold_ranges(vec![(0, 2)]);
        assert!(e.fold(0));
        let b = draw(&e, 6, 2);
        // No gutter, so there is no column for the chevron; the ellipsis is
        // cut off at the edge rather than overflowing.
        assert_eq!(row(&b, 0), "head..");
        let b = draw(&e, 8, 2);
        assert_eq!(row(&b, 0).trim_end(), "head...");
        assert!(row(&b, 1).starts_with("tail"));
    }

    #[test]
    fn a_grapheme_wider_than_the_view_gets_a_row_of_its_own() {
        let e = bare("text", "\u{4e2d}\u{6587}x");
        let rows = e.visual_rows(1, 10);
        let spans: Vec<(usize, usize)> = rows.iter().map(|r| (r.start, r.end)).collect();
        assert_eq!(spans, vec![(0, 1), (1, 2), (2, 3)]);
        // A zero-width view wraps at one column rather than looping forever.
        assert_eq!(e.visual_rows(0, 10).len(), 3);
        // Rows stop at the requested maximum.
        assert_eq!(e.visual_rows(1, 2).len(), 2);
        // Width two fits one wide character per row, plus the narrow tail.
        let spans: Vec<(usize, usize)> = e
            .visual_rows(2, 10)
            .iter()
            .map(|r| (r.start, r.end))
            .collect();
        assert_eq!(spans, vec![(0, 1), (1, 2), (2, 3)]);
        let b = draw(&e, 2, 3);
        assert_eq!(row(&b, 0).chars().next(), Some('\u{4e2d}'));
        assert_eq!(row(&b, 2).chars().next(), Some('x'));
    }

    #[test]
    fn a_wide_grapheme_at_the_edge_is_clipped_not_overflowed() {
        let mut e = bare("text", "a\u{4e2d}");
        e.set_selection_range(0, 2);
        e.set_selection_style(Style::default().bg(Color::Blue));
        // Two columns wide: "a" fits, the wide char wraps to the next row.
        let b = draw(&e, 2, 2);
        assert_eq!(row(&b, 0).chars().next(), Some('a'));
        assert_eq!(b[(0, 1)].bg, Color::Blue);
        // Three columns hold both.
        let b = draw(&e, 3, 1);
        assert_eq!(b[(1, 0)].bg, Color::Blue);
        assert_eq!(b[(2, 0)].bg, Color::Blue);
    }

    #[test]
    fn whitespace_glyphs_and_bracket_match_in_wrapped_rows() {
        let mut e = bare("text", "(a\tb)");
        e.show_whitespace(true);
        e.set_whitespace_style(Style::default().fg(Color::Green));
        e.set_bracket_style(Style::default().bg(Color::Magenta));
        e.set_cursor(0);
        let b = draw(&e, 8, 1);
        assert_eq!(row(&b, 0).trim_end(), "(a\u{2192}b)");
        assert_eq!(b[(2, 0)].fg, Color::Green);
        assert_eq!(
            b[(4, 0)].bg,
            Color::Magenta,
            "the partner of the cursor's `(`"
        );
    }

    #[test]
    fn rows_past_the_area_are_not_drawn_and_lens_rows_precede_their_line() {
        let mut e = bare("text", "one\ntwo\nthree\nfour");
        e.set_code_lenses(vec![(1, "refs".to_string())]);
        let b = draw(&e, 8, 3);
        assert_eq!(row(&b, 0).trim_end(), "one");
        assert_eq!(row(&b, 1).trim_end(), "refs");
        assert_eq!(row(&b, 2).trim_end(), "two");
        // The viewport is only as tall as the area: nothing is clipped wrongly.
        let b = draw(&e, 8, 1);
        assert_eq!(row(&b, 0).trim_end(), "one");
    }
}
