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
        self.target
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }

    /// The target's environment for the run confirmation, when connections
    /// carry one.
    #[must_use]
    pub fn target_environment(&self) -> Option<String> {
        // T781 hook: return the target connection's `environment` (local,
        // development, testing, staging, production) once it exists.
        None
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
            return Err(t!("msg.db_compare_failed", n = i + 1, error = e.to_string()).to_string());
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
                            c.script_scroll = c.script_scroll.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => c.script_scroll += 1,
                        KeyCode::PageUp => c.script_scroll = c.script_scroll.saturating_sub(step),
                        KeyCode::PageDown => c.script_scroll += step,
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
            KeyCode::Up => c.pick = c.pick.saturating_sub(1),
            KeyCode::Down => c.pick = (c.pick + 1).min(c.candidates.len().saturating_sub(1)),
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
        let source_name = self
            .conn
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default();
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
        if let Some(c) = self.compare.as_mut() {
            c.source = source;
            c.schema = schema;
            c.target_write = target.writable;
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
        let last = c.changes.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => c.cursor = c.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => c.cursor = (c.cursor + 1).min(last),
            KeyCode::PageUp => c.cursor = c.cursor.saturating_sub(page.max(1)),
            KeyCode::PageDown => c.cursor = (c.cursor + page.max(1)).min(last),
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
}
