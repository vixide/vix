//! The schema compare screens of the workbench (T785): choose the target
//! connection, review the changes, and copy, save, open or run the
//! migration.
//!
//! The compare is a modal layer over the workbench held in a private
//! [`Browser`] field, so it needs no new [`crate::View`]; the host draws it
//! when [`Browser::compare`] is `Some`. The source is the workbench's own
//! connection; the target is another saved connection of the same engine,
//! opened on its own session through the normal credential waterfall
//! (`password_command`, keyring, then a prompt). Connecting and reading the
//! target, and running the migration, happen on a background thread polled
//! by [`Browser::poll_compare`].

use std::sync::mpsc;
use std::time::Instant;

use vix_input::{KeyCode, KeyEvent};

use super::diff::{Change, diff};
use super::plan::{Plan, plan};
use super::read::{default_schema, read_session, supported};
use super::snapshot::Snapshot;
use crate::lifecycle::{ConnectOutcome, connect_worker};
use crate::{Browser, Outcome, Pane, View, connect, editor, session, tunnel};

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// Which screen of the schema compare is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Screen {
    /// Choosing the target connection (and the schema name).
    Pick,
    /// The target's password prompt, when no stored credential was found.
    Password,
    /// Connecting to and reading the target, or running the migration.
    Working,
    /// The list of changes with their checkboxes.
    Review,
    /// Confirming a run on the target.
    Confirm,
    /// Typing the path to save the script to.
    SavePath,
    /// The whole script, scrollable.
    Script,
}

/// What the background thread reports.
enum Work {
    /// The target is connected; its snapshot, or the read error.
    Connected {
        /// The target session.
        session: session::Session,
        /// Its SSH tunnel, if any.
        tunnel: Option<tunnel::Tunnel>,
        /// The target snapshot, or the read error text.
        snapshot: Result<Snapshot, String>,
    },
    /// No stored password: prompt for one.
    NeedsPassword,
    /// The connect failed.
    Failed(String),
    /// The migration ran (or failed and was rolled back).
    Ran {
        /// The target session, handed back.
        session: session::Session,
        /// The statement count, or the failure text.
        result: Result<usize, String>,
        /// The target re-read after a successful run.
        snapshot: Option<Result<Snapshot, String>>,
    },
}

/// Work running on a background thread.
struct Pending {
    /// Its eventual report.
    rx: mpsc::Receiver<Work>,
    /// When it started, for the elapsed-seconds line.
    started: Instant,
    /// Whether it is a migration run (which cannot be abandoned).
    running: bool,
    /// A password typed on the prompt, kept only until the connect succeeds
    /// so it can be saved to the keyring when the connection asks for that.
    password: String,
}

/// The schema compare state, read by the host's renderer.
pub struct Compare {
    /// The showing screen.
    pub screen: Screen,
    /// The schema compared on both sides (editable on [`Screen::Pick`]).
    pub schema: String,
    /// Indices into [`Browser::connections`] that can be the target.
    pub candidates: Vec<usize>,
    /// Selection in [`Compare::candidates`].
    pub pick: usize,
    /// The password being typed on [`Screen::Password`] (memory only).
    pub password: String,
    /// The chosen target connection.
    pub target: Option<connect::Connection>,
    /// The target connection's environment (T781), recorded when the target
    /// is chosen.
    pub target_env: Option<connect::Environment>,
    /// The source connection's name.
    pub source_name: String,
    /// The changes, in diff order.
    pub changes: Vec<Change>,
    /// Whether each change is chosen (parallel to [`Compare::changes`]).
    pub checked: Vec<bool>,
    /// The highlighted change.
    pub cursor: usize,
    /// The path typed on [`Screen::SavePath`].
    pub path: String,
    /// Scroll offset (lines) of [`Screen::Script`].
    pub script_scroll: usize,
    /// The source snapshot.
    source: Snapshot,
    /// The target snapshot.
    target_snapshot: Snapshot,
    /// The target session, once connected.
    session: Option<session::Session>,
    /// The target's SSH tunnel, kept alive with the session.
    tunnel: Option<tunnel::Tunnel>,
    /// Whether the target session allows writes (starts from the saved
    /// connection's access, toggled with F8).
    target_write: bool,
    /// Background work in flight.
    pending: Option<Pending>,
}

impl Compare {
    /// Whether the target session allows writes.
    #[must_use]
    pub fn target_write_enabled(&self) -> bool {
        self.target_write
    }

