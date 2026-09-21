//! The DB workbench: connection management, schema browsing, and SQL queries.
//!
//! Implements `spec/index.md`: a **DB** menu opens a full-screen overlay that walks
//! through saved connections (passwords are prompted per session and never
//! written to disk), then presents a three-pane workbench — schema tree on
//! the left, a syntax-highlighted SQL editor with autocomplete on the right,
//! and a filterable results grid below it. Queries run over one persistent
//! [`sqlx`] connection per workbench (the `Any` driver: bundled `SQLite`,
//! pure-Rust `PostgreSQL`/`MySQL` over rustls), held open on a worker thread by
//! [`session`] so transactions span statements — no external client tools.
//!
//! Pure state lives in the submodules ([`catalog`], [`complete`], [`editor`],
//! [`format`](mod@format), [`highlight`], [`results`]); [`connect`] models the saved
//! connection and its URL, and [`session`] owns the live connection. This
//! module is the state machine the host drives with keys — split, where a
//! cohesive slice of it justified its own file (T516), into `lifecycle`
//! (connecting/disconnecting), `execute` (running statements, transactions,
//! cancelling a hung query), [`ai_features`] (Ask/fix-error/explain/
//! optimize — `pub`, unlike its other siblings, since [`ai_features::
//! AiRequest`] crosses the crate boundary to the host), `cell_edit`
//! (previewing an editable table, staging/committing cell edits, the
//! detail/DDL popup), and `panels` (history/saved/log/ER-diagram/import/
//! export, and the tree/editor/results panes' own key dispatch). What's
//! left here is genuinely shared: the `Browser`/`Form`/`Pane`/`View`/
//! `Popup` state itself and the two top-level dispatchers.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

// Shared workspace i18n (see the vix_i18n crate).
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

pub mod ai;
pub mod ai_features;
pub mod catalog;
mod cell_edit;
pub mod chart;
pub mod complete;
pub mod connect;
pub mod editor;
pub mod erd;
mod execute;
pub mod export;
pub mod format;
pub mod highlight;
pub mod import;
mod lifecycle;
mod panels;
pub mod params;
pub mod results;
pub mod secret;
pub mod session;
pub mod store;
pub mod tunnel;

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
    /// The session query log (Ctrl+L).
    Log,
    /// The generated ER-diagram viewer (Ctrl+E).
    Erd,
    /// The natural-language "Ask AI" prompt (Ctrl+A).
    Ask,
    /// Collecting values for `:name` bind parameters before a run.
    Params,
    /// The CSV/TSV import file-path prompt (Ctrl+U).
    Import,
    /// Inline editor for a single result cell (staged, `i`).
    CellEdit,
}

/// Client-side transaction state, tracked from executed statements so the
/// workbench can badge it and relax the write-confirmation gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TxState {
    /// Autocommit — each statement stands alone.
    #[default]
    None,
    /// Inside an explicit `BEGIN … ` transaction.
    Open,
    /// A statement failed inside the transaction; a `ROLLBACK` is needed.
    Aborted,
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

/// Number of text form fields (name, kind, file, host, port, user, database,
/// password command, ssh host/user/port/identity, sslmode).
pub const FORM_FIELDS: usize = 13;

/// Index of the kind row in the form (cycled, not typed).
pub const FORM_KIND: usize = 1;

/// Index of the password-command text field.
pub const FORM_PASSWORD_COMMAND: usize = 7;

/// Index of the SSH-tunnel host text field (the following three are user,
/// port, identity).
pub const FORM_SSH_HOST: usize = 8;

/// Index of the TLS-mode text field (`sslmode` / `ssl-mode`).
pub const FORM_SSLMODE: usize = 12;

/// Index of the access row (read-only / read-write), toggled, not typed.
pub const FORM_WRITABLE: usize = FORM_FIELDS;

/// Index of the keyring-store row, toggled, not typed.
pub const FORM_STORE: usize = FORM_FIELDS + 1;

/// Total navigable form rows (text fields plus the two toggles).
pub const FORM_ROWS: usize = FORM_FIELDS + 2;

/// The add/edit connection form.
#[derive(Debug, Clone, Default)]
pub struct Form {
    /// Text of the fields, by index: 0 name, 2 file, 3 host, 4 port, 5 user,
    /// 6 database, 7 password command (index 1 is [`Form::kind`]).
    pub fields: [String; FORM_FIELDS],
    /// The engine picked on the kind row.
    pub kind: connect::Kind,
    /// Whether the connection may write (the access row); read-only by default.
    pub writable: bool,
    /// Whether to store a prompted password in the OS keyring.
    pub store_keyring: bool,
    /// The selected field row.
    pub sel: usize,
    /// Index of the connection being edited; `None` when adding.
    pub editing: Option<usize>,
}

