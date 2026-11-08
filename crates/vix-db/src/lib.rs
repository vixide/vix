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
//! [`format`](mod@format), [`highlight`], [`plan`], [`results`], and for editing a
//! table preview [`staging`], [`columns`] and [`row_filter`]); [`connect`]
//! models the saved connection and its URL, and [`session`] owns the live
//! connection. This module is the state machine the host drives with keys —
//! split, where a cohesive slice of it justified its own file (T516), into
//! `lifecycle` (connecting/disconnecting), `execute` (running statements,
//! transactions, cancelling a hung query), [`ai_features`] (Ask/fix-error/
//! explain/optimize — `pub`, unlike its other siblings, since
//! [`ai_features::AiRequest`] crosses the crate boundary to the host),
//! `cell_edit` (previewing an editable table, staging/committing row changes,
//! the detail/DDL popup), `row_search` (the server-side row search), and
//! `panels` (history/saved/log/ER-diagram/import/export, and the
//! tree/editor/results panes' own key dispatch). What's left here is
//! genuinely shared: the `Browser`/`Form`/`Pane`/`View`/`Popup` state itself
//! and the two top-level dispatchers.
//!
//! # Architecture and invariants
//!
//! - **No rendering here.** This crate is a pure state machine: the host
//!   (`tui/src/ui`) reads the public fields and draws; the host's key handler
//!   calls [`Browser::handle_key`] and acts on the returned [`Outcome`].
//! - **Single-threaded UI, threaded I/O.** Anything that can block (connect,
//!   a user query, a cancel's reconnect) runs on a background thread, and the
//!   host polls the matching `poll_*` method each tick; AI requests are run by
//!   the host itself (see [`ai_features`]). The [`Browser`] thus never blocks
//!   the event loop on a stuck database (Run H, T531/T532).
//! - **Secrets stay in memory.** Passwords live only in [`Browser::password`]
//!   and the in-memory connection URL; persisted history stores bind-parameter
//!   *templates*, never substituted values (see `history_override`).
//! - **Read-only by default.** A session starts read-only whatever the saved
//!   connection says until the user opts in; write/DDL statements are then
//!   additionally gated behind a [`View::Confirm`] prompt.
//! - **Host persistence is pull-based.** Mutations set a private dirty flag
//!   and the host collects the changed store through the `take_dirty_*`
//!   methods, so this crate never touches the settings file itself.
//!
//! The owning spec is `crates/vix-db/spec/index.md` (the live-session design
//! is in `crates/vix-db/spec/session.md`).

// Shared workspace i18n (see the vix_i18n crate).
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

pub mod ai;
pub mod ai_features;
pub mod alias;
pub mod catalog;
mod cell_edit;
pub mod chart;
pub mod columns;
pub mod compare;
pub mod complete;
pub mod conn_string;
pub mod connect;
mod copy_ai;
pub mod editor;
pub mod environment;
pub mod erd;
mod execute;
pub mod export;
pub mod format;
pub mod highlight;
pub mod import;
mod lifecycle;
pub mod mcp_share;
mod panels;
pub mod params;
pub mod plan;
pub mod relations;
pub mod results;
pub mod row_filter;
mod row_search;
pub mod secret;
pub mod session;
pub mod staging;
pub mod store;
pub mod table_export;
pub mod tunnel;

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// What the host should do after [`Browser::handle_key`].
///
/// Only two outcomes exist because every other effect (status messages,
/// view changes, persistence flags) is communicated through [`Browser`]'s own
/// fields rather than the return value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The key was handled; nothing further.
    Consumed,
    /// Close the DB overlay.
    Close,
}

