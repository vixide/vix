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
//! [`format`], [`highlight`], [`results`]); [`connect`] models the saved
//! connection and its URL, and [`session`] owns the live connection. This
//! module is the state machine the host drives with keys.

// Shared workspace i18n (see the vix_i18n crate).
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

pub mod ai;
pub mod catalog;
pub mod chart;
pub mod complete;
pub mod connect;
pub mod editor;
pub mod erd;
pub mod export;
pub mod format;
pub mod highlight;
pub mod import;
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

/// Where an assistant reply is routed once it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiReply {
    /// Recover SQL and place it in the editor (Ask, optimize, fix-error).
    Sql,
    /// Show the reply verbatim in the text viewer (explain, schema Q&A).
    Prose,
}

/// A pending request for the host to run through the configured assistant CLI:
/// a fixed command-line `prompt` and the schema-plus-question `context` fed on
/// stdin (see [`ai`]).
#[derive(Debug, Clone)]
pub struct AiRequest {
    /// The instruction placed on the assistant's command line.
    pub prompt: String,
    /// The schema-only brief and question, fed to the CLI on stdin.
    pub context: String,
    /// Where the reply should go (not exposed to the host).
    reply: AiReply,
}

/// The AI request lifecycle for the workbench.
#[derive(Debug, Clone, Default)]
enum AiState {
    /// No request outstanding.
    #[default]
    Idle,
    /// A request is queued for the host to spawn (drained by
    /// [`Browser::take_ai_request`]).
    Pending(AiRequest),
    /// A request has been spawned and its reply is awaited.
    Running(AiReply),
}

/// Typed columns `(table, column, type)` from the catalog.
type SchemaColumns = Vec<(String, String, String)>;

/// Foreign-key edges `(child, child_col, parent, parent_col)` from the catalog.
type SchemaRels = Vec<(String, String, String, String)>;

/// An execution awaiting write/DDL confirmation.
#[derive(Debug, Clone)]
enum PendingRun {
    /// One statement (possibly EXPLAIN-wrapped).
    One(String),
    /// Every statement in the buffer, in order.
    All(Vec<String>),
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

/// What to do with the reply from an in-flight asynchronous query.
#[derive(Debug, Clone, Copy)]
enum QueryKind {
    /// A user statement — fill the grid and record history.
    Run,
    /// An `EXPLAIN` — fill the grid and flag full scans for the engine.
    Explain(connect::Kind),
}

/// A user query running on the worker thread, awaited by [`Browser::poll_query`].
#[derive(Debug, Clone)]
struct Pending {
    /// When the query was sent (for the elapsed indicator).
    started: std::time::Instant,
    /// The statement (for logging / transaction tracking) — the exact SQL sent.
    sql: String,
    /// The text to persist in history — the bind-parameter template when the
    /// statement came from substitution, otherwise identical to `sql`. Keeping
    /// this separate stops prompted secret values from being written to disk.
    history_sql: String,
    /// How to apply the reply.
    kind: QueryKind,
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
    pending_run: Option<PendingRun>,
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
    ai: AiState,
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
    pending_query: Option<Pending>,
    /// The active SSH tunnel, kept alive for the connection's lifetime (drop
    /// kills `ssh`).
    tunnel: Option<tunnel::Tunnel>,
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
            ai: AiState::Idle,
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

    /// The current client-side transaction state.
    #[must_use]
    pub fn tx_state(&self) -> TxState {
        self.tx
    }

    /// Whether an AI request is queued or in flight.
    #[must_use]
    pub fn ai_busy(&self) -> bool {
        !matches!(self.ai, AiState::Idle)
    }

