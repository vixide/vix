//! The DB workbench: connection management, schema browsing, and SQL queries.
//!
//! Implements `spec/db`: a **DB** menu opens a full-screen overlay that walks
//! through saved connections (passwords are prompted per session and never
//! written to disk), then presents a three-pane workbench — schema tree on
//! the left, a syntax-highlighted SQL editor with autocomplete on the right,
//! and a filterable results grid below it. Queries run through the engine's
//! own CLI client (`sqlite3` / `psql` / `mysql`), mirroring how the git
//! module shells out to `git`.
//!
//! Pure state lives in the submodules ([`catalog`], [`complete`], [`editor`],
//! [`format`], [`highlight`], [`results`]); [`connect`] holds the one process
//! wrapper. This module is the state machine the host drives with keys.

pub mod catalog;
pub mod complete;
pub mod connect;
pub mod editor;
pub mod export;
pub mod format;
pub mod highlight;
pub mod results;
pub mod session;
pub mod store;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the host should do after [`Browser::handle_key`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The key was handled; nothing further.
    Consumed,
    /// Close the DB overlay.
    Close,
}

/// Which overlay screen is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    /// The saved-connections list.
    Connections,
    /// The add/edit connection form.
    Form,
    /// The connect-time password prompt.
    Password,
    /// The tree + editor + results workbench.
    Workbench,
    /// The query-history list (Ctrl+R).
    History,
    /// The saved-queries list (Ctrl+B).
    Saved,
    /// Naming prompt when saving the statement at the cursor (Ctrl+S).
    SaveName,
    /// Write/DDL confirmation before a pending execution.
    Confirm,
    /// Full-content viewer for the selected results cell.
    Cell,
    /// The results-export dialog.
    Export,
}

/// An execution awaiting write/DDL confirmation.
#[derive(Debug, Clone)]
enum PendingRun {
    /// One statement (possibly EXPLAIN-wrapped).
    One(String),
    /// Every statement in the buffer, in order.
    All(Vec<String>),
}

/// Which persisted stores changed since the host last collected them.
#[derive(Debug, Clone, Copy, Default)]
struct Dirty {
    /// The connection list changed.
    connections: bool,
    /// The query history changed.
    history: bool,
    /// The saved queries changed.
    saved: bool,
}

/// Which workbench pane has focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The schema tree.
    Tree,
    /// The SQL query editor.
    Editor,
    /// The results grid.
    Results,
}

impl Pane {
    /// The next pane in Tab order.
    #[must_use]
    pub fn next(self) -> Pane {
        match self {
            Pane::Tree => Pane::Editor,
            Pane::Editor => Pane::Results,
            Pane::Results => Pane::Tree,
        }
    }

    /// The previous pane in Tab order.
    #[must_use]
    pub fn prev(self) -> Pane {
        self.next().next()
    }
}

/// Number of editable form fields (name, kind, file, host, port, user,
/// database).
pub const FORM_FIELDS: usize = 7;

/// Index of the kind row in the form (cycled, not typed).
pub const FORM_KIND: usize = 1;

/// The add/edit connection form.
#[derive(Debug, Clone, Default)]
pub struct Form {
    /// Text of the fields, by index: 0 name, 2 file, 3 host, 4 port, 5 user,
    /// 6 database (index 1 is [`Form::kind`]).
    pub fields: [String; FORM_FIELDS],
    /// The engine picked on the kind row.
    pub kind: connect::Kind,
    /// The selected field row.
    pub sel: usize,
    /// Index of the connection being edited; `None` when adding.
    pub editing: Option<usize>,
}

impl Form {
    /// A form pre-filled from `conn` (for editing).
    #[must_use]
    pub fn from_connection(conn: &connect::Connection, editing: Option<usize>) -> Form {
        let mut f = Form { kind: conn.kind, editing, ..Form::default() };
        f.fields[0].clone_from(&conn.name);
        f.fields[2].clone_from(&conn.file);
        f.fields[3].clone_from(&conn.host);
        f.fields[4].clone_from(&conn.port);
        f.fields[5].clone_from(&conn.user);
        f.fields[6].clone_from(&conn.database);
        f
    }

