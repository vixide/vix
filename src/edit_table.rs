//! The table editor: a spreadsheet-like grid for viewing and editing delimited
//! data (CSV/TSV) as rows and columns.
//!
//! Vix's Tools menu offers an *Edit Table* command that parses the active buffer as
//! CSV or TSV (per the file extension) into a rectangular grid. The first row is
//! treated as the header. The user navigates a cell cursor with the arrow keys
//! (or `h`/`j`/`k`/`l`), edits a cell in place, inserts and deletes rows and
//! columns, sorts by the current column, and searches across all cells. Saving
//! serializes the grid back to delimited text.
//!
//! This module is self-contained and host-agnostic: it owns the grid data,
//! the cursor, the edit/find buffers, and an undo/redo history, and it
//! interprets key events itself (returning an [`Outcome`] telling the host when
//! to close or save). The host ([`crate::app`]) only renders the grid, syncs
//! the visible scroll window, and acts on the returned outcome.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cmp::Ordering;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::convert_tabular::{parse_csv, parse_tsv, write_csv, write_tsv};

/// What the host should do after the grid handled a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The key was handled internally; nothing further for the host to do.
    Consumed,
    /// The user asked to close the table editor (Esc/`q` in normal mode).
    Close,
    /// The user asked to save (Ctrl+S); the host should persist the grid.
    Save,
}

/// The current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Navigating the cell cursor and issuing commands.
    Normal,
    /// Editing the current cell; keystrokes go to the edit buffer.
    Edit,
    /// Typing a search query; keystrokes go to the find buffer.
    Find,
}

/// A point-in-time snapshot of the grid for undo/redo.
#[derive(Clone)]
struct Snapshot {
    rows: Vec<Vec<String>>,
    row: usize,
    col: usize,
}

/// Maximum number of undo steps retained.
const HISTORY_CAP: usize = 200;

/// A spreadsheet-like grid of string cells with a cell cursor and edit history.
pub struct Grid {
    /// Cells, rectangular (every row has the same length). Row 0 is the header.
    rows: Vec<Vec<String>>,
    /// Whether the source was TSV (tab-separated); otherwise CSV.
    tsv: bool,
    /// Selected row index, `0..rows.len()`.
    row: usize,
    /// Selected column index, `0..cols`.
    col: usize,
    /// First visible row (vertical scroll), synced by [`Grid::ensure_row_visible`].
    row_scroll: usize,
    /// First visible column (horizontal scroll), synced by the host renderer.
    col_scroll: usize,
    /// Whether the grid has unsaved changes.
    dirty: bool,
    /// Current interaction mode.
    mode: Mode,
    /// Edit buffer, valid while editing a cell.
    edit_buf: String,
    /// Find buffer, valid while typing a query.
    find_buf: String,
    /// The last committed search query, reused by "find next".
    last_query: String,
    /// Undo history (most recent last).
    undo: Vec<Snapshot>,
    /// Redo history (most recent last).
    redo: Vec<Snapshot>,
}

