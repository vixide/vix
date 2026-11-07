//! The server-side row search on a table preview (`F`, T778): the
//! [`Browser`] side of [`crate::row_filter`]. The filter row picks a column,
//! an operator and a value; `Enter` re-runs the preview with the built
//! `WHERE` clause, and the result stays editable like any preview. Owning
//! spec: `crates/vix-db/spec/index.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{Browser, Outcome, Pane, catalog, editor, row_filter};

impl Browser {
    /// Open the row search for the current table preview (`F`), on the
    /// selected column. The operator and value from the last search are kept.
    pub(super) fn open_row_filter(&mut self) {
        if self.preview.is_none() {
            self.message = Some(t!("msg.db_filter_needs_preview").to_string());
            return;
        }
        self.row_filter.column = self
            .grid
            .cur_col
            .min(self.grid.headers.len().saturating_sub(1));
        self.row_filter.field = 2;
        self.row_filter.open = true;
    }

    /// Whether the row-search form (`F`) is open. It is drawn in place of
    /// the workbench panes and takes the keys until `Esc` or a search.
    #[must_use]
    pub fn row_filter_open(&self) -> bool {
        self.row_filter.open
    }

    /// The condition the filter row currently describes, for the live
    /// preview under it.
    ///
    /// # Errors
    ///
    /// A [`row_filter::FilterError`] while the value does not fit the
    /// operator yet.
    pub fn row_filter_clause(&self) -> Result<String, row_filter::FilterError> {
        let kind = self.conn.as_ref().map_or_else(Default::default, |c| c.kind);
        self.row_filter.clause(kind, &self.grid.headers)
    }

    /// Keys on the filter row: `↑`/`↓` move between column, operator and
    /// value; `←`/`→` choose the column or operator; typing edits the value;
    /// `Enter` searches; `Delete` clears the active filter; `Esc` closes.
    pub(super) fn key_row_filter(&mut self, key: KeyEvent) -> Outcome {
        let form = &mut self.row_filter;
        let columns = self.grid.headers.len().max(1);
        let ops = row_filter::OPS.len();
        match key.code {
            KeyCode::Up | KeyCode::BackTab => form.field = (form.field + 2) % 3,
            KeyCode::Down | KeyCode::Tab => form.field = (form.field + 1) % 3,
            KeyCode::Left | KeyCode::Right => {
                let step = |i: usize, n: usize| {
                    if key.code == KeyCode::Left {
                        (i + n - 1) % n
                    } else {
                        (i + 1) % n
                    }
                };
                match form.field {
                    0 => form.column = step(form.column, columns),
                    1 => form.op = step(form.op, ops),
                    _ => {}
                }
            }
            KeyCode::Char(c) => form.value.push(c),
            KeyCode::Backspace => {
                form.value.pop();
            }
            KeyCode::Enter => self.apply_row_filter(),
            KeyCode::Delete => self.clear_row_filter(),
            KeyCode::Esc => self.row_filter.open = false,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Re-run the preview with the filter row's condition. Refused while
    /// changes are staged (the re-run would drop them), and for a raw
    /// condition that writes.
    fn apply_row_filter(&mut self) {
        let clause = match self.row_filter_clause() {
            Ok(clause) => clause,
            Err(e) => {
                self.message = Some(e.to_string());
                return;
            }
        };
        self.run_row_filter(Some(clause));
    }

    /// Drop the active row filter and re-run the plain preview.
    fn clear_row_filter(&mut self) {
        if self.run_row_filter(None) {
            self.message = Some(t!("msg.db_filter_cleared").to_string());
        }
    }

    /// Run the preview narrowed by `clause` (or not at all, for `None`) and
    /// make it the grid, editable as before. Returns whether it ran; on
    /// `false`, `self.message` says why.
    fn run_row_filter(&mut self, clause: Option<String>) -> bool {
        let (Some(kind), Some((schema, table))) =
            (self.conn.as_ref().map(|c| c.kind), self.preview.clone())
        else {
            self.message = Some(t!("msg.db_filter_needs_preview").to_string());
            return false;
        };
        if self.has_staged_changes() {
            self.message = Some(t!("msg.db_filter_staged").to_string());
            return false;
        }
        let sql = match &clause {
            Some(c) => catalog::filtered_preview_sql(kind, &schema, &table, c),
            None => catalog::preview_sql(kind, &schema, &table),
        };
        // Only a raw condition can carry a write; refuse it, whatever the
        // session's write mode, since a search must never change data.
        if editor::is_write_statement(&sql) {
            self.message = Some(t!("msg.db_filter_write").to_string());
            return false;
        }
        match self.run_catalog(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.server_filter = clause;
                self.set_editable(&schema, &table, kind);
                self.focus = Pane::Results;
                self.row_filter.open = false;
                true
            }
            Err(e) => {
                self.message = Some(e.to_string());
                false
            }
        }
    }
}