    /// The connection this form currently describes.
    #[must_use]
    pub fn to_connection(&self) -> connect::Connection {
        connect::Connection {
            name: self.fields[0].clone(),
            kind: self.kind,
            file: self.fields[2].clone(),
            host: self.fields[3].clone(),
            port: self.fields[4].clone(),
            user: self.fields[5].clone(),
            database: self.fields[6].clone(),
        }
    }
}

/// The autocomplete popup: candidates plus the prefix start they replace.
#[derive(Debug, Clone, Default)]
pub struct Popup {
    /// Candidate completions.
    pub items: Vec<String>,
    /// Highlighted candidate.
    pub sel: usize,
    /// Char column where the replaced prefix begins.
    pub start: usize,
}

/// Visible page sizes (rows) of the workbench panes, from the last layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pages {
    /// Schema-tree rows.
    pub tree: usize,
    /// Editor rows.
    pub editor: usize,
    /// Results rows.
    pub results: usize,
}

/// The DB overlay: saved connections, the active session, and the workbench.
pub struct Browser {
    /// Saved connections (mirrors the persisted setting).
    pub connections: Vec<connect::Connection>,
    /// Which screen is showing.
    pub view: View,
    /// Connections-list selection.
    pub sel: usize,
    /// Connections-list scroll.
    pub scroll: usize,
    /// The add/edit form.
    pub form: Form,
    /// Password being typed (session memory only; never persisted).
    pub password: String,
    /// Connection index awaiting its password.
    pending: Option<usize>,
    /// The active connection.
    pub conn: Option<connect::Connection>,
    /// The live database session (persistent sqlx connection), when connected.
    session: Option<session::Session>,
    /// Schema tree.
    pub tree: catalog::Tree,
    /// SQL query editor.
    pub query: editor::Query,
    /// Results grid.
    pub grid: results::Grid,
    /// Autocomplete engine (fed at connect time).
    completer: complete::Completer,
    /// Open autocomplete popup, if any.
    pub popup: Option<Popup>,
    /// Focused workbench pane.
    pub focus: Pane,
    /// Status or error line shown at the bottom of the overlay.
    pub message: Option<String>,
    /// Which persisted stores changed and should be collected by the host.
    dirty: Dirty,
    /// Executed-statement history (host loads/persists it).
    pub history: store::History,
    /// Saved queries (host loads/persists them).
    pub saved: store::Saved,
    /// Selection in the history / saved-queries lists.
    pub list_sel: usize,
    /// Name being typed on the save-query prompt.
    pub save_name: String,
    /// The statement captured when the save-query prompt opened.
    save_sql: String,
    /// Execution awaiting write/DDL confirmation.
    pending_run: Option<PendingRun>,
    /// Raw content shown by the cell viewer.
    pub cell_text: String,
    /// Whether the cell viewer pretty-prints JSON content.
    pub cell_pretty: bool,
    /// Selected index into [`export::FORMATS`] on the export dialog.
    pub export_format: usize,
    /// Destination path being typed on the export dialog.
    pub export_path: String,
    /// Whether the export goes to the clipboard instead of a file.
    pub export_clipboard: bool,
    /// The table behind the last preview, naming SQL-INSERT exports.
    last_table: Option<String>,
}

impl Browser {
    /// Open the overlay on the connections list.
    #[must_use]
    pub fn new(connections: Vec<connect::Connection>) -> Browser {
        Browser {
            connections,
            view: View::Connections,
            sel: 0,
            scroll: 0,
            form: Form::default(),
            password: String::new(),
            pending: None,
            conn: None,
            session: None,
            tree: catalog::Tree::default(),
            query: editor::Query::default(),
            grid: results::Grid::default(),
            completer: complete::Completer::default(),
            popup: None,
            focus: Pane::Editor,
            message: None,
            dirty: Dirty::default(),
            history: store::History::default(),
            saved: store::Saved::default(),
            list_sel: 0,
            save_name: String::new(),
            save_sql: String::new(),
            pending_run: None,
            cell_text: String::new(),
            cell_pretty: false,
            export_format: 0,
            export_path: String::new(),
            export_clipboard: false,
            last_table: None,
        }
    }