    /// Drain a queued AI request for the host to spawn, marking it in flight.
    pub fn take_ai_request(&mut self) -> Option<AiRequest> {
        if let AiState::Pending(req) = &self.ai {
            let reply = req.reply;
            if let AiState::Pending(req) = std::mem::replace(&mut self.ai, AiState::Running(reply))
            {
                return Some(req);
            }
        }
        None
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

    /// Keys on the CSV/TSV import prompt.
    fn key_import(&mut self, key: KeyEvent) -> Outcome {
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

    /// Keys on the "Ask AI" prompt.
    fn key_ask(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.ask_input.push(c),
            KeyCode::Backspace => {
                self.ask_input.pop();
            }
            KeyCode::Enter => self.submit_ask(),
            KeyCode::Esc => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
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
                // Cancelling a confirmed (possibly parameterized) write discards
                // any staged history template so it can't attach to a later query.
                self.history_override = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys in the cell / text viewer (scrolls long content).
    fn key_cell(&mut self, key: KeyEvent) -> Outcome {
        let last = self.cell_text.lines().count().saturating_sub(1);
        match key.code {
            KeyCode::Char('p') => self.cell_pretty = !self.cell_pretty,
            KeyCode::Char('y') => {
                let text = self.cell_text.clone();
                self.yank(&text);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.view_scroll = self.view_scroll.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.view_scroll = (self.view_scroll + 1).min(last)
            }
            KeyCode::PageUp => self.view_scroll = self.view_scroll.saturating_sub(10),
            KeyCode::PageDown => self.view_scroll = (self.view_scroll + 10).min(last),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Keys in the session query log (scroll; Enter reloads the statement).
    fn key_log(&mut self, key: KeyEvent) -> Outcome {
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

    /// Keys in the ER-diagram viewer (scroll; `y` yanks the Mermaid text).
    fn key_erd(&mut self, key: KeyEvent) -> Outcome {
        let last = self.cell_text.lines().count().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.view_scroll = self.view_scroll.saturating_sub(1)
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
            KeyCode::Down | KeyCode::Tab => self.form.sel = (self.form.sel + 1) % FORM_ROWS,
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.form.sel == FORM_KIND => {
                self.form.kind = self.form.kind.next();
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if self.form.sel == FORM_WRITABLE =>
            {
                self.form.writable = !self.form.writable;
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.form.sel == FORM_STORE => {
                self.form.store_keyring = !self.form.store_keyring;
            }
            KeyCode::Char(c) if self.form.sel < FORM_FIELDS && self.form.sel != FORM_KIND => {
                self.form.fields[self.form.sel].push(c);
            }
            KeyCode::Backspace if self.form.sel < FORM_FIELDS && self.form.sel != FORM_KIND => {
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
                    // Save the just-entered password for next time, if the
                    // connection opted in and we actually connected.
                    if conn.store_keyring && self.conn.is_some() {
                        let _ = secret::store(&conn, &password);
                    }
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

    /// Begin connecting to connection `idx`. `SQLite` needs no password; server
    /// engines first try the credential waterfall (`password_command`, then the
    /// OS keyring) and only prompt when that comes up empty.
    fn start_connect(&mut self, idx: usize) {
        let Some(conn) = self.connections.get(idx).cloned() else {
            return;
        };
        if !conn.needs_password() {
            self.finish_connect(&conn, "");
            return;
        }
        if let Some(password) = secret::resolve(&conn) {
            self.finish_connect(&conn, &password);
            return;
        }
        self.pending = Some(idx);
        self.password.clear();
        self.view = View::Password;
    }

    /// Run a user-initiated `sql` on the live session, timing and logging it.
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is
    /// open.
    fn run_sql(&mut self, sql: &str) -> Result<session::Table, String> {
        self.run_traced(sql, store::Origin::User)
    }

    /// Run a background workbench `sql` (catalog, preview, ERD), logged as
    /// [`store::Origin::App`].
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is open.
    fn run_catalog(&mut self, sql: &str) -> Result<session::Table, String> {
        self.run_traced(sql, store::Origin::App)
    }

    /// Run `sql`, record its duration and outcome in the query log, and return
    /// the result unchanged.
    fn run_traced(&mut self, sql: &str, origin: store::Origin) -> Result<session::Table, String> {
        let start = std::time::Instant::now();
        let result = match self.session.as_mut() {
            Some(session) => session.run(sql),
            None => Err(t!("msg.db_not_connected").to_string()),
        };
        self.log.push(store::LogEntry {
            sql: sql.trim().to_string(),
            ms: start.elapsed().as_millis(),
            rows: result.as_ref().map_or(0, |(_, rows)| rows.len()),
            ok: result.is_ok(),
            origin,
        });
        result
    }

    /// Open the persistent session, load the catalog, and enter the
    /// workbench.
    fn finish_connect(&mut self, conn: &connect::Connection, password: &str) {
        // Bring up an SSH tunnel first, if configured, and point the URL at its
        // local end. The tunnel is held for the connection's lifetime.
        let tunnel = match tunnel::open(conn) {
            Ok(tunnel) => tunnel,
            Err(e) => {
                self.message = Some(e);
                self.view = View::Connections;
                return;
            }
        };
        let url = match &tunnel {
            Some(t) => connect::url_via_local(conn, password, t.local_port),
            None => connect::url(conn, password),
        };
        // A read-only connection asks the server to reject writes too, where
        // the engine supports it; the client guard covers the rest.
        let setup: Vec<String> = if conn.writable {
            Vec::new()
        } else {
            connect::read_only_sql(conn.kind, true)
                .into_iter()
                .collect()
        };
        match session::Session::connect(&url, &setup) {
            Ok(session) => {
                self.session = Some(session);
                self.tunnel = tunnel;
            }
            Err(e) => {
                self.message = Some(e);
                self.view = View::Connections;
                return;
            }
        }
        self.write_enabled = conn.writable;
        self.tx = TxState::None;
        match self.run_catalog(catalog::objects_sql(conn.kind)) {
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
            .run_catalog(catalog::columns_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 2)
                    .map(|r| (r[0].clone(), r[1].clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.completer.set_schema(self.tree.table_names(), columns);
        // Foreign-key edges power `JOIN … ON` autocomplete.
        let rels = self
            .run_catalog(catalog::relationships_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 4)
                    .map(|r| (r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone()))
                    .collect()
            })
            .unwrap_or_default();
        self.completer.set_relationships(rels);
    }

    /// Drop the active connection — closing the persistent session and its
    /// in-memory password — and return to the connections list.
    pub fn disconnect(&mut self) {
        self.conn = None;
        self.session = None;
        self.tunnel = None; // drop → kills the ssh forward
        self.write_enabled = false;
        self.log = store::Log::default();
        self.ai = AiState::Idle;
        self.last_error = None;
        self.params = None;
        self.import_path.clear();
        self.tx = TxState::None;
        self.pending_query = None;
        self.set_uneditable();
        self.ask_input.clear();
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
        match self.run_catalog(catalog::objects_sql(kind)) {
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
        // While a query runs the workbench is busy; only Ctrl+C responds.
        if self.query_running() {
            if ctrl && matches!(key.code, KeyCode::Char('c' | 'C')) {
                self.cancel_query();
            }
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

    /// Open the session query log.
    pub fn open_log(&mut self) {
        self.list_sel = 0;
        self.view = View::Log;
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

    /// Open the natural-language "Ask AI" prompt (needs an active connection).
    pub fn open_ask(&mut self) {
        if self.conn.is_none() {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        }
        self.ask_input.clear();
        self.view = View::Ask;
    }

    /// The live schema as `(columns, relationships)` for an AI brief — types and
    /// foreign keys, never row data.
    fn schema_facts(&mut self) -> (SchemaColumns, SchemaRels) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return (Vec::new(), Vec::new());
        };
        let columns = self
            .run_catalog(catalog::columns_typed_sql(kind))
            .map(|(_, rows)| object_triples(rows))
            .unwrap_or_default();
        let relationships = self
            .run_catalog(catalog::relationships_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 4)
                    .map(|r| (r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone()))
                    .collect()
            })
            .unwrap_or_default();
        (columns, relationships)
    }

    /// Send the typed question to the assistant: build a schema-only brief and
    /// queue an [`AiRequest`] for the host. Read-only unless writes are enabled.
    pub fn submit_ask(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        // A leading "?" asks a data-model question (prose answer); otherwise the
        // input is a request to generate SQL.
        let raw = self.ask_input.trim();
        let (prose, question) = match raw.strip_prefix('?') {
            Some(rest) => (true, rest.trim().to_string()),
            None => (false, raw.to_string()),
        };
        if question.is_empty() {
            self.view = View::Workbench;
            return;
        }
        let Some(engine) = self.conn.as_ref().map(|c| c.kind.label()) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::context(engine, &columns, &rels, &question);
        if prose {
            self.queue_ai(ai::answer_instruction(), context, AiReply::Prose);
        } else {
            self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
        }
        self.view = View::Workbench;
    }

    /// Queue an [`AiRequest`] for the host and mark the workbench busy.
    fn queue_ai(&mut self, prompt: String, context: String, reply: AiReply) {
        self.ai = AiState::Pending(AiRequest {
            prompt,
            context,
            reply,
        });
        self.message = Some(t!("msg.db_ai_thinking").to_string());
    }

    /// Ask the assistant to fix the last failed query, feeding it the query and
    /// the database's error message alongside the schema.
    pub fn fix_error(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let (Some(engine), Some((sql, error))) = (
            self.conn.as_ref().map(|c| c.kind.label()),
            self.last_error.clone(),
        ) else {
            self.message = Some(t!("msg.db_ai_no_error").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::error_context(engine, &columns, &rels, &sql, &error);
        self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
    }

    /// Ask the assistant to explain the statement at the cursor in plain
    /// English (answer shown in the viewer, no SQL run).
    pub fn explain_query(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let Some(engine) = self.conn.as_ref().map(|c| c.kind.label()) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::explain_context(engine, &columns, &rels, &stmt);
        self.queue_ai(ai::explain_instruction(), context, AiReply::Prose);
    }

    /// Ask the assistant to optimize the statement at the cursor, feeding it the
    /// query's own `EXPLAIN` plan (the surus draft → EXPLAIN → iterate loop).
    pub fn optimize_current(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let plan = self
            .run_catalog(&catalog::explain_sql(kind, &stmt, false))
            .map(|(_, rows)| {
                rows.iter()
                    .map(|r| r.join(" | "))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let (columns, rels) = self.schema_facts();
        let context = ai::optimize_context(kind.label(), &columns, &rels, &stmt, &plan);
        self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
    }

    /// Apply the assistant's reply, routed by the request kind: SQL replies land
    /// in the editor (validated with `EXPLAIN`); prose replies open in the
    /// scrollable text viewer.
    pub fn apply_ai_reply(&mut self, reply: &str) {
        let kind = match self.ai {
            AiState::Running(k) => k,
            _ => AiReply::Sql,
        };
        self.ai = AiState::Idle;
        match kind {
            AiReply::Sql => {
                let sql = ai::extract_sql(reply);
                if sql.trim().is_empty() {
                    self.message = Some(t!("msg.db_ai_empty").to_string());
                    return;
                }
                self.query = editor::Query::default();
                self.insert_statement(&sql);
                self.focus = Pane::Editor;
                // Validate with EXPLAIN synchronously (never ANALYZE, so it is
                // read-only even if the model drafted a write). Not the async
                // user path — the reply is applied outside the event loop.
                if let Some(kind) = self.conn.as_ref().map(|c| c.kind) {
                    let wrapped = catalog::explain_sql(kind, &sql, false);
                    if let Ok((headers, rows)) = self.run_catalog(&wrapped) {
                        self.grid.set(headers, rows);
                        self.focus = Pane::Results;
                        self.set_uneditable();
                    }
                }
                self.message = Some(t!("msg.db_ai_ready").to_string());
            }
            AiReply::Prose => {
                let text = reply.trim();
                if text.is_empty() {
                    self.message = Some(t!("msg.db_ai_empty").to_string());
                    return;
                }
                self.cell_text = text.to_string();
                self.cell_pretty = false;
                self.view_scroll = 0;
                self.view = View::Cell;
                self.message = Some(t!("msg.db_ai_answer").to_string());
            }
        }
    }

    /// Report that the AI request failed (spawn error or empty output).
    pub fn ai_failed(&mut self) {
        self.ai = AiState::Idle;
        self.message = Some(t!("msg.db_ai_failed").to_string());
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
            KeyCode::Char('s') => self.show_detail(catalog::Detail::Stats),
            KeyCode::Char('D') => self.show_ddl(),
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
            PendingRun::All(stmts) => Some(format!(
                "{} × … {}",
                stmts.len(),
                stmts.first().map_or("", String::as_str)
            )),
        }
    }

    /// Toggle write mode for this session (the surus "enable write mode from
    /// the editor" gesture, F8). Where the engine has a session-level switch,
    /// the database's read-only flag is flipped to match; otherwise only the
    /// client guard changes. Refuses with a message when disconnected.
    pub fn toggle_write_mode(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let want_writable = !self.write_enabled;
        if let Some(sql) = connect::read_only_sql(kind, !want_writable)
            && let Err(e) = self.run_sql(&sql)
        {
            self.message = Some(e);
            return;
        }
        self.write_enabled = want_writable;
        self.message = Some(if want_writable {
            t!("msg.db_write_enabled").to_string()
        } else {
            t!("msg.db_read_only_on").to_string()
        });
    }

    /// Execute the statement at the cursor, showing its rows in the grid.
    /// Statements with `:name` parameters prompt for values first; write and
    /// DDL statements go through the confirmation view.
    pub fn execute(&mut self) {
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let names = params::names(&stmt);
        if names.is_empty() {
            self.execute_sql(stmt);
        } else {
            self.params = Some(ParamPrompt {
                sql: stmt,
                names,
                ..ParamPrompt::default()
            });
            self.view = View::Params;
        }
    }

    /// Execute one fully-formed statement, gating writes as [`Self::execute`]
    /// does. Shared by the plain path and the bind-parameter path. Inside an
    /// explicit transaction the confirmation is skipped — the change is
    /// provisional and `ROLLBACK` can undo it.
    fn execute_sql(&mut self, stmt: String) {
        if editor::is_write_statement(&stmt) {
            if !self.write_enabled {
                self.message = Some(t!("msg.db_read_only").to_string());
                return;
            }
            if self.tx == TxState::None {
                self.pending_run = Some(PendingRun::One(stmt));
                self.view = View::Confirm;
                return;
            }
        }
        self.run_statement(&stmt);
    }

    /// Keys on the bind-parameter prompt: each `Enter` records the value for the
    /// current placeholder; once all are filled the substituted statement runs.
    fn key_params(&mut self, key: KeyEvent) -> Outcome {
        let Some(prompt) = self.params.as_mut() else {
            self.view = View::Workbench;
            return Outcome::Consumed;
        };
        match key.code {
            KeyCode::Char(c) => prompt.input.push(c),
            KeyCode::Backspace => {
                prompt.input.pop();
            }
            KeyCode::Enter => {
                let value = std::mem::take(&mut prompt.input);
                prompt.values.push(value);
                if prompt.values.len() >= prompt.names.len() {
                    let prompt = self.params.take().expect("prompt present");
                    let pairs: Vec<(String, String)> =
                        prompt.names.into_iter().zip(prompt.values).collect();
                    let sql = params::substitute(&prompt.sql, &pairs);
                    // Record the placeholder template — never the substituted
                    // secret values — in the persisted history.
                    self.history_override = Some(prompt.sql.clone());
                    self.view = View::Workbench;
                    self.execute_sql(sql);
                }
            }
            KeyCode::Esc => {
                self.params = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Execute every statement in the buffer, in order (confirmed once when
    /// any of them writes). The grid shows the last statement's rows.
    pub fn execute_all(&mut self) {
        let text = self.query.text();
        let stmts: Vec<String> = editor::statement_spans(&text)
            .iter()
            .map(|&(s, e)| {
                text.chars()
                    .skip(s)
                    .take(e - s)
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
        if stmts.is_empty() {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        }
        if stmts.iter().any(|s| editor::is_write_statement(s)) {
            if !self.write_enabled {
                self.message = Some(t!("msg.db_read_only").to_string());
                return;
            }
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
            if !self.write_enabled {
                self.message = Some(t!("msg.db_read_only").to_string());
                return;
            }
            self.pending_run = Some(PendingRun::One(wrapped));
            self.view = View::Confirm;
            return;
        }
        self.start_query(wrapped, QueryKind::Explain(kind));
    }

    /// Run one statement now (no confirmation), recording it in the history.
    fn run_statement(&mut self, stmt: &str) {
        let sql = stmt.trim_end_matches(';').to_string();
        self.start_query(sql, QueryKind::Run);
    }

    /// Send `sql` to the worker without blocking and remember how to apply its
    /// reply; [`Self::poll_query`] finishes it. The workbench is "busy" until
    /// then (only `Ctrl+C` responds).
    fn start_query(&mut self, sql: String, kind: QueryKind) {
        let Some(session) = self.session.as_ref() else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        if let Err(e) = session.send(&sql) {
            self.message = Some(e);
            return;
        }
        self.message = Some(t!("msg.db_running").to_string());
        // Persist the template if one was staged (a parameterized query),
        // otherwise persist the executed SQL. `take` ensures the override applies
        // to exactly one query.
        let history_sql = self.history_override.take().unwrap_or_else(|| sql.clone());
        self.pending_query = Some(Pending {
            started: std::time::Instant::now(),
            sql,
            history_sql,
            kind,
        });
    }

    /// Drain streamed result chunks into the grid, applying batches as they
    /// arrive. Called by the host each event-loop tick; cheap when idle.
    pub fn poll_query(&mut self) {
        if self.pending_query.is_none() {
            return;
        }
        loop {
            let Some(chunk) = self.session.as_ref().and_then(session::Session::poll) else {
                return; // nothing new this tick — keep the query pending
            };
            match chunk {
                session::Chunk::Head(headers) => {
                    self.grid.set(headers, Vec::new());
                    self.focus = Pane::Results;
                }
                session::Chunk::Rows(batch) => self.grid.append_rows(batch),
                session::Chunk::Done(truncated) => {
                    self.finish_stream(truncated);
                    return;
                }
                session::Chunk::Err(e) => {
                    self.finish_stream_err(&e);
                    return;
                }
            }
        }
    }

    /// Cancel the in-flight query: abandon the worker (its result is discarded)
    /// and reconnect so the UI is usable again. Transaction state is lost.
    pub fn cancel_query(&mut self) {
        if self.pending_query.take().is_none() {
            return;
        }
        match self.session.as_mut().map(session::Session::restart) {
            Some(Ok(())) => {
                self.tx = TxState::None;
                self.message = Some(t!("msg.db_cancelled").to_string());
            }
            Some(Err(e)) => {
                self.message = Some(e);
                self.disconnect();
            }
            None => {}
        }
    }

    /// Finalize a fully streamed statement (the async tail of the run / explain
    /// paths): record it and set the completion message.
    fn finish_stream(&mut self, truncated: bool) {
        let Some(pending) = self.pending_query.take() else {
            return;
        };
        let rows = self.grid.rows.len();
        self.log.push(store::LogEntry {
            sql: pending.sql.trim().to_string(),
            ms: pending.started.elapsed().as_millis(),
            rows,
            ok: true,
            origin: store::Origin::User,
        });
        self.focus = Pane::Results;
        self.set_uneditable();
        match pending.kind {
            QueryKind::Run => {
                // Persist the template (never the substituted secret) to the
                // on-disk history; the in-memory session log above keeps the real
                // executed SQL.
                self.history.push(&pending.history_sql);
                self.dirty.history = true;
                self.last_error = None;
                self.note_tx(&pending.sql, true);
                self.message = Some(if truncated {
                    t!(
                        "msg.db_rows_truncated",
                        count = rows,
                        max = session::MAX_ROWS
                    )
                    .to_string()
                } else {
                    t!("msg.db_rows", count = rows).to_string()
                });
            }
            QueryKind::Explain(kind) => {
                let insight = catalog::scan_insight(kind, &self.grid.rows);
                self.message = Some(if insight {
                    t!("msg.db_insight_scan").to_string()
                } else {
                    t!("msg.db_rows", count = rows).to_string()
                });
            }
        }
    }

    /// Finalize a streamed statement that errored partway.
    fn finish_stream_err(&mut self, error: &str) {
        let Some(pending) = self.pending_query.take() else {
            return;
        };
        self.log.push(store::LogEntry {
            sql: pending.sql.trim().to_string(),
            ms: pending.started.elapsed().as_millis(),
            rows: 0,
            ok: false,
            origin: store::Origin::User,
        });
        self.note_tx(&pending.sql, false);
        self.last_error = Some((pending.sql.clone(), error.to_string()));
        self.message = Some(error.to_string());
    }

    /// Update the client-side transaction state from an executed statement:
    /// `BEGIN`/`START` opens it, `COMMIT`/`ROLLBACK` closes it, and any failure
    /// inside an open transaction marks it aborted.
    fn note_tx(&mut self, sql: &str, ok: bool) {
        if !ok {
            if self.tx == TxState::Open {
                self.tx = TxState::Aborted;
            }
            return;
        }
        let word = sql
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        match word.as_str() {
            "BEGIN" | "START" => self.tx = TxState::Open,
            "COMMIT" | "ROLLBACK" => self.tx = TxState::None,
            _ => {}
        }
    }

    /// Run a transaction-control statement, update the badge, and report it
    /// without disturbing the results grid.
    fn run_tx(&mut self, sql: &str, msg: &str) {
        if self.conn.is_none() {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        }
        match self.run_sql(sql) {
            Ok(_) => {
                self.note_tx(sql, true);
                self.message = Some(t!(msg).to_string());
            }
            Err(e) => {
                self.note_tx(sql, false);
                self.message = Some(e);
            }
        }
    }

    /// Begin an explicit transaction (DB → Transaction → Begin).
    pub fn begin_tx(&mut self) {
        self.run_tx("BEGIN", "msg.db_tx_begin");
    }

    /// Commit the open transaction.
    pub fn commit_tx(&mut self) {
        if self.tx == TxState::None {
            self.message = Some(t!("msg.db_tx_none").to_string());
            return;
        }
        self.run_tx("COMMIT", "msg.db_tx_commit");
    }

    /// Roll back the open (or aborted) transaction.
    pub fn rollback_tx(&mut self) {
        if self.tx == TxState::None {
            self.message = Some(t!("msg.db_tx_none").to_string());
            return;
        }
        self.run_tx("ROLLBACK", "msg.db_tx_rollback");
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
                    self.note_tx(stmt, true);
                    last = table;
                }
                Err(e) => {
                    self.last_error = Some((stmt.clone(), e.clone()));
                    self.note_tx(stmt, false);
                    self.message = Some(format!("{}/{}: {e}", i + 1, stmts.len()));
                    return;
                }
            }
        }
        let (headers, rows) = last;
        self.grid.set(headers, rows);
        self.focus = Pane::Results;
        self.set_uneditable();
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
    fn set_uneditable(&mut self) {
        self.edit_table = None;
        self.pk_cols.clear();
        self.edits.clear();
        self.editing_cell = None;
    }

    /// Show the selected result row vertically as `column: value` lines in the
    /// text viewer (psql's expanded `\x` display), for wide rows.
    fn expand_row(&mut self) {
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
    fn follow_fk(&mut self) {
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
    fn begin_cell_edit(&mut self) {
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
    fn key_cell_edit(&mut self, key: KeyEvent) -> Outcome {
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
    fn commit_edits(&mut self) {
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

        let mut edits: Vec<((usize, usize), (String, String))> =
            self.edits.iter().map(|(k, v)| (*k, v.clone())).collect();
        edits.sort_by_key(|(k, _)| *k);

        // Build the WHERE from the row's primary-key cells, and a re-read query
        // for optimistic conflict detection.
        let mut updates = Vec::new();
        for ((row, col), (original, new)) in &edits {
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
                        return;
                    }
                }
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            }
            updates.push(format!(
                "UPDATE {target} SET {} = {} WHERE {where_sql}",
                catalog::quote_ident(kind, &column),
                catalog::quote_literal(new)
            ));
        }

        // Apply in one transaction; roll back on the first error.
        let count = updates.len();
        if self.run_sql("BEGIN").is_err() {
            self.message = Some(t!("msg.db_ai_failed").to_string());
            return;
        }
        for sql in &updates {
            if let Err(e) = self.run_sql(sql) {
                let _ = self.run_sql("ROLLBACK");
                self.tx = TxState::None;
                self.message = Some(e);
                return;
            }
        }
        if let Err(e) = self.run_sql("COMMIT") {
            let _ = self.run_sql("ROLLBACK");
            self.tx = TxState::None;
            self.message = Some(e);
            return;
        }
        self.tx = TxState::None;
        self.edits.clear();
        self.message = Some(t!("msg.db_edit_committed", count = count).to_string());
        self.preview_selected_refresh(&schema, &table, kind);
    }

    /// Re-run a table preview after a commit to show the persisted values.
    fn preview_selected_refresh(&mut self, schema: &str, table: &str, kind: connect::Kind) {
        let sql = catalog::preview_sql(kind, schema, table);
        if let Ok((headers, rows)) = self.run_catalog(&sql) {
            self.grid.set(headers, rows);
            self.set_editable(schema, table, kind);
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

    /// Beautify the statement at the cursor in place.
    pub fn format_at_cursor(&mut self) {
        if let Some(stmt) = self.query.statement_at_cursor() {
            self.query
                .replace_statement_at_cursor(&format::beautify(&stmt));
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
    fn show_ddl(&mut self) {
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
    fn refresh_popup(&mut self) {
        let line = self.query.lines()[self.query.row].clone();
        let s = self.completer.suggest(&line, self.query.col);
        self.popup = (!s.items.is_empty()).then_some(Popup {
            items: s.items,
            sel: 0,
            start: s.start,
        });
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
        assert!(b.pending_query.is_none(), "query never finished: {:?}", b.message);
        let entry = b.history.entries.first().expect("a history entry was recorded");
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
