//! Connecting to (and disconnecting from) a saved database connection —
//! the credential waterfall, the SSH tunnel, opening the persistent
//! [`session::Session`], and loading the initial catalog — all off the UI
//! thread (Run H, T531). Extracted from `lib.rs` (T516): the one cohesive
//! slice of [`Browser`]'s ~110 methods concerned with the connection's
//! *lifecycle* rather than anything the workbench does once connected.
//!
//! # Connect flow
//!
//! ```text
//! Enter on a saved connection
//!   -> start_connect -> begin_connect   (spawns a thread running connect_worker)
//!        connect_worker: resolve password (secret) -> open SSH tunnel -> Session::connect
//!   -> poll_connect, once per host tick, turns the ConnectOutcome into:
//!        Connected     -> finish_connected (load catalog, enter the workbench)
//!        NeedsPassword -> View::Password; Enter there calls begin_connect again
//!                         with the typed password
//!        Failed        -> status message, back to the connections list
//! ```
//!
//! While a connect is pending the connections list ignores keys so a second
//! attempt cannot race the first. The same module owns the saved-connection
//! list and form key handling, and [`Browser::disconnect`], which resets every
//! piece of per-connection state. Owning spec: `crates/vix-db/spec/index.md`
//! ("Connection Management") and `spec/session.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{
    Browser, FORM_ENVIRONMENT, FORM_FIELDS, FORM_KIND, FORM_MCP_SHARED, FORM_ROWS, FORM_STORE,
    FORM_WRITABLE, Form, Outcome, Pane, TxState, View, ai_features::AiState, catalog, complete,
    connect, mcp_share, object_triples, secret, session, store, tunnel,
};

/// Outcome of a background connect attempt (Run H, T531): fully connected,
/// no stored credential found (fall back to the interactive password
/// prompt, exactly like the old synchronous credential waterfall did), or
/// failed outright (bad credentials, unreachable host, a failing setup
/// statement, …).
pub(crate) enum ConnectOutcome {
    /// The session is open and ready; `tunnel` is `Some` only when this
    /// connection is configured to go over one.
    Connected {
        /// The newly opened session.
        session: session::Session,
        /// The SSH tunnel the session's URL points through, if configured.
        tunnel: Option<tunnel::Tunnel>,
    },
    /// `conn` needs a password and none was found via `password_command` or
    /// the keyring — the host should show the interactive password prompt.
    NeedsPassword,
    /// The connect failed; a display-ready message.
    Failed(String),
}

/// A connect running on a background thread, awaited by
/// [`Browser::poll_connect`] (Run H, T531). Covers both the initial connect
/// from the connections list and a retry with a manually-typed password.
pub(crate) struct PendingConnect {
    /// The eventual [`ConnectOutcome`].
    rx: std::sync::mpsc::Receiver<ConnectOutcome>,
    /// When the attempt began, for the elapsed-seconds status line that
    /// [`Browser::poll_connect`] refreshes while waiting.
    started: std::time::Instant,
    /// The connection being connected to.
    conn: connect::Connection,
    /// The connections-list index, so a [`ConnectOutcome::NeedsPassword`]
    /// can route to the password prompt exactly like the old synchronous
    /// `start_connect` did. `None` for a retry that already carried an
    /// explicit password (typed on the prompt itself), where
    /// `NeedsPassword` can't recur.
    idx: Option<usize>,
    /// The password this attempt used, kept only long enough to store it in
    /// the keyring on success if the connection opted in (T545); empty (and
    /// irrelevant) for the auto-resolved-credential path, since a resolved
    /// credential is already stored by definition.
    password: String,
}

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

