//! Code lenses (T726): one dimmed, virtual row drawn *above* a line, holding
//! that line's lens titles (`▸ Run | 3 references`). A lens row is not buffer
//! text: the cursor, selections, line numbers and every buffer coordinate are
//! unchanged; only the mapping between logical lines and screen rows grows a
//! row per lensed, visible line. That mapping lives here ([`Editor::view_rows`]
//! for the classic no-wrap view, `wrap.rs`'s `visual_rows` for soft wrap) so
//! every render layer, the cursor scroll and mouse hit-testing agree on it.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;

use crate::editor::{Editor, Flags};

/// One screen row of the no-wrap view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewRow {
    /// The virtual lens row above logical line `.0`.
    Lens(usize),
    /// Logical line `.0`'s own text.
    Text(usize),
}

impl Editor {
    /// Set the code lenses to draw: `(line, label)`, 0-based lines. Labels on
    /// the same line are joined with ` | ` into one row; the list is kept
    /// sorted by line. An empty list removes every lens row.
    pub fn set_code_lenses(&mut self, lenses: Vec<(usize, String)>) {
        let mut lenses = lenses;
        lenses.sort_by_key(|l| l.0);
        let mut merged: Vec<(usize, String)> = Vec::with_capacity(lenses.len());
        for (line, label) in lenses {
            if label.is_empty() {
                continue;
            }
            match merged.last_mut() {
                Some((l, text)) if *l == line => {
                    text.push_str(" | ");
                    text.push_str(&label);
                }
                _ => merged.push((line, label)),
            }
        }
        self.code_lenses = merged;
    }

    /// Remove every code lens row.
    pub fn clear_code_lenses(&mut self) {
        self.code_lenses.clear();
    }

    /// Whether any code lens is set.
    #[must_use]
    pub fn has_code_lenses(&self) -> bool {
        !self.code_lenses.is_empty()
    }

    /// The lens label drawn above `line`, if any.
    #[must_use]
    pub fn code_lens_at(&self, line: usize) -> Option<&str> {
        let i = self.code_lenses.binary_search_by_key(&line, |l| l.0).ok()?;
        Some(self.code_lenses[i].1.as_str())
    }

    /// Whether a lens row is drawn above `line` (it has a lens and the line
    /// still exists: lenses can go stale after an edit until refreshed).
    pub(crate) fn lens_row_above(&self, line: usize) -> bool {
        line < self.code.len_lines() && self.code_lens_at(line).is_some()
    }

    /// The no-wrap layout: up to `max_rows` screen rows from the top of the
    /// view, skipping folded-away lines and inserting a lens row before each
    /// visible lensed line.
    pub(crate) fn view_rows(&self, max_rows: usize) -> Vec<ViewRow> {
        let total = self.code.len_lines();
        let mut rows = Vec::with_capacity(max_rows.min(total.saturating_mul(2)));
        let mut line = self.offset_y;
        while line < total && rows.len() < max_rows {
            if !self.is_line_hidden(line) {
                if self.lens_row_above(line) {
                    rows.push(ViewRow::Lens(line));
                    if rows.len() >= max_rows {
                        break;
                    }
                }
                rows.push(ViewRow::Text(line));
            }
            line += 1;
        }
        rows
    }

    /// `(screen row, line)` for each text row of the no-wrap layout, top to
    /// bottom, up to `max_rows` screen rows.
    pub(crate) fn text_rows(&self, max_rows: usize) -> Vec<(usize, usize)> {
        self.view_rows(max_rows)
            .into_iter()
            .enumerate()
            .filter_map(|(row, r)| match r {
                ViewRow::Text(line) => Some((row, line)),
                ViewRow::Lens(_) => None,
            })
            .collect()
    }

    /// In the no-wrap layout, how many screen rows below the top of the view
    /// `line`'s text sits (folds and lens rows counted), or `None` when the
    /// line is above the view or folded away. Unbounded: the caller compares
    /// it with the view height.
    pub(crate) fn rows_to_text(&self, line: usize) -> Option<usize> {
        if line < self.offset_y || line >= self.code.len_lines().max(1) {
            return None;
        }
        if !self.has_folds() && self.code_lenses.is_empty() {
            return Some(line - self.offset_y);
        }
        if self.is_line_hidden(line) {
            return None;
        }
        let rows = if self.has_folds() {
            (self.offset_y..line)
                .filter(|&l| !self.is_line_hidden(l))
                .map(|l| 1 + usize::from(self.lens_row_above(l)))
                .sum()
        } else {
            let lo = self.code_lenses.partition_point(|l| l.0 < self.offset_y);
            let hi = self.code_lenses.partition_point(|l| l.0 < line);
            (line - self.offset_y) + (hi - lo)
        };
        Some(rows + usize::from(self.lens_row_above(line)))
    }