    /// The history if it changed since the last call (for the host to
    /// persist), clearing the flag.
    pub fn take_dirty_history(&mut self) -> Option<store::History> {
        if self.dirty.history {
            self.dirty.history = false;
            Some(self.history.clone())
        } else {
            None
        }
    }

    /// The saved queries if they changed since the last call (for the host to
    /// persist), clearing the flag.
    pub fn take_dirty_saved(&mut self) -> Option<store::Saved> {
        if self.dirty.saved {
            self.dirty.saved = false;
            Some(self.saved.clone())
        } else {
            None
        }
    }

    /// The connections list if it changed since the last call (for the host
    /// to persist), clearing the flag.
    pub fn take_dirty_connections(&mut self) -> Option<Vec<connect::Connection>> {
        if self.dirty.connections {
            self.dirty.connections = false;
            Some(self.connections.clone())
        } else {
            None
        }
    }

    /// Route a key to the current view.
    pub fn handle_key(&mut self, key: KeyEvent, pages: Pages) -> Outcome {
        self.message = None;
        match self.view {
            View::Connections => self.key_connections(key),
            View::Form => self.key_form(key),
            View::Password => self.key_password(key),
            View::Workbench => self.key_workbench(key, pages),
            View::History | View::Saved => self.key_query_list(key),
            View::SaveName => self.key_save_name(key),
            View::Confirm => self.key_confirm(key),
            View::Cell => self.key_cell(key),
            View::Export => self.key_export(key),
        }
    }