impl Grid {
    /// Build a grid from delimited `text`. `tsv` selects the tab-separated
    /// parser; otherwise RFC 4180 CSV is used. The grid is normalized to a
    /// rectangle (short rows are padded with empty cells) and always has at
    /// least one row and one column.
    #[must_use]
    pub fn from_text(text: &str, tsv: bool) -> Self {
        let mut rows = if tsv {
            parse_tsv(text)
        } else {
            parse_csv(text)
        };
        normalize(&mut rows);
        Grid {
            rows,
            tsv,
            row: 0,
            col: 0,
            row_scroll: 0,
            col_scroll: 0,
            dirty: false,
            mode: Mode::Normal,
            edit_buf: String::new(),
            find_buf: String::new(),
            last_query: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Number of rows (including the header).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns.
    #[must_use]
    pub fn col_count(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    /// The cell at `(r, c)`, or `""` when out of range.
    #[must_use]
    pub fn cell(&self, r: usize, c: usize) -> &str {
        self.rows
            .get(r)
            .and_then(|row| row.get(c))
            .map_or("", String::as_str)
    }

    /// The selected row index.
    #[must_use]
    pub fn row(&self) -> usize {
        self.row
    }

    /// The selected column index.
    #[must_use]
    pub fn col(&self) -> usize {
        self.col
    }

    /// The first visible row (vertical scroll offset).
    #[must_use]
    pub fn row_scroll(&self) -> usize {
        self.row_scroll
    }

    /// The first visible column (horizontal scroll offset).
    #[must_use]
    pub fn col_scroll(&self) -> usize {
        self.col_scroll
    }

    /// Whether the grid has unsaved edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether a cell is currently being edited.
    #[must_use]
    pub fn is_editing(&self) -> bool {
        matches!(self.mode, Mode::Edit)
    }

    /// Whether a search query is currently being typed.
    #[must_use]
    pub fn is_finding(&self) -> bool {
        matches!(self.mode, Mode::Find)
    }

    /// The in-progress edit text (valid while [`Grid::is_editing`]).
    #[must_use]
    pub fn edit_buffer(&self) -> &str {
        &self.edit_buf
    }

    /// The in-progress find text (valid while [`Grid::is_finding`]).
    #[must_use]
    pub fn find_buffer(&self) -> &str {
        &self.find_buf
    }

    /// Serialize the grid back to delimited text (TSV or CSV per the source).
    #[must_use]
    pub fn to_text(&self) -> String {
        if self.tsv {
            write_tsv(&self.rows)
        } else {
            write_csv(&self.rows)
        }
    }

    /// Mark the grid as saved (called by the host after a successful write).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Adjust the vertical scroll so the selected data row stays within the body
    /// window of `height` rows. Row 0 is a pinned header rendered separately, so
    /// the body always starts at row 1 or later. Called before drawing.
    pub fn ensure_row_visible(&mut self, height: usize) {
        let height = height.max(1);
        let count = self.row_count();
        if count <= 1 {
            self.row_scroll = count;
            return;
        }
        if self.row >= 1 {
            if self.row < self.row_scroll {
                self.row_scroll = self.row;
            } else if self.row >= self.row_scroll + height {
                self.row_scroll = self.row + 1 - height;
            }
        }
        let max_scroll = count.saturating_sub(height).max(1);
        self.row_scroll = self.row_scroll.clamp(1, max_scroll);
    }

    /// Set the horizontal scroll so the selected column is visible. The renderer
    /// computes `first_visible` from the variable column widths and the viewport.
    pub fn set_col_scroll(&mut self, first_visible: usize) {
        self.col_scroll = first_visible.min(self.col.min(self.col_count().saturating_sub(1)));
    }

    /// Interpret a key event, mutating the grid, and report what the host should
    /// do next. `page` is the number of rows to move for `PageUp`/`PageDown`.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        // Ctrl+S saves from any mode, committing an in-progress edit first.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            if self.mode == Mode::Edit {
                self.commit_edit();
            }
            return Outcome::Save;
        }
        match self.mode {
            Mode::Edit => self.edit_key(key),
            Mode::Find => self.find_key(key),
            Mode::Normal => return self.normal_key(key, page),
        }
        Outcome::Consumed
    }

