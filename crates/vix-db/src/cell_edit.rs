//! The results grid's editable-table workflow: previewing a single table so
//! its rows can be edited (`preview_selected`/`set_editable`), staging cell
//! edits, new rows and row deletions (`begin_cell_edit`/`key_cell_edit`,
//! `stage_new_row`, `toggle_delete_row`) and committing them
//! (`commit_edits`), following a foreign key, expanding a row into the
//! detail viewer, and the detail/DDL popup itself. Extracted from `lib.rs`
//! (T516): the one cohesive slice of [`Browser`]'s methods concerned with
//! *what a single cell or row means and how it gets changed*, as opposed to
//! running SQL.
//!
//! # Edit model
//!
//! A grid is *editable* only when it came from a single-table preview whose
//! table has a primary key (`edit_table` + `pk_cols`); any other result is
//! read-only. Changes are **staged** in [`Browser::staged`]
//! ([`crate::staging::Staged`]), keyed by the *underlying* row index (stable
//! across filtering and sorting, see
//! [`crate::results::Grid::selected_index`]); a new row is a blank row
//! appended to the grid. Nothing touches the database until `commit_edits`,
//! which builds one statement per changed row ([`crate::staging::statements`]),
//! re-reads every updated row first (optimistic conflict check), then runs
//! them all inside one `BEGIN`/`COMMIT`, rolling the whole batch back on an
//! error or on any statement that does not affect exactly one row (T777).
//! Writes require write mode. Owning spec: `crates/vix-db/spec/index.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{
    Browser, Outcome, Pane, Popup, TxState, View, catalog, columns, connect, execute, relations,
    staging,
};

impl Browser {
    /// The staged new value for a cell, if any (for the grid overlay).
    /// `row` is the underlying row index, not the filtered position.
    #[must_use]
    pub fn staged_value(&self, row: usize, col: usize) -> Option<&str> {
        self.edits.get(&(row, col)).map(|(_, new)| new.as_str())
    }

    /// What is staged for a row (modified / inserted / deleted), for the
    /// grid's colors. `row` is the underlying row index.
    #[must_use]
    pub fn row_change(&self, row: usize) -> Option<staging::ChangeKind> {
        self.staged.row_kind(&self.edits, row)
    }

    /// Whether anything is staged: a cell edit, a new row or a deletion.
    #[must_use]
    pub fn has_staged_changes(&self) -> bool {
        !self.staged.is_empty(&self.edits)
    }

    /// The staged `(updated, inserted, deleted)` row counts.
    #[must_use]
    pub fn staged_counts(&self) -> (usize, usize, usize) {
        self.staged.counts(&self.edits)
    }

    /// The values the cell being edited can be picked from (empty for free
    /// text): a boolean, enum or `CHECK (col IN (…))` column's values.
    #[must_use]
    pub fn edit_choices(&self) -> &[String] {
        self.editing_cell
            .and_then(|(_, col)| self.column_info(col))
            .map_or(&[], |c| c.choices.as_slice())
    }

    /// The `WHERE` condition the current table preview was narrowed by on
    /// the server (`F`), if any.
    #[must_use]
    pub fn server_filter(&self) -> Option<&str> {
        self.server_filter.as_deref()
    }

    /// The facts about grid column `col` of the editable table, if known.
    fn column_info(&self, col: usize) -> Option<&columns::ColumnInfo> {
        let name = self.grid.headers.get(col)?;
        self.columns.iter().find(|c| &c.name == name)
    }

    /// Whether the current grid can be edited in place (a single-table view
    /// with a primary key).
    #[must_use]
    pub fn editable(&self) -> bool {
        self.edit_table.is_some()
    }

    /// Keys in the cell / text viewer (scrolls long content). Also used for
    /// the AI prose answer, the row-expansion view and the DDL view, all of
    /// which are shown through `cell_text`.
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

    /// Preview the selected table or view (`SELECT * … LIMIT`). Functions are
    /// ignored. A successful preview also makes the grid editable when the
    /// table has a primary key.
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
                self.grid_source = Some((sql, Some((schema.clone(), table.clone()))));
                self.last_table = Some(table.clone());
                self.focus = Pane::Results;
                self.server_filter = None;
                self.set_editable(&schema, &table, kind);
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Mark the current grid as a preview of `schema`.`table`, editable when
    /// the table has a primary key (no key ⇒ read-only), and load its column
    /// facts. Clears any staged changes.
    pub(super) fn set_editable(&mut self, schema: &str, table: &str, kind: connect::Kind) {
        self.clear_staging();
        self.preview = Some((schema.to_string(), table.to_string()));
        let pk = self
            .run_catalog(&catalog::primary_key_sql(kind, schema, table))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter_map(|r| r.into_iter().next())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if pk.is_empty() {
            self.edit_table = None;
            self.pk_cols.clear();
            return;
        }
        self.pk_cols = pk;
        self.edit_table = Some((schema.to_string(), table.to_string()));
        // Column facts are best effort: a catalog view an older server lacks
        // only means plain free-text cells.
        let cols = self
            .run_catalog(&columns::columns_sql(kind, schema, table))
            .map(|(_, rows)| rows)
            .unwrap_or_default();
        let checks = self
            .run_catalog(&columns::checks_sql(kind, schema, table))
            .map(|(_, rows)| rows)
            .unwrap_or_default();
        self.columns = columns::from_rows(kind, &cols, &checks);
    }

