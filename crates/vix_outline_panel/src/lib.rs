//! The code-outline list (symbol kind + name + line) and the panel's
//! selection/scroll state.
//!
//! Pure data. The host scans the active buffer for declarations, builds an
//! [`Outline`] of [`Entry`] rows, renders the list, and jumps to a chosen
//! symbol's line. On open it can select the symbol nearest the cursor with
//! [`Outline::select_nearest`].

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One outline row: a symbol's kind, name, and 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Structural keyword (`fn`, `struct`, `mod`, `impl`, …); may be empty.
    pub kind: String,
    /// The symbol's identifier.
    pub name: String,
    /// 1-based line of the declaration.
    pub line: usize,
}

/// The outline list plus selection and scroll offset.
pub struct Outline {
    /// Symbol rows in source order.
    pub entries: Vec<Entry>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Outline::ensure_visible`].
    pub scroll: usize,
}

impl Outline {
    /// Build an outline from `entries`, selecting the first row.
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Outline { entries, selected: 0, scroll: 0 }
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the outline is empty (no symbols found).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + page.max(1)).min(self.entries.len() - 1);
        }
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.entries.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Select the last symbol at or before `cursor_line` (1-based) — the symbol
    /// the cursor is currently inside. No-op when the outline is empty.
    pub fn select_nearest(&mut self, cursor_line: usize) {
        if let Some(idx) = self.entries.iter().rposition(|e| e.line <= cursor_line) {
            self.selected = idx;
        }
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.entries.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The 1-based line of the highlighted symbol, if any.
    #[must_use]
    pub fn selected_line(&self) -> Option<usize> {
        self.entries.get(self.selected).map(|e| e.line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline() -> Outline {
        Outline::new(vec![
            Entry { kind: "struct".into(), name: "App".into(), line: 10 },
            Entry { kind: "fn".into(), name: "new".into(), line: 20 },
            Entry { kind: "fn".into(), name: "run".into(), line: 40 },
        ])
    }

    #[test]
    fn navigation_clamps_and_reports_line() {
        let mut o = outline();
        assert_eq!(o.selected_line(), Some(10));
        o.up();
        assert_eq!(o.selected, 0);
        o.page_down(100);
        assert_eq!(o.selected, 2);
        assert_eq!(o.selected_line(), Some(40));
        o.down();
        assert_eq!(o.selected, 2);
    }

    #[test]
    fn select_nearest_picks_the_enclosing_symbol() {
        let mut o = outline();
        o.select_nearest(25); // inside fn new (20..40)
        assert_eq!(o.selected, 1);
        o.select_nearest(5); // before the first symbol → unchanged
        assert_eq!(o.selected, 1);
        o.select_nearest(100);
        assert_eq!(o.selected, 2);
    }

    #[test]
    fn select_index_guards_range() {
        let mut o = outline();
        assert!(o.select_index(2));
        assert!(!o.select_index(3));
    }

    #[test]
    fn ensure_visible_scrolls_to_selection() {
        let mut o = Outline::new((0..20).map(|i| Entry { kind: "fn".into(), name: format!("f{i}"), line: i + 1 }).collect());
        o.selected = 15;
        o.ensure_visible(5);
        assert!(o.scroll <= 15 && 15 < o.scroll + 5);
    }
}