impl Form {
    /// A form pre-filled from `conn` (for editing).
    #[must_use]
    pub fn from_connection(conn: &connect::Connection, editing: Option<usize>) -> Form {
        let mut f = Form {
            kind: conn.kind,
            writable: conn.writable,
            store_keyring: conn.store_keyring,
            editing,
            ..Form::default()
        };
        f.fields[0].clone_from(&conn.name);
        f.fields[2].clone_from(&conn.file);
        f.fields[3].clone_from(&conn.host);
        f.fields[4].clone_from(&conn.port);
        f.fields[5].clone_from(&conn.user);
        f.fields[6].clone_from(&conn.database);
        f.fields[FORM_PASSWORD_COMMAND].clone_from(&conn.password_command);
        f.fields[FORM_SSH_HOST].clone_from(&conn.ssh_host);
        f.fields[FORM_SSH_HOST + 1].clone_from(&conn.ssh_user);
        f.fields[FORM_SSH_HOST + 2].clone_from(&conn.ssh_port);
        f.fields[FORM_SSH_HOST + 3].clone_from(&conn.ssh_identity);
        f.fields[FORM_SSLMODE].clone_from(&conn.sslmode);
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
            writable: self.writable,
            password_command: self.fields[FORM_PASSWORD_COMMAND].clone(),
            store_keyring: self.store_keyring,
            ssh_host: self.fields[FORM_SSH_HOST].clone(),
            ssh_user: self.fields[FORM_SSH_HOST + 1].clone(),
            ssh_port: self.fields[FORM_SSH_HOST + 2].clone(),
            ssh_identity: self.fields[FORM_SSH_HOST + 3].clone(),
            sslmode: self.fields[FORM_SSLMODE].clone(),
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
    pending_run: Option<execute::PendingRun>,
    /// When set, the text to record in the *persisted* history for the next
    /// started query, in place of the executed SQL. Used to keep bind-parameter
    /// **templates** (`… = :name`) in history rather than the substituted
    /// statement, so a prompted secret value is never written to disk.
    history_override: Option<String>,
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
    /// Whether writes are allowed this session. Starts from the connection's
    /// `writable` flag (read-only by default) and toggles with F8.
    write_enabled: bool,
    /// The session query log (Ctrl+L), newest first.
    pub log: store::Log,
    /// Scroll offset (lines) of the text/ERD viewer.
    pub view_scroll: usize,
    /// The natural-language question being typed on the Ask prompt.
    pub ask_input: String,
    /// The AI request lifecycle (idle / queued / in flight).
    ai: ai_features::AiState,
    /// The last failed user statement and its error, for AI fix-error.
    last_error: Option<(String, String)>,
    /// The statement awaiting bind-parameter values, with its placeholder
    /// names, the values collected so far, and the value being typed.
    pub params: Option<ParamPrompt>,
    /// The file path being typed on the CSV/TSV import prompt.
    pub import_path: String,
    /// Client-side transaction state (autocommit / open / aborted).
    tx: TxState,
    /// The `(schema, table)` behind an editable grid — `Some` only when the
    /// current grid is a single-table view with a known primary key.
    edit_table: Option<(String, String)>,
    /// Primary-key column names for [`Self::edit_table`].
    pk_cols: Vec<String>,
    /// Staged cell edits, keyed by `(underlying row index, column index)` →
    /// `(original value, new value)`.
    pub edits: std::collections::HashMap<(usize, usize), (String, String)>,
    /// The cell being edited (underlying row, column), and the value typed.
    editing_cell: Option<(usize, usize)>,
    /// The value being typed on the cell editor.
    pub edit_input: String,
    /// A user query running asynchronously on the worker thread, if any.
    pending_query: Option<execute::Pending>,
    /// The active SSH tunnel, kept alive for the connection's lifetime (drop
    /// kills `ssh`).
    tunnel: Option<tunnel::Tunnel>,
    /// A connect (or password-prompted reconnect) running on a background
    /// thread, awaited by [`Browser::poll_connect`] (Run H, T531). `None`
    /// once resolved either way.
    pending_connect: Option<lifecycle::PendingConnect>,
    /// A cancelled query's reconnect running on a background thread, awaited
    /// by [`Browser::poll_reconnect`] (Run H, T531/T532). `None` once
    /// resolved either way.
    pending_reconnect: Option<execute::PendingReconnect>,
}

/// State for the bind-parameter prompt: the SQL and its `:name` placeholders,
/// filled one value at a time.
#[derive(Debug, Clone, Default)]
pub struct ParamPrompt {
    /// The original statement holding the placeholders.
    sql: String,
    /// Placeholder names, in prompt order.
    pub names: Vec<String>,
    /// Values collected so far (parallel to the first `values.len()` names).
    pub values: Vec<String>,
    /// The value currently being typed.
    pub input: String,
}

impl ParamPrompt {
    /// The placeholder name currently being prompted for, if any remain.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.names.get(self.values.len()).map(String::as_str)
    }
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
            history_override: None,
            cell_text: String::new(),
            cell_pretty: false,
            export_format: 0,
            export_path: String::new(),
            export_clipboard: false,
            last_table: None,
            write_enabled: false,
            log: store::Log::default(),
            view_scroll: 0,
            ask_input: String::new(),
            ai: ai_features::AiState::Idle,
            last_error: None,
            params: None,
            import_path: String::new(),
            tx: TxState::None,
            edit_table: None,
            pk_cols: Vec::new(),
            edits: std::collections::HashMap::new(),
            editing_cell: None,
            edit_input: String::new(),
            pending_query: None,
            tunnel: None,
            pending_connect: None,
            pending_reconnect: None,
        }
    }

    /// Whether a user query is running asynchronously.
    #[must_use]
    pub fn query_running(&self) -> bool {
        self.pending_query.is_some()
    }

    /// Whole seconds a running query has been in flight (for the indicator).
    #[must_use]
    pub fn query_elapsed_secs(&self) -> Option<u64> {
        self.pending_query
            .as_ref()
            .map(|p| p.started.elapsed().as_secs())
    }

    /// The current client-side transaction state.
    #[must_use]
    pub fn tx_state(&self) -> TxState {
        self.tx
    }

    /// Whether writes are allowed on the active session.
    #[must_use]
    pub fn write_enabled(&self) -> bool {
        self.write_enabled
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
            View::Log => self.key_log(key),
            View::Erd => self.key_erd(key),
            View::Ask => self.key_ask(key),
            View::Params => self.key_params(key),
            View::Import => self.key_import(key),
            View::CellEdit => self.key_cell_edit(key),
        }
    }

    /// Keys inside the workbench, after the pane-independent chords.
    fn key_workbench(&mut self, key: KeyEvent, pages: Pages) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if self.workbench_busy(key, ctrl) {
            return Outcome::Consumed;
        }
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
            KeyCode::F(8) => {
                self.toggle_write_mode();
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
            KeyCode::Char('l' | 'L') if ctrl => {
                self.open_log();
                return Outcome::Consumed;
            }
            KeyCode::Char('e' | 'E') if ctrl => {
                self.generate_erd();
                return Outcome::Consumed;
            }
            KeyCode::Char('a' | 'A') if ctrl => {
                self.open_ask();
                return Outcome::Consumed;
            }
            KeyCode::Char('o' | 'O') if ctrl => {
                self.optimize_current();
                return Outcome::Consumed;
            }
            KeyCode::Char('f' | 'F') if ctrl => {
                self.fix_error();
                return Outcome::Consumed;
            }
            KeyCode::Char('k' | 'K') if ctrl => {
                self.explain_query();
                return Outcome::Consumed;
            }
            KeyCode::Char('u' | 'U') if ctrl => {
                self.open_import();
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
            KeyCode::Esc
                if self.popup.is_none() && !self.grid.filtering && !self.tree.filtering =>
            {
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

    /// Wait for a background connect/reconnect (Run H, T531/T532) to
    /// resolve, so a test can observe its outcome synchronously, as a real
    /// event loop would.
    fn wait_for_connect(b: &mut Browser) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while b.connect_running() || b.reconnect_running() {
            b.poll_connect();
            b.poll_reconnect();
            assert!(
                std::time::Instant::now() < deadline,
                "connect/reconnect did not finish within 5s"
            );
            std::thread::yield_now();
        }
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
        assert!(
            b.take_dirty_connections().is_some(),
            "host is told to persist"
        );
        assert!(
            b.take_dirty_connections().is_none(),
            "flag clears after take"
        );
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
        // The credential waterfall (password_command, then the keyring) now
        // runs in the background (T531); wait for it.
        wait_for_connect(&mut b);
        assert_eq!(
            b.view,
            View::Password,
            "server engines prompt for a password"
        );
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
        b.completer
            .set_schema(vec!["users".into()], vec![("users".into(), "name".into())]);
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
        b.write_enabled = true; // writes are gated behind read-only by default
        for c in "drop table users".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(b.view, View::Confirm, "write statements are gated");
        assert!(b.pending_summary().unwrap().contains("drop table users"));
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.view, View::Workbench);
        assert!(
            b.pending_summary().is_none(),
            "cancel clears the pending run"
        );
    }

    #[test]
    fn read_only_default_refuses_writes_until_toggled() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        assert!(!b.write_enabled(), "a session starts read-only");
        for c in "delete from users".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(b.view, View::Workbench, "a write is refused, not confirmed");
        assert!(b.pending_summary().is_none(), "nothing is queued to run");
        assert_eq!(b.message.as_deref(), Some(&*t!("msg.db_read_only")));
    }

    #[test]
    fn access_row_toggles_writable_with_space() {
        let mut b = browser();
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        for _ in 0..FORM_WRITABLE {
            b.handle_key(key(KeyCode::Down), Pages::default()); // walk to the access row
        }
        assert_eq!(b.form.sel, FORM_WRITABLE);
        assert!(!b.form.writable, "new connections are read-only");
        b.handle_key(key(KeyCode::Char(' ')), Pages::default());
        assert!(b.form.writable, "space flips the access row to read-write");
        assert!(
            b.form.to_connection().writable,
            "the toggle carries into the connection"
        );
    }

    #[test]
    fn history_and_saved_lists_insert_and_delete() {
        let mut b = browser();
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        b.history.push("select 42");
        b.handle_key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            Pages::default(),
        );
        assert_eq!(b.view, View::History);
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.view, View::Workbench);
        assert_eq!(
            b.query.text(),
            "select 42",
            "history entry lands in the editor"
        );
        // Save it under a name, then find and delete it in the saved list.
        b.handle_key(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            Pages::default(),
        );
        assert_eq!(b.view, View::SaveName);
        for c in "answer".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.saved.queries.len(), 1);
        assert!(b.take_dirty_saved().is_some());
        b.handle_key(
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            Pages::default(),
        );
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
    fn parameterized_query_history_stores_template_not_secret() {
        let mut b = browser();
        // A real in-memory session so the query actually runs and finalizes.
        b.session = Some(session::Session::connect("sqlite::memory:", &[]).expect("memory db"));
        b.view = View::Params;
        b.params = Some(ParamPrompt {
            sql: "SELECT :secret AS x".into(),
            names: vec!["secret".into()],
            values: vec![],
            input: String::new(),
        });
        // Type the secret value, then Enter runs the substituted statement.
        for c in "hunter2".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        // Drive the async query to completion.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while b.pending_query.is_some() && std::time::Instant::now() < deadline {
            b.poll_query();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            b.pending_query.is_none(),
            "query never finished: {:?}",
            b.message
        );
        let entry = b
            .history
            .entries
            .first()
            .expect("a history entry was recorded");
        assert_eq!(
            entry, "SELECT :secret AS x",
            "history must store the placeholder template"
        );
        assert!(
            !entry.contains("hunter2"),
            "the prompted secret must never reach persisted history: {entry:?}"
        );
    }

    #[test]
    fn readonly_rejected_param_write_does_not_corrupt_later_history() {
        let mut b = browser();
        b.session = Some(session::Session::connect("sqlite::memory:", &[]).expect("memory db"));
        b.write_enabled = false; // read-only: a write is rejected before running

        // Attempt a parameterized WRITE; it is rejected and must NOT leave its
        // template staged for the next query.
        b.view = View::Params;
        b.params = Some(ParamPrompt {
            sql: "UPDATE t SET x = :v".into(),
            names: vec!["v".into()],
            values: vec![],
            input: String::new(),
        });
        for c in "sekret".chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert!(b.pending_query.is_none(), "read-only write must not run");

        // Now run a plain parameterized READ to completion.
        b.view = View::Params;
        b.params = Some(ParamPrompt {
            sql: "SELECT :n AS n".into(),
            names: vec!["n".into()],
            values: vec![],
            input: String::new(),
        });
        b.handle_key(key(KeyCode::Char('1')), Pages::default());
        b.handle_key(key(KeyCode::Enter), Pages::default());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while b.pending_query.is_some() && std::time::Instant::now() < deadline {
            b.poll_query();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let entry = b.history.entries.first().expect("read query recorded");
        assert_eq!(
            entry, "SELECT :n AS n",
            "history reflects the read, not the rejected write"
        );
        assert!(
            !b.history.entries.iter().any(|e| e.contains("UPDATE")),
            "the rejected write's template leaked into history: {:?}",
            b.history.entries
        );
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
        assert_eq!(
            b.view,
            View::Workbench,
            "Esc clears the search, not the workbench"
        );
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
        b.grid
            .set(vec!["id".into()], vec![vec!["1".into()], vec!["2".into()]]);
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        assert_eq!(b.view, View::Export);
        assert_eq!(
            b.export_path, "vix-export.csv",
            "default path follows the format"
        );
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
        assert_eq!(
            b.grid.filtered(),
            vec![1, 0],
            "sorted ascending by the id column"
        );
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
        b.handle_key(
            KeyEvent::new(KeyCode::Char('F'), KeyModifiers::ALT | KeyModifiers::SHIFT),
            Pages::default(),
        );
        assert_eq!(b.query.text(), "SELECT a\nFROM t\nWHERE x = 1");
    }
}
