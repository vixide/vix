//! The server-side row search on a table preview (`F`, T778): the
//! [`Browser`] side of [`crate::row_filter`]. The filter row picks a column,
//! an operator and a value; `Enter` re-runs the preview with the built
//! `WHERE` clause, and the result stays editable like any preview. Owning
//! spec: `crates/vix-db/spec/index.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{Browser, Outcome, Pane, catalog, editor, row_filter};

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// The slot after `i` in a ring of `n` slots (the first after the last).
fn ring_next(i: usize, n: usize) -> usize {
    i.checked_add(1).filter(|next| *next < n).unwrap_or(0)
}

/// The slot before `i` in a ring of `n` slots (the last before the first).
fn ring_prev(i: usize, n: usize) -> usize {
    i.checked_sub(1).unwrap_or_else(|| sub_or(n, 1, 0))
}

impl Browser {
    /// Open the row search for the current table preview (`F`), on the
    /// selected column. The operator and value from the last search are kept.
    pub(super) fn open_row_filter(&mut self) {
        if self.preview.is_none() {
            self.message = Some(t!("msg.db_filter_needs_preview").to_string());
            return;
        }
        self.row_filter.column = self.grid.cur_col.min(sub_or(self.grid.headers.len(), 1, 0));
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
            KeyCode::Up | KeyCode::BackTab => form.field = ring_prev(form.field, 3),
            KeyCode::Down | KeyCode::Tab => form.field = ring_next(form.field, 3),
            KeyCode::Left | KeyCode::Right => {
                let step = |i: usize, n: usize| {
                    if key.code == KeyCode::Left {
                        ring_prev(i, n)
                    } else {
                        ring_next(i, n)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{family, press, preview, typed, workbench};
    use crate::{View, row_filter};
    use vix_input::KeyCode;

    fn open(b: &mut Browser, table: &str) {
        preview(b, table);
        press(b, KeyCode::Char('F'));
        assert!(b.row_filter_open());
    }

    #[test]
    fn the_ring_helpers_wrap_at_both_ends() {
        assert_eq!(ring_next(0, 3), 1);
        assert_eq!(ring_next(2, 3), 0);
        assert_eq!(ring_prev(1, 3), 0);
        assert_eq!(ring_prev(0, 3), 2);
        assert_eq!(ring_prev(0, 0), 0);
    }

    #[test]
    fn f_needs_a_table_preview() {
        let mut b = family(true);
        b.focus = Pane::Results;
        press(&mut b, KeyCode::Char('F'));
        assert!(!b.row_filter_open());
        assert_eq!(
            b.message,
            Some(t!("msg.db_filter_needs_preview").to_string())
        );
        // Applying without a preview says the same.
        assert!(!b.run_row_filter(None));
        assert_eq!(
            b.message,
            Some(t!("msg.db_filter_needs_preview").to_string())
        );
    }

    #[test]
    fn the_form_opens_on_the_selected_column_with_the_value_field_active() {
        let mut b = family(true);
        preview(&mut b, "parent");
        b.grid.cur_col = 1;
        press(&mut b, KeyCode::Char('F'));
        assert_eq!((b.row_filter.column, b.row_filter.field), (1, 2));
        assert_eq!(
            b.row_filter_clause(),
            Err(row_filter::FilterError::MissingValue)
        );
    }

    #[test]
    fn fields_and_choices_navigate_with_the_arrows() {
        let mut b = family(true);
        open(&mut b, "parent");
        press(&mut b, KeyCode::Up);
        assert_eq!(b.row_filter.field, 1, "up from value is the operator");
        press(&mut b, KeyCode::Right);
        assert_eq!(b.row_filter.op, 1, "Contains -> Equals");
        press(&mut b, KeyCode::Left);
        press(&mut b, KeyCode::Left);
        assert_eq!(
            b.row_filter.op,
            row_filter::OPS.len() - 1,
            "wraps to the last"
        );
        press(&mut b, KeyCode::Right);
        assert_eq!(b.row_filter.op, 0);
        press(&mut b, KeyCode::BackTab);
        assert_eq!(b.row_filter.field, 0);
        press(&mut b, KeyCode::Right);
        assert_eq!(b.row_filter.column, 1);
        press(&mut b, KeyCode::Right);
        assert_eq!(b.row_filter.column, 0, "two columns, so it wraps");
        press(&mut b, KeyCode::Left);
        assert_eq!(b.row_filter.column, 1);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Right);
        assert_eq!(b.row_filter.op, 1);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.row_filter.field, 2);
        press(&mut b, KeyCode::Right);
        press(&mut b, KeyCode::Left);
        assert_eq!(
            (b.row_filter.column, b.row_filter.op),
            (1, 1),
            "the value row ignores them"
        );
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.row_filter.field, 0, "wraps");
        press(&mut b, KeyCode::PageUp);
    }

    #[test]
    fn searching_reruns_the_preview_on_the_server_and_stays_editable() {
        let mut b = family(true);
        open(&mut b, "parent");
        b.row_filter.column = 1; // the name column
        typed(&mut b, "BET");
        assert_eq!(b.row_filter.value, "BET");
        press(&mut b, KeyCode::Backspace);
        typed(&mut b, "T");
        assert!(b.row_filter_clause().unwrap().contains("'%BET%'"));
        press(&mut b, KeyCode::Enter);
        assert!(!b.row_filter_open());
        assert_eq!(b.grid.rows, [["2", "beta"]]);
        assert!(b.editable());
        assert_eq!(b.focus, Pane::Results);
        assert_eq!(b.view, View::Workbench);
        assert!(b.server_filter().is_some_and(|c| c.contains("BET")));
        assert_eq!(b.message, Some(t!("msg.db_rows", count = 1).to_string()));
        // Delete drops the filter and restores the plain preview.
        press(&mut b, KeyCode::Char('F'));
        press(&mut b, KeyCode::Delete);
        assert_eq!(b.grid.rows.len(), 2);
        assert_eq!(b.server_filter(), None);
        assert_eq!(b.message, Some(t!("msg.db_filter_cleared").to_string()));
    }

    #[test]
    fn an_unfinished_value_is_reported_not_run() {
        let mut b = family(true);
        open(&mut b, "parent");
        press(&mut b, KeyCode::Enter);
        assert_eq!(
            b.message,
            Some(row_filter::FilterError::MissingValue.to_string())
        );
        assert!(b.row_filter_open(), "the form stays open");
        assert_eq!(b.grid.rows.len(), 2);
    }

    #[test]
    fn staged_changes_block_a_search() {
        let mut b = family(true);
        preview(&mut b, "parent");
        b.grid.cur_col = 1;
        press(&mut b, KeyCode::Char('i'));
        typed(&mut b, "x");
        press(&mut b, KeyCode::Enter);
        press(&mut b, KeyCode::Char('F'));
        typed(&mut b, "a");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.message, Some(t!("msg.db_filter_staged").to_string()));
        assert!(b.row_filter_open());
        press(&mut b, KeyCode::Delete);
        assert_eq!(b.message, Some(t!("msg.db_filter_staged").to_string()));
        press(&mut b, KeyCode::Esc);
        assert!(!b.row_filter_open());
    }

