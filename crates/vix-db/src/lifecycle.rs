//! Connecting to (and disconnecting from) a saved database connection —
//! the credential waterfall, the SSH tunnel, opening the persistent
//! [`session::Session`], and loading the initial catalog — all off the UI
//! thread (Run H, T531). Extracted from `lib.rs` (T516): the one cohesive
//! slice of [`Browser`]'s ~110 methods concerned with the connection's
//! *lifecycle* rather than anything the workbench does once connected.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    AiState, Browser, FORM_FIELDS, FORM_KIND, FORM_ROWS, FORM_STORE, FORM_WRITABLE, Form, Outcome,
    Pane, TxState, View, catalog, complete, connect, object_triples, secret, session, store,
    tunnel,
};

/// Outcome of a background connect attempt (Run H, T531): fully connected,
/// no stored credential found (fall back to the interactive password
/// prompt, exactly like the old synchronous credential waterfall did), or
/// failed outright (bad credentials, unreachable host, a failing setup
/// statement, …).
enum ConnectOutcome {
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
    /// When the attempt began (for a future "still connecting…" indicator).
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

impl Browser {
    /// Keys on the saved-connections list.
    pub(super) fn key_connections(&mut self, key: KeyEvent) -> Outcome {
        // A connect is running in the background (T531); ignore other input
        // rather than race a second connect attempt against it.
        if self.connect_running() {
            return Outcome::Consumed;
        }
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
    pub(super) fn key_form(&mut self, key: KeyEvent) -> Outcome {
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
        self.begin_connect(conn, None, Some(idx));
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
        self.pending_connect = Some(PendingConnect {
            rx,
            started: std::time::Instant::now(),
            conn,
            idx,
            password: password.unwrap_or_default(),
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
        // Signal the worker before dropping it (T532): if it's stuck mid-
        // statement on a hung network read, this is what actually gets it
        // to notice and exit, rather than leaving the connection open (and
        // counting against the database's own connection limit) until a
        // network condition that may never resolve on its own does.
        if let Some(session) = self.session.take() {
            session.cancel();
        }
        self.tunnel = None; // drop → kills the ssh forward
        self.write_enabled = false;
        self.log = store::Log::default();
        self.ai = AiState::Idle;
        self.last_error = None;
        self.params = None;
        self.import_path.clear();
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
fn connect_worker(conn: &connect::Connection, password: Option<&str>) -> ConnectOutcome {
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
        Err(e) => return ConnectOutcome::Failed(e),
    };
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
        Err(e) => ConnectOutcome::Failed(e),
    }
}