/// Which overlay screen is showing.
///
/// Exactly one view is active at a time; [`Browser::handle_key`] dispatches
/// on it, and the host renders the matching screen. [`View::Workbench`] is
/// the "home" view that most modal views return to when dismissed.
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
    /// The export dialog: the results grid, or whole tables from the tree
    /// while [`Browser::table_export`] is set.
    Export,
    /// The session query log (Ctrl+L).
    Log,
    /// The generated ER-diagram viewer (Ctrl+E).
    Erd,
    /// The natural-language "Ask AI" prompt (Ctrl+A).
    Ask,
    /// Collecting values for `:name` bind parameters before a run.
    Params,
    /// The file-path prompt: CSV/TSV import (Ctrl+U), or a `.sql` file to
    /// load and run (Ctrl+P, see [`Browser::import_is_sql_file`]).
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
///
/// Set by mutating code, cleared by the `take_dirty_*` accessors on
/// [`Browser`]. Keeping this as flags (rather than pushing to the host) lets
/// the host batch writes to the settings file once per tick.
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
    /// The next pane in Tab order (tree, editor, results, then wrapping).
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::Pane;
    /// assert_eq!(Pane::Tree.next(), Pane::Editor);
    /// assert_eq!(Pane::Results.next(), Pane::Tree);
    /// ```
    #[must_use]
    pub fn next(self) -> Pane {
        match self {
            Pane::Tree => Pane::Editor,
            Pane::Editor => Pane::Results,
            Pane::Results => Pane::Tree,
        }
    }

    /// The previous pane in Tab order (Shift+Tab).
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::Pane;
    /// assert_eq!(Pane::Tree.prev(), Pane::Results);
    /// ```
    #[must_use]
    pub fn prev(self) -> Pane {
        // With three panes, two forward steps equal one step back.
        self.next().next()
    }
}

/// Number of text form fields (name, kind, file, host, port, user, database,
/// password command, ssh host/user/port/identity, sslmode).
///
/// The `FORM_*` constants below are indices into [`Form::fields`] (and, for
/// the two toggles past the end, row numbers into the form); they must stay in
/// step with the field order documented on [`Form::fields`]. The kind row
/// occupies slot 1 for navigation but holds no text.
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

/// Index of the environment row (local … production), cycled, not typed
/// (T781). Its value is [`Browser::form_environment`].
pub const FORM_ENVIRONMENT: usize = FORM_FIELDS + 2;

/// Index of the **Share with MCP** row, toggled, not typed (T783). Its
/// value is [`Browser::form_mcp_sharing`].
pub const FORM_MCP_SHARED: usize = FORM_FIELDS + 3;