    /// Whole seconds the background work has run, if any is running.
    #[must_use]
    pub fn elapsed_secs(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.started.elapsed().as_secs())
    }

    /// Whether the background work is a migration run (rather than reading).
    #[must_use]
    pub fn running_migration(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.running)
    }

    /// The target's name (empty before one is chosen).
    #[must_use]
    pub fn target_name(&self) -> String {
        self.target.as_ref().map_or_default(|c| c.name.clone())
    }

    /// The target's environment for the run confirmation, as its translated
    /// badge (`[production]`, …), once a target is chosen (T781).
    #[must_use]
    pub fn target_environment(&self) -> Option<String> {
        self.target_env.map(crate::environment::badge)
    }

    /// The migration for the chosen changes, in run order.
    #[must_use]
    pub fn plan(&self) -> Plan {
        let chosen: Vec<&Change> = self
            .changes
            .iter()
            .zip(&self.checked)
            .filter(|(_, on)| **on)
            .map(|(c, _)| c)
            .collect();
        plan(&chosen, &self.source, &self.target_snapshot)
    }

    /// The migration as one script with an explanatory header.
    #[must_use]
    pub fn script(&self) -> String {
        let header = t!(
            "ui.db_compare_script_header",
            source = self.source_name,
            target = self.target_name(),
            schema = self.schema
        )
        .to_string();
        self.plan().script(&[header])
    }

    /// Replace the changes with a fresh diff; destructive ones start
    /// unchecked.
    fn set_changes(&mut self) {
        self.changes = diff(&self.source, &self.target_snapshot);
        self.checked = self.changes.iter().map(|c| !c.destructive).collect();
        self.cursor = 0;
    }
}

/// Run `statements` on `session` in one transaction; on the first failure
/// roll back and report which statement failed.
fn run_migration(session: &mut session::Session, statements: &[String]) -> Result<usize, String> {
    session.run("BEGIN").map_err(|e| e.to_string())?;
    for (i, sql) in statements.iter().enumerate() {
        if let Err(e) = session.run(sql) {
            let _ = session.run("ROLLBACK");
            return Err(t!(
                "msg.db_compare_failed",
                n = add_or(i, 1, usize::MAX),
                error = e.to_string()
            )
            .to_string());
        }
    }
    if let Err(e) = session.run("COMMIT") {
        let _ = session.run("ROLLBACK");
        return Err(e.to_string());
    }
    Ok(statements.len())
}

/// A file name for the script from the target's name.
fn default_path(target: &str) -> String {
    let safe: String = target
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("migration-{safe}.sql")
}

impl Browser {
    /// The schema compare, when it is open (for the renderer).
    #[must_use]
    pub fn compare(&self) -> Option<&Compare> {
        self.compare.as_ref()
    }

    /// Whether the schema compare has background work running (keeps the
    /// host's event loop polling fast).
    #[must_use]
    pub fn compare_running(&self) -> bool {
        self.compare.as_ref().is_some_and(|c| c.pending.is_some())
    }