    /// Mark the current grid as read-only (arbitrary query result), discarding
    /// any staged changes and forgetting the preview and its row filter.
    pub(super) fn set_uneditable(&mut self) {
        self.clear_staging();
        self.edit_table = None;
        self.pk_cols.clear();
        self.preview = None;
        self.server_filter = None;
    }

    /// Drop staged changes, the open cell editor and the column facts.
    fn clear_staging(&mut self) {
        self.edits.clear();
        self.staged.clear();
        self.columns.clear();
        self.editing_cell = None;
        self.edit_choice = None;
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
        // The foreign-key graph is re-read each time; it is a cheap catalog
        // query and keeps the lookup correct after schema changes.
        let (_, rels) = self.schema_facts();
        let edge = rels
            .iter()
            .find(|(child, child_col, _, _)| child == &table && child_col == &column);
        let Some((_, _, parent, parent_col)) = edge else {
            self.message = Some(t!("msg.db_fk_none").to_string());
            return;
        };
        let (parent, parent_col) = (parent.clone(), parent_col.clone());
        // SQLite may report no parent column when the FK targets the parent's
        // implicit primary key; `rowid` is the best available guess there.
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
                self.grid_source = Some((sql, None));
                self.last_table = Some(parent);
                self.set_uneditable();
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Begin editing the selected cell (staged, not applied until commit).
    /// Only editable single-table views with a primary key qualify; a row
    /// staged for deletion and a generated column cannot be edited.
    pub(super) fn begin_cell_edit(&mut self) {
        if !self.editable() {
            self.message = Some(t!("msg.db_edit_readonly").to_string());
            return;
        }
        let (Some(row), col) = (self.grid.selected_index(), self.grid.cur_col) else {
            return;
        };
        if self.staged.row_kind(&self.edits, row) == Some(staging::ChangeKind::Deleted) {
            self.message = Some(t!("msg.db_edit_deleted_row").to_string());
            return;
        }
        if let Some(info) = self.column_info(col).filter(|c| c.generated) {
            self.message =
                Some(t!("msg.db_edit_generated", column = info.name.clone()).to_string());
            return;
        }
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
        self.edit_choice = self
            .edit_choices()
            .iter()
            .position(|c| c == &self.edit_input);
        self.view = View::CellEdit;
    }

    /// Keys in the cell editor: typing edits the value, `↑`/`↓` pick from
    /// the column's allowed values (when it has some), `Enter` stages the
    /// value, `Esc` cancels.
    pub(super) fn key_cell_edit(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.edit_input.push(c),
            KeyCode::Backspace => {
                self.edit_input.pop();
            }
            KeyCode::Up | KeyCode::Down => {
                let n = self.edit_choices().len();
                if n > 0 {
                    let next = match (self.edit_choice, key.code) {
                        (None, KeyCode::Up) => n - 1,
                        (None, _) => 0,
                        (Some(i), KeyCode::Up) => (i + n - 1) % n,
                        (Some(i), _) => (i + 1) % n,
                    };
                    self.edit_input = self.edit_choices()[next].clone();
                    self.edit_choice = Some(next);
                }
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
                    // Staging the original value again is "undo": the entry
                    // goes, so commit never issues a no-op UPDATE.
                    staging::set_cell(&mut self.edits, row, col, &original, &new);
                }
                self.edit_choice = None;
                self.view = View::Workbench;
            }
            KeyCode::Esc => {
                self.editing_cell = None;
                self.edit_choice = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Stage a new, blank row (`a`): it is appended to the grid, selected,
    /// and filled with `i` like any other row; cells left blank take the
    /// column default on commit. `page` is the results pane height.
    pub(super) fn stage_new_row(&mut self, page: usize) {
        if !self.editable() {
            self.message = Some(t!("msg.db_edit_readonly").to_string());
            return;
        }
        let row = self.grid.rows.len();
        self.grid
            .rows
            .push(vec![String::new(); self.grid.headers.len()]);
        self.staged.insert_row(row);
        // A live `/` filter would hide the blank row; drop it so the new row
        // is visible and selectable.
        if !self.grid.filtered().contains(&row) {
            self.grid.filter.clear();
        }
        if let Some(pos) = self.grid.filtered().iter().position(|&r| r == row) {
            self.grid.sel = pos;
            self.grid.ensure_visible(page.max(1));
        }
        if let Some(col) =
            (0..self.grid.headers.len()).find(|&c| self.column_info(c).is_none_or(|i| !i.generated))
        {
            self.grid.cur_col = col;
        }
        self.message = Some(t!("msg.db_edit_row_added").to_string());
    }

    /// Stage the selected row for deletion, or restore it when it already is
    /// (`d`). A staged new row is simply discarded.
    pub(super) fn toggle_delete_row(&mut self) {
        if !self.editable() {
            self.message = Some(t!("msg.db_edit_readonly").to_string());
            return;
        }
        let Some(row) = self.grid.selected_index() else {
            return;
        };
        if self.staged.is_inserted(row) {
            self.discard_new_row(row);
            return;
        }
        self.message = Some(if self.staged.toggle_delete(row) {
            t!("msg.db_edit_row_deleted").to_string()
        } else {
            t!("msg.db_edit_row_restored").to_string()
        });
    }

    /// Un-stage every change to the selected row (`u`): its cell edits and
    /// its deletion, or the whole row when it is new.
    pub(super) fn discard_row_changes(&mut self) {
        let Some(row) = self.grid.selected_index() else {
            return;
        };
        if self.staged.is_inserted(row) {
            self.discard_new_row(row);
            return;
        }
        if self.staged.row_kind(&self.edits, row).is_some() {
            self.staged.discard_row(&mut self.edits, row);
            self.message = Some(t!("msg.db_edit_row_discarded").to_string());
        }
    }

    /// Remove a staged new row from the grid, keeping the selection in range.
    fn discard_new_row(&mut self, row: usize) {
        self.staged
            .remove_inserted(&mut self.edits, row, &mut self.grid.rows);
        let visible = self.grid.filtered().len();
        self.grid.sel = self.grid.sel.min(visible.saturating_sub(1));
        self.grid.scroll = self.grid.scroll.min(self.grid.sel);
        self.message = Some(t!("msg.db_edit_row_discarded").to_string());
    }

    /// Commit every staged change (`W`): one `DELETE`, `UPDATE` or `INSERT`
    /// per row, in one transaction, after re-checking each updated row for a
    /// concurrent change. If any statement fails, or affects other than
    /// exactly one row, the whole batch rolls back and nothing is kept.
    /// Needs write mode.
    pub(super) fn commit_edits(&mut self) {
        if !self.has_staged_changes() {
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
        let target = catalog::table_target(kind, &schema, &table);
        let statements = staging::statements(
            &staging::Target {
                kind,
                table: &target,
                headers: &self.grid.headers,
                pk: &pk_idx,
                rows: &self.grid.rows,
                columns: &self.columns,
            },
            &self.edits,
            &self.staged,
        );
        if !self.check_conflicts(&statements) {
            return;
        }
        // On production every write shows its SQL and asks first (T781).
        if self.active_environment().is_production() {
            self.pending_run = Some(execute::PendingRun::Commit {
                statements,
                schema,
                table,
                kind,
            });
            self.view = View::Confirm;
            return;
        }
        self.finish_commit(&statements, &schema, &table, kind);
    }

    /// Run the built statements in one transaction, then clear what was
    /// staged and refresh the preview (the tail of [`Self::commit_edits`],
    /// also reached from the production confirmation).
    pub(super) fn finish_commit(
        &mut self,
        statements: &[staging::Statement],
        schema: &str,
        table: &str,
        kind: connect::Kind,
    ) {
        if !self.apply_in_transaction(statements) {
            return;
        }
        let (updated, inserted, deleted) = self.staged_counts();
        self.edits.clear();
        self.staged.clear();
        self.message = Some(
            t!(
                "msg.db_changes_committed",
                updated = updated,
                inserted = inserted,
                deleted = deleted
            )
            .to_string(),
        );
        self.refresh_preview(schema, table, kind);
    }

    /// Re-read every row an `UPDATE` will touch and compare it with the
    /// values loaded into the grid (optimistic conflict check). Sets
    /// `self.message` and returns `false` on the first conflict or read error.
    fn check_conflicts(&mut self, statements: &[staging::Statement]) -> bool {
        for check in statements.iter().filter_map(|s| s.check.as_ref()) {
            match self.run_catalog(&check.sql) {
                Ok((_, rows)) => {
                    let live = rows.first().cloned().unwrap_or_default();
                    let changed = check
                        .columns
                        .iter()
                        .enumerate()
                        .find(|(i, _)| live.get(*i) != check.expected.get(*i));
                    if let Some((_, column)) = changed {
                        self.message =
                            Some(t!("msg.db_edit_conflict", column = column.clone()).to_string());
                        return false;
                    }
                }
                Err(e) => {
                    self.message = Some(e.to_string());
                    return false;
                }
            }
        }
        true
    }

    /// Run every statement in one transaction. Any error, or any statement
    /// that affects other than exactly one row, rolls the whole batch back
    /// and sets `self.message`. Returns whether the commit succeeded.
    ///
    /// Uses the real `BEGIN`/`COMMIT` on the persistent session and resets the
    /// client-side [`TxState`] afterwards, since a user transaction cannot be
    /// open while the commit runs.
    fn apply_in_transaction(&mut self, statements: &[staging::Statement]) -> bool {
        if let Err(e) = self.run_sql("BEGIN") {
            self.message = Some(e.to_string());
            return false;
        }
        for stmt in statements {
            let failure = match self.exec_sql(&stmt.sql) {
                Ok(1) => None,
                Ok(count) => Some(
                    t!(
                        "msg.db_edit_not_one_row",
                        count = count,
                        sql = stmt.sql.clone()
                    )
                    .to_string(),
                ),
                Err(e) => Some(e.to_string()),
            };
            if let Some(message) = failure {
                let _ = self.run_sql("ROLLBACK");
                self.tx = TxState::None;
                self.message = Some(message);
                return false;
            }
        }
        if let Err(e) = self.run_sql("COMMIT") {
            let _ = self.run_sql("ROLLBACK");
            self.tx = TxState::None;
            self.message = Some(e.to_string());
            return false;
        }
        self.tx = TxState::None;
        true
    }

    /// Re-run the table preview — with the active row filter, if any — to
    /// show the persisted values.
    pub(super) fn refresh_preview(&mut self, schema: &str, table: &str, kind: connect::Kind) {
        let sql = match &self.server_filter {
            Some(clause) => catalog::filtered_preview_sql(kind, schema, table, clause),
            None => catalog::preview_sql(kind, schema, table),
        };
        if let Ok((headers, rows)) = self.run_catalog(&sql) {
            self.grid.set(headers, rows);
            self.grid_source = Some((sql, Some((schema.to_string(), table.to_string()))));
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
                self.grid_source = Some((sql, None));
                self.focus = Pane::Results;
                self.set_uneditable();
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Show the relations view of the selected table or view in the grid
    /// (`R` in the tree, T779): outgoing foreign keys, then incoming ones, then
    /// dependent views. A failed foreign-key query reports its error; a failed
    /// view query retries the engine's fallback (`MySQL` before 8.0.13) and
    /// otherwise leaves the views out.
    pub(super) fn show_relations(&mut self) {
        let Some((schema, table, folder)) = self.tree.selected_object() else {
            return;
        };
        if folder == catalog::Folder::Functions {
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let q = relations::relations_sql(kind, &schema, &table);
        let mut found = Vec::new();
        for (sql, dir) in [
            (&q.outgoing, relations::RelationKind::Outgoing),
            (&q.incoming, relations::RelationKind::Incoming),
        ] {
            match self.run_catalog(sql) {
                Ok((_, rows)) => found.extend(relations::fk_relations(dir, &rows, &schema)),
                Err(e) => {
                    self.message = Some(e.to_string());
                    return;
                }
            }
        }
        let views = match self.run_catalog(&q.views) {
            Ok((_, rows)) => rows,
            Err(_) => q
                .views_fallback
                .as_deref()
                .and_then(|sql| self.run_catalog(sql).ok())
                .map(|(_, rows)| rows)
                .unwrap_or_default(),
        };
        found.extend(relations::view_relations(
            &views,
            &schema,
            &table,
            q.views_scan,
        ));
        let headers = relations::HEADER_KEYS
            .iter()
            .map(|k| t!(*k).to_string())
            .collect();
        let rows: Vec<Vec<String>> = found
            .into_iter()
            .map(|r| {
                vec![
                    t!(r.kind.label_key()).to_string(),
                    r.constraint,
                    r.from,
                    r.to,
                ]
            })
            .collect();
        self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
        self.grid.set(headers, rows);
        self.focus = Pane::Results;
        self.set_uneditable();
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
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Recompute the autocomplete popup for the cursor position. The popup is
    /// `None` when the completer has no candidates, which is how typing
    /// dismisses it. Aliases resolve over the whole statement under the
    /// cursor, not just its line (T780).
    pub(super) fn refresh_popup(&mut self) {
        let line = self.query.lines()[self.query.row].clone();
        let statement = self.query.statement_at_cursor().unwrap_or_default();
        let s = self.completer.suggest_in(&line, self.query.col, &statement);
        self.popup = (!s.items.is_empty()).then_some(Popup {
            items: s.items,
            sel: 0,
            start: s.start,
        });
    }
}
