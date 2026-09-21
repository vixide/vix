//! The workbench's side panels (history, saved queries, query log, ER
//! diagram, CSV/TSV import, results export) and the tree/editor/results
//! panes' own key dispatch (including the shared autocomplete-popup
//! handling and the chart/yank/format-at-cursor odds and ends that don't
//! belong to any other slice). Extracted from `lib.rs` (T516): the last
//! cohesive slice of [`Browser`]'s methods -- everything that isn't
//! connecting, running SQL, talking to the assistant, or editing a cell.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    Browser, Outcome, Pane, View, catalog, chart, editor, erd, export, format, import,
    object_triples,
};

impl Browser {
    /// Keys on the CSV/TSV import prompt.
    pub(super) fn key_import(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.import_path.push(c),
            KeyCode::Backspace => {
                self.import_path.pop();
            }
            KeyCode::Enter => self.do_import(),
            KeyCode::Esc => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Open the CSV/TSV import prompt. Import creates and writes a table, so it
    /// needs write mode.
    pub fn open_import(&mut self) {
        if self.conn.is_none() {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        }
        if !self.write_enabled {
            self.message = Some(t!("msg.db_read_only").to_string());
            return;
        }
        self.import_path.clear();
        self.view = View::Import;
    }

    /// Read the delimited file at `import_path`, create a table from its header,
    /// and load its rows.
    fn do_import(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let path = self.import_path.trim().to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                self.message = Some(e.to_string());
                return;
            }
        };
        let records = import::parse(&content, import::delimiter(&path));
        let table = import::table_name(&path);
        let statements = import::statements(kind, &table, &records);
        if statements.is_empty() {
            self.message = Some(t!("msg.db_import_empty").to_string());
            return;
        }
        for sql in &statements {
            if let Err(e) = self.run_sql(sql) {
                self.message = Some(e);
                return;
            }
        }
        let rows = records.len().saturating_sub(1);
        self.refresh_catalog();
        self.view = View::Workbench;
        self.message = Some(t!("msg.db_imported", count = rows, table = table).to_string());
    }

    /// Keys on the query-history and saved-queries lists.
    pub(super) fn key_query_list(&mut self, key: KeyEvent) -> Outcome {
        let len = if self.view == View::History {
            self.history.entries.len()
        } else {
            self.saved.queries.len()
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.list_sel = self.list_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.list_sel = (self.list_sel + 1).min(len.saturating_sub(1));
            }
            KeyCode::Home => self.list_sel = 0,
            KeyCode::End => self.list_sel = len.saturating_sub(1),
            KeyCode::Enter => {
                let sql = if self.view == View::History {
                    self.history.entries.get(self.list_sel).cloned()
                } else {
                    self.saved.queries.get(self.list_sel).map(|q| q.sql.clone())
                };
                if let Some(sql) = sql {
                    self.insert_statement(&sql);
                    self.view = View::Workbench;
                    self.focus = Pane::Editor;
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.view == View::History {
                    if self.list_sel < self.history.entries.len() {
                        self.history.entries.remove(self.list_sel);
                        self.dirty.history = true;
                    }
                } else if self.list_sel < self.saved.queries.len() {
                    self.saved.queries.remove(self.list_sel);
                    self.dirty.saved = true;
                }
                self.list_sel = self.list_sel.min(len.saturating_sub(2));
            }
            KeyCode::Esc | KeyCode::Char('q') => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys on the save-query naming prompt.
    pub(super) fn key_save_name(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.save_name.push(c),
            KeyCode::Backspace => {
                self.save_name.pop();
            }
            KeyCode::Enter => {
                if self.save_name.trim().is_empty() {
                    self.message = Some(t!("msg.db_name_required").to_string());
                } else {
                    let name = self.save_name.trim().to_string();
                    let sql = std::mem::take(&mut self.save_sql);
                    self.saved.upsert(&name, &sql);
                    self.dirty.saved = true;
                    self.message = Some(t!("msg.db_saved_query", name = name).to_string());
                    self.view = View::Workbench;
                }
            }
            KeyCode::Esc => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Open the naming prompt for saving the statement at the cursor.
    pub fn open_save_name(&mut self) {
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        self.save_sql = stmt;
        self.save_name.clear();
        self.view = View::SaveName;
    }

    /// Append `sql` to the editor as its own statement and put the cursor on
    /// it.
    pub(super) fn insert_statement(&mut self, sql: &str) {
        let text = self.query.text();
        let joined = if text.trim().is_empty() {
            sql.to_string()
        } else {
            let sep = if text.trim_end().ends_with(';') {
                "\n"
            } else {
                ";\n"
            };
            format!("{}{sep}{sql}", text.trim_end())
        };
        self.query = editor::Query::default();
        for (i, line) in joined.split('\n').enumerate() {
            if i > 0 {
                self.query.newline();
            }
            for c in line.chars() {
                self.query.insert_char(c);
            }
        }
        self.popup = None;
    }

    /// Keys in the session query log (scroll; Enter reloads the statement).
    pub(super) fn key_log(&mut self, key: KeyEvent) -> Outcome {
        let len = self.log.entries.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.list_sel = self.list_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.list_sel = (self.list_sel + 1).min(len.saturating_sub(1));
            }
            KeyCode::Home => self.list_sel = 0,
            KeyCode::End => self.list_sel = len.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(entry) = self.log.entries.get(self.list_sel) {
                    let sql = entry.sql.clone();
                    self.insert_statement(&sql);
                    self.view = View::Workbench;
                    self.focus = Pane::Editor;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Open the session query log.
    pub fn open_log(&mut self) {
        self.list_sel = 0;
        self.view = View::Log;
    }

    /// Keys in the ER-diagram viewer (scroll; `y` yanks the Mermaid text).
    pub(super) fn key_erd(&mut self, key: KeyEvent) -> Outcome {
        let last = self.cell_text.lines().count().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.view_scroll = self.view_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.view_scroll = (self.view_scroll + 1).min(last);
            }
            KeyCode::PageUp => self.view_scroll = self.view_scroll.saturating_sub(10),
            KeyCode::PageDown => self.view_scroll = (self.view_scroll + 10).min(last),
            KeyCode::Home => self.view_scroll = 0,
            KeyCode::End => self.view_scroll = last,
            KeyCode::Char('y') => {
                let text = self.cell_text.clone();
                self.yank(&text);
            }
            KeyCode::Esc | KeyCode::Char('q') => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Build an ER diagram from the live schema and show it in the viewer.
    pub fn generate_erd(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let columns: Vec<(String, String, String)> = self
            .run_catalog(catalog::columns_typed_sql(kind))
            .map(|(_, rows)| object_triples(rows))
            .unwrap_or_default();
        if columns.is_empty() {
            self.message = Some(t!("msg.db_erd_empty").to_string());
            return;
        }
        let relationships: Vec<(String, String, String, String)> = self
            .run_catalog(catalog::relationships_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 4)
                    .map(|r| (r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.cell_text = erd::mermaid(&columns, &relationships);
        self.view_scroll = 0;
        self.message = Some(t!("msg.db_erd_built", count = columns.len()).to_string());
        self.view = View::Erd;
    }

    /// Keys on the export dialog.
    pub(super) fn key_export(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Left => {
                self.export_format =
                    (self.export_format + export::FORMATS.len() - 1) % export::FORMATS.len();
            }
            KeyCode::Right => self.export_format = (self.export_format + 1) % export::FORMATS.len(),
            KeyCode::Tab => self.export_clipboard = !self.export_clipboard,
            KeyCode::Char(c) => self.export_path.push(c),
            KeyCode::Backspace => {
                self.export_path.pop();
            }
            KeyCode::Enter => self.run_export(),
            KeyCode::Esc => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Open the export dialog for the current results (no-op when empty).
    pub fn open_export(&mut self) {
        if self.grid.headers.is_empty() {
            return;
        }
        self.export_path = format!("vix-export.{}", export::FORMATS[self.export_format].label());
        self.view = View::Export;
    }

    /// Render the current (filtered, sorted) grid and write it to the chosen
    /// destination.
    fn run_export(&mut self) {
        let format = export::FORMATS[self.export_format % export::FORMATS.len()];
        let order = self.grid.filtered();
        let rows: Vec<&Vec<String>> = order.iter().map(|&i| &self.grid.rows[i]).collect();
        let table = self
            .last_table
            .clone()
            .unwrap_or_else(|| "vix_export".to_string());
        let text = export::render(format, &self.grid.headers, &rows, &table);
        if self.export_clipboard {
            self.yank(&text);
            self.view = View::Workbench;
            return;
        }
        let path = self.export_path.trim();
        if path.is_empty() {
            self.message = Some(t!("msg.db_name_required").to_string());
            return;
        }
        match std::fs::write(path, text) {
            Ok(()) => {
                self.message = Some(t!("msg.db_exported", path = path).to_string());
                self.view = View::Workbench;
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Open the query-history list.
    pub fn open_history(&mut self) {
        self.list_sel = 0;
        self.view = View::History;
    }

    /// Open the saved-queries list.
    pub fn open_saved(&mut self) {
        self.list_sel = 0;
        self.view = View::Saved;
    }

    /// Keys in the schema tree (including the live search filter).
    pub(super) fn key_tree(&mut self, key: KeyEvent, page: usize) {
        if self.tree.filtering {
            match key.code {
                KeyCode::Char(c) => self.tree.filter_key(Some(c)),
                KeyCode::Backspace => self.tree.filter_key(None),
                KeyCode::Enter => self.tree.filtering = false,
                KeyCode::Esc => {
                    self.tree.filtering = false;
                    self.tree.filter.clear();
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('/') => {
                self.tree.filtering = true;
                return;
            }
            KeyCode::Char('p') => {
                self.preview_selected();
                return;
            }
            _ => {}
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.tree.step(true, 1),
            KeyCode::Down | KeyCode::Char('j') => self.tree.step(false, 1),
            KeyCode::PageUp => self.tree.step(true, page.max(1)),
            KeyCode::PageDown => self.tree.step(false, page.max(1)),
            KeyCode::Home => self.tree.sel = 0,
            KeyCode::End => self.tree.sel = self.tree.rows().len().saturating_sub(1),
            KeyCode::Left => self.tree.toggle(Some(false)),
            KeyCode::Right => self.tree.toggle(Some(true)),
            KeyCode::Char(' ') => self.tree.toggle(None),
            KeyCode::Enter => {
                if self.tree.selected_object().is_some() {
                    self.show_detail(catalog::Detail::Columns);
                } else {
                    self.tree.toggle(None);
                }
            }
            KeyCode::Char('i') => self.show_detail(catalog::Detail::Indexes),
            KeyCode::Char('f') => self.show_detail(catalog::Detail::ForeignKeys),
            KeyCode::Char('g') => self.show_detail(catalog::Detail::Triggers),
            KeyCode::Char('x') => self.show_detail(catalog::Detail::Constraints),
            KeyCode::Char('s') => self.show_detail(catalog::Detail::Stats),
            KeyCode::Char('D') => self.show_ddl(),
            KeyCode::Char('r') => self.refresh_catalog(),
            _ => {}
        }
    }

    /// Keys in the SQL editor (including the autocomplete popup).
    pub(super) fn key_editor(&mut self, key: KeyEvent, page: usize) {
        if let Some(popup) = self.popup.as_mut() {
            match key.code {
                KeyCode::Up => {
                    popup.sel = popup.sel.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    popup.sel = (popup.sel + 1).min(popup.items.len() - 1);
                    return;
                }
                KeyCode::Tab => {
                    let (start, text) = (popup.start, popup.items[popup.sel].clone());
                    self.query.replace_prefix(start, &text);
                    self.popup = None;
                    return;
                }
                KeyCode::Esc => {
                    self.popup = None;
                    return;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.insert_char(c);
                self.refresh_popup();
            }
            KeyCode::Enter => {
                self.query.newline();
                self.popup = None;
            }
            KeyCode::Backspace => {
                self.query.backspace();
                self.refresh_popup();
            }
            KeyCode::Delete => self.query.delete(),
            KeyCode::Up => self.close_popup_and(|q| q.arrow(0, -1)),
            KeyCode::Down => self.close_popup_and(|q| q.arrow(0, 1)),
            KeyCode::Left => self.close_popup_and(|q| q.arrow(-1, 0)),
            KeyCode::Right => self.close_popup_and(|q| q.arrow(1, 0)),
            KeyCode::Home => self.close_popup_and(|q| q.home_end(true)),
            KeyCode::End => self.close_popup_and(|q| q.home_end(false)),
            KeyCode::PageUp => self.close_popup_and(move |q| q.page(true, page)),
            KeyCode::PageDown => self.close_popup_and(move |q| q.page(false, page)),
            _ => {}
        }
    }

    /// Close the popup, then apply `f` to the query editor.
    fn close_popup_and(&mut self, f: impl FnOnce(&mut editor::Query)) {
        self.popup = None;
        f(&mut self.query);
    }

    /// Keys in the results grid (including the live filter).
    pub(super) fn key_results(&mut self, key: KeyEvent, page: usize) {
        if self.grid.filtering {
            match key.code {
                KeyCode::Char(c) => self.grid.filter_key(Some(c)),
                KeyCode::Backspace => self.grid.filter_key(None),
                KeyCode::Enter => self.grid.filtering = false,
                KeyCode::Esc => {
                    self.grid.filtering = false;
                    self.grid.filter.clear();
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.grid.step(true, 1),
            KeyCode::Down | KeyCode::Char('j') => self.grid.step(false, 1),
            KeyCode::PageUp => self.grid.step(true, page.max(1)),
            KeyCode::PageDown => self.grid.step(false, page.max(1)),
            KeyCode::Home => self.grid.home_end(true),
            KeyCode::End => self.grid.home_end(false),
            KeyCode::Left | KeyCode::Char('h') => self.grid.select_col(true),
            KeyCode::Right | KeyCode::Char('l') => self.grid.select_col(false),
            KeyCode::Char('s') => self.grid.cycle_sort(),
            KeyCode::Char('/') => self.grid.filtering = true,
            KeyCode::Char('y') => {
                if let Some(cell) = self.grid.selected_cell().map(str::to_string) {
                    self.yank(&cell);
                }
            }
            KeyCode::Char('Y') => {
                if let Some(row) = self.grid.selected_row().map(|r| r.join("\t")) {
                    self.yank(&row);
                }
            }
            KeyCode::Char('v') => {
                if let Some(cell) = self.grid.selected_cell() {
                    self.cell_text = cell.to_string();
                    self.cell_pretty = false;
                    self.view_scroll = 0;
                    self.view = View::Cell;
                }
            }
            KeyCode::Char('e') => self.open_export(),
            KeyCode::Char('x') => self.expand_row(),
            KeyCode::Char('f') => self.follow_fk(),
            KeyCode::Char('c') => self.chart_results(),
            KeyCode::Char('i') => self.begin_cell_edit(),
            KeyCode::Char('W') => self.commit_edits(),
            _ => {}
        }
    }

    /// Render the current two-column result as a horizontal ASCII bar chart in
    /// the text viewer: the first column labels each bar, the last numeric
    /// column sizes it.
    fn chart_results(&mut self) {
        let order = self.grid.filtered();
        let rows: Vec<&Vec<String>> = order.iter().map(|&i| &self.grid.rows[i]).collect();
        match chart::bars(&self.grid.headers, &rows) {
            Some(text) => {
                self.cell_text = text;
                self.cell_pretty = false;
                self.view_scroll = 0;
                self.view = View::Cell;
            }
            None => self.message = Some(t!("msg.db_chart_needs_number").to_string()),
        }
    }

    /// Copy `text` to the system clipboard.
    pub(super) fn yank(&mut self, text: &str) {
        // Go through the shared, lock-guarded helper so this never races the
        // editor's clipboard access on the (non-thread-safe) platform backend.
        self.message = Some(match vix_clipboard::set(text) {
            Ok(()) => t!("msg.db_copied").to_string(),
            Err(e) => e.to_string(),
        });
    }

    /// Beautify the statement at the cursor in place.
    pub fn format_at_cursor(&mut self) {
        if let Some(stmt) = self.query.statement_at_cursor() {
            self.query
                .replace_statement_at_cursor(&format::beautify(&stmt));
            self.popup = None;
        }
    }
}
