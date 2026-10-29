//! A small titled list of rows with a selection, for overlays that let you
//! pick one line: Local History snapshots, run configurations, a definition
//! peek.
//!
//! The panel owns only presentation state (rows, selection, scroll); what a
//! chosen row *means* is the host's business, so one overlay serves several
//! features. Titles, hints and rows arrive already translated.

/// A pick-one list overlay's state.
#[derive(Debug, Clone, Default)]
pub struct Panel {
    /// Already-translated title shown in the frame.
    pub title: String,
    /// Already-translated key hint shown under the rows.
    pub hint: String,
    /// The rows, display-ready.
    pub rows: Vec<String>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Panel {
    /// A list over `rows` with the first row highlighted.
    #[must_use]
    pub fn new(title: String, hint: String, rows: Vec<String>) -> Self {
        Self {
            title,
            hint,
            rows,
            selected: 0,
            scroll: 0,
        }
    }

    /// A list with row `selected` highlighted (clamped to the last row).
    #[must_use]
    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(self.rows.len().saturating_sub(1));
        self
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there are no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(n: usize) -> Panel {
        Panel::new(
            "t".into(),
            "h".into(),
            (0..n).map(|i| format!("row {i}")).collect(),
        )
    }

    #[test]
    fn navigation_stays_in_bounds() {
        let mut p = panel(3);
        p.up();
        assert_eq!(p.selected, 0);
        p.down();
        p.down();
        p.down();
        assert_eq!(p.selected, 2);
        p.page_up(10);
        assert_eq!(p.selected, 0);
        p.page_down(10);
        assert_eq!(p.selected, 2);
    }

    #[test]
    fn select_index_rejects_missing_rows() {
        let mut p = panel(2);
        assert!(p.select_index(1));
        assert_eq!(p.selected, 1);
        assert!(!p.select_index(5));
        assert_eq!(p.selected, 1);
    }

    #[test]
    fn with_selected_clamps_and_scroll_follows() {
        let mut p = panel(30).with_selected(99);
        assert_eq!(p.selected, 29);
        p.ensure_visible(5);
        assert!(p.scroll > 0 && p.scroll <= 29);
        assert!(panel(0).is_empty());
    }
}