    /// Keys on the query-history and saved-queries lists.
    fn key_query_list(&mut self, key: KeyEvent) -> Outcome {
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
    fn key_save_name(&mut self, key: KeyEvent) -> Outcome {
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

    /// Keys on the write/DDL confirmation.
    fn key_confirm(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.view = View::Workbench;
                match self.pending_run.take() {
                    Some(PendingRun::One(sql)) => self.run_statement(&sql),
                    Some(PendingRun::All(stmts)) => self.run_all(&stmts),
                    None => {}
                }
            }
            KeyCode::Esc | KeyCode::Char('n' | 'q') => {
                self.pending_run = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys in the cell viewer.
    fn key_cell(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char('p') => self.cell_pretty = !self.cell_pretty,
            KeyCode::Char('y') => {
                let text = self.cell_text.clone();
                self.yank(&text);
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys on the export dialog.
    fn key_export(&mut self, key: KeyEvent) -> Outcome {
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

    /// Keys on the saved-connections list.
    fn key_connections(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.sel = (self.sel + 1).min(self.connections.len().saturating_sub(1));
            }
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = self.connections.len().saturating_sub(1),
            KeyCode::Enter => self.start_connect(self.sel),
            KeyCode::Char('a') => {
                self.form = Form::default();
                self.view = View::Form;
            }
            KeyCode::Char('e') => {
                if let Some(conn) = self.connections.get(self.sel) {
                    self.form = Form::from_connection(conn, Some(self.sel));
                    self.view = View::Form;
                }
            }
            KeyCode::Char('d') => {
                if self.sel < self.connections.len() {
                    self.connections.remove(self.sel);
                    self.sel = self.sel.min(self.connections.len().saturating_sub(1));
                    self.dirty.connections = true;
                }
            }
            KeyCode::Esc => {
                if self.conn.is_some() {
                    self.view = View::Workbench;
                } else {
                    return Outcome::Close;
                }
            }
            KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys on the add/edit connection form.
    fn key_form(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Up => self.form.sel = self.form.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.form.sel = (self.form.sel + 1) % FORM_FIELDS,
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.form.sel == FORM_KIND => {
                self.form.kind = self.form.kind.next();
            }
            KeyCode::Char(c) if self.form.sel != FORM_KIND => self.form.fields[self.form.sel].push(c),
            KeyCode::Backspace if self.form.sel != FORM_KIND => {
                self.form.fields[self.form.sel].pop();
            }
            KeyCode::Enter => {
                if self.form.fields[0].trim().is_empty() {
                    self.message = Some(t!("msg.db_name_required").to_string());
                } else {
                    let conn = self.form.to_connection();
                    match self.form.editing {
                        Some(i) if i < self.connections.len() => self.connections[i] = conn,
                        _ => self.connections.push(conn),
                    }
                    self.dirty.connections = true;
                    self.view = View::Connections;
                }
            }
            KeyCode::Esc => self.view = View::Connections,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys on the password prompt.
    fn key_password(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.password.push(c),
            KeyCode::Backspace => {
                self.password.pop();
            }
            KeyCode::Enter => {
                if let Some(i) = self.pending.take()
                    && let Some(conn) = self.connections.get(i).cloned()
                {
                    let password = std::mem::take(&mut self.password);
                    self.finish_connect(&conn, &password);
                }
            }
            KeyCode::Esc => {
                self.password.clear();
                self.pending = None;
                self.view = View::Connections;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Begin connecting to connection `idx`, via the password prompt when the
    /// engine needs one.
    fn start_connect(&mut self, idx: usize) {
        let Some(conn) = self.connections.get(idx).cloned() else {
            return;
        };
        if conn.needs_password() {
            self.pending = Some(idx);
            self.password.clear();
            self.view = View::Password;
        } else {
            self.finish_connect(&conn, "");
        }
    }

    /// Run `sql` on the live session.
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is
    /// open.
    fn run_sql(&mut self, sql: &str) -> Result<session::Table, String> {
        match self.session.as_mut() {
            Some(session) => session.run(sql),
            None => Err(t!("msg.db_not_connected").to_string()),
        }
    }

    /// Open the persistent session, load the catalog, and enter the
    /// workbench.
    fn finish_connect(&mut self, conn: &connect::Connection, password: &str) {
        let url = connect::url(conn, password);
        match session::Session::connect(&url) {
            Ok(session) => self.session = Some(session),
            Err(e) => {
                self.message = Some(e);
                self.view = View::Connections;
                return;
            }
        }
        match self.run_sql(catalog::objects_sql(conn.kind)) {
            Ok((_, rows)) => {
                self.tree = catalog::Tree::from_objects(&object_triples(rows));
                self.load_columns(conn.kind);
                self.message = Some(t!("msg.db_connected", name = conn.name).to_string());
                self.conn = Some(conn.clone());
                self.view = View::Workbench;
                self.focus = Pane::Editor;
            }
            Err(e) => {
                self.session = None;
                self.message = Some(e);
                self.view = View::Connections;
            }
        }
    }

    /// Fetch every `(table, column)` pair for autocomplete (best effort).
    fn load_columns(&mut self, kind: connect::Kind) {
        let columns = self
            .run_sql(catalog::columns_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 2)
                    .map(|r| (r[0].clone(), r[1].clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.completer.set_schema(self.tree.table_names(), columns);
    }

    /// Drop the active connection — closing the persistent session and its
    /// in-memory password — and return to the connections list.
    pub fn disconnect(&mut self) {
        self.conn = None;
        self.session = None;
        self.password.clear();
        self.pending = None;
        self.tree = catalog::Tree::default();
        self.completer = complete::Completer::default();
        self.popup = None;
        self.view = View::Connections;
        self.message = Some(t!("msg.db_disconnected").to_string());
    }

    /// Re-run the catalog queries on the live session.
    pub fn refresh_catalog(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        match self.run_sql(catalog::objects_sql(kind)) {
            Ok((_, rows)) => {
                self.tree = catalog::Tree::from_objects(&object_triples(rows));
                self.load_columns(kind);
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Keys inside the workbench, after the pane-independent chords.
    fn key_workbench(&mut self, key: KeyEvent, pages: Pages) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::F(5) => {
                self.execute();
                return Outcome::Consumed;
            }
            KeyCode::F(6) => {
                self.explain(false);
                return Outcome::Consumed;
            }
            KeyCode::F(7) => {
                self.explain(true);
                return Outcome::Consumed;
            }
            KeyCode::F(9) => {
                self.execute_all();
                return Outcome::Consumed;
            }
            KeyCode::Enter if ctrl => {
                self.execute();
                return Outcome::Consumed;
            }
            KeyCode::Char('r' | 'R') if ctrl => {
                self.open_history();
                return Outcome::Consumed;
            }
            KeyCode::Char('b' | 'B') if ctrl => {
                self.open_saved();
                return Outcome::Consumed;
            }
            KeyCode::Char('s' | 'S') if ctrl => {
                self.open_save_name();
                return Outcome::Consumed;
            }
            KeyCode::Char('f' | 'F') if alt => {
                self.format_at_cursor();
                return Outcome::Consumed;
            }
            KeyCode::Tab if self.popup.is_none() => {
                self.focus = self.focus.next();
                return Outcome::Consumed;
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                return Outcome::Consumed;
            }
            KeyCode::Esc if self.popup.is_none() && !self.grid.filtering && !self.tree.filtering => {
                self.view = View::Connections;
                return Outcome::Consumed;
            }
            _ => {}
        }
        match self.focus {
            Pane::Tree => self.key_tree(key, pages.tree),
            Pane::Editor => self.key_editor(key, pages.editor),
            Pane::Results => self.key_results(key, pages.results),
        }
        Outcome::Consumed
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
    fn insert_statement(&mut self, sql: &str) {
        let text = self.query.text();
        let joined = if text.trim().is_empty() {
            sql.to_string()
        } else {
            let sep = if text.trim_end().ends_with(';') { "\n" } else { ";\n" };
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

    /// Keys in the schema tree (including the live search filter).
    fn key_tree(&mut self, key: KeyEvent, page: usize) {
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
            KeyCode::Char('r') => self.refresh_catalog(),
            _ => {}
        }
    }

    /// Keys in the SQL editor (including the autocomplete popup).
    fn key_editor(&mut self, key: KeyEvent, page: usize) {
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
    fn key_results(&mut self, key: KeyEvent, page: usize) {
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
                    self.view = View::Cell;
                }
            }
            KeyCode::Char('e') => self.open_export(),
            _ => {}
        }
    }

    /// Open the export dialog for the current results (no-op when empty).
    pub fn open_export(&mut self) {
        if self.grid.headers.is_empty() {
            return;
        }
        self.export_path = format!("vix-export.{}", export::FORMATS[self.export_format].label());
        self.view = View::Export;
    }

    /// A short description of the execution awaiting confirmation, for the
    /// confirmation view: the statement itself, or the statement count for a
    /// run-all.
    #[must_use]
    pub fn pending_summary(&self) -> Option<String> {
        match self.pending_run.as_ref()? {
            PendingRun::One(sql) => Some(sql.clone()),
            PendingRun::All(stmts) => {
                Some(format!("{} × … {}", stmts.len(), stmts.first().map_or("", String::as_str)))
            }
        }
    }

    /// Execute the statement at the cursor, showing its rows in the grid.
    /// Write and DDL statements go through the confirmation view first.
    pub fn execute(&mut self) {
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        if editor::is_write_statement(&stmt) {
            self.pending_run = Some(PendingRun::One(stmt));
            self.view = View::Confirm;
        } else {
            self.run_statement(&stmt);
        }
    }

    /// Execute every statement in the buffer, in order (confirmed once when
    /// any of them writes). The grid shows the last statement's rows.
    pub fn execute_all(&mut self) {
        let text = self.query.text();
        let stmts: Vec<String> = editor::statement_spans(&text)
            .iter()
            .map(|&(s, e)| text.chars().skip(s).take(e - s).collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if stmts.is_empty() {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        }
        if stmts.iter().any(|s| editor::is_write_statement(s)) {
            self.pending_run = Some(PendingRun::All(stmts));
            self.view = View::Confirm;
        } else {
            self.run_all(&stmts);
        }
    }

    /// Run the EXPLAIN (or engine equivalent) of the statement at the cursor,
    /// with the plan-doctor full-scan insight. `EXPLAIN ANALYZE` really
    /// executes the statement, so a write inside still needs confirmation.
    pub fn explain(&mut self, analyze: bool) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let wrapped = catalog::explain_sql(kind, &stmt, analyze);
        if analyze && editor::is_write_statement(&stmt) {
            self.pending_run = Some(PendingRun::One(wrapped));
            self.view = View::Confirm;
            return;
        }
        match self.run_sql(&wrapped) {
            Ok((headers, rows)) => {
                let insight = catalog::scan_insight(kind, &rows);
                self.message = Some(if insight {
                    t!("msg.db_insight_scan").to_string()
                } else {
                    t!("msg.db_rows", count = rows.len()).to_string()
                });
                self.grid.set(headers, rows);
                self.focus = Pane::Results;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Run one statement now (no confirmation), recording it in the history.
    fn run_statement(&mut self, stmt: &str) {
        let sql = stmt.trim_end_matches(';').to_string();
        match self.run_sql(&sql) {
            Ok((headers, rows)) => {
                self.history.push(&sql);
                self.dirty.history = true;
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.focus = Pane::Results;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Run `stmts` in order on the live session, stopping at the first error;
    /// the grid shows the last statement's output.
    fn run_all(&mut self, stmts: &[String]) {
        let mut last = (Vec::new(), Vec::new());
        for (i, stmt) in stmts.iter().enumerate() {
            match self.run_sql(stmt) {
                Ok(table) => {
                    self.history.push(stmt);
                    self.dirty.history = true;
                    last = table;
                }
                Err(e) => {
                    self.message = Some(format!("{}/{}: {e}", i + 1, stmts.len()));
                    return;
                }
            }
        }
        let (headers, rows) = last;
        self.grid.set(headers, rows);
        self.focus = Pane::Results;
        self.message = Some(t!("msg.db_ran_all", count = stmts.len()).to_string());
    }

    /// Preview the selected table or view (`SELECT * … LIMIT`).
    fn preview_selected(&mut self) {
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
        match self.run_sql(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.last_table = Some(table);
                self.focus = Pane::Results;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Copy `text` to the system clipboard.
    fn yank(&mut self, text: &str) {
        let outcome = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string()));
        self.message = Some(match outcome {
            Ok(()) => t!("msg.db_copied").to_string(),
            Err(e) => e.to_string(),
        });
    }

    /// Render the current (filtered, sorted) grid and write it to the chosen
    /// destination.
    fn run_export(&mut self) {
        let format = export::FORMATS[self.export_format % export::FORMATS.len()];
        let order = self.grid.filtered();
        let rows: Vec<&Vec<String>> = order.iter().map(|&i| &self.grid.rows[i]).collect();
        let table = self.last_table.clone().unwrap_or_else(|| "vix_export".to_string());
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

    /// Beautify the statement at the cursor in place.
    pub fn format_at_cursor(&mut self) {
        if let Some(stmt) = self.query.statement_at_cursor() {
            self.query.replace_statement_at_cursor(&format::beautify(&stmt));
            self.popup = None;
        }
    }

    /// Fetch one detail report for the selected table into the grid.
    fn show_detail(&mut self, detail: catalog::Detail) {
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
        match self.run_sql(&sql) {
            Ok((headers, rows)) => {
                self.message = Some(t!("msg.db_rows", count = rows.len()).to_string());
                self.grid.set(headers, rows);
                self.focus = Pane::Results;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Recompute the autocomplete popup for the cursor position.
    fn refresh_popup(&mut self) {
        let line = self.query.lines()[self.query.row].clone();
        let s = self.completer.suggest(&line, self.query.col);
        self.popup =
            (!s.items.is_empty()).then_some(Popup { items: s.items, sel: 0, start: s.start });
    }
}

/// Rows of the catalog objects query as `(schema, name, kind)` triples.
fn object_triples(rows: Vec<Vec<String>>) -> Vec<(String, String, String)> {
    rows.into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| (r[0].clone(), r[1].clone(), r[2].clone()))
        .collect()
}

/// Pretty-print `text` when it parses as JSON (the cell viewer's `p` toggle);
/// `None` for non-JSON content.
#[must_use]
pub fn pretty_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn browser() -> Browser {
        Browser::new(vec![connect::Connection {
            name: "app".into(),
            kind: connect::Kind::Sqlite,
            file: "/tmp/app.db".into(),
            ..connect::Connection::default()
        }])
    }

    #[test]
    fn add_edit_delete_manage_the_connection_list() {
        let mut b = browser();
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        assert_eq!(b.view, View::Form);
        for c in "prod".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Connections);
        assert_eq!(b.connections.len(), 2);
        assert_eq!(b.connections[1].name, "prod");
        assert!(b.take_dirty_connections().is_some(), "host is told to persist");
        assert!(b.take_dirty_connections().is_none(), "flag clears after take");
        b.sel = 1;
        b.handle_key(key(KeyCode::Char('d')), Pages::default());
        assert_eq!(b.connections.len(), 1);
    }

    #[test]
    fn form_requires_a_name_and_esc_cancels() {
        let mut b = browser();
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Form, "nameless connection is rejected");
        assert!(b.message.is_some());
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Connections);
        assert_eq!(b.connections.len(), 1, "cancel adds nothing");
    }

    #[test]
    fn kind_row_cycles_with_space() {
        let mut b = browser();
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        b.handle_key(key(KeyCode::Down), Pages::default()); // to the kind row
        b.handle_key(key(KeyCode::Char(' ')), Pages::default());
        assert_eq!(b.form.kind, connect::Kind::Postgres);
    }

    #[test]
    fn password_prompt_gates_server_connections() {
        let mut b = Browser::new(vec![connect::Connection {
            name: "pg".into(),
            kind: connect::Kind::Postgres,
            ..connect::Connection::default()
        }]);
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Password, "server engines prompt for a password");
        for c in "pw".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        assert_eq!(b.password, "pw");
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Connections);
        assert!(b.password.is_empty(), "cancel wipes the typed password");
    }

    #[test]
    fn workbench_tab_cycles_focus_and_esc_returns_to_connections() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        b.handle_key(key(KeyCode::Tab), Pages::default());
        assert_eq!(b.focus, Pane::Results);
        b.handle_key(key(KeyCode::BackTab), Pages::default());
        assert_eq!(b.focus, Pane::Editor);
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Connections);
    }

    #[test]
    fn typing_pops_up_completions_and_tab_accepts() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        b.completer.set_schema(vec!["users".into()], vec![("users".into(), "name".into())]);
        for c in "select us".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        let popup = b.popup.as_ref().expect("popup after a 2-char prefix");
        assert!(popup.items.contains(&"users".to_string()));
        b.handle_key(key(KeyCode::Tab), Pages::default());
        assert!(b.popup.is_none());
        assert_eq!(b.query.text(), "select users");
        // Tab with no popup cycles focus instead.
        b.handle_key(key(KeyCode::Tab), Pages::default());
        assert_eq!(b.focus, Pane::Results);
    }

    #[test]
    fn results_filter_captures_typing_until_enter() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Results;
        b.grid.set(
            vec!["name".into()],
            vec![vec!["ada".into()], vec!["grace".into()]],
        );
        b.handle_key(key(KeyCode::Char('/')), Pages::default());
        for c in "gra".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        assert_eq!(b.grid.filtered(), vec![1]);
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert!(!b.grid.filtering);
    }

    #[test]
    fn write_statement_asks_for_confirmation_and_esc_cancels() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        for c in "drop table users".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(b.view, View::Confirm, "write statements are gated");
        assert!(b.pending_summary().unwrap().contains("drop table users"));
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Workbench);
        assert!(b.pending_summary().is_none(), "cancel clears the pending run");
    }

    #[test]
    fn history_and_saved_lists_insert_and_delete() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        b.history.push("select 42");
        b.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL), Pages::default());
        assert_eq!(b.view, View::History);
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Workbench);
        assert_eq!(b.query.text(), "select 42", "history entry lands in the editor");
        // Save it under a name, then find and delete it in the saved list.
        b.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), Pages::default());
        assert_eq!(b.view, View::SaveName);
        for c in "answer".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.saved.queries.len(), 1);
        assert!(b.take_dirty_saved().is_some());
        b.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL), Pages::default());
        assert_eq!(b.view, View::Saved);
        b.handle_key(key(KeyCode::Char('d')), Pages::default());
        assert!(b.saved.queries.is_empty());
    }

