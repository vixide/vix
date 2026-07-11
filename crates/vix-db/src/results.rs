//! The query-results grid: a scrollable table with filtering and sorting.
//!
//! Holds the headers and rows of the last statement's output plus view state
//! (selected row and column, vertical scroll, first visible column for wide
//! result sets, a live substring filter typed after `/`, and a numeric-aware
//! sort cycled per column). Pure state — the UI sizes columns and draws; row
//! counts come from [`Grid::filtered`].

use std::cmp::Ordering;

/// The results table plus its view state.
#[derive(Debug, Clone, Default)]
pub struct Grid {
    /// Column headers.
    pub headers: Vec<String>,
    /// Data rows (each padded to the header count).
    pub rows: Vec<Vec<String>>,
    /// Selected index into [`Grid::filtered`].
    pub sel: usize,
    /// First visible filtered-row index.
    pub scroll: usize,
    /// First visible column (horizontal scroll).
    pub col_off: usize,
    /// The selected column (sort target, cell-viewer/yank source).
    pub cur_col: usize,
    /// Active sort: `(column, ascending)`. `None` keeps arrival order.
    pub sort: Option<(usize, bool)>,
    /// Case-insensitive substring the rows are filtered by.
    pub filter: String,
    /// Whether the filter box is capturing typed keys.
    pub filtering: bool,
}

impl Grid {
    /// Replace the table contents, resetting the view state.
    pub fn set(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        self.sel = 0;
        self.scroll = 0;
        self.col_off = 0;
        self.cur_col = 0;
        self.sort = None;
        self.filter.clear();
        self.filtering = false;
    }

    /// Append a streamed batch of rows, keeping the current selection, sort, and
    /// filter (M4 progressive loading after [`Grid::set`] laid down the header).
    pub fn append_rows(&mut self, mut rows: Vec<Vec<String>>) {
        self.rows.append(&mut rows);
    }

    /// Indices of the rows passing the filter, in sort order (arrival order
    /// when no sort is active; all rows when the filter is empty).
    #[must_use]
    pub fn filtered(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        let mut out: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                needle.is_empty()
                    || self.rows[i]
                        .iter()
                        .any(|cell| cell.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((col, asc)) = self.sort {
            out.sort_by(|&a, &b| {
                let (a, b) = (cell(&self.rows[a], col), cell(&self.rows[b], col));
                let ord = compare_cells(a, b);
                if asc { ord } else { ord.reverse() }
            });
        }
        out
    }

    /// The selected cell's content, if the grid has one.
    #[must_use]
    pub fn selected_cell(&self) -> Option<&str> {
        let ri = *self.filtered().get(self.sel)?;
        Some(cell(&self.rows[ri], self.cur_col))
    }

    /// The selected row, if the grid has one.
    #[must_use]
    pub fn selected_row(&self) -> Option<&[String]> {
        let ri = *self.filtered().get(self.sel)?;
        self.rows.get(ri).map(Vec::as_slice)
    }

    /// The underlying `rows` index of the selected row (stable across filtering
    /// and sorting), for keying staged cell edits.
    #[must_use]
    pub fn selected_index(&self) -> Option<usize> {
        self.filtered().get(self.sel).copied()
    }

    /// Move the column selection left or right, dragging the horizontal
    /// scroll along so the selection stays in view.
    pub fn select_col(&mut self, left: bool) {
        if left {
            self.cur_col = self.cur_col.saturating_sub(1);
        } else {
            self.cur_col = (self.cur_col + 1).min(self.headers.len().saturating_sub(1));
        }
        if self.cur_col < self.col_off {
            self.col_off = self.cur_col;
        } else if self.cur_col > self.col_off + 3 {
            self.col_off = self.cur_col - 3;
        }
    }

    /// Cycle the sort on the selected column: ascending → descending → off.
    /// Selecting a different column starts a fresh ascending sort.
    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            Some((col, true)) if col == self.cur_col => Some((col, false)),
            Some((col, false)) if col == self.cur_col => None,
            _ => Some((self.cur_col, true)),
        };
    }

    /// Move the selection `n` filtered rows up or down, clamped.
    pub fn step(&mut self, up: bool, n: usize) {
        let len = self.filtered().len();
        if up {
            self.sel = self.sel.saturating_sub(n);
        } else {
            self.sel = (self.sel + n).min(len.saturating_sub(1));
        }
    }

    /// Jump to the first or last filtered row.
    pub fn home_end(&mut self, home: bool) {
        self.sel = if home {
            0
        } else {
            self.filtered().len().saturating_sub(1)
        };
    }

    /// Append or erase one char of the live filter, clamping the selection.
    pub fn filter_key(&mut self, c: Option<char>) {
        match c {
            Some(ch) => self.filter.push(ch),
            None => {
                self.filter.pop();
            }
        }
        self.sel = self.sel.min(self.filtered().len().saturating_sub(1));
    }

    /// Keep the selection within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + height {
            self.scroll = self.sel + 1 - height;
        }
        let max_scroll = self.filtered().len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }
}

