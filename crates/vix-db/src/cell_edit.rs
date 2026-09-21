//! The results grid's editable-cell workflow: previewing a single table so
//! its rows can be edited (`preview_selected`/`set_editable`), staging and
//! committing cell edits (`begin_cell_edit`/`key_cell_edit`/`commit_edits`),
//! following a foreign key, expanding a row into the detail viewer, and the
//! detail/DDL popup itself. Extracted from `lib.rs` (T516): the one
//! cohesive slice of [`Browser`]'s methods concerned with *what a single
//! cell or row means and how it gets changed*, as opposed to running SQL.

use crossterm::event::{KeyCode, KeyEvent};

use super::{Browser, Outcome, Pane, Popup, TxState, View, catalog, connect};

/// One staged cell edit: `(underlying row index, column index)` →
/// `(original value, new value)`.
type CellEdit = ((usize, usize), (String, String));

impl Browser {
    /// The staged new value for a cell, if any (for the grid overlay).
    #[must_use]
    pub fn staged_value(&self, row: usize, col: usize) -> Option<&str> {
        self.edits.get(&(row, col)).map(|(_, new)| new.as_str())
    }

    /// Whether the current grid can be edited in place (a single-table view
    /// with a primary key).
    #[must_use]
    pub fn editable(&self) -> bool {
        self.edit_table.is_some()
    }