    /// Open the schema compare on the active connection (`Ctrl+D`, DB →
    /// Schema Compare…): choose a target among the other saved connections
    /// of the same engine.
    pub fn open_compare(&mut self) {
        let Some(conn) = self.conn.clone() else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        if !supported(conn.kind) {
            self.message =
                Some(t!("msg.db_compare_unsupported", kind = conn.kind.label()).to_string());
            return;
        }
        if self.query_running() {
            return;
        }
        let candidates: Vec<usize> = self
            .connections
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == conn.kind && **c != conn)
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            self.message =
                Some(t!("msg.db_compare_no_target", kind = conn.kind.label()).to_string());
            return;
        }
        let schema = match conn.kind {
            connect::Kind::Postgres => self
                .tree
                .selected_object()
                .map_or_else(|| default_schema(conn.kind).to_string(), |(s, _, _)| s),
            _ => default_schema(conn.kind).to_string(),
        };
        self.popup = None;
        self.compare = Some(Compare {
            screen: Screen::Pick,
            schema,
            candidates,
            pick: 0,
            password: String::new(),
            target: None,
            target_env: None,
            source_name: conn.name,
            changes: Vec::new(),
            checked: Vec::new(),
            cursor: 0,
            path: String::new(),
            script_scroll: 0,
            source: Snapshot::default(),
            target_snapshot: Snapshot::default(),
            session: None,
            tunnel: None,
            target_write: false,
            pending: None,
        });
    }

    /// Keys while the schema compare is open.
    pub(crate) fn key_compare(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let Some(screen) = self.compare.as_ref().map(|c| c.screen) else {
            return Outcome::Consumed;
        };
        match screen {
            Screen::Pick => self.key_compare_pick(key),
            Screen::Password => self.key_compare_password(key),
            Screen::Working => {
                // Reading can be abandoned; a running migration cannot.
                if key.code == KeyCode::Esc
                    && let Some(c) = self.compare.as_mut()
                    && !c.running_migration()
                {
                    c.pending = None;
                    c.screen = Screen::Pick;
                }
            }
            Screen::Review => self.key_compare_review(key, page),
            Screen::Confirm => self.key_compare_confirm(key),
            Screen::SavePath => self.key_compare_save(key),
            Screen::Script => {
                if let Some(c) = self.compare.as_mut() {
                    let step = page.max(1);
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            c.script_scroll = sub_or(c.script_scroll, 1, 0);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            c.script_scroll = add_or(c.script_scroll, 1, usize::MAX);
                        }
                        KeyCode::PageUp => c.script_scroll = sub_or(c.script_scroll, step, 0),
                        KeyCode::PageDown => {
                            c.script_scroll = add_or(c.script_scroll, step, usize::MAX);
                        }
                        KeyCode::Home => c.script_scroll = 0,
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                            c.screen = Screen::Review;
                        }
                        _ => {}
                    }
                }
            }
        }
        Outcome::Consumed
    }

    /// Keys on the target picker: arrows choose, typing edits the schema
    /// name, Enter compares, Esc closes.
    fn key_compare_pick(&mut self, key: KeyEvent) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => c.pick = sub_or(c.pick, 1, 0),
            KeyCode::Down => {
                c.pick = add_or(c.pick, 1, usize::MAX).min(sub_or(c.candidates.len(), 1, 0));
            }
            KeyCode::Char(ch) => c.schema.push(ch),
            KeyCode::Backspace => {
                c.schema.pop();
            }
            KeyCode::Esc => self.compare = None,
            KeyCode::Enter => self.start_compare(None),
            _ => {}
        }
    }

    /// Keys on the target's password prompt.
    fn key_compare_password(&mut self, key: KeyEvent) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(ch) => c.password.push(ch),
            KeyCode::Backspace => {
                c.password.pop();
            }
            KeyCode::Esc => {
                c.password.clear();
                c.screen = Screen::Pick;
            }
            KeyCode::Enter => {
                let password = std::mem::take(&mut c.password);
                self.start_compare(Some(password));
            }
            _ => {}
        }
    }

    /// Read the source snapshot on the workbench session, then connect to
    /// the picked target and read it on a background thread. `password` is
    /// the one typed on the prompt, or `None` for the credential waterfall.
    fn start_compare(&mut self, password: Option<String>) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        let Some((schema, target)) = self.compare.as_ref().and_then(|c| {
            let idx = *c.candidates.get(c.pick)?;
            Some((
                c.schema.trim().to_string(),
                self.connections.get(idx)?.clone(),
            ))
        }) else {
            return;
        };
        if schema.is_empty() {
            return;
        }
        let source = {
            let start = Instant::now();
            let result = match self.session.as_mut() {
                Some(s) => read_session(s, kind, &schema),
                None => Err(super::read::ReadError::Query(
                    session::SessionError::NotConnected,
                )),
            };
            self.log.push(crate::store::LogEntry {
                sql: t!("ui.db_compare_title").to_string(),
                ms: start.elapsed().as_millis(),
                rows: 0,
                ok: result.is_ok(),
                origin: crate::store::Origin::App,
            });
            match result {
                Ok(s) => s,
                Err(e) => {
                    self.message = Some(e.to_string());
                    return;
                }
            }
        };
        let source_name = self.conn.as_ref().map_or_default(|c| c.name.clone());
        if source.missing {
            self.message = Some(
                t!(
                    "msg.db_compare_no_schema",
                    schema = schema,
                    name = source_name
                )
                .to_string(),
            );
            return;
        }
        let typed = password.clone().unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let worker_conn = target.clone();
        let worker_schema = schema.clone();
        std::thread::spawn(move || {
            let work = match connect_worker(&worker_conn, password.as_deref()) {
                ConnectOutcome::Connected {
                    mut session,
                    tunnel,
                } => {
                    let snapshot = read_session(&mut session, worker_conn.kind, &worker_schema)
                        .map_err(|e| e.to_string());
                    Work::Connected {
                        session,
                        tunnel,
                        snapshot,
                    }
                }
                ConnectOutcome::NeedsPassword => Work::NeedsPassword,
                ConnectOutcome::Failed(e) => Work::Failed(e),
            };
            let _ = tx.send(work);
        });
        let target_env = self.environment_of(&target.name);
        if let Some(c) = self.compare.as_mut() {
            c.source = source;
            c.schema = schema;
            c.target_write = target.writable;
            c.target_env = Some(target_env);
            c.target = Some(target);
            c.session = None;
            c.tunnel = None;
            c.screen = Screen::Working;
            c.pending = Some(Pending {
                rx,
                started: Instant::now(),
                running: false,
                password: typed,
            });
        }
    }

    /// Drain finished background compare work. Called by the host each
    /// event-loop tick; cheap when nothing is running.
    pub fn poll_compare(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        let Some(pending) = c.pending.as_mut() else {
            return;
        };
        let work = match pending.rx.try_recv() {
            Ok(work) => work,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Work::Failed(t!("msg.db_not_connected").to_string())
            }
        };
        let typed = std::mem::take(&mut pending.password);
        c.pending = None;
        match work {
            Work::NeedsPassword => {
                c.password.clear();
                c.screen = Screen::Password;
            }
            Work::Failed(e) => {
                c.screen = Screen::Pick;
                self.message = Some(e);
            }
            Work::Connected {
                session,
                tunnel,
                snapshot,
            } => match snapshot {
                Ok(snapshot) => {
                    c.session = Some(session);
                    c.tunnel = tunnel;
                    c.target_snapshot = snapshot;
                    c.set_changes();
                    c.screen = Screen::Review;
                    // Like the workbench's own connect: a typed password is
                    // saved for next time when the connection opted in.
                    if let Some(target) = c.target.as_ref()
                        && target.store_keyring
                        && !typed.is_empty()
                        && !crate::secret::store(target, &typed)
                    {
                        self.message = Some(t!("msg.db_keyring_save_failed").to_string());
                    }
                }
                Err(e) => {
                    c.screen = Screen::Pick;
                    self.message = Some(e);
                }
            },
            Work::Ran {
                session,
                result,
                snapshot,
            } => {
                c.session = Some(session);
                c.screen = Screen::Review;
                let name = c.target_name();
                match result {
                    Ok(count) => {
                        if let Some(Ok(snapshot)) = snapshot {
                            c.target_snapshot = snapshot;
                            c.set_changes();
                        }
                        self.message = Some(
                            t!("msg.db_compare_applied", count = count, name = name).to_string(),
                        );
                    }
                    Err(e) => self.message = Some(e),
                }
            }
        }
    }

    /// Keys on the review screen.
    fn key_compare_review(&mut self, key: KeyEvent, page: usize) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        let last = sub_or(c.changes.len(), 1, 0);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => c.cursor = sub_or(c.cursor, 1, 0),
            KeyCode::Down | KeyCode::Char('j') => {
                c.cursor = add_or(c.cursor, 1, usize::MAX).min(last);
            }
            KeyCode::PageUp => c.cursor = sub_or(c.cursor, page.max(1), 0),
            KeyCode::PageDown => c.cursor = add_or(c.cursor, page.max(1), usize::MAX).min(last),
            KeyCode::Home => c.cursor = 0,
            KeyCode::End => c.cursor = last,
            KeyCode::Char(' ') => {
                if let Some(on) = c.checked.get_mut(c.cursor) {
                    *on = !*on;
                }
            }
            KeyCode::Enter | KeyCode::Char('v') => {
                c.script_scroll = 0;
                c.screen = Screen::Script;
            }
            KeyCode::Char('s') => {
                c.path = default_path(&c.target_name());
                c.screen = Screen::SavePath;
            }
            KeyCode::Char('y') => {
                let script = c.script();
                self.yank(&script);
            }
            KeyCode::Char('o') => self.open_compare_script(),
            KeyCode::Char('r') => self.confirm_compare_run(),
            KeyCode::F(8) => self.toggle_target_write(),
            KeyCode::Esc | KeyCode::Char('q') => self.compare = None,
            _ => {}
        }
    }

    /// Put the script in the workbench editor and close the compare. The
    /// editor runs on the source connection, so the message says so.
    fn open_compare_script(&mut self) {
        let Some(c) = self.compare.take() else { return };
        let script = c.script();
        self.query = editor::Query::default();
        self.insert_statement(&script);
        self.view = View::Workbench;
        self.focus = Pane::Editor;
        self.message = Some(t!("msg.db_compare_opened", name = c.target_name()).to_string());
    }

    /// Move to the run confirmation, or refuse: nothing to run, or the
    /// target session is read-only.
    fn confirm_compare_run(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        if c.plan().runnable().is_empty() {
            self.message = Some(t!("msg.db_compare_nothing").to_string());
        } else if !c.target_write {
            self.message = Some(t!("msg.db_compare_read_only", name = c.target_name()).to_string());
        } else {
            c.screen = Screen::Confirm;
        }
    }

    /// F8 on the review screen: toggle write mode on the target session.
    fn toggle_target_write(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        let (Some(kind), Some(session)) = (c.target.as_ref().map(|t| t.kind), c.session.as_mut())
        else {
            return;
        };
        let want = !c.target_write;
        if let Some(sql) = connect::read_only_sql(kind, !want)
            && let Err(e) = session.run(&sql)
        {
            self.message = Some(e.to_string());
            return;
        }
        c.target_write = want;
        self.message = Some(if want {
            t!("msg.db_write_enabled").to_string()
        } else {
            t!("msg.db_read_only_on").to_string()
        });
    }

    /// Keys on the run confirmation: Enter/`y` runs, Esc/`n` goes back.
    fn key_compare_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => self.run_compare(),
            KeyCode::Esc | KeyCode::Char('n' | 'q') => {
                if let Some(c) = self.compare.as_mut() {
                    c.screen = Screen::Review;
                }
            }
            _ => {}
        }
    }

    /// Run the migration on the target in one transaction, on a background
    /// thread, then re-read the target.
    fn run_compare(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        // Checked again here: the gate must hold whatever led to this call.
        if !c.target_write {
            c.screen = Screen::Review;
            self.message = Some(t!("msg.db_compare_read_only", name = c.target_name()).to_string());
            return;
        }
        let statements: Vec<String> = c.plan().runnable().into_iter().map(String::from).collect();
        let (Some(mut session), Some(kind)) = (c.session.take(), c.target.as_ref().map(|t| t.kind))
        else {
            c.screen = Screen::Review;
            return;
        };
        let schema = c.schema.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run_migration(&mut session, &statements);
            let snapshot = result
                .is_ok()
                .then(|| read_session(&mut session, kind, &schema).map_err(|e| e.to_string()));
            let _ = tx.send(Work::Ran {
                session,
                result,
                snapshot,
            });
        });
        c.screen = Screen::Working;
        c.pending = Some(Pending {
            rx,
            started: Instant::now(),
            running: true,
            password: String::new(),
        });
    }

    /// Keys on the save-path prompt.
    fn key_compare_save(&mut self, key: KeyEvent) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(ch) => c.path.push(ch),
            KeyCode::Backspace => {
                c.path.pop();
            }
            KeyCode::Esc => c.screen = Screen::Review,
            KeyCode::Enter => {
                let path = c.path.trim().to_string();
                if path.is_empty() {
                    return;
                }
                match std::fs::write(&path, c.script()) {
                    Ok(()) => {
                        c.screen = Screen::Review;
                        self.message = Some(t!("msg.db_exported", path = path).to_string());
                    }
                    Err(e) => self.message = Some(e.to_string()),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_is_a_safe_file_name() {
        assert_eq!(default_path("prod db/1"), "migration-prod_db_1.sql");
    }

    #[test]
    fn a_failing_statement_rolls_the_whole_migration_back() {
        let mut s = session::Session::connect("sqlite::memory:", &[]).unwrap();
        let err = run_migration(
            &mut s,
            &[
                "CREATE TABLE a (x INTEGER)".to_string(),
                "CREATE TABLE a (x INTEGER)".to_string(),
            ],
        )
        .unwrap_err();
        assert_ne!(err, String::new());
        let (_, rows) = s
            .run("SELECT count(*) FROM sqlite_master WHERE name = 'a'")
            .unwrap();
        assert_eq!(rows, vec![vec!["0".to_string()]]);
        assert_eq!(
            run_migration(&mut s, &["CREATE TABLE b (x)".to_string()]),
            Ok(1)
        );
    }

    #[test]
    fn opening_needs_a_connection() {
        let mut b = Browser::new(Vec::new());
        b.open_compare();
        assert!(b.compare().is_none());
        assert!(b.message.is_some());
    }

    use crate::test_support::{press, typed, workbench};

    /// A fresh scratch directory for one test.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-cmp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A `SQLite` file holding `ddl`, as a saved connection.
    fn target_file(
        dir: &std::path::Path,
        name: &str,
        ddl: &[&str],
        writable: bool,
    ) -> connect::Connection {
        let path = dir.join(format!("{name}.db")).display().to_string();
        let mut s =
            session::Session::connect(&crate::catalog::sqlite_url(&path, "rwc"), &[]).unwrap();
        for sql in ddl {
            s.run(sql).unwrap();
        }
        connect::Connection {
            name: name.into(),
            kind: connect::Kind::Sqlite,
            file: path,
            writable,
            ..connect::Connection::default()
        }
    }

    /// Let the background compare work finish, without sleeping.
    fn settle(b: &mut Browser) {
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while b.compare_running() {
            b.poll_compare();
            assert!(Instant::now() < deadline, "compare work stalled");
            std::thread::yield_now();
        }
        b.poll_compare();
    }

    /// A source with `a` and `shared`, and a saved target with `shared` and
    /// `extra`: the diff is "create a" (checked) and "drop extra" (unchecked).
    fn compare_pair(dir: &std::path::Path, target_writable: bool) -> Browser {
        let mut b = workbench(
            true,
            &[
                "CREATE TABLE a (id INTEGER)",
                "CREATE TABLE shared (x INTEGER)",
            ],
        );
        let target = target_file(
            dir,
            "other",
            &[
                "CREATE TABLE shared (x INTEGER)",
                "CREATE TABLE extra (y INTEGER)",
            ],
            target_writable,
        );
        b.connections = vec![
            connect::Connection {
                name: "mem".into(),
                kind: connect::Kind::Sqlite,
                writable: true,
                ..connect::Connection::default()
            },
            target,
        ];
        b
    }

    fn at_review(dir: &std::path::Path, target_writable: bool) -> Browser {
        let mut b = compare_pair(dir, target_writable);
        b.open_compare();
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::Working);
        settle(&mut b);
        assert_eq!(
            b.compare().unwrap().screen,
            Screen::Review,
            "{:?}",
            b.message
        );
        b
    }

    #[test]
    fn opening_refuses_unsupported_engines_and_a_missing_target() {
        let mut b = workbench(true, &[]);
        b.conn.as_mut().unwrap().kind = connect::Kind::Mysql;
        b.open_compare();
        assert!(b.compare().is_none());
        assert_eq!(
            b.message,
            Some(
                t!(
                    "msg.db_compare_unsupported",
                    kind = connect::Kind::Mysql.label()
                )
                .to_string()
            )
        );
        b.conn.as_mut().unwrap().kind = connect::Kind::Sqlite;
        b.open_compare();
        assert_eq!(
            b.message,
            Some(
                t!(
                    "msg.db_compare_no_target",
                    kind = connect::Kind::Sqlite.label()
                )
                .to_string()
            )
        );
        assert!(!b.compare_running());
    }

    #[test]
    fn the_picker_chooses_a_target_and_edits_the_schema_name() {
        let dir = scratch("pick");
        let mut b = compare_pair(&dir, true);
        b.open_compare();
        let c = b.compare().unwrap();
        assert_eq!(c.screen, Screen::Pick);
        assert_eq!(c.candidates, [1], "only the other saved connection");
        assert_eq!(c.schema, "main");
        assert_eq!(c.source_name, "mem");
        assert_eq!(c.target_name(), "");
        assert_eq!(c.target_environment(), None);
        assert_eq!(c.elapsed_secs(), None);
        assert!(!c.running_migration());
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Up);
        press(&mut b, KeyCode::Up);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.compare().unwrap().pick, 0);
        press(&mut b, KeyCode::Backspace);
        typed(&mut b, "x");
        assert_eq!(b.compare().unwrap().schema, "maix");
        press(&mut b, KeyCode::Esc);
        assert!(b.compare().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_schema_name_does_not_start() {
        let dir = scratch("schema");
        let mut b = compare_pair(&dir, true);
        b.open_compare();
        for _ in 0..4 {
            press(&mut b, KeyCode::Backspace);
        }
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::Pick);
        assert!(!b.compare_running());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_source_that_cannot_be_read_reports_why() {
        let dir = scratch("nosession");
        let mut b = compare_pair(&dir, true);
        b.open_compare();
        b.session = None;
        press(&mut b, KeyCode::Enter);
        assert!(b.message.is_some());
        assert_eq!(b.compare().unwrap().screen, Screen::Pick);
        assert!(
            b.log.entries.iter().any(|e| !e.ok),
            "the failed read is logged"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_can_be_abandoned_with_escape() {
        let dir = scratch("abandon");
        let mut b = compare_pair(&dir, true);
        b.open_compare();
        press(&mut b, KeyCode::Enter);
        assert!(b.compare_running());
        press(&mut b, KeyCode::Char('x'));
        assert_eq!(
            b.compare().unwrap().screen,
            Screen::Working,
            "other keys wait"
        );
        press(&mut b, KeyCode::Esc);
        assert_eq!(b.compare().unwrap().screen, Screen::Pick);
        assert!(!b.compare_running());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreachable_target_returns_to_the_picker_with_the_error() {
        let dir = scratch("unreachable");
        let mut b = compare_pair(&dir, true);
        b.connections[1].file = dir.join("no/such/dir/x.db").display().to_string();
        b.open_compare();
        press(&mut b, KeyCode::Enter);
        settle(&mut b);
        assert_eq!(b.compare().unwrap().screen, Screen::Pick);
        assert!(b.message.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_target_that_needs_a_password_prompts_for_it() {
        let dir = scratch("password");
        let mut b = compare_pair(&dir, true);
        b.open_compare();
        press(&mut b, KeyCode::Enter);
        settle(&mut b);
        // Drive the password screen directly: SQLite never asks for one.
        b.compare.as_mut().unwrap().screen = Screen::Password;
        typed(&mut b, "pw!");
        press(&mut b, KeyCode::Backspace);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.compare().unwrap().password, "pw");
        press(&mut b, KeyCode::Esc);
        assert_eq!(b.compare().unwrap().screen, Screen::Pick);
        assert_eq!(b.compare().unwrap().password, "");
        b.compare.as_mut().unwrap().screen = Screen::Password;
        typed(&mut b, "pw");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::Working);
        settle(&mut b);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_review_lists_changes_with_destructive_ones_unchecked() {
        let dir = scratch("review");
        let mut b = at_review(&dir, false);
        let c = b.compare().unwrap();
        assert_eq!(c.changes.len(), 2, "{:?}", c.changes.len());
        assert_eq!(c.checked.iter().filter(|on| **on).count(), 1);
        assert_eq!(c.target_name(), "other");
        assert!(!c.target_write_enabled());
        assert_eq!(
            c.target_environment(),
            Some(crate::environment::badge(connect::Environment::Local))
        );
        let first_checked = c.checked[0];
        // Cursor movement clamps at both ends; space toggles the change.
        press(&mut b, KeyCode::Up);
        assert_eq!(b.compare().unwrap().cursor, 0);
        press(&mut b, KeyCode::Char(' '));
        assert_eq!(b.compare().unwrap().checked[0], !first_checked);
        press(&mut b, KeyCode::Char(' '));
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Char('j'));
        assert_eq!(b.compare().unwrap().cursor, 1);
        press(&mut b, KeyCode::PageUp);
        assert_eq!(b.compare().unwrap().cursor, 0);
        press(&mut b, KeyCode::PageDown);
        assert_eq!(b.compare().unwrap().cursor, 1);
        press(&mut b, KeyCode::Home);
        assert_eq!(b.compare().unwrap().cursor, 0);
        press(&mut b, KeyCode::End);
        assert_eq!(b.compare().unwrap().cursor, 1);
        press(&mut b, KeyCode::Char('k'));
        assert_eq!(b.compare().unwrap().cursor, 0);
        let script = b.compare().unwrap().script();
        assert!(script.contains("CREATE TABLE"), "{script}");
        assert!(
            !script.contains("DROP TABLE"),
            "the destructive change is unchecked"
        );
        press(&mut b, KeyCode::Esc);
        assert!(b.compare().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_script_view_scrolls_and_returns() {
        let dir = scratch("script");
        let mut b = at_review(&dir, false);
        press(&mut b, KeyCode::Char('v'));
        assert_eq!(b.compare().unwrap().screen, Screen::Script);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Char('j'));
        press(&mut b, KeyCode::PageDown);
        assert_eq!(b.compare().unwrap().script_scroll, 2 + 1);
        press(&mut b, KeyCode::PageUp);
        press(&mut b, KeyCode::Up);
        press(&mut b, KeyCode::Char('k'));
        assert_eq!(b.compare().unwrap().script_scroll, 0);
        press(&mut b, KeyCode::PageDown);
        press(&mut b, KeyCode::Home);
        assert_eq!(b.compare().unwrap().script_scroll, 0);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.compare().unwrap().screen, Screen::Script);
        for leave in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Enter] {
            b.compare.as_mut().unwrap().screen = Screen::Script;
            press(&mut b, leave);
            assert_eq!(b.compare().unwrap().screen, Screen::Review);
        }
        press(&mut b, KeyCode::Enter);
        assert_eq!(
            b.compare().unwrap().screen,
            Screen::Script,
            "Enter views it too"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_script_can_be_copied_saved_and_opened_in_the_editor() {
        let dir = scratch("save");
        let mut b = at_review(&dir, false);
        press(&mut b, KeyCode::Char('y'));
        assert_eq!(b.message, Some(t!("msg.db_copied").to_string()));
        press(&mut b, KeyCode::Char('s'));
        assert_eq!(b.compare().unwrap().screen, Screen::SavePath);
        assert_eq!(b.compare().unwrap().path, "migration-other.sql");
        // An empty path does nothing; Esc goes back.
        b.compare.as_mut().unwrap().path.clear();
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::SavePath);
        press(&mut b, KeyCode::Tab);
        press(&mut b, KeyCode::Esc);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        // An unwritable path reports the error and stays on the prompt.
        press(&mut b, KeyCode::Char('s'));
        b.compare.as_mut().unwrap().path = dir.join("no/dir/x.sql").display().to_string();
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::SavePath);
        assert!(b.message.is_some());
        let out = dir.join("m.sql");
        b.compare.as_mut().unwrap().path.clear();
        typed(&mut b, &out.display().to_string());
        press(&mut b, KeyCode::Backspace);
        typed(&mut b, "l");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        assert_eq!(
            b.message,
            Some(t!("msg.db_exported", path = out.display().to_string()).to_string())
        );
        let saved = std::fs::read_to_string(&out).unwrap();
        assert_eq!(saved, b.compare().unwrap().script());
        // `o` moves the script into the workbench editor and closes the compare.
        press(&mut b, KeyCode::Char('o'));
        assert!(b.compare().is_none());
        assert_eq!(b.focus, Pane::Editor);
        assert!(b.query.text().contains("CREATE TABLE"));
        assert_eq!(
            b.message,
            Some(t!("msg.db_compare_opened", name = "other").to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn running_needs_something_to_run_and_a_writable_target() {
        let dir = scratch("gates");
        let mut b = at_review(&dir, false);
        press(&mut b, KeyCode::Char('r'));
        assert_eq!(
            b.message,
            Some(t!("msg.db_compare_read_only", name = "other").to_string())
        );
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        // Un-check everything: nothing to run.
        b.compare.as_mut().unwrap().checked.fill(false);
        press(&mut b, KeyCode::Char('r'));
        assert_eq!(b.message, Some(t!("msg.db_compare_nothing").to_string()));
        // The run gate holds even if the confirmation were reached anyway.
        b.compare.as_mut().unwrap().screen = Screen::Confirm;
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        assert_eq!(
            b.message,
            Some(t!("msg.db_compare_read_only", name = "other").to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn f8_toggles_write_mode_on_the_target_session() {
        let dir = scratch("f8");
        let mut b = at_review(&dir, false);
        press(&mut b, KeyCode::F(8));
        assert!(b.compare().unwrap().target_write_enabled());
        assert_eq!(b.message, Some(t!("msg.db_write_enabled").to_string()));
        press(&mut b, KeyCode::F(8));
        assert!(!b.compare().unwrap().target_write_enabled());
        assert_eq!(b.message, Some(t!("msg.db_read_only_on").to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn confirming_runs_the_migration_on_the_target_and_rereads_it() {
        let dir = scratch("run");
        let mut b = at_review(&dir, true);
        press(&mut b, KeyCode::Char('r'));
        assert_eq!(b.compare().unwrap().screen, Screen::Confirm);
        // Declining goes back to the review.
        press(&mut b, KeyCode::Char('n'));
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        press(&mut b, KeyCode::Char('r'));
        press(&mut b, KeyCode::Tab);
        press(&mut b, KeyCode::Char('y'));
        assert_eq!(b.compare().unwrap().screen, Screen::Working);
        assert!(b.compare().unwrap().running_migration());
        assert!(b.compare().unwrap().elapsed_secs().is_some());
        press(&mut b, KeyCode::Esc);
        assert_eq!(
            b.compare().unwrap().screen,
            Screen::Working,
            "a running migration cannot be abandoned"
        );
        settle(&mut b);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        assert_eq!(
            b.message,
            Some(t!("msg.db_compare_applied", count = 1, name = "other").to_string())
        );
        // The target now has `a`; only the unchecked drop of `extra` remains.
        let changes = b.compare().unwrap().changes.len();
        assert_eq!(changes, 1);
        let target = &b.connections[1].file;
        let mut s =
            session::Session::connect(&crate::catalog::sqlite_url(target, "ro"), &[]).unwrap();
        let (_, rows) = s
            .run("SELECT name FROM sqlite_master WHERE name = 'a'")
            .unwrap();
        assert_eq!(rows, [["a"]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_migration_rolls_back_and_reports_the_statement() {
        let dir = scratch("fail");
        let mut b = at_review(&dir, true);
        // Choose the drop of `extra`, then take `extra` away behind its back so
        // the migration's DROP fails.
        b.compare.as_mut().unwrap().checked.fill(true);
        let target = b.connections[1].file.clone();
        let mut other =
            session::Session::connect(&crate::catalog::sqlite_url(&target, "rwc"), &[]).unwrap();
        other.run("DROP TABLE extra").unwrap();
        drop(other);
        press(&mut b, KeyCode::Char('r'));
        press(&mut b, KeyCode::Enter);
        settle(&mut b);
        assert_eq!(b.compare().unwrap().screen, Screen::Review);
        let msg = b.message.clone().unwrap();
        assert!(msg.contains("extra"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_are_ignored_without_a_compare_and_polling_is_a_no_op() {
        let mut b = workbench(true, &[]);
        b.poll_compare();
        assert_eq!(
            b.key_compare(crate::test_support::key(KeyCode::Enter), 5),
            Outcome::Consumed
        );
        b.confirm_compare_run();
        b.toggle_target_write();
        b.run_compare();
        b.open_compare_script();
        assert!(b.compare().is_none());
    }
}