/// Total navigable form rows (text fields plus the four cycled rows).
pub const FORM_ROWS: usize = FORM_FIELDS + 4;

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
    ///
    /// `editing` is the index of `conn` in the saved list so a later save
    /// replaces it rather than appending; pass `None` to treat the form as a
    /// copy-and-add. The inverse is [`Form::to_connection`].
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::{connect::Connection, Form};
    /// let conn = Connection { name: "app".into(), ..Connection::default() };
    /// let form = Form::from_connection(&conn, Some(0));
    /// assert_eq!(form.fields[0], "app");
    /// assert_eq!(form.to_connection().name, "app");
    /// ```
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
    ///
    /// Performs no validation (an empty name is rejected by the form's key
    /// handler on save, not here), so the result may be incomplete.
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
///
/// Built from [`complete::Completer`] output when typing in the SQL editor;
/// Tab accepts the highlighted item, replacing the text from [`Popup::start`]
/// to the cursor.
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
///
/// Only the renderer knows the pane heights, so the host passes the latest
/// values to every [`Browser::handle_key`] call; PageUp/PageDown and
/// scroll-into-view use them. All zero (the default) is safe in tests.
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
///
/// Public fields are the read side for the renderer (and a few host
/// mutators); private fields are internal state machines for in-flight work.
/// Behavior is spread over `impl Browser` blocks in `lifecycle`, `execute`,
/// `panels`, `cell_edit` and [`ai_features`], all sharing this one struct.
pub struct Browser {
    /// Saved connections (mirrors the persisted setting).
    pub connections: Vec<connect::Connection>,
    /// Each saved connection's environment, by connection name (mirrors the
    /// persisted `db_environments` setting; a missing name is `local`). The
    /// host fills it with [`Browser::set_environments`] and collects changes
    /// with [`Browser::take_dirty_environments`] (T781).
    environments: std::collections::BTreeMap<String, connect::Environment>,
    /// The environments as last handed to or collected by the host; a
    /// difference from `environments` is what makes them dirty.
    saved_environments: std::collections::BTreeMap<String, connect::Environment>,
    /// The environment picked on the add/edit form's environment row.
    pub form_environment: connect::Environment,
    /// The names of the saved connections `vix --mcp` shares with MCP
    /// clients (mirrors the persisted `db_mcp_shared` setting, T783). The
    /// host fills it with [`Browser::set_mcp_shared`] and collects changes
    /// with [`Browser::take_dirty_mcp_shared`].
    mcp_shared: std::collections::BTreeSet<String>,
    /// The shared names as last handed to or collected by the host.
    saved_mcp_shared: std::collections::BTreeSet<String>,
    /// The add/edit form's **Share with MCP** row.
    pub form_mcp_sharing: mcp_share::Sharing,
    /// A password taken from a connection string pasted into the open form,
    /// until the form is saved or cancelled. Never persisted.
    form_password: Option<String>,
    /// Passwords from pasted connection strings, by connection name, waiting
    /// for that connection's next connect, where they are used like a typed
    /// password (and saved to the keyring only if the connection opts in).
    /// Memory only.
    held_passwords: std::collections::HashMap<String, String>,
    /// The statement behind the current grid and, for a table preview, its
    /// `(schema, table)`; read by Copy for AI (T782).
    grid_source: Option<(String, Option<(String, String)>)>,
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
    /// Connection index awaiting its password (set while [`View::Password`]
    /// is showing; `None` otherwise).
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
    /// Execution awaiting write/DDL confirmation (the [`View::Confirm`]
    /// screen); cleared on confirm or cancel.
    pending_run: Option<execute::PendingRun>,
    /// The plan tree's rows and their marks, from the last `PostgreSQL`
    /// explain (T776). Kept beside the grid rather than in it, and only
    /// reported by [`Browser::plan_marks`] while the grid still holds
    /// exactly those rows, so any later result drops them on its own.
    plan_marks: Option<(Vec<Vec<String>>, Vec<results::Mark>)>,
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
    pub edits: staging::CellEdits,
    /// Staged new rows and row deletions on the editable grid (T777), keyed
    /// by underlying row index.
    pub staged: staging::Staged,
    /// Column facts of the editable table (generated columns, pick lists).
    pub columns: Vec<columns::ColumnInfo>,
    /// The cell being edited (underlying row, column), and the value typed.
    editing_cell: Option<(usize, usize)>,
    /// The value being typed on the cell editor.
    pub edit_input: String,
    /// The highlighted value of the cell editor's pick list, if the column
    /// offers one (an index into [`Browser::edit_choices`]).
    pub edit_choice: Option<usize>,
    /// The `(schema, table)` the grid is a preview of, editable or not; the
    /// server-side row search re-runs it.
    preview: Option<(String, String)>,
    /// The row-search form (`F`).
    pub row_filter: row_filter::FilterForm,
    /// The `WHERE` condition the current preview was narrowed by, if any.
    server_filter: Option<String>,
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
    /// Tables marked in the tree (`m`) for a table export, as
    /// `(schema, table)` (T784).
    pub export_marks: std::collections::BTreeSet<(String, String)>,
    /// The table-export dialog, while [`View::Export`] exports whole tables
    /// rather than the results grid (T784).
    pub table_export: Option<table_export::Dialog>,
    /// A table export running on its own connection (T784).
    pending_export: Option<table_export::Pending>,
    /// What the [`View::Import`] path prompt does with its file: import
    /// CSV/TSV, or load and run a `.sql` file (T786).
    import_target: panels::PathTarget,
    /// The schema compare (T785), a modal layer over the workbench while
    /// open; see [`Browser::compare`].
    compare: Option<compare::review::Compare>,
}