    /// Keys in the cell / text viewer (scrolls long content).
    pub(super) fn key_cell(&mut self, key: KeyEvent) -> Outcome {
        let last = self.cell_text.lines().count().saturating_sub(1);
        match key.code {
            KeyCode::Char('p') => self.cell_pretty = !self.cell_pretty,
            KeyCode::Char('y') => {
                let text = self.cell_text.clone();
                self.yank(&text);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view_scroll = self.view_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.view_scroll = (self.view_scroll + 1).min(last);
            }
            KeyCode::PageUp => self.view_scroll = self.view_scroll.saturating_sub(10),
            KeyCode::PageDown => self.view_scroll = (self.view_scroll + 10).min(last),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Preview the selected table or view (`SELECT * … LIMIT`).
    pub(super) fn preview_selected(&mut self) {
        let Some((schema, table, folder)) = self.tree.selected_object() else {
            return;
        };
        if folder == catalog::Folder::Functions {
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let sql = catalog::preview_sql(kind, &schema, &table);
        match self.run_catalog(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.last_table = Some(table.clone());
                self.focus = Pane::Results;
                self.set_editable(&schema, &table, kind);
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Mark the current grid as an editable single-table view, fetching the
    /// table's primary key (no key ⇒ not editable). Clears any staged edits.
    fn set_editable(&mut self, schema: &str, table: &str, kind: connect::Kind) {
        self.edits.clear();
        self.editing_cell = None;
        let pk = self
            .run_catalog(&catalog::primary_key_sql(kind, schema, table))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter_map(|r| r.into_iter().next())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if pk.is_empty() {
            self.set_uneditable();
        } else {
            self.pk_cols = pk;
            self.edit_table = Some((schema.to_string(), table.to_string()));
        }
    }

    /// Mark the current grid as read-only (arbitrary query result), discarding
    /// any staged edits.
    pub(super) fn set_uneditable(&mut self) {
        self.edit_table = None;
        self.pk_cols.clear();
        self.edits.clear();
        self.editing_cell = None;
    }

    /// Show the selected result row vertically as `column: value` lines in the
    /// text viewer (psql's expanded `\x` display), for wide rows.
    pub(super) fn expand_row(&mut self) {
        use std::fmt::Write as _;
        let Some(row) = self.grid.selected_row() else {
            return;
        };
        let width = self
            .grid
            .headers
            .iter()
            .map(|h| h.chars().count())
            .max()
            .unwrap_or(0);
        let mut out = String::new();
        for (i, header) in self.grid.headers.iter().enumerate() {
            let value = row.get(i).map(String::as_str).unwrap_or_default();
            let _ = writeln!(out, "{header:<width$} : {value}");
        }
        self.cell_text = out;
        self.cell_pretty = false;
        self.view_scroll = 0;
        self.view = View::Cell;
    }

    /// Follow the foreign key on the selected result cell to its parent row.
    /// Works when the grid came from a table preview: if the current column is
    /// a foreign key of that table, run `SELECT * FROM parent WHERE pk = value`.
    pub(super) fn follow_fk(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let (Some(table), Some(column), Some(value)) = (
            self.last_table.clone(),
            self.grid.headers.get(self.grid.cur_col).cloned(),
            self.grid.selected_cell().map(str::to_string),
        ) else {
            return;
        };
        let (_, rels) = self.schema_facts();
        let edge = rels
            .iter()
            .find(|(child, child_col, _, _)| child == &table && child_col == &column);
        let Some((_, _, parent, parent_col)) = edge else {
            self.message = Some(t!("msg.db_fk_none").to_string());
            return;
        };
        let (parent, parent_col) = (parent.clone(), parent_col.clone());
        let col = if parent_col.is_empty() {
            "rowid".to_string()
        } else {
            parent_col
        };
        let sql = format!(
            "SELECT * FROM {} WHERE {} = {} LIMIT {}",
            catalog::quote_ident(kind, &parent),
            catalog::quote_ident(kind, &col),
            catalog::quote_literal(&value),
            catalog::PREVIEW_LIMIT,
        );
        match self.run_catalog(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.last_table = Some(parent);
                self.set_uneditable();
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Begin editing the selected cell (staged, not applied until commit).
    /// Only editable single-table views with a primary key qualify.
    pub(super) fn begin_cell_edit(&mut self) {
        if !self.editable() {
            self.message = Some(t!("msg.db_edit_readonly").to_string());
            return;
        }
        let (Some(row), col) = (self.grid.selected_index(), self.grid.cur_col) else {
            return;
        };
        let current = self
            .edits
            .get(&(row, col))
            .map(|(_, new)| new.clone())
            .or_else(|| self.grid.rows.get(row).and_then(|r| r.get(col)).cloned());
        let Some(current) = current else {
            return;
        };
        self.edit_input = current;
        self.editing_cell = Some((row, col));
        self.view = View::CellEdit;
    }

    /// Keys in the cell editor: `Enter` stages the value, `Esc` cancels.
    pub(super) fn key_cell_edit(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.edit_input.push(c),
            KeyCode::Backspace => {
                self.edit_input.pop();
            }
            KeyCode::Enter => {
                if let Some((row, col)) = self.editing_cell.take() {
                    let original = self
                        .grid
                        .rows
                        .get(row)
                        .and_then(|r| r.get(col))
                        .cloned()
                        .unwrap_or_default();
                    let new = std::mem::take(&mut self.edit_input);
                    if new == original {
                        self.edits.remove(&(row, col)); // reverting clears the stage
                    } else {
                        self.edits.insert((row, col), (original, new));
                    }
                }
                self.view = View::Workbench;
            }
            KeyCode::Esc => {
                self.editing_cell = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Commit every staged cell edit as an `UPDATE`, wrapped in one
    /// transaction, after re-checking each row for a concurrent change. Needs
    /// write mode.
    pub(super) fn commit_edits(&mut self) {
        if self.edits.is_empty() {
            self.message = Some(t!("msg.db_edit_none").to_string());
            return;
        }
        if !self.write_enabled {
            self.message = Some(t!("msg.db_read_only").to_string());
            return;
        }
        let (Some(kind), Some((schema, table))) =
            (self.conn.as_ref().map(|c| c.kind), self.edit_table.clone())
        else {
            return;
        };
        // Resolve primary-key column indices in the current grid.
        let pk_idx: Option<Vec<usize>> = self
            .pk_cols
            .iter()
            .map(|name| self.grid.headers.iter().position(|h| h == name))
            .collect();
        let Some(pk_idx) = pk_idx else {
            self.message = Some(t!("msg.db_edit_readonly").to_string());
            return;
        };
        let target = if matches!(kind, connect::Kind::Sqlite) {
            catalog::quote_ident(kind, &table)
        } else {
            format!(
                "{}.{}",
                catalog::quote_ident(kind, &schema),
                catalog::quote_ident(kind, &table)
            )
        };

        let mut edits: Vec<CellEdit> = self.edits.iter().map(|(k, v)| (*k, v.clone())).collect();
        edits.sort_by_key(|(k, _)| *k);

        let Some(updates) = self.build_pending_updates(kind, &target, &pk_idx, &edits) else {
            return;
        };
        let count = updates.len();
        if !self.apply_updates_in_transaction(&updates) {
            return;
        }
        self.edits.clear();
        self.message = Some(t!("msg.db_edit_committed", count = count).to_string());
        self.preview_selected_refresh(&schema, &table, kind);
    }

    /// Build one `UPDATE` statement per staged edit, re-reading each cell first
    /// to detect a concurrent change (optimistic conflict check). Sets
    /// `self.message` and returns `None` on the first conflict or read error.
    fn build_pending_updates(
        &mut self,
        kind: connect::Kind,
        target: &str,
        pk_idx: &[usize],
        edits: &[CellEdit],
    ) -> Option<Vec<String>> {
        let mut updates = Vec::new();
        for ((row, col), (original, new)) in edits {
            let Some(cells) = self.grid.rows.get(*row) else {
                continue;
            };
            let where_clause: Vec<String> = pk_idx
                .iter()
                .filter_map(|&pi| {
                    let name = self.grid.headers.get(pi)?;
                    let value = cells.get(pi)?;
                    Some(format!(
                        "{} = {}",
                        catalog::quote_ident(kind, name),
                        catalog::quote_literal(value)
                    ))
                })
                .collect();
            if where_clause.len() != pk_idx.len() {
                continue;
            }
            let where_sql = where_clause.join(" AND ");
            let column = self.grid.headers.get(*col).cloned().unwrap_or_default();
            // Conflict check: the cell must still hold the value we loaded.
            let check = format!(
                "SELECT {} FROM {target} WHERE {where_sql}",
                catalog::quote_ident(kind, &column)
            );
            match self.run_catalog(&check) {
                Ok((_, rows)) => {
                    let live = rows
                        .first()
                        .and_then(|r| r.first())
                        .cloned()
                        .unwrap_or_default();
                    if &live != original {
                        self.message =
                            Some(t!("msg.db_edit_conflict", column = column).to_string());
                        return None;
                    }
                }
                Err(e) => {
                    self.message = Some(e);
                    return None;
                }
            }
            updates.push(format!(
                "UPDATE {target} SET {} = {} WHERE {where_sql}",
                catalog::quote_ident(kind, &column),
                catalog::quote_literal(new)
            ));
        }
        Some(updates)
    }

    /// Apply every `UPDATE` in one transaction, rolling back and setting
    /// `self.message` on the first error. Returns whether the commit succeeded.
    fn apply_updates_in_transaction(&mut self, updates: &[String]) -> bool {
        if self.run_sql("BEGIN").is_err() {
            self.message = Some(t!("msg.db_ai_failed").to_string());
            return false;
        }
        for sql in updates {
            if let Err(e) = self.run_sql(sql) {
                let _ = self.run_sql("ROLLBACK");
                self.tx = TxState::None;
                self.message = Some(e);
                return false;
            }
        }
        if let Err(e) = self.run_sql("COMMIT") {
            let _ = self.run_sql("ROLLBACK");
            self.tx = TxState::None;
            self.message = Some(e);
            return false;
        }
        self.tx = TxState::None;
        true
    }

    /// Re-run a table preview after a commit to show the persisted values.
    fn preview_selected_refresh(&mut self, schema: &str, table: &str, kind: connect::Kind) {
        let sql = catalog::preview_sql(kind, schema, table);
        if let Ok((headers, rows)) = self.run_catalog(&sql) {
            self.grid.set(headers, rows);
            self.set_editable(schema, table, kind);
        }
    }

    /// Fetch one detail report for the selected table into the grid.
    pub(super) fn show_detail(&mut self, detail: catalog::Detail) {
        let Some((schema, table, folder)) = self.tree.selected_object() else {
            return;
        };
        if folder == catalog::Folder::Functions {
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let sql = catalog::detail_sql(kind, detail, &schema, &table);
        match self.run_catalog(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.focus = Pane::Results;
                self.set_uneditable();
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Fetch the `CREATE` statement for the selected table and show it in the
    /// text viewer (`y` copies it).
    pub(super) fn show_ddl(&mut self) {
        let Some((schema, table, folder)) = self.tree.selected_object() else {
            return;
        };
        if folder == catalog::Folder::Functions {
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let sql = catalog::ddl_sql(kind, &schema, &table);
        match self.run_catalog(&sql) {
            // The DDL is the last column of the first row (MySQL's SHOW CREATE
            // returns Table + Create Table; the others a single column).
            Ok((_, rows)) => {
                let ddl = rows
                    .first()
                    .and_then(|r| r.last())
                    .cloned()
                    .unwrap_or_default();
                if ddl.trim().is_empty() {
                    self.message = Some(t!("msg.db_ddl_none").to_string());
                    return;
                }
                self.cell_text = ddl;
                self.cell_pretty = false;
                self.view_scroll = 0;
                self.view = View::Cell;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Recompute the autocomplete popup for the cursor position.
    pub(super) fn refresh_popup(&mut self) {
        let line = self.query.lines()[self.query.row].clone();
        let s = self.completer.suggest(&line, self.query.col);
        self.popup = (!s.items.is_empty()).then_some(Popup {
            items: s.items,
            sel: 0,
            start: s.start,
        });
    }
}