/// The cell at `col`, empty when the row is short.
fn cell(row: &[String], col: usize) -> &str {
    row.get(col).map_or("", String::as_str)
}

/// Numeric-aware cell ordering: two numbers compare numerically, everything
/// else lexicographically.
fn compare_cells(a: &str, b: &str) -> Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        let mut g = Grid::default();
        g.set(
            vec!["id".into(), "name".into()],
            vec![
                vec!["1".into(), "Ada Lovelace".into()],
                vec!["2".into(), "Grace Hopper".into()],
                vec!["3".into(), "Radia Perlman".into()],
            ],
        );
        g
    }

    #[test]
    fn filter_matches_any_cell_case_insensitively() {
        let mut g = grid();
        for c in "ada".chars() {
            g.filter_key(Some(c));
        }
        assert_eq!(g.filtered(), vec![0]);
        g.filter_key(None); // backspace -> "ad"
        assert_eq!(g.filtered(), vec![0, 2], "'ad' matches Ada and Radia");
        g.filter.clear();
        assert_eq!(g.filtered().len(), 3);
    }

    #[test]
    fn selection_clamps_to_the_filtered_set() {
        let mut g = grid();
        g.step(false, 10);
        assert_eq!(g.sel, 2, "clamped to the last row");
        for c in "hopper".chars() {
            g.filter_key(Some(c));
        }
        assert_eq!(g.sel, 0, "selection pulled back inside the filtered set");
    }

    #[test]
    fn column_selection_clamps_and_drags_the_scroll() {
        let mut g = Grid::default();
        g.set(
            (0..8).map(|i| format!("c{i}")).collect(),
            vec![(0..8).map(|i| i.to_string()).collect()],
        );
        for _ in 0..6 {
            g.select_col(false);
        }
        assert_eq!(g.cur_col, 6);
        assert_eq!(g.col_off, 3, "scroll follows a far-right selection");
        for _ in 0..10 {
            g.select_col(false);
        }
        assert_eq!(g.cur_col, 7, "clamped to the last column");
        for _ in 0..10 {
            g.select_col(true);
        }
        assert_eq!((g.cur_col, g.col_off), (0, 0));
    }

    #[test]
    fn sort_cycles_asc_desc_off_and_compares_numbers_numerically() {
        let mut g = Grid::default();
        g.set(
            vec!["n".into()],
            vec![vec!["10".into()], vec!["9".into()], vec!["100".into()]],
        );
        g.cycle_sort(); // ascending
        assert_eq!(g.filtered(), vec![1, 0, 2], "9 < 10 < 100 numerically");
        g.cycle_sort(); // descending
        assert_eq!(g.filtered(), vec![2, 0, 1]);
        g.cycle_sort(); // off
        assert_eq!(g.filtered(), vec![0, 1, 2], "arrival order restored");
        g.cycle_sort();
        assert_eq!(g.sort, Some((0, true)), "cycle restarts ascending");
    }

    #[test]
    fn selected_cell_and_row_respect_filter_and_sort() {
        let mut g = grid();
        g.cur_col = 1;
        g.cycle_sort(); // sort by name ascending: Ada, Grace, Radia
        g.sel = 1;
        assert_eq!(g.selected_cell(), Some("Grace Hopper"));
        assert_eq!(g.selected_row().unwrap()[0], "2");
    }

    #[test]
    fn set_resets_the_view() {
        let mut g = grid();
        g.sel = 2;
        g.filter = "x".into();
        g.set(vec!["a".into()], vec![]);
        assert_eq!((g.sel, g.col_off), (0, 0));
        assert!(g.filter.is_empty());
    }
}