    /// Handle a key while editing a cell.
    fn edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.commit_edit();
                self.move_down();
            }
            KeyCode::Tab => {
                self.commit_edit();
                self.move_right();
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.edit_buf.pop();
            }
            KeyCode::Char(c) => self.edit_buf.push(c),
            _ => {}
        }
    }

    /// Handle a key while typing a search query.
    fn find_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.last_query = std::mem::take(&mut self.find_buf);
                self.mode = Mode::Normal;
                self.find_from(self.row, self.col, true);
            }
            KeyCode::Esc => {
                self.find_buf.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.find_buf.pop();
            }
            KeyCode::Char(c) => self.find_buf.push(c),
            _ => {}
        }
    }

    /// Handle a key in normal (navigation/command) mode.
    fn normal_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if alt {
            self.structural_key(key.code);
            return Outcome::Consumed;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Left | KeyCode::Char('h') => self.move_left(),
            KeyCode::Right | KeyCode::Char('l') => self.move_right(),
            KeyCode::PageUp => self.row = self.row.saturating_sub(page.max(1)),
            KeyCode::PageDown => {
                self.row = (self.row + page.max(1)).min(self.row_count().saturating_sub(1));
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.col_count().saturating_sub(1),
            KeyCode::Char('g') => self.row = 0,
            KeyCode::Char('G') => self.row = self.row_count().saturating_sub(1),
            KeyCode::Enter | KeyCode::F(2) => self.begin_edit(),
            KeyCode::Delete => self.clear_cell(),
            KeyCode::Char('s') => self.sort(true),
            KeyCode::Char('S') => self.sort(false),
            KeyCode::Char('/') => {
                self.mode = Mode::Find;
                self.find_buf.clear();
            }
            KeyCode::Char('n') => self.find_from(self.row, self.col, true),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => self.redo(),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Handle an Alt+key structural command (insert/delete rows and columns).
    fn structural_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Up => self.insert_row(self.row),
            KeyCode::Down => self.insert_row(self.row + 1),
            KeyCode::Left => self.insert_col(self.col),
            KeyCode::Right => self.insert_col(self.col + 1),
            KeyCode::Delete => self.delete_row(),
            KeyCode::Backspace => self.delete_col(),
            _ => {}
        }
    }

    // ----- cursor movement --------------------------------------------------

    /// Move the cursor up one row, stopping at the top.
    fn move_up(&mut self) {
        self.row = self.row.saturating_sub(1);
    }

    /// Move the cursor down one row, stopping at the bottom.
    fn move_down(&mut self) {
        if self.row + 1 < self.row_count() {
            self.row += 1;
        }
    }

    /// Move the cursor left one column, stopping at the first.
    fn move_left(&mut self) {
        self.col = self.col.saturating_sub(1);
    }

    /// Move the cursor right one column, stopping at the last.
    fn move_right(&mut self) {
        if self.col + 1 < self.col_count() {
            self.col += 1;
        }
    }

    // ----- editing ----------------------------------------------------------

    /// Begin editing the current cell, seeding the buffer with its contents.
    fn begin_edit(&mut self) {
        self.edit_buf = self.cell(self.row, self.col).to_string();
        self.mode = Mode::Edit;
    }

    /// Commit the edit buffer into the current cell as one undo step.
    fn commit_edit(&mut self) {
        let text = std::mem::take(&mut self.edit_buf);
        self.mode = Mode::Normal;
        if self.cell(self.row, self.col) == text {
            return;
        }
        self.push_undo();
        if let Some(cell) = self
            .rows
            .get_mut(self.row)
            .and_then(|r| r.get_mut(self.col))
        {
            *cell = text;
            self.dirty = true;
        }
    }

    /// Clear the current cell (set it to empty) as one undo step.
    fn clear_cell(&mut self) {
        if self.cell(self.row, self.col).is_empty() {
            return;
        }
        self.push_undo();
        if let Some(cell) = self
            .rows
            .get_mut(self.row)
            .and_then(|r| r.get_mut(self.col))
        {
            cell.clear();
            self.dirty = true;
        }
    }

    // ----- structural edits -------------------------------------------------

    /// Insert an empty row at index `at` (clamped), as one undo step.
    fn insert_row(&mut self, at: usize) {
        self.push_undo();
        let cols = self.col_count().max(1);
        let at = at.min(self.row_count());
        self.rows.insert(at, vec![String::new(); cols]);
        self.row = at;
        self.dirty = true;
    }

    /// Delete the current row, keeping at least one row, as one undo step.
    fn delete_row(&mut self) {
        if self.row_count() <= 1 {
            return;
        }
        self.push_undo();
        self.rows.remove(self.row);
        self.row = self.row.min(self.row_count().saturating_sub(1));
        self.dirty = true;
    }

    /// Insert an empty column at index `at` (clamped), as one undo step.
    fn insert_col(&mut self, at: usize) {
        self.push_undo();
        let at = at.min(self.col_count());
        for row in &mut self.rows {
            row.insert(at.min(row.len()), String::new());
        }
        self.col = at;
        self.dirty = true;
    }

    /// Delete the current column, keeping at least one column, as one undo step.
    fn delete_col(&mut self) {
        if self.col_count() <= 1 {
            return;
        }
        self.push_undo();
        let c = self.col;
        for row in &mut self.rows {
            if c < row.len() {
                row.remove(c);
            }
        }
        self.col = self.col.min(self.col_count().saturating_sub(1));
        self.dirty = true;
    }

    // ----- sort -------------------------------------------------------------

    /// Sort the data rows (keeping the header fixed) by the current column.
    /// `ascending` selects the direction. Numeric cells compare numerically.
    fn sort(&mut self, ascending: bool) {
        if self.row_count() <= 2 {
            return;
        }
        self.push_undo();
        let c = self.col;
        let header = self.rows.remove(0);
        self.rows.sort_by(|a, b| {
            let ord = cmp_cells(
                a.get(c).map_or("", String::as_str),
                b.get(c).map_or("", String::as_str),
            );
            if ascending { ord } else { ord.reverse() }
        });
        self.rows.insert(0, header);
        self.dirty = true;
    }

    // ----- find -------------------------------------------------------------

    /// Move the cursor to the next cell containing the last query (case
    /// insensitive), scanning row-major from just after `(from_row, from_col)`
    /// and wrapping. Does nothing if there is no query or no match.
    fn find_from(&mut self, from_row: usize, from_col: usize, _forward: bool) {
        if self.last_query.is_empty() {
            return;
        }
        let needle = self.last_query.to_lowercase();
        let cols = self.col_count();
        let total = self.row_count() * cols;
        if total == 0 {
            return;
        }
        let start = from_row * cols + from_col;
        for step in 1..=total {
            let idx = (start + step) % total;
            let (r, c) = (idx / cols, idx % cols);
            if self.cell(r, c).to_lowercase().contains(&needle) {
                self.row = r;
                self.col = c;
                return;
            }
        }
    }

    // ----- undo/redo --------------------------------------------------------

    /// Capture the current grid state onto the undo stack and clear redo.
    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > HISTORY_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// A snapshot of the current grid + cursor.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            rows: self.rows.clone(),
            row: self.row,
            col: self.col,
        }
    }

    /// Restore `snap` into the grid and return the prior state for the other stack.
    fn restore(&mut self, snap: Snapshot) -> Snapshot {
        let prior = self.snapshot();
        self.rows = snap.rows;
        self.row = snap.row.min(self.row_count().saturating_sub(1));
        self.col = snap.col.min(self.col_count().saturating_sub(1));
        self.dirty = true;
        prior
    }

    /// Undo the most recent change.
    fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            let prior = self.restore(snap);
            self.redo.push(prior);
        }
    }

    /// Redo the most recently undone change.
    fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            let prior = self.restore(snap);
            self.undo.push(prior);
        }
    }
}