    /// The row (0-based, within a view of `area`'s size) where `line`'s text
    /// is drawn, or `None` when it is scrolled out of view or folded away.
    /// Accounts for folds and lens rows; in soft-wrap mode it is the line's
    /// first visual row. Hosts use it to anchor overlays (popups, jump
    /// labels) to a line.
    #[must_use]
    pub fn text_screen_row(&self, line: usize, area: Rect) -> Option<usize> {
        let height = area.height as usize;
        if self.flags.contains(Flags::SOFT_WRAP) {
            let text_width = (area.width as usize).saturating_sub(self.get_line_number_width());
            return self
                .visual_rows(text_width, height)
                .iter()
                .position(|r| !r.lens && r.line == line);
        }
        self.rows_to_text(line).filter(|&r| r < height)
    }

    /// Draw the lens row above `line` at screen row `y`: the label, dimmed
    /// like inlay hints, indented to the line's own indentation and clipped
    /// to the view. The gutter stays blank.
    pub(crate) fn draw_lens_row(
        &self,
        buf: &mut Buffer,
        line: usize,
        text_x: u16,
        right: u16,
        y: u16,
    ) {
        let Some(label) = self.code_lens_at(line) else {
            return;
        };
        if text_x >= right {
            return;
        }
        let start = self.code.line_to_char(line);
        let len = self.code.line_len(line);
        let indent: usize = self
            .code
            .char_slice(start, start + len)
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let indent = indent.saturating_sub(self.offset_x);
        let x = text_x.saturating_add(u16::try_from(indent).unwrap_or(u16::MAX));
        if x >= right {
            return;
        }
        let width = usize::from(right - x);
        buf.set_stringn(x, y, label, width, self.whitespace_style);
    }
}

#[cfg(test)]
mod tests {
    use super::ViewRow;
    use crate::editor::Editor;
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::widgets::Widget;