/// State for the bind-parameter prompt: the SQL and its `:name` placeholders,
/// filled one value at a time.
#[derive(Debug, Clone, Default)]
pub struct ParamPrompt {
    /// The original statement holding the placeholders. Private so the host
    /// cannot desynchronize it from [`ParamPrompt::names`].
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
    ///
    /// `connections` is the host's saved-connection list (from settings).
    /// The new browser is disconnected and read-only; the host should also
    /// load [`Browser::history`] and [`Browser::saved`] from their stores.
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::{Browser, View};
    /// let b = Browser::new(Vec::new());
    /// assert_eq!(b.view, View::Connections);
    /// assert!(!b.write_enabled());
    /// ```
    #[must_use]
    pub fn new(connections: Vec<connect::Connection>) -> Browser {
        Browser {
            connections,
            environments: std::collections::BTreeMap::new(),
            saved_environments: std::collections::BTreeMap::new(),
            form_environment: connect::Environment::Local,
            mcp_shared: std::collections::BTreeSet::new(),
            saved_mcp_shared: std::collections::BTreeSet::new(),
            form_mcp_sharing: mcp_share::Sharing::Private,
            form_password: None,
            held_passwords: std::collections::HashMap::new(),
            grid_source: None,
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
            plan_marks: None,
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
            edits: staging::CellEdits::new(),
            staged: staging::Staged::default(),
            columns: Vec::new(),
            editing_cell: None,
            edit_input: String::new(),
            edit_choice: None,
            preview: None,
            row_filter: row_filter::FilterForm::default(),
            server_filter: None,
            pending_query: None,
            tunnel: None,
            pending_connect: None,
            pending_reconnect: None,
            export_marks: std::collections::BTreeSet::new(),
            table_export: None,
            pending_export: None,
            import_target: panels::PathTarget::Import,
            compare: None,
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

    /// The environment of the saved connection named `name` (`local` when it
    /// has none recorded).
    #[must_use]
    pub fn environment_of(&self, name: &str) -> connect::Environment {
        self.environments.get(name).copied().unwrap_or_default()
    }

    /// The environment of the active connection (`local` when disconnected).
    #[must_use]
    pub fn active_environment(&self) -> connect::Environment {
        self.conn
            .as_ref()
            .map(|c| self.environment_of(&c.name))
            .unwrap_or_default()
    }

    /// The connection environments if they changed since the last call (for
    /// the host to persist as `db_environments`), clearing the flag.
    pub fn take_dirty_environments(
        &mut self,
    ) -> Option<std::collections::BTreeMap<String, connect::Environment>> {
        if self.environments == self.saved_environments {
            None
        } else {
            self.saved_environments.clone_from(&self.environments);
            Some(self.environments.clone())
        }
    }

    /// Load the saved connections' environments (the host's
    /// `db_environments` setting), by connection name.
    pub fn set_environments(
        &mut self,
        environments: std::collections::BTreeMap<String, connect::Environment>,
    ) {
        self.saved_environments.clone_from(&environments);
        self.environments = environments;
    }

    /// Whether the saved connection named `name` is shared with MCP clients
    /// (T783; off unless the user turned it on).
    #[must_use]
    pub fn is_mcp_shared(&self, name: &str) -> bool {
        self.mcp_shared.contains(name)
    }

    /// The MCP sharing flags if they changed since the last call (for the
    /// host to persist as `db_mcp_shared`, one `name = true` entry per shared
    /// connection), clearing the flag.
    pub fn take_dirty_mcp_shared(&mut self) -> Option<std::collections::BTreeMap<String, bool>> {
        if self.mcp_shared == self.saved_mcp_shared {
            None
        } else {
            self.saved_mcp_shared.clone_from(&self.mcp_shared);
            Some(self.mcp_shared.iter().map(|n| (n.clone(), true)).collect())
        }
    }

    /// Load which saved connections are shared with MCP clients (the host's
    /// `db_mcp_shared` setting); a `false` entry is the same as none.
    pub fn set_mcp_shared(&mut self, shared: &std::collections::BTreeMap<String, bool>) {
        self.mcp_shared = shared
            .iter()
            .filter(|(_, on)| **on)
            .map(|(name, _)| name.clone())
            .collect();
        self.saved_mcp_shared.clone_from(&self.mcp_shared);
    }

    /// Whether the open form holds a password from a pasted connection
    /// string (shown as a hint; the password itself is never displayed).
    #[must_use]
    pub fn form_password_held(&self) -> bool {
        self.form_password.is_some()
    }

    /// Offer a bracketed paste to the overlay. On the add/edit form, a
    /// connection string (see [`conn_string::parse`]) fills the engine,
    /// file, host, port, user, database and TLS-mode fields — and the name,
    /// when it is still empty — and its password is held in memory for the
    /// next connect, never saved. Returns `true` when the paste was used
    /// that way (or was a malformed connection string, reported on the
    /// status line); `false` means the host should deliver it as keystrokes.
    pub fn paste(&mut self, text: &str) -> bool {
        if self.view != View::Form {
            return false;
        }
        match conn_string::parse(text) {
            Ok(parsed) => {
                self.fill_form(parsed);
                true
            }
            Err(conn_string::ParseError::NotAConnectionString) => false,
            Err(e) => {
                self.message = Some(t!("msg.db_dsn_invalid", error = e.to_string()).to_string());
                true
            }
        }
    }

    /// Fill the form from a parsed connection string (see [`Self::paste`]).
    fn fill_form(&mut self, parsed: conn_string::Parsed) {
        let c = parsed.connection;
        let f = &mut self.form;
        f.kind = c.kind;
        f.fields[2] = c.file;
        f.fields[3] = c.host;
        f.fields[4] = c.port;
        f.fields[5] = c.user;
        f.fields[6] = c.database;
        f.fields[FORM_SSLMODE] = c.sslmode;
        if f.fields[0].trim().is_empty() {
            f.fields[0] = default_name(&f.fields[6], &f.fields[2], &f.fields[3]);
        }
        let held = parsed.password.is_some();
        self.form_password = parsed.password;
        let mut message = if held {
            t!("msg.db_dsn_filled_password").to_string()
        } else {
            t!("msg.db_dsn_filled").to_string()
        };
        if !parsed.ignored.is_empty() {
            message.push(' ');
            message.push_str(&t!("msg.db_dsn_ignored", keys = parsed.ignored.join(", ")));
        }
        self.message = Some(message);
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
    ///
    /// Clears the one-line [`Browser::message`] first, so a message lives
    /// exactly until the next key press. `pages` are the pane heights from
    /// the latest layout (see [`Pages`]).
    pub fn handle_key(&mut self, key: KeyEvent, pages: Pages) -> Outcome {
        self.message = None;
        if self.compare.is_some() {
            return self.key_compare(key, pages.results);
        }
        match self.view {
            View::Connections => self.key_connections(key),
            View::Form => self.key_form(key),
            View::Password => self.key_password(key),
            View::Workbench if self.row_filter.open => self.key_row_filter(key),
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

    /// The workbench's `Ctrl+` letter chords (sub-views, AI helpers, import,
    /// run file, schema compare). Returns whether `code` was one of them.
    fn workbench_ctrl_chord(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r' | 'R') => self.open_history(),
            KeyCode::Char('b' | 'B') => self.open_saved(),
            KeyCode::Char('s' | 'S') => self.open_save_name(),
            KeyCode::Char('l' | 'L') => self.open_log(),
            KeyCode::Char('e' | 'E') => self.generate_erd(),
            KeyCode::Char('a' | 'A') => self.open_ask(),
            KeyCode::Char('o' | 'O') => self.optimize_current(),
            KeyCode::Char('f' | 'F') => self.fix_error(),
            KeyCode::Char('k' | 'K') => self.explain_query(),
            KeyCode::Char('u' | 'U') => self.open_import(),
            KeyCode::Char('p' | 'P') => self.open_run_file(),
            KeyCode::Char('d' | 'D') => self.open_compare(),
            _ => return false,
        }
        true
    }

    /// Keys inside the workbench, after the pane-independent chords.
    ///
    /// Order matters: a busy workbench (a query running, or a cancelled
    /// query's reconnect in flight) swallows keys first; then the global chords (F5/F6/F7/F8/F9 run, explain and
    /// toggle write mode; Ctrl+R/B/S/L/E/A/O/F/K/U/P/D open the sub-views and AI
    /// helpers; Alt+Shift+F formats); then Tab focus cycling (suppressed while the
    /// autocomplete popup owns Tab); and only then the focused pane's own
    /// handler.
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
            code if ctrl && self.workbench_ctrl_chord(code) => return Outcome::Consumed,
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
            // Esc only leaves the workbench when nothing more local can
            // consume it (popup, results filter, tree filter).
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

/// A name for a connection filled from a connection string whose form had
/// none: the database, else the file's stem, else the host.
fn default_name(database: &str, file: &str, host: &str) -> String {
    if !database.is_empty() {
        return database.to_string();
    }
    if !file.is_empty() {
        return std::path::Path::new(file)
            .file_stem()
            .map_or_else(|| file.to_string(), |s| s.to_string_lossy().into_owned());
    }
    host.to_string()
}

/// Rows of the catalog objects query as `(schema, name, kind)` triples.
///
/// Rows with fewer than three columns are dropped rather than panicking, since
/// the row shape comes from a driver and a catalog query.
fn object_triples(rows: Vec<Vec<String>>) -> Vec<(String, String, String)> {
    rows.into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| (r[0].clone(), r[1].clone(), r[2].clone()))
        .collect()
}

/// Pretty-print `text` when it parses as JSON (the cell viewer's `p` toggle);
/// `None` for non-JSON content.
///
/// # Examples
///
/// ```
/// let pretty = vix_db::pretty_json(r#"{"a":1}"#).unwrap();
/// assert!(pretty.contains("\"a\": 1"));
/// assert_eq!(vix_db::pretty_json("not json"), None);
/// ```
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
    fn a_postgres_plan_replaces_the_grid_with_its_tree_and_others_stay_text() {
        let yaml = "- Plan: \n    Node Type: \"Seq Scan\"\n    Relation Name: \"users\"\n    \
                    Total Cost: 1.1\n    Plan Rows: 1\n    Actual Total Time: 0.2\n    \
                    Actual Rows: 40\n    Actual Loops: 1\n  Planning Time: 0.1\n  \
                    Execution Time: 0.3";
        let mut b = browser();
        b.grid
            .set(vec!["QUERY PLAN".into()], vec![vec![yaml.into()]]);
        let plan = b.show_plan_tree(connect::Kind::Postgres).expect("parses");
        assert_eq!(plan.nodes.len(), 1);
        assert_eq!(b.grid.rows.len(), 1);
        assert_eq!(b.grid.rows[0][0], "Seq Scan on users");
        assert_eq!(b.grid.headers.len(), b.grid.rows[0].len());
        let marks = b.plan_marks().expect("the grid holds the plan tree");
        assert!(marks[0].hot && marks[0].warn, "100% and 40× off");
        // The plan-doctor full-scan hint still sees the tree's titles.
        assert!(catalog::scan_insight(connect::Kind::Postgres, &b.grid.rows));

        // A text plan (any other engine, or unparsable text) is left alone.
        let text = vec![vec!["SCAN users".to_string()]];
        b.grid.set(vec!["detail".into()], text.clone());
        assert!(b.show_plan_tree(connect::Kind::Sqlite).is_none());
        assert!(b.show_plan_tree(connect::Kind::Postgres).is_none());
        assert_eq!(b.grid.rows, text);
        assert_eq!(b.plan_marks(), None, "the marks left with the plan rows");
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
        assert_eq!(b.saved.queries, [] as [store::SavedQuery; 0]);
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
        assert_eq!(b.tree.filter, "");
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

    fn type_into(b: &mut Browser, text: &str) {
        for c in text.chars() {
            b.handle_key(key(KeyCode::Char(c)), Pages::default());
        }
    }

    #[test]
    fn pasted_connection_string_fills_the_form_and_holds_the_password() {
        let mut b = browser();
        assert!(
            !b.paste("postgres://u:pw@h/d"),
            "only the form takes a paste"
        );
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        assert!(!b.paste("just some text"), "ordinary text is typed instead");
        assert!(b.paste("postgresql://joel:s%40cret@db.example.com:5433/orders?sslmode=require"));
        assert_eq!(b.form.kind, connect::Kind::Postgres);
        assert_eq!(
            b.form.fields[0], "orders",
            "the name defaults to the database"
        );
        assert_eq!(b.form.fields[3], "db.example.com");
        assert_eq!(b.form.fields[4], "5433");
        assert_eq!(b.form.fields[5], "joel");
        assert_eq!(b.form.fields[6], "orders");
        assert_eq!(b.form.fields[FORM_SSLMODE], "require");
        assert!(b.form_password_held());
        assert!(
            !b.form.fields.iter().any(|f| f.contains("cret")),
            "the password never lands in a field"
        );
        b.handle_key(key(KeyCode::Enter), Pages::default());
        let saved = b.connections.last().unwrap().clone();
        assert_eq!(saved.name, "orders");
        assert_eq!(
            b.held_passwords.get("orders").map(String::as_str),
            Some("s@cret")
        );
        assert!(!b.form_password_held());
        // The persisted connection has nowhere to put a password.
        let toml = format!("{saved:?}");
        assert!(!toml.contains("s@cret"), "{toml}");
    }

    #[test]
    fn malformed_connection_string_is_reported_and_cancel_drops_the_password() {
        let mut b = browser();
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        type_into(&mut b, "keep");
        assert!(b.paste("postgres://h:99999/d"));
        assert!(b.message.is_some(), "a malformed string is reported");
        assert_eq!(b.form.fields[0], "keep", "nothing was filled");
        assert!(b.paste("host=h password=pw"));
        assert_eq!(b.form.fields[0], "keep", "a typed name is kept");
        assert!(b.form_password_held());
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert!(
            !b.form_password_held(),
            "cancel forgets the pasted password"
        );
        assert!(b.held_passwords.is_empty());
    }

    #[test]
    fn environment_row_cycles_and_saves_by_name() {
        let mut b = browser();
        b.set_environments(std::collections::BTreeMap::new());
        assert!(b.take_dirty_environments().is_none());
        assert_eq!(b.environment_of("app"), connect::Environment::Local);
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        while b.form.sel != FORM_ENVIRONMENT {
            b.handle_key(key(KeyCode::Down), Pages::default());
        }
        for _ in 0..4 {
            b.handle_key(key(KeyCode::Char(' ')), Pages::default());
        }
        assert_eq!(b.form_environment, connect::Environment::Production);
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.environment_of("app"), connect::Environment::Production);
        let envs = b.take_dirty_environments().expect("environments changed");
        assert_eq!(envs.get("app"), Some(&connect::Environment::Production));
        assert!(
            b.take_dirty_environments().is_none(),
            "flag clears after take"
        );
        // Renaming carries the environment to the new name.
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        assert_eq!(b.form_environment, connect::Environment::Production);
        type_into(&mut b, "2");
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert_eq!(b.environment_of("app2"), connect::Environment::Production);
        assert_eq!(b.environment_of("app"), connect::Environment::Local);
        // Deleting drops it.
        b.handle_key(key(KeyCode::Char('d')), Pages::default());
        assert_eq!(
            b.take_dirty_environments(),
            Some(std::collections::BTreeMap::new())
        );
    }

    #[test]
    fn share_row_is_off_by_default_and_saves_by_name() {
        let mut b = browser();
        b.set_mcp_shared(&std::collections::BTreeMap::from([(
            "gone".to_string(),
            false,
        )]));
        assert!(b.take_dirty_mcp_shared().is_none());
        assert!(!b.is_mcp_shared("app"));
        // A new connection starts unshared.
        b.handle_key(key(KeyCode::Char('a')), Pages::default());
        assert!(!b.form_mcp_sharing.is_shared());
        b.handle_key(key(KeyCode::Esc), Pages::default());
        // Turn sharing on for `app`.
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        while b.form.sel != FORM_MCP_SHARED {
            b.handle_key(key(KeyCode::Down), Pages::default());
        }
        b.handle_key(key(KeyCode::Char(' ')), Pages::default());
        assert!(b.form_mcp_sharing.is_shared());
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert!(b.is_mcp_shared("app"));
        let shared = b.take_dirty_mcp_shared().expect("sharing changed");
        assert_eq!(
            shared,
            std::collections::BTreeMap::from([("app".to_string(), true)])
        );
        assert!(
            b.take_dirty_mcp_shared().is_none(),
            "flag clears after take"
        );
        // Renaming carries it; deleting drops it.
        b.handle_key(key(KeyCode::Char('e')), Pages::default());
        assert!(b.form_mcp_sharing.is_shared());
        type_into(&mut b, "2");
        b.handle_key(key(KeyCode::Enter), Pages::default());
        assert!(b.is_mcp_shared("app2"));
        assert!(!b.is_mcp_shared("app"));
        b.handle_key(key(KeyCode::Char('d')), Pages::default());
        assert!(!b.is_mcp_shared("app2"));
        assert_eq!(
            b.take_dirty_mcp_shared(),
            Some(std::collections::BTreeMap::new())
        );
    }

    fn workbench_on(env: connect::Environment, tx: TxState) -> Browser {
        let mut b = browser();
        let mut envs = std::collections::BTreeMap::new();
        envs.insert("app".to_string(), env);
        b.set_environments(envs);
        b.conn = Some(b.connections[0].clone());
        b.view = View::Workbench;
        b.focus = Pane::Editor;
        b.write_enabled = true;
        b.tx = tx;
        b
    }

    #[test]
    fn production_confirms_writes_inside_a_transaction() {
        let mut b = workbench_on(connect::Environment::Production, TxState::Open);
        assert_eq!(b.active_environment(), connect::Environment::Production);
        type_into(&mut b, "update t set x = 1");
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(
            b.view,
            View::Confirm,
            "production asks even in a transaction"
        );
        assert_eq!(
            b.pending_statements(),
            vec!["update t set x = 1".to_string()]
        );
    }

    #[test]
    fn local_skips_the_confirmation_in_a_transaction_but_not_for_drop() {
        let mut b = workbench_on(connect::Environment::Local, TxState::Open);
        type_into(&mut b, "drop table t");
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(b.view, View::Confirm, "DROP always asks");
        b.handle_key(key(KeyCode::Esc), Pages::default());
        assert_eq!(b.pending_statements(), Vec::<String>::new());
        // A plain write inside the transaction runs without asking (it fails
        // here, with no session, which is all this test needs).
        b.query = editor::Query::default();
        type_into(&mut b, "update t set x = 1");
        b.handle_key(key(KeyCode::F(5)), Pages::default());
        assert_eq!(b.view, View::Workbench);
        assert_eq!(b.pending_statements(), Vec::<String>::new());
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