/// Pad every row to the width of the widest row so the grid is rectangular, and
/// guarantee at least one row and one column.
fn normalize(rows: &mut Vec<Vec<String>>) {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    if rows.is_empty() {
        rows.push(Vec::new());
    }
    for row in rows.iter_mut() {
        if row.len() < width {
            row.resize(width, String::new());
        }
    }
}

/// Compare two cells: numerically when both parse as numbers, else as strings.
fn cmp_cells(a: &str, b: &str) -> Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn grid() -> Grid {
        Grid::from_text("a,b,c\n1,2,3\n4,5,6\n", false)
    }

    #[test]
    fn parses_into_a_rectangle() {
        let g = Grid::from_text("a,b,c\n1,2\n", false);
        assert_eq!(g.row_count(), 2);
        assert_eq!(g.col_count(), 3);
        assert_eq!(g.cell(1, 2), "", "short row is padded");
    }

    #[test]
    fn empty_input_has_one_cell() {
        let g = Grid::from_text("", false);
        assert_eq!(g.row_count(), 1);
        assert_eq!(g.col_count(), 1);
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut g = grid();
        assert_eq!((g.row(), g.col()), (0, 0));
        g.handle_key(code(KeyCode::Up), 1);
        assert_eq!(g.row(), 0, "up at top stays");
        g.handle_key(code(KeyCode::Down), 1);
        g.handle_key(key('l'), 1);
        assert_eq!((g.row(), g.col()), (1, 1));
        g.handle_key(code(KeyCode::End), 1);
        assert_eq!(g.col(), 2);
        g.handle_key(key('G'), 1);
        assert_eq!(g.row(), 2);
        g.handle_key(code(KeyCode::Down), 1);
        assert_eq!(g.row(), 2, "down at bottom stays");
    }

    #[test]
    fn edits_a_cell() {
        let mut g = grid();
        g.handle_key(code(KeyCode::Down), 1); // row 1
        g.handle_key(code(KeyCode::Enter), 1); // begin edit
        assert!(g.is_editing());
        assert_eq!(g.edit_buffer(), "1", "seeded with current value");
        g.handle_key(code(KeyCode::Backspace), 1);
        g.handle_key(key('9'), 1);
        g.handle_key(code(KeyCode::Enter), 1); // commit, move down
        assert!(!g.is_editing());
        assert_eq!(g.cell(1, 0), "9");
        assert!(g.is_dirty());
        assert_eq!(g.row(), 2, "Enter commits and moves down");
    }

    #[test]
    fn escape_cancels_an_edit() {
        let mut g = grid();
        g.handle_key(code(KeyCode::Enter), 1);
        g.handle_key(key('z'), 1);
        g.handle_key(code(KeyCode::Esc), 1);
        assert_eq!(g.cell(0, 0), "a", "cell unchanged");
        assert!(!g.is_dirty());
    }

    #[test]
    fn inserts_and_deletes_rows() {
        let mut g = grid();
        let before = g.row_count();
        g.handle_key(alt(KeyCode::Down), 1); // insert row below
        assert_eq!(g.row_count(), before + 1);
        g.handle_key(alt(KeyCode::Delete), 1); // delete it
        assert_eq!(g.row_count(), before);
    }

    #[test]
    fn inserts_and_deletes_columns() {
        let mut g = grid();
        let before = g.col_count();
        g.handle_key(alt(KeyCode::Right), 1);
        assert_eq!(g.col_count(), before + 1);
        g.handle_key(alt(KeyCode::Backspace), 1);
        assert_eq!(g.col_count(), before);
    }

    #[test]
    fn keeps_at_least_one_row_and_column() {
        let mut g = Grid::from_text("x", false);
        g.handle_key(alt(KeyCode::Delete), 1);
        g.handle_key(alt(KeyCode::Backspace), 1);
        assert_eq!(g.row_count(), 1);
        assert_eq!(g.col_count(), 1);
    }

    #[test]
    fn sorts_by_column_keeping_header() {
        let mut g = Grid::from_text("n\n3\n1\n2\n", false);
        g.handle_key(key('s'), 1); // ascending
        assert_eq!(g.cell(0, 0), "n", "header stays first");
        assert_eq!(g.cell(1, 0), "1");
        assert_eq!(g.cell(3, 0), "3");
        g.handle_key(key('S'), 1); // descending
        assert_eq!(g.cell(1, 0), "3");
    }

    #[test]
    fn sort_is_numeric_when_possible() {
        let mut g = Grid::from_text("n\n10\n9\n", false);
        g.handle_key(key('s'), 1);
        assert_eq!(g.cell(1, 0), "9", "9 < 10 numerically, not lexically");
    }

    #[test]
    fn finds_a_cell() {
        let mut g = grid();
        g.handle_key(key('/'), 1);
        for c in "5".chars() {
            g.handle_key(key(c), 1);
        }
        g.handle_key(code(KeyCode::Enter), 1);
        assert_eq!((g.row(), g.col()), (2, 1), "jumped to the cell holding 5");
    }

    #[test]
    fn undo_and_redo_round_trip() {
        let mut g = grid();
        g.handle_key(code(KeyCode::Enter), 1);
        g.handle_key(key('Z'), 1);
        g.handle_key(code(KeyCode::Enter), 1); // commit -> cell becomes "aZ"? seeded "a"+"Z"
        assert_eq!(g.cell(0, 0), "aZ");
        g.handle_key(key('u'), 1);
        assert_eq!(g.cell(0, 0), "a", "undo restores");
        g.handle_key(ctrl('r'), 1);
        assert_eq!(g.cell(0, 0), "aZ", "redo reapplies");
    }

    #[test]
    fn round_trips_to_text() {
        let g = grid();
        assert_eq!(g.to_text(), "a,b,c\n1,2,3\n4,5,6\n");
    }

    #[test]
    fn save_and_close_outcomes() {
        let mut g = grid();
        assert_eq!(g.handle_key(ctrl('s'), 1), Outcome::Save);
        assert_eq!(g.handle_key(key('q'), 1), Outcome::Close);
        assert_eq!(g.handle_key(code(KeyCode::Esc), 1), Outcome::Close);
        // Esc while editing cancels rather than closing.
        g.handle_key(code(KeyCode::Enter), 1);
        assert_eq!(g.handle_key(code(KeyCode::Esc), 1), Outcome::Consumed);
    }

    #[test]
    fn vertical_scroll_pins_the_header_and_follows_the_cursor() {
        let mut g = Grid::from_text("h\n1\n2\n3\n4\n5\n6\n", false);
        // Header selected: body starts at the first data row.
        g.ensure_row_visible(3);
        assert_eq!(g.row_scroll(), 1, "body never shows row 0 (pinned header)");
        // Move to the last row; the window follows but stays >= 1.
        g.handle_key(key('G'), 3);
        g.ensure_row_visible(3);
        assert!(g.row_scroll() >= 1);
        assert!(g.row() < g.row_scroll() + 3 && g.row() >= g.row_scroll());
    }

    #[test]
    fn tsv_round_trips_with_tabs() {
        let g = Grid::from_text("a\tb\n1\t2\n", true);
        assert_eq!(g.col_count(), 2);
        assert_eq!(g.to_text(), "a\tb\n1\t2\n");
    }
}