    fn editor(text: &str) -> Editor {
        let mut ed = Editor::new("text", text, Vec::new()).unwrap();
        ed.show_line_numbers(false);
        ed.set_left_code_padding(0);
        ed
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn same_line_lenses_merge_and_sort() {
        let mut ed = editor("a\nb\nc\n");
        ed.set_code_lenses(vec![
            (2, "Run".into()),
            (0, "1 reference".into()),
            (2, "Debug".into()),
        ]);
        assert_eq!(ed.code_lens_at(0), Some("1 reference"));
        assert_eq!(ed.code_lens_at(1), None);
        assert_eq!(ed.code_lens_at(2), Some("Run | Debug"));
    }

    #[test]
    fn layout_inserts_a_lens_row_before_its_line() {
        let mut ed = editor("a\nb\nc\nd");
        ed.set_code_lenses(vec![(1, "Run".into())]);
        assert_eq!(
            ed.view_rows(10),
            vec![
                ViewRow::Text(0),
                ViewRow::Lens(1),
                ViewRow::Text(1),
                ViewRow::Text(2),
                ViewRow::Text(3),
            ]
        );
        assert_eq!(ed.rows_to_text(0), Some(0));
        assert_eq!(ed.rows_to_text(1), Some(2), "below its lens row");
        assert_eq!(ed.rows_to_text(3), Some(4));
        // The view is cut at max_rows, lens rows included.
        assert_eq!(ed.view_rows(2), vec![ViewRow::Text(0), ViewRow::Lens(1)]);
    }

    #[test]
    fn layout_skips_folded_lines_and_their_lenses() {
        let mut ed = editor("a\nb\nc\nd\ne");
        ed.set_fold_ranges(vec![(0, 2)]);
        assert!(ed.fold(0));
        ed.set_code_lenses(vec![(1, "hidden".into()), (3, "Run".into())]);
        assert_eq!(
            ed.view_rows(10),
            vec![
                ViewRow::Text(0),
                ViewRow::Lens(3),
                ViewRow::Text(3),
                ViewRow::Text(4),
            ]
        );
        assert_eq!(ed.rows_to_text(1), None, "folded away");
        assert_eq!(ed.rows_to_text(3), Some(2));
        assert_eq!(ed.rows_to_text(4), Some(3));
    }

    #[test]
    fn stale_lens_past_the_end_is_ignored() {
        let mut ed = editor("a\nb");
        ed.set_code_lenses(vec![(9, "gone".into())]);
        assert_eq!(ed.view_rows(10), vec![ViewRow::Text(0), ViewRow::Text(1)]);
        assert_eq!(ed.rows_to_text(1), Some(1));
    }

    #[test]
    fn lens_row_renders_above_the_line_indented_and_text_shifts_down() {
        let mut ed = editor("fn a() {}\n    fn b() {}\n");
        ed.set_code_lenses(vec![(1, "Run".into())]);
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert_eq!(row(&buf, 0), "fn a() {}");
        assert_eq!(row(&buf, 1), "    Run", "indented like its line");
        // (Indent guides may draw in the leading spaces.)
        assert!(row(&buf, 2).ends_with("    fn b() {}".trim_start()));
        assert_eq!(buf[(4, 2)].symbol(), "f", "the text keeps its column");
    }

    #[test]
    fn cursor_and_mouse_map_through_the_lens_row() {
        let mut ed = editor("a\nb\nc");
        ed.set_code_lenses(vec![(1, "Run".into())]);
        let area = Rect::new(0, 0, 10, 5);
        ed.set_cursor(2); // line 1, "b"
        assert_eq!(ed.get_visible_cursor(&area), Some((0, 2)));
        // Clicking line 2's text row (screen row 3) lands on "c".
        assert_eq!(ed.cursor_from_mouse(0, 3, &area), Some(4));
        // Clicking the lens row lands on the start of the line it annotates.
        assert_eq!(ed.cursor_from_mouse(0, 1, &area), Some(2));
        assert_eq!(ed.text_screen_row(1, area), Some(2));
    }

    #[test]
    fn focus_scrolls_far_enough_for_lens_rows() {
        let text = "l\n".repeat(10);
        let mut ed = editor(&text);
        ed.set_code_lenses((0..10).map(|l| (l, "lens".to_string())).collect());
        let area = Rect::new(0, 0, 10, 4);
        ed.set_cursor(2 * 3); // line 3
        ed.focus(&area);
        let r = ed.rows_to_text(3).unwrap();
        assert!(r < 4, "line 3's text row ({r}) is inside the 4-row view");
    }

    #[test]
    fn wheel_scroll_reaches_the_last_line_below_lens_rows() {
        let mut ed = editor("a\nb\nc\nd");
        ed.set_code_lenses((0..4).map(|l| (l, "lens".to_string())).collect());
        // 8 rows of content in a 4-row view: scrolling must be possible.
        ed.scroll_down(4);
        assert_eq!(ed.get_offset_y(), 1);
        ed.scroll_down(4);
        ed.scroll_down(4);
        ed.scroll_down(4);
        assert!(
            ed.rows_to_text(3).is_some_and(|r| r < 4),
            "last line in view"
        );
        let at = ed.get_offset_y();
        ed.scroll_down(4);
        assert_eq!(ed.get_offset_y(), at, "and no further");
    }

    #[test]
    fn soft_wrap_draws_the_lens_row_too() {
        let mut ed = editor("abcdefgh\nxy");
        ed.set_soft_wrap(true);
        ed.set_code_lenses(vec![(1, "Run".into())]);
        let area = Rect::new(0, 0, 4, 5);
        let mut buf = Buffer::empty(area);
        (&ed).render(area, &mut buf);
        assert_eq!(row(&buf, 0), "abcd");
        assert_eq!(row(&buf, 1), "efgh");
        assert_eq!(row(&buf, 2), "Run");
        assert_eq!(row(&buf, 3), "xy");
        assert_eq!(ed.text_screen_row(1, area), Some(3));
        // Clicking the lens row lands on its line's start.
        assert_eq!(ed.cursor_from_mouse(0, 2, &area), Some(9));
    }
}