    #[test]
    fn inserting_into_a_nonempty_buffer_appends_a_statement() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        for c in "select 1".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.history.push("select 2");
        b.open_history();
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.query.text(), "select 1;\nselect 2");
    }

    #[test]
    fn tree_search_narrows_and_esc_clears() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Tree;
        b.tree = catalog::Tree::from_objects(&[
            ("main".into(), "users".into(), "table".into()),
            ("main".into(), "orders".into(), "table".into()),
        ]);
        b.handle_key(key(KeyCode::Char('/')), Pages::default());
        for c in "ord".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        let rows = b.tree.rows();
        assert!(rows.iter().any(|r| r.text == "orders"));
        assert!(!rows.iter().any(|r| r.text == "users"));
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Workbench, "Esc clears the search, not the workbench");
        assert!(b.tree.filter.is_empty());
    }

    #[test]
    fn cell_viewer_shows_selected_cell_and_pretty_prints() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Results;
        b.grid.set(vec!["j".into()], vec![vec!["{\"a\":1}".into()]]);
        b.handle_key(key(KeyCode::Char('v')), Pages::default());
        assert_eq!(b.view, View::Cell);
        assert_eq!(b.cell_text, "{\"a\":1}");
        b.handle_key(key(KeyCode::Char('p')), Pages::default());
        assert!(b.cell_pretty);
        assert!(pretty_json(&b.cell_text).unwrap().contains("\"a\": 1"));
        assert_eq!(pretty_json("not json"), None);
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Workbench);
    }

    #[test]
    fn export_dialog_cycles_formats_and_writes_a_file() {
        let dir = std::env::temp_dir().join(format!("vix-db-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.csv");
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Results;
        b.grid.set(vec!["id".into()], vec![vec!["1".into()], vec!["2".into()]]);
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        assert_eq!(b.view, View::Export);
        assert_eq!(b.export_path, "vix-export.csv", "default path follows the format");
        b.handle_key(key(KeyCode::Right), Pages::default());
        assert_eq!(export::FORMATS[b.export_format], export::Format::Tsv);
        b.handle_key(key(KeyCode::Left), Pages::default());
        b.export_path.clear();
        for c in path.display().to_string().chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Workbench, "{:?}", b.message);
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "id\n1\n2\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn results_sort_and_column_keys_route_to_the_grid() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Results;
        b.grid.set(
            vec!["id".into(), "n".into()],
            vec![vec!["2".into(), "b".into()], vec!["1".into(), "a".into()]],
        );
        b.handle_key(key(KeyCode::Char('s')), Pages::default());
        assert_eq!(b.grid.filtered(), vec![1, 0], "sorted ascending by the id column");
        b.handle_key(key(KeyCode::Right), Pages::default());
        assert_eq!(b.grid.cur_col, 1);
    }

    #[test]
    fn format_chord_beautifies_the_cursor_statement() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        for c in "select a from t where x=1".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::ALT | KeyModifiers::SHIFT), Pages::default());
        assert_eq!(b.query.text(), "SELECT a\nFROM t\nWHERE x = 1");
    }
}