impl Browser {
    /// Keys on the saved-connections list: navigate (arrows or `j`/`k`),
    /// Enter to connect, `a`dd, `e`dit, `d`elete, Esc/`q` to leave.
    pub(super) fn key_connections(&mut self, key: KeyEvent) -> Outcome {
        // A connect is running in the background (T531); ignore other input
        // rather than race a second connect attempt against it.
        if self.connect_running() {
            return Outcome::Consumed;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = sub_or(self.sel, 1, 0),
            KeyCode::Down | KeyCode::Char('j') => {
                self.sel =
                    add_or(self.sel, 1, usize::MAX).min(sub_or(self.connections.len(), 1, 0));
            }
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = sub_or(self.connections.len(), 1, 0),
            KeyCode::Enter => self.start_connect(self.sel),
            KeyCode::Char('a') => {
                self.form = Form::default();
                self.form_environment = connect::Environment::Local;
                self.form_mcp_sharing = mcp_share::Sharing::Private;
                self.form_password = None;
                self.view = View::Form;
            }
            KeyCode::Char('e') => {
                if let Some(conn) = self.connections.get(self.sel) {
                    self.form = Form::from_connection(conn, Some(self.sel));
                    self.form_environment = self.environment_of(&conn.name);
                    self.form_mcp_sharing = if self.is_mcp_shared(&conn.name) {
                        mcp_share::Sharing::Shared
                    } else {
                        mcp_share::Sharing::Private
                    };
                    self.form_password = None;
                    self.view = View::Form;
                }
            }
            KeyCode::Char('d') => {
                if self.sel < self.connections.len() {
                    let removed = self.connections.remove(self.sel);
                    // A name another saved connection still uses keeps its
                    // environment entry.
                    if !self.connections.iter().any(|c| c.name == removed.name) {
                        self.environments.remove(&removed.name);
                        self.mcp_shared.remove(&removed.name);
                        self.held_passwords.remove(&removed.name);
                    }
                    self.sel = self.sel.min(sub_or(self.connections.len(), 1, 0));
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
    ///
    /// Row `FORM_KIND`, the toggle rows (`FORM_WRITABLE`, `FORM_STORE`,
    /// `FORM_MCP_SHARED`) and the environment row (`FORM_ENVIRONMENT`) respond to
    /// Left/Right/Space instead of text entry; the others are plain text
    /// fields. Enter saves (rejecting an empty name) and flags the connection
    /// list and environments dirty for the host to persist.
    pub(super) fn key_form(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Up => self.form.sel = sub_or(self.form.sel, 1, 0),
            KeyCode::Down | KeyCode::Tab => {
                self.form.sel = add_or(self.form.sel, 1, usize::MAX)
                    .checked_rem(FORM_ROWS)
                    .unwrap_or(0);
            }
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
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if self.form.sel == FORM_ENVIRONMENT =>
            {
                self.form_environment = self.form_environment.next();
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if self.form.sel == FORM_MCP_SHARED =>
            {
                self.form_mcp_sharing = self.form_mcp_sharing.toggled();
            }
            KeyCode::Char(c) if self.form.sel < FORM_FIELDS && self.form.sel != FORM_KIND => {
                if let Some(field) = self.form.fields.get_mut(self.form.sel) {
                    field.push(c);
                }
            }
            KeyCode::Backspace if self.form.sel < FORM_FIELDS && self.form.sel != FORM_KIND => {
                if let Some(field) = self.form.fields.get_mut(self.form.sel) {
                    field.pop();
                }
            }
            KeyCode::Enter => {
                // The name is the only required field: it identifies the
                // connection in the list and keys its keyring entry.
                if self.form.fields[0].trim().is_empty() {
                    self.message = Some(t!("msg.db_name_required").to_string());
                } else {
                    self.save_form();
                }
            }
            KeyCode::Esc => {
                self.form_password = None;
                self.view = View::Connections;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Save the form as a connection (replacing the edited one, or appending),
    /// record its environment under its (possibly new) name, and hold a
    /// pasted password for its next connect.
    fn save_form(&mut self) {
        let conn = self.form.to_connection();
        let name = conn.name.clone();
        let old_name = match self.form.editing {
            Some(i) if i < self.connections.len() => self
                .connections
                .get_mut(i)
                .map(|slot| std::mem::replace(slot, conn).name),
            _ => {
                self.connections.push(conn);
                None
            }
        };
        // A rename carries the held password; the environment is rewritten
        // under the new name below.
        if let Some(old) = old_name.filter(|old| *old != name)
            && !self.connections.iter().any(|c| c.name == old)
        {
            self.environments.remove(&old);
            self.mcp_shared.remove(&old);
            if let Some(pw) = self.held_passwords.remove(&old) {
                self.held_passwords.insert(name.clone(), pw);
            }
        }
        if self.form_environment == connect::Environment::Local {
            self.environments.remove(&name);
        } else {
            self.environments
                .insert(name.clone(), self.form_environment);
        }
        if self.form_mcp_sharing.is_shared() {
            self.mcp_shared.insert(name.clone());
        } else {
            self.mcp_shared.remove(&name);
        }
        if let Some(pw) = self.form_password.take() {
            self.held_passwords.insert(name, pw);
        }
        self.dirty.connections = true;
        self.view = View::Connections;
    }

    /// Keys on the password prompt. Typed characters go to
    /// [`Browser::password`] (rendered masked by the host); Esc wipes it.
    pub(super) fn key_password(&mut self, key: KeyEvent) -> Outcome {
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
                    // T531 (Run H): connects in the background now, so the
                    // "did it actually connect" keyring-store check (T545)
                    // moved into `poll_connect`'s `Connected` arm, where the
                    // outcome is actually known.
                    self.begin_connect(conn, Some(password), None);
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

    /// Begin connecting to connection `idx` on a background thread (Run H,
    /// T531): the credential waterfall (`password_command`, then the OS
    /// keyring), any SSH tunnel, and the connection itself all used to run
    /// inline on the UI thread here — an unreachable host alone can hit a
    /// 60-130s OS TCP timeout. [`Self::poll_connect`] finishes it: either
    /// straight into the workbench, or onto the password prompt when no
    /// stored credential was found (mirroring the old synchronous
    /// waterfall's behavior, just no longer blocking while it runs).
    fn start_connect(&mut self, idx: usize) {
        let Some(conn) = self.connections.get(idx).cloned() else {
            return;
        };
        // A password from a pasted connection string is used once, exactly
        // like one typed on the prompt (and saved to the keyring on success
        // only if the connection opts in).
        let held = if conn.needs_password() {
            self.held_passwords.remove(&conn.name)
        } else {
            None
        };
        self.begin_connect(conn, held, Some(idx));
    }

    /// Kick off a background connect to `conn`. `password`: `Some` uses
    /// exactly that password (the password prompt's retry, which already
    /// resolved it interactively); `None` has the background thread resolve
    /// it non-interactively first (`SQLite` needs none; server engines try
    /// `password_command` then the keyring), reporting back
    /// [`ConnectOutcome::NeedsPassword`] when that comes up empty. `idx` is
    /// only meaningful for the `None` case — see [`PendingConnect::idx`].
    fn begin_connect(
        &mut self,
        conn: connect::Connection,
        password: Option<String>,
        idx: Option<usize>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_conn = conn.clone();
        let worker_password = password.clone();
        std::thread::spawn(move || {
            let _ = tx.send(connect_worker(&worker_conn, worker_password.as_deref()));
        });
        self.message = Some(t!("msg.db_connecting", secs = 0).to_string());
        // No password supplied is the empty password.
        let mut held_password = String::new();
        if let Some(given) = password {
            held_password = given;
        }
        self.pending_connect = Some(PendingConnect {
            rx,
            started: std::time::Instant::now(),
            conn,
            idx,
            password: held_password,
        });
    }

    /// Whether a connect (or password-prompted retry) is running in the
    /// background (keeps the host's event loop polling fast).
    #[must_use]
    pub fn connect_running(&self) -> bool {
        self.pending_connect.is_some()
    }

    /// Drain a finished background connect into the workbench. Called by the
    /// host each event-loop tick; cheap when nothing is pending. While still
    /// pending, refreshes the status line with the elapsed time — a connect
    /// can now genuinely take a while (an unreachable host's OS TCP timeout
    /// is commonly 60-130s), so the user needs proof it isn't stuck.
    pub fn poll_connect(&mut self) {
        // `take()` then re-insert on `Empty` keeps the borrow checker happy
        // while still letting the other arms consume `pending` by value.
        let Some(pending) = self.pending_connect.take() else {
            return;
        };
        let outcome = match pending.rx.try_recv() {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.message = Some(
                    t!(
                        "msg.db_connecting",
                        secs = pending.started.elapsed().as_secs()
                    )
                    .to_string(),
                );
                self.pending_connect = Some(pending); // still waiting -- put it back
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                ConnectOutcome::Failed(t!("msg.db_not_connected").to_string())
            }
        };
        match outcome {
            ConnectOutcome::NeedsPassword => {
                self.pending = pending.idx;
                self.password.clear();
                self.view = View::Password;
            }
            ConnectOutcome::Failed(e) => {
                self.message = Some(e);
                self.view = View::Connections;
            }
            ConnectOutcome::Connected { session, tunnel } => {
                self.finish_connected(pending.conn.clone(), session, tunnel);
                // Save the just-entered password for next time, if the
                // connection opted in and we actually connected — checked
                // after, exactly like the old synchronous path, so a
                // keyring-save failure's message can still overwrite the
                // "connected" one below it (T545's existing behavior).
                if pending.conn.store_keyring
                    && !pending.password.is_empty()
                    && self.conn.is_some()
                    && !secret::store(&pending.conn, &pending.password)
                {
                    self.message = Some(t!("msg.db_keyring_save_failed").to_string());
                }
            }
        }
    }

    /// Install a freshly (and, since T531, asynchronously) connected
    /// `session`/`tunnel` for `conn`: load the catalog and enter the
    /// workbench, or roll back to the connections list on a catalog-load
    /// failure. The catalog load stays a synchronous, one-shot `session.run`
    /// call — bounded and fast on an already-open connection, unlike the
    /// network-risky work `connect_worker` now does off the UI thread.
    fn finish_connected(
        &mut self,
        conn: connect::Connection,
        session: session::Session,
        tunnel: Option<tunnel::Tunnel>,
    ) {
        self.session = Some(session);
        self.tunnel = tunnel;
        // A session starts with exactly the access the saved connection
        // grants (read-only unless the user flagged it writable); F8 can
        // change it for this session only, and it is not saved back.
        self.write_enabled = conn.writable;
        self.tx = TxState::None;
        match self.run_catalog(catalog::objects_sql(conn.kind)) {
            Ok((_, rows)) => {
                self.tree = catalog::Tree::from_objects(&object_triples(rows));
                self.load_columns(conn.kind);
                self.message = Some(t!("msg.db_connected", name = conn.name).to_string());
                self.conn = Some(conn);
                self.view = View::Workbench;
                self.focus = Pane::Editor;
            }
            Err(e) => {
                self.session = None;
                self.message = Some(e.to_string());
                self.view = View::Connections;
            }
        }
    }

    /// Fetch every `(table, column)` pair, and every foreign-key edge, for
    /// autocomplete (best effort: a failing query just leaves that part of the
    /// completer empty).
    fn load_columns(&mut self, kind: connect::Kind) {
        let columns = self
            .run_catalog(catalog::columns_sql(kind))
            .map_or_default(|(_, rows)| {
                rows.into_iter()
                    .filter_map(|r| match r.as_slice() {
                        [a, b, ..] => Some((a.clone(), b.clone())),
                        _ => None,
                    })
                    .collect()
            });
        self.completer.set_schema(self.tree.table_names(), columns);
        // Foreign-key edges power `JOIN … ON` autocomplete.
        let rels = self
            .run_catalog(catalog::relationships_sql(kind))
            .map_or_default(|(_, rows)| {
                rows.into_iter()
                    .filter_map(|r| match r.as_slice() {
                        [a, b, c, d, ..] => Some((a.clone(), b.clone(), c.clone(), d.clone())),
                        _ => None,
                    })
                    .collect()
            });
        self.completer.set_relationships(rels);
    }

    /// Drop the active connection — closing the persistent session and its
    /// in-memory password — and return to the connections list.
    ///
    /// Resets all per-connection state (tree, completer, log, transaction
    /// badge, pending work, staged edits) so nothing from one database can
    /// leak into the next. Saved queries and history are deliberately kept.
    pub fn disconnect(&mut self) {
        self.conn = None;
        // Signal the worker before dropping it (T532): if it's stuck mid-
        // statement on a hung network read, this is what actually gets it
        // to notice and exit, rather than leaving the connection open (and
        // counting against the database's own connection limit) until a
        // network condition that may never resolve on its own does.
        if let Some(session) = self.session.take() {
            session.cancel();
        }
        self.tunnel = None; // drop → kills the ssh forward
        self.compare = None; // and the schema compare's target session
        self.write_enabled = false;
        self.log = store::Log::default();
        self.ai = AiState::Idle;
        self.last_error = None;
        self.params = None;
        self.import_path.clear();
        self.import_target = super::panels::PathTarget::Import;
        // Dropping a running table export's receiver stops its worker at
        // the next progress report (it rolls back and removes its `.part`).
        self.pending_export = None;
        self.table_export = None;
        self.export_marks.clear();
        self.tx = TxState::None;
        self.pending_query = None;
        self.pending_connect = None;
        self.pending_reconnect = None;
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

    /// Re-run the catalog queries on the live session, rebuilding the tree
    /// (and thereby resetting its expansion and selection state). On failure
    /// the old tree is kept and the error is shown. A no-op when disconnected.
    pub fn refresh_catalog(&mut self) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return;
        };
        match self.run_catalog(catalog::objects_sql(kind)) {
            Ok((_, rows)) => {
                self.tree = catalog::Tree::from_objects(&object_triples(rows));
                self.load_columns(kind);
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }
}

/// The actual (blocking) connect work, run on a background thread by
/// [`Browser::begin_connect`] (Run H, T531): resolve a password
/// non-interactively when `password` is `None` (`SQLite` needs none; server
/// engines try `password_command` then the keyring), open an SSH tunnel if
/// configured, then open the session. Every step here can block for a real
/// amount of time — a slow/hanging `password_command`, the tunnel's
/// `wait_ready` (up to 10s), or an unreachable host's OS TCP timeout
/// (commonly 60-130s) — which is exactly why it no longer runs on the UI
/// thread.
pub(crate) fn connect_worker(conn: &connect::Connection, password: Option<&str>) -> ConnectOutcome {
    let password = match password {
        Some(p) => p.to_string(),
        None if !conn.needs_password() => String::new(),
        None => match secret::resolve(conn) {
            Some(p) => p,
            None => return ConnectOutcome::NeedsPassword,
        },
    };
    // Bring up an SSH tunnel first, if configured, and point the URL at its
    // local end. The tunnel is held for the connection's lifetime.
    let tunnel = match tunnel::open(conn) {
        Ok(tunnel) => tunnel,
        Err(e) => return ConnectOutcome::Failed(e.to_string()),
    };
    // With a tunnel the driver must dial the local end, not the real host.
    let url = match &tunnel {
        Some(t) => connect::url_via_local(conn, &password, t.local_port),
        None => connect::url(conn, &password),
    };
    // A read-only connection asks the server to reject writes too, where the
    // engine supports it; the client guard covers the rest.
    let setup: Vec<String> = if conn.writable {
        Vec::new()
    } else {
        connect::read_only_sql(conn.kind, true)
            .into_iter()
            .collect()
    };
    match session::Session::connect(&url, &setup) {
        Ok(session) => ConnectOutcome::Connected { session, tunnel },
        Err(e) => ConnectOutcome::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{family, file_workbench, press, typed};
    use crate::{FORM_KIND, FORM_STORE};

    fn conn(name: &str) -> connect::Connection {
        connect::Connection {
            name: name.into(),
            kind: connect::Kind::Sqlite,
            file: format!("/tmp/{name}.db"),
            ..connect::Connection::default()
        }
    }

    fn list(names: &[&str]) -> Browser {
        Browser::new(names.iter().map(|n| conn(n)).collect())
    }

    #[test]
    fn the_connection_list_moves_clamps_and_leaves() {
        let mut b = list(&["a", "b", "c"]);
        press(&mut b, KeyCode::Up);
        assert_eq!(b.sel, 0);
        press(&mut b, KeyCode::Down);
        press(&mut b, KeyCode::Char('j'));
        press(&mut b, KeyCode::Down);
        assert_eq!(b.sel, 2);
        press(&mut b, KeyCode::Char('k'));
        assert_eq!(b.sel, 1);
        press(&mut b, KeyCode::End);
        assert_eq!(b.sel, 2);
        press(&mut b, KeyCode::Home);
        assert_eq!(b.sel, 0);
        press(&mut b, KeyCode::Char('?'));
        assert_eq!(press_outcome(&mut b, KeyCode::Char('q')), Outcome::Close);
        assert_eq!(press_outcome(&mut b, KeyCode::Esc), Outcome::Close);
        // With a live connection Esc returns to the workbench instead.
        let mut live = family(false);
        live.connections = vec![conn("a")];
        live.view = View::Connections;
        assert_eq!(press_outcome(&mut live, KeyCode::Esc), Outcome::Consumed);
        assert_eq!(live.view, View::Workbench);
    }

    fn press_outcome(b: &mut Browser, code: KeyCode) -> Outcome {
        b.handle_key(crate::test_support::key(code), crate::Pages::default())
    }

    #[test]
    fn editing_a_missing_row_and_deleting_the_last_row_are_safe() {
        let mut b = list(&[]);
        press(&mut b, KeyCode::Char('e'));
        assert_eq!(b.view, View::Connections);
        press(&mut b, KeyCode::Char('d'));
        press(&mut b, KeyCode::Enter);
        assert!(!b.connect_running());
        let mut b = list(&["only"]);
        press(&mut b, KeyCode::Char('d'));
        assert_eq!(b.sel, 0);
        assert_eq!(b.connections.len(), 0);
    }

    #[test]
    fn deleting_keeps_the_environment_of_a_name_still_in_use() {
        let mut b = list(&["x", "x"]);
        b.environments
            .insert("x".into(), connect::Environment::Production);
        b.mcp_shared.insert("x".into());
        press(&mut b, KeyCode::Char('d'));
        assert_eq!(b.environment_of("x"), connect::Environment::Production);
        press(&mut b, KeyCode::Char('d'));
        assert_eq!(b.environment_of("x"), connect::Environment::Local);
        assert!(!b.is_mcp_shared("x"));
    }

    #[test]
    fn the_form_cycles_toggles_and_edits_its_rows() {
        let mut b = list(&["a"]);
        press(&mut b, KeyCode::Char('e'));
        assert_eq!(b.view, View::Form);
        press(&mut b, KeyCode::Up);
        assert_eq!(b.form.sel, 0);
        typed(&mut b, "z");
        press(&mut b, KeyCode::Backspace);
        press(&mut b, KeyCode::Backspace);
        assert_eq!(b.form.fields[0], "");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.message, Some(t!("msg.db_name_required").to_string()));
        assert_eq!(b.view, View::Form);
        typed(&mut b, "n");
        // Kind row cycles the engine; typing there is ignored.
        b.form.sel = FORM_KIND;
        let before = b.form.kind;
        press(&mut b, KeyCode::Right);
        assert_ne!(b.form.kind, before);
        press(&mut b, KeyCode::Char('x'));
        press(&mut b, KeyCode::Backspace);
        press(&mut b, KeyCode::Left);
        press(&mut b, KeyCode::Char(' '));
        // The toggle rows.
        b.form.sel = FORM_WRITABLE;
        press(&mut b, KeyCode::Char(' '));
        assert!(b.form.writable);
        b.form.sel = FORM_STORE;
        press(&mut b, KeyCode::Right);
        assert!(b.form.store_keyring);
        b.form.sel = FORM_ENVIRONMENT;
        press(&mut b, KeyCode::Right);
        assert_ne!(b.form_environment, connect::Environment::Local);
        b.form.sel = FORM_MCP_SHARED;
        press(&mut b, KeyCode::Left);
        assert!(b.form_mcp_sharing.is_shared());
        // Down and Tab walk the rows and wrap.
        b.form.sel = FORM_ROWS - 1;
        press(&mut b, KeyCode::Down);
        assert_eq!(b.form.sel, 0);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.form.sel, 1);
        press(&mut b, KeyCode::F(3));
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.view, View::Connections);
        assert!(b.dirty.connections);
        assert!(b.connections[0].writable && b.connections[0].store_keyring);
        assert_ne!(b.environment_of("n"), connect::Environment::Local);
        assert!(b.is_mcp_shared("n"));
    }

    #[test]
    fn renaming_a_connection_moves_its_environment_sharing_and_held_password() {
        let mut b = list(&["old"]);
        b.environments
            .insert("old".into(), connect::Environment::Production);
        b.mcp_shared.insert("old".into());
        b.held_passwords.insert("old".into(), "pw".into());
        press(&mut b, KeyCode::Char('e'));
        assert_eq!(b.form_environment, connect::Environment::Production);
        assert!(b.form_mcp_sharing.is_shared());
        for _ in 0..3 {
            press(&mut b, KeyCode::Backspace);
        }
        typed(&mut b, "new");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.connections[0].name, "new");
        assert_eq!(b.environment_of("new"), connect::Environment::Production);
        assert_eq!(b.environment_of("old"), connect::Environment::Local);
        assert!(b.is_mcp_shared("new") && !b.is_mcp_shared("old"));
        assert_eq!(b.held_passwords.get("new").map(String::as_str), Some("pw"));
        assert!(!b.held_passwords.contains_key("old"));
        // Saving back to Local drops the environment entry; editing a stale
        // index appends instead.
        press(&mut b, KeyCode::Char('e'));
        b.form_environment = connect::Environment::Local;
        b.form_mcp_sharing = mcp_share::Sharing::Private;
        b.form.editing = Some(9);
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.connections.len(), 2);
        assert_eq!(b.environment_of("new"), connect::Environment::Local);
        assert!(!b.is_mcp_shared("new"));
    }

    #[test]
    fn a_missing_database_file_directory_fails_the_background_connect() {
        let mut b = Browser::new(vec![connect::Connection {
            name: "bad".into(),
            kind: connect::Kind::Sqlite,
            file: "/no/such/dir/x.db".into(),
            ..connect::Connection::default()
        }]);
        press(&mut b, KeyCode::Enter);
        assert!(b.connect_running());
        assert_eq!(
            b.message,
            Some(t!("msg.db_connecting", secs = 0).to_string())
        );
        // While connecting, the list ignores keys.
        press(&mut b, KeyCode::Char('d'));
        assert_eq!(b.connections.len(), 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while b.connect_running() {
            b.poll_connect();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(b.view, View::Connections);
        assert!(b.conn.is_none());
        assert!(b.message.is_some());
        b.poll_connect();
    }

    #[test]
    fn a_dropped_connect_worker_reports_not_connected() {
        let mut b = list(&["a"]);
        let (tx, rx) = std::sync::mpsc::channel::<ConnectOutcome>();
        drop(tx);
        b.pending_connect = Some(PendingConnect {
            rx,
            started: std::time::Instant::now(),
            conn: conn("a"),
            idx: Some(0),
            password: String::new(),
        });
        b.poll_connect();
        assert_eq!(b.message, Some(t!("msg.db_not_connected").to_string()));
        assert!(!b.connect_running());
    }

    #[test]
    fn the_password_prompt_retries_the_connection_with_what_was_typed() {
        let (mut b, dir) = file_workbench("lc-pw", true, &["CREATE TABLE t (a)"]);
        let saved = b.connections[0].clone();
        b.disconnect();
        assert_eq!(b.view, View::Connections);
        // Put the browser on the password prompt for that connection.
        b.pending = Some(0);
        b.view = View::Password;
        typed(&mut b, "secret");
        press(&mut b, KeyCode::Backspace);
        press(&mut b, KeyCode::Tab);
        assert_eq!(b.password, "secre");
        press(&mut b, KeyCode::Enter);
        assert!(b.password.is_empty(), "the typed password is taken");
        assert!(b.connect_running());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while b.connect_running() {
            b.poll_connect();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(b.view, View::Workbench);
        assert_eq!(
            b.conn.as_ref().map(|c| c.name.as_str()),
            Some(saved.name.as_str())
        );
        assert!(b.write_enabled());
        assert!(b.tree.table_names().contains(&"t".to_string()));
        // Enter with nothing pending does nothing.
        b.view = View::Password;
        b.pending = None;
        press(&mut b, KeyCode::Enter);
        assert!(!b.connect_running());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_only_connection_starts_read_only_and_a_writable_one_does_not() {
        let (b, dir) = file_workbench("lc-ro", false, &["CREATE TABLE t (a)"]);
        assert!(!b.write_enabled());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disconnecting_resets_every_piece_of_per_connection_state() {
        let mut b = family(true);
        b.begin_tx();
        b.ask_input = "x".into();
        b.last_error = Some(("a".into(), "b".into()));
        b.import_path = "p".into();
        b.password = "pw".into();
        b.disconnect();
        assert!(b.conn.is_none() && b.session.is_none());
        assert!(!b.write_enabled());
        assert_eq!(b.tx_state(), TxState::None);
        assert!(b.log.entries.is_empty());
        assert!(b.tree.schemas.is_empty());
        assert!(b.ask_input.is_empty() && b.import_path.is_empty() && b.password.is_empty());
        assert!(b.last_error.is_none());
        assert_eq!(b.view, View::Connections);
        assert_eq!(b.message, Some(t!("msg.db_disconnected").to_string()));
        assert!(!b.editable());
    }

    #[test]
    fn refreshing_rebuilds_the_tree_and_reports_a_failed_catalog() {
        let mut b = family(true);
        b.session
            .as_mut()
            .unwrap()
            .run("CREATE TABLE extra (x)")
            .unwrap();
        b.refresh_catalog();
        assert!(b.tree.table_names().contains(&"extra".to_string()));
        assert!(
            b.completer
                .suggest_in("select * from ext", 17, "")
                .items
                .iter()
                .any(|i| i == "extra")
        );
        let before = b.tree.table_names();
        b.session = None;
        b.refresh_catalog();
        assert!(b.message.is_some());
        assert_eq!(b.tree.table_names(), before, "the old tree is kept");
        b.conn = None;
        b.message = None;
        b.refresh_catalog();
        assert_eq!(b.message, None, "a no-op when disconnected");
    }

    #[test]
    fn an_unreadable_catalog_sends_the_connect_back_to_the_list() {
        let mut b = list(&["a"]);
        let mut s = session::Session::connect("sqlite::memory:", &[]).unwrap();
        s.run("SELECT 1").unwrap();
        // A catalog query cannot fail on SQLite, so use a session whose
        // worker has been told to stop.
        s.cancel();
        b.finish_connected(conn("a"), s, None);
        assert_eq!(b.view, View::Connections);
        assert!(b.session.is_none());
        assert!(b.message.is_some());
    }
}