    #[test]
    fn a_raw_condition_that_writes_is_refused() {
        let mut b = family(true);
        open(&mut b, "parent");
        b.row_filter.op = row_filter::OPS
            .iter()
            .position(|o| *o == row_filter::Op::Raw)
            .unwrap();
        typed(&mut b, "1=1; DELETE FROM parent");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.message, Some(t!("msg.db_filter_write").to_string()));
        assert_eq!(b.grid.rows.len(), 2);
        assert_eq!(
            b.session
                .as_mut()
                .unwrap()
                .run("SELECT count(*) FROM parent")
                .unwrap()
                .1,
            [["2"]]
        );
    }

    #[test]
    fn a_condition_the_server_rejects_shows_the_error() {
        let mut b = family(true);
        open(&mut b, "parent");
        b.row_filter.op = row_filter::OPS
            .iter()
            .position(|o| *o == row_filter::Op::Raw)
            .unwrap();
        typed(&mut b, "no_such_column = 1");
        press(&mut b, KeyCode::Enter);
        assert!(
            b.message.as_deref().unwrap().contains("no_such_column"),
            "{:?}",
            b.message
        );
        assert!(b.row_filter_open());
        assert_eq!(b.grid.rows.len(), 2);
    }

    #[test]
    fn the_clause_without_a_connection_falls_back_to_the_default_engine() {
        let mut b = workbench(true, &["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]);
        b.conn = None;
        b.grid.set(vec!["v".into()], Vec::new());
        b.row_filter.value = "x".into();
        assert!(b.row_filter_clause().is_ok());
    }
}
