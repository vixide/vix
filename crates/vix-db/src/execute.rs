//! Running SQL: the write/DDL confirmation and bind-parameter prompts,
//! executing statements (synchronously via [`Browser::run_sql`]/
//! [`Browser::run_catalog`] for internal work, or streamed asynchronously
//! for user-initiated ones), client-side transaction tracking, and
//! cancelling a hung query. Extracted from `lib.rs` (T516): the one
//! cohesive slice of [`Browser`]'s methods concerned with *running a
//! statement*, as opposed to connecting or editing results.
//!
//! # Two execution paths
//!
//! - **Synchronous, internal** -- [`Browser::run_catalog`] (and
//!   [`Browser::run_sql`]) call [`session::Session::run`] and block. They are
//!   for fast background work (catalog, preview, ERD, commit/begin/rollback)
//!   and are logged as [`store::Origin::App`] (or `User` for `run_sql`).
//! - **Asynchronous, user** -- `start_query` sends the statement to the worker
//!   and records a `Pending`; the host calls [`Browser::poll_query`] every tick
//!   to drain streamed [`session::Chunk`]s into the grid. While one is in
//!   flight the workbench is busy and only Ctrl+C responds.
//!
//! # Safety gates, in order
//!
//! 1. `:name` parameters are prompted for (`View::Params`) and substituted.
//! 2. A write/DDL statement (see [`editor::is_write_statement`]) is refused
//!    outright on a read-only session.
//! 3. Otherwise it needs a `View::Confirm` approval -- except inside an
//!    explicit transaction, where `ROLLBACK` can undo it. Two exceptions to
//!    the exception (T781, [`editor::needs_confirmation`]): `DROP` and
//!    `TRUNCATE` always ask, and on a `production` connection every write
//!    asks, staged cell edits and CSV imports included.
//!
//! Owning specs: `crates/vix-db/spec/index.md` and `spec/session.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{
    Browser, Outcome, Pane, ParamPrompt, TxState, View, catalog, connect, editor, params, session,
    store,
};

/// An execution awaiting write/DDL confirmation. Stored in
/// `Browser::pending_run` while the confirm view is up.
#[derive(Debug, Clone)]
pub(crate) enum PendingRun {
    /// One statement (possibly EXPLAIN-wrapped).
    One(String),
    /// Every statement in the buffer, in order.
    All(Vec<String>),
    /// An `EXPLAIN ANALYZE` of a write, already wrapped for `kind`; once
    /// confirmed it runs as an explain (plan tree, plan-doctor insight).
    Explain(String, connect::Kind),
    /// Staged row changes on a production connection (T781, T777): the
    /// statements `W` would run, and the previewed table to refresh afterwards.
    Commit {
        /// The `DELETE`/`UPDATE`/`INSERT` statements, in run order.
        statements: Vec<crate::staging::Statement>,
        /// Schema of the edited table.
        schema: String,
        /// The edited table.
        table: String,
        /// The connection's engine.
        kind: connect::Kind,
    },
    /// A CSV/TSV import on a production connection (T781).
    Import {
        /// The `CREATE TABLE` and `INSERT` statements.
        statements: Vec<String>,
        /// The new table's name.
        table: String,
        /// Data rows imported (for the status line).
        rows: usize,
    },
}

/// What to do with the reply from an in-flight asynchronous query. Chosen at
/// send time so `finish_stream` knows whether to record history or run the
/// plan-doctor scan check.
#[derive(Debug, Clone, Copy)]
enum QueryKind {
    /// A user statement — fill the grid and record history.
    Run,
    /// An `EXPLAIN` — fill the grid and flag full scans for the engine.
    Explain(connect::Kind),
}

/// A user query running on the worker thread, awaited by [`Browser::poll_query`].
#[derive(Debug)]
pub(crate) struct Pending {
    /// When the query was sent (for the elapsed indicator and the log's
    /// duration).
    pub(super) started: std::time::Instant,
    /// The statement (for logging / transaction tracking) — the exact SQL sent.
    sql: String,
    /// The text to persist in history — the bind-parameter template when the
    /// statement came from substitution, otherwise identical to `sql`. Keeping
    /// this separate stops prompted secret values from being written to disk.
    history_sql: String,
    /// How to apply the reply.
    kind: QueryKind,
    /// Where the statement sits in the editor buffer, so a server error
    /// position can move the cursor there (T775); `None` when the sent SQL
    /// does not contain the editor's text verbatim (bind parameters were
    /// substituted, or it did not come from the cursor statement).
    anchor: Option<Anchor>,
    /// A server-side cancel sent for this statement (T774), if any.
    cancel: Option<ServerCancel>,
}

/// How a statement's text maps back onto the editor buffer (T775).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Anchor {
    /// Char offset in [`editor::Query::text`] where the statement starts.
    start: usize,
    /// Chars of the sent SQL before the statement (an `EXPLAIN` prefix).
    prefix: usize,
    /// The statement's length in chars.
    len: usize,
}

/// The editor offset a server error position points at (T775): `position`
/// is 1-based in chars of the sent SQL; a position inside a wrapping prefix
/// (an `EXPLAIN …` the user did not type) maps nowhere, and one past the
/// statement's end is clamped to it.
fn error_offset(anchor: Anchor, position: usize) -> Option<usize> {
    let rel = position.checked_sub(1)?.checked_sub(anchor.prefix)?;
    Some(anchor.start + rel.min(anchor.len))
}

/// A server-side cancel (T774) in flight or done, awaited by
/// [`Browser::poll_query`].
#[derive(Debug)]
struct ServerCancel {
    /// The cancel's outcome from its background thread (see
    /// [`session::CancelHandle::send`]); `None` once received.
    rx: Option<std::sync::mpsc::Receiver<Result<bool, session::SessionError>>>,
}

/// A cancelled query's reconnect running on a background thread, awaited by
/// [`Browser::poll_reconnect`] (Run H, T531/T532).
pub(crate) struct PendingReconnect {
    /// The eventual reconnect result: the fresh session, or the reconnect's
    /// own error (in which case the workbench gives up and disconnects,
    /// exactly like the old synchronous path did).
    rx: std::sync::mpsc::Receiver<Result<session::Session, session::SessionError>>,
    /// Whether a transaction was open (or aborted) when the reconnect began,
    /// so the status line can say it was lost (T774).
    lost_tx: bool,
}

impl Browser {
    /// Run a user-initiated `sql` on the live session, timing and logging it.
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is
    /// open.
    pub(super) fn run_sql(&mut self, sql: &str) -> Result<session::Table, session::SessionError> {
        self.run_traced(sql, store::Origin::User)
    }

    /// Run a background workbench `sql` (catalog, preview, ERD), logged as
    /// [`store::Origin::App`].
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is open.
    pub(super) fn run_catalog(
        &mut self,
        sql: &str,
    ) -> Result<session::Table, session::SessionError> {
        self.run_traced(sql, store::Origin::App)
    }

    /// Run a user-initiated `sql` for its effect and return how many rows it
    /// affected (the staged-change commit, T777), timing and logging it.
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is
    /// open.
    pub(super) fn exec_sql(&mut self, sql: &str) -> Result<u64, session::SessionError> {
        let start = std::time::Instant::now();
        let result = match self.session.as_mut() {
            Some(session) => session.execute(sql),
            None => Err(session::SessionError::NotConnected),
        };
        self.log.push(store::LogEntry {
            sql: sql.trim().to_string(),
            ms: start.elapsed().as_millis(),
            rows: result
                .as_ref()
                .map_or(0, |n| usize::try_from(*n).unwrap_or(usize::MAX)),
            ok: result.is_ok(),
            origin: store::Origin::User,
        });
        result
    }

    /// Run `sql`, record its duration and outcome in the query log, and return
    /// the result unchanged. Blocks until the statement finishes.
    fn run_traced(
        &mut self,
        sql: &str,
        origin: store::Origin,
    ) -> Result<session::Table, session::SessionError> {
        let start = std::time::Instant::now();
        let result = match self.session.as_mut() {
            Some(session) => session.run(sql),
            None => Err(session::SessionError::NotConnected),
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

    /// Keys on the write/DDL confirmation: Enter or `y` runs the pending
    /// execution; Esc, `n` or `q` discards it.
    pub(super) fn key_confirm(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.view = View::Workbench;
                match self.pending_run.take() {
                    Some(PendingRun::One(sql)) => self.run_statement(&sql),
                    Some(PendingRun::All(stmts)) => self.run_all(&stmts),
                    Some(PendingRun::Explain(sql, kind)) => {
                        self.start_query(sql, QueryKind::Explain(kind));
                    }
                    Some(PendingRun::Commit {
                        statements,
                        schema,
                        table,
                        kind,
                    }) => self.finish_commit(&statements, &schema, &table, kind),
                    Some(PendingRun::Import {
                        statements,
                        table,
                        rows,
                    }) => self.run_import(&statements, &table, rows),
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

    /// Keys on the bind-parameter prompt: each `Enter` records the value for the
    /// current placeholder; once all are filled the substituted statement runs.
    #[expect(
        clippy::expect_used,
        reason = "invariant established earlier in this function"
    )]
    pub(super) fn key_params(&mut self, key: KeyEvent) -> Outcome {
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
                    self.history_override = Some(prompt.sql);
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

    /// Whether the workbench is currently "busy" (a query running, or a
    /// cancelled query's background reconnect in flight, T531/T532) and, if
    /// so, handles the one key that's still live while busy (`Ctrl+C`
    /// cancels a running query). Callers treat a `true` result as
    /// `Outcome::Consumed`.
    pub(super) fn workbench_busy(&mut self, key: KeyEvent, ctrl: bool) -> bool {
        if self.query_running() {
            if ctrl && matches!(key.code, KeyCode::Char('c' | 'C')) {
                self.cancel_query();
            }
            return true;
        }
        // Nothing to do but wait for the reconnect -- there is no session to
        // cancel-and-restart again.
        self.pending_reconnect.is_some()
    }

    /// Execute the statement at the cursor, showing its rows in the grid
    /// (F5 / Ctrl+Enter).
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
    /// provisional and `ROLLBACK` can undo it — unless the statement drops or
    /// truncates, or the connection is `production` (T781).
    fn execute_sql(&mut self, stmt: String) {
        if editor::is_write_statement(&stmt) {
            if !self.write_enabled {
                self.message = Some(t!("msg.db_read_only").to_string());
                // This attempt never reaches `start_query`, so drop any staged
                // history template rather than let it attach to a later query.
                self.history_override = None;
                return;
            }
            if editor::needs_confirmation(
                &stmt,
                self.active_environment(),
                self.tx != TxState::None,
            ) {
                self.pending_run = Some(PendingRun::One(stmt));
                self.view = View::Confirm;
                return;
            }
        }
        self.run_statement(&stmt);
    }

    /// Execute every statement in the buffer, in order (confirmed once when
    /// any of them writes). The grid shows the last statement's rows.
    ///
    /// Unlike [`Self::execute`], this path runs synchronously (it is not
    /// streamed or cancellable), does not prompt for `:name` parameters, and
    /// asks for confirmation even inside an explicit transaction. A buffer
    /// holding psql meta-commands (see [`editor::meta_commands`]) is refused
    /// before anything runs.
    pub fn execute_all(&mut self) {
        let text = self.query.text();
        // psql meta-commands (`\copy`, `\i`, …) are not SQL: refuse the whole
        // run before any statement executes, naming their lines (T786).
        let meta = editor::meta_commands(&text);
        if !meta.is_empty() {
            let lines: Vec<String> = meta.iter().map(|(line, _)| line.to_string()).collect();
            self.message = Some(t!("msg.db_meta_commands", lines = lines.join(", ")).to_string());
            return;
        }
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
    /// with the plan-doctor full-scan insight; on `PostgreSQL` the plan comes
    /// back structured and is shown as a tree (T776, see [`crate::plan`]).
    /// `EXPLAIN ANALYZE` really executes the statement, so a write inside
    /// still needs write mode and confirmation.
    pub fn explain(&mut self, analyze: bool) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let wrapped = catalog::explain_tree_sql(kind, &stmt, analyze);
        if analyze && editor::is_write_statement(&stmt) {
            if !self.write_enabled {
                self.message = Some(t!("msg.db_read_only").to_string());
                return;
            }
            self.pending_run = Some(PendingRun::Explain(wrapped, kind));
            self.view = View::Confirm;
            return;
        }
        self.start_query(wrapped, QueryKind::Explain(kind));
    }

    /// Run one statement now (no confirmation), recording it in the history.
    /// Trailing `;` characters are stripped before sending.
    fn run_statement(&mut self, stmt: &str) {
        let sql = stmt.trim_end_matches(';').to_string();
        self.start_query(sql, QueryKind::Run);
    }

    /// Send `sql` to the worker without blocking and remember how to apply its
    /// reply; [`Self::poll_query`] finishes it. The workbench is "busy" until
    /// then (only `Ctrl+C` responds).
    fn start_query(&mut self, sql: String, kind: QueryKind) {
        // Consume any staged history template up front so it applies to exactly
        // this query and can never leak onto a later one, even if we bail below.
        let history_sql = self.history_override.take().unwrap_or_else(|| sql.clone());
        let Some(session) = self.session.as_ref() else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        if let Err(e) = session.send(&sql) {
            self.message = Some(e.to_string());
            return;
        }
        let anchor = self.anchor_for(&sql);
        self.message = Some(t!("msg.db_running").to_string());
        self.pending_query = Some(Pending {
            started: std::time::Instant::now(),
            sql,
            history_sql,
            kind,
            anchor,
            cancel: None,
        });
    }

    /// Where `sql` (about to be sent) sits in the editor: found when the
    /// statement under the cursor appears in it verbatim, as it does for a
    /// plain run (trailing `;` stripped) and an `EXPLAIN` wrap (T775).
    fn anchor_for(&self, sql: &str) -> Option<Anchor> {
        let start = self.query.statement_offset_at_cursor()?;
        let stmt = self.query.statement_at_cursor()?;
        let core = stmt.trim_end_matches(';').trim_end();
        if core.is_empty() {
            return None;
        }
        let at = sql.find(core)?;
        Some(Anchor {
            start,
            prefix: sql[..at].chars().count(),
            len: core.chars().count(),
        })
    }

    /// Drain streamed result chunks into the grid, applying batches as they
    /// arrive. Called by the host each event-loop tick; cheap when idle.
    pub fn poll_query(&mut self) {
        if self.pending_query.is_none() {
            return;
        }
        if self.poll_server_cancel() {
            return;
        }
        loop {
            let Some(chunk) = self.session.as_ref().and_then(session::Session::poll) else {
                return; // nothing new this tick — keep the query pending
            };
            match chunk {
                session::Chunk::Head(headers) => {
                    self.grid.set(headers, Vec::new());
                    if let Some(p) = self.pending_query.as_ref() {
                        self.grid_source = Some((p.sql.clone(), None));
                    }
                    self.focus = Pane::Results;
                }
                session::Chunk::Rows(batch) => self.grid.append_rows(batch),
                session::Chunk::Done(truncated) => {
                    self.finish_stream(truncated);
                    return;
                }
                session::Chunk::Err(e) => {
                    let position = self
                        .session
                        .as_ref()
                        .and_then(session::Session::error_position);
                    self.finish_stream_err(&e, position);
                    return;
                }
            }
        }
    }

    /// Cancel the in-flight query (`Ctrl+C`).
    ///
    /// First choice (T774): ask the server to stop the statement over a
    /// short-lived second connection ([`session::CancelHandle`]), sent from
    /// a background thread. The session stays: the statement comes back as
    /// an error through [`Self::poll_query`], temporary tables and session
    /// settings survive, and an open transaction is left aborted (the `TX!`
    /// badge) for the user to roll back.
    ///
    /// Fallback: when there is no server cancel (`SQLite`, or the backend id
    /// was not read at connect), when the server cancel fails, or when
    /// `Ctrl+C` is pressed again while one is already under way (a hung
    /// network read that the server cancel cannot reach), abandon the
    /// worker and reconnect in the background (Run H, T531) -- the
    /// transaction and session state are lost, and the status line says so
    /// when a transaction was open. [`Self::poll_reconnect`] finishes it.
    pub fn cancel_query(&mut self) {
        let Some(pending) = self.pending_query.as_mut() else {
            return;
        };
        if pending.cancel.is_none()
            && let Some(handle) = self
                .session
                .as_ref()
                .and_then(session::Session::cancel_handle)
        {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(handle.send());
            });
            pending.cancel = Some(ServerCancel { rx: Some(rx) });
            self.message = Some(t!("msg.db_cancelling_server").to_string());
            return;
        }
        self.cancel_by_reconnect();
    }

    /// Receive a finished server-side cancel's outcome (T774). A refused or
    /// failed one falls back to [`Self::cancel_by_reconnect`]; returns `true`
    /// when it did, so the caller stops draining the abandoned session.
    fn poll_server_cancel(&mut self) -> bool {
        let Some(cancel) = self.pending_query.as_mut().and_then(|p| p.cancel.as_mut()) else {
            return false;
        };
        let Some(rx) = cancel.rx.as_ref() else {
            return false;
        };
        let accepted = match rx.try_recv() {
            Ok(result) => matches!(result, Ok(true)),
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        };
        cancel.rx = None;
        if accepted {
            // The statement will now fail with "cancelled"; keep draining.
            return false;
        }
        self.cancel_by_reconnect();
        true
    }

    /// Cancel by abandoning the worker and reconnecting, in the background
    /// (Run H, T531): the pre-T774 path, and still the fallback. The old
    /// synchronous `Session::restart` here could itself hang on the network
    /// condition that caused the hang; backgrounding it fixes that. The old
    /// worker's stuck read is abandoned by [`session::Session::restart`],
    /// which signals it to stop (T532) before connecting afresh.
    fn cancel_by_reconnect(&mut self) {
        if self.pending_query.take().is_none() {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        // `session` moves onto the thread: `restart` may block on the network,
        // and the result (the same, now-fresh `Session`) comes back by channel.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = session.restart().map(|()| session);
            let _ = tx.send(result);
        });
        let lost_tx = self.tx != TxState::None;
        self.tx = TxState::None;
        self.message = Some(t!("msg.db_cancelling").to_string());
        self.pending_reconnect = Some(PendingReconnect { rx, lost_tx });
    }

    /// Whether a cancelled query's reconnect is running in the background
    /// (keeps the host's event loop polling fast).
    #[must_use]
    pub fn reconnect_running(&self) -> bool {
        self.pending_reconnect.is_some()
    }

    /// Drain a finished background reconnect (from [`Self::cancel_query`]).
    /// Called by the host each event-loop tick; cheap when nothing is
    /// pending.
    pub fn poll_reconnect(&mut self) {
        let Some(pending) = self.pending_reconnect.as_ref() else {
            return;
        };
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(session::SessionError::NotConnected)
            }
        };
        let lost_tx = pending.lost_tx;
        self.pending_reconnect = None;
        match result {
            Ok(session) => {
                self.session = Some(session);
                self.message = Some(if lost_tx {
                    t!("msg.db_cancelled_tx_lost").to_string()
                } else {
                    t!("msg.db_cancelled").to_string()
                });
            }
            Err(e) => {
                self.message = Some(e.to_string());
                self.disconnect();
            }
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
                let tree = self.show_plan_tree(kind);
                let insight = catalog::scan_insight(kind, &self.grid.rows);
                self.message = Some(if insight {
                    t!("msg.db_insight_scan").to_string()
                } else if let Some(plan) = tree {
                    let count = plan.nodes.len();
                    match plan.execution_ms {
                        Some(ms) => t!("msg.db_plan_timed", count = count, ms = format!("{ms:.3}"))
                            .to_string(),
                        None => t!("msg.db_plan", count = count).to_string(),
                    }
                } else {
                    t!("msg.db_rows", count = rows).to_string()
                });
            }
        }
    }

    /// On `PostgreSQL`, replace the streamed `QUERY PLAN` text in the grid
    /// with the parsed plan tree and its row marks, returning the plan. Any
    /// other engine, or a plan that does not parse, leaves the grid as it
    /// came back.
    pub(super) fn show_plan_tree(&mut self, kind: connect::Kind) -> Option<crate::plan::Plan> {
        self.plan_marks = None;
        if kind != connect::Kind::Postgres {
            return None;
        }
        let text = self
            .grid
            .rows
            .iter()
            .filter_map(|r| r.first())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let plan = crate::plan::parse_yaml(&text).or_else(|| crate::plan::parse_json(&text))?;
        let (headers, rows, marks) = crate::plan::grid(&plan);
        self.grid.set(headers, rows.clone());
        self.plan_marks = Some((rows, marks));
        Some(plan)
    }

    /// Each results row's plan-tree mark (indexed like `grid.rows`) while the
    /// grid shows a `PostgreSQL` plan tree (T776); `None` for any other
    /// result, including one that replaced the plan since.
    #[must_use]
    pub fn plan_marks(&self) -> Option<&[crate::results::Mark]> {
        match &self.plan_marks {
            Some((rows, marks)) if *rows == self.grid.rows => Some(marks),
            _ => None,
        }
    }

    /// Finalize a streamed statement that errored partway. `last_error`
    /// (fed to "fix the last error" AI prompts) always keeps the driver's
    /// own raw message; `message` (the status line shown to the user) gets
    /// a clarifying hint on top of it when the error's [`session::
    /// QueryErrorKind`] (Run H, T538) says the connection itself is gone —
    /// a plain I/O error's text alone doesn't make that obvious.
    /// `position` is the server's error position (T775), from
    /// [`session::Session::error_position`].
    fn finish_stream_err(&mut self, error: &session::QueryError, position: Option<usize>) {
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
        // A statement stopped by our own server-side cancel (T774) is not an
        // error to fix: say it was cancelled, and whether the transaction it
        // ran in now needs a rollback.
        if pending.cancel.is_some() && error.is_cancelled() {
            self.message = Some(if self.tx == TxState::Aborted {
                t!("msg.db_cancelled_tx_aborted").to_string()
            } else {
                t!("msg.db_cancelled").to_string()
            });
            return;
        }
        self.last_error = Some((pending.sql.clone(), error.message.clone()));
        // Put the cursor where the server says the error is (T775).
        if let Some(offset) = pending
            .anchor
            .zip(position)
            .and_then(|(anchor, position)| error_offset(anchor, position))
        {
            self.query.set_cursor_offset(offset);
            self.focus = Pane::Editor;
        }
        self.message = Some(if error.kind == session::QueryErrorKind::ConnectionLost {
            t!("msg.db_connection_lost", error = &error.message).to_string()
        } else {
            error.message.clone()
        });
    }

    /// Update the client-side transaction state from an executed statement:
    /// `BEGIN`/`START` opens it, `COMMIT`/`ROLLBACK` closes it, and any failure
    /// inside an open transaction marks it aborted.
    ///
    /// This only inspects the first word, so it is a client-side *mirror* of
    /// the server's state: it can drift (e.g. `END`, or `COMMIT` inside a
    /// multi-statement string), which is why it only gates convenience
    /// behaviour (the badge, skipping a confirmation) and the read-only
    /// guard never depends on it.
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
                self.message = Some(e.to_string());
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
    /// the grid shows the last statement's output. On failure the status line
    /// reads `n/total: error` and later statements are not run.
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
                    self.last_error = Some((stmt.clone(), e.to_string()));
                    self.note_tx(stmt, false);
                    self.message = Some(format!("{}/{}: {e}", i + 1, stmts.len()));
                    return;
                }
            }
        }
        let (headers, rows) = last;
        self.grid.set(headers, rows);
        self.grid_source = stmts.last().map(|s| (s.clone(), None));
        self.focus = Pane::Results;
        self.set_uneditable();
        self.message = Some(t!("msg.db_ran_all", count = stmts.len()).to_string());
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
            self.message = Some(e.to_string());
            return;
        }
        self.write_enabled = want_writable;
        self.message = Some(if want_writable {
            t!("msg.db_write_enabled").to_string()
        } else {
            t!("msg.db_read_only_on").to_string()
        });
    }

    /// A short description of the execution awaiting confirmation, for the
    /// confirmation view: the statement itself, or the statement count and
    /// the first statement for several.
    #[must_use]
    pub fn pending_summary(&self) -> Option<String> {
        match self.pending_run.as_ref()? {
            PendingRun::One(sql) | PendingRun::Explain(sql, _) => Some(sql.clone()),
            PendingRun::Commit { statements, .. } => Some(format!(
                "{} × … {}",
                statements.len(),
                statements.first().map_or("", |s| s.sql.as_str())
            )),
            PendingRun::All(stmts)
            | PendingRun::Import {
                statements: stmts, ..
            } => Some(format!(
                "{} × … {}",
                stmts.len(),
                stmts.first().map_or("", String::as_str)
            )),
        }
    }

    /// Every statement awaiting confirmation, in run order, so the
    /// confirmation view can show the SQL in full (T781). Empty when nothing
    /// is pending.
    #[must_use]
    pub fn pending_statements(&self) -> Vec<String> {
        match self.pending_run.as_ref() {
            None => Vec::new(),
            Some(PendingRun::One(sql) | PendingRun::Explain(sql, _)) => vec![sql.clone()],
            Some(PendingRun::Commit { statements, .. }) => {
                statements.iter().map(|s| s.sql.clone()).collect()
            }
            Some(
                PendingRun::All(stmts)
                | PendingRun::Import {
                    statements: stmts, ..
                },
            ) => stmts.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Browser {
        let mut b = Browser::new(Vec::new());
        b.session = Some(session::Session::connect("sqlite::memory:", &[]).expect("memory db"));
        b
    }

    fn set_text(b: &mut Browser, text: &str) {
        for c in text.chars() {
            if c == '\n' {
                b.query.newline();
            } else {
                b.query.insert_char(c);
            }
        }
    }

    /// Drive the async query and any cancel reconnect to completion.
    fn drain(b: &mut Browser) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while b.query_running() || b.reconnect_running() {
            b.poll_query();
            b.poll_reconnect();
            assert!(std::time::Instant::now() < deadline, "async work stalled");
            std::thread::yield_now();
        }
    }

    /// Whether the session is still the one that created the temp table.
    fn same_session(b: &mut Browser) -> bool {
        b.session
            .as_mut()
            .is_some_and(|s| s.run("SELECT count(*) FROM keep").is_ok())
    }

    #[test]
    fn error_offset_maps_a_server_position_into_the_buffer() {
        let plain = Anchor {
            start: 10,
            prefix: 0,
            len: 8,
        };
        assert_eq!(error_offset(plain, 1), Some(10), "position is 1-based");
        assert_eq!(error_offset(plain, 5), Some(14));
        assert_eq!(error_offset(plain, 99), Some(18), "clamped to the end");
        assert_eq!(error_offset(plain, 0), None);
        let explain = Anchor {
            start: 0,
            prefix: 8, // "EXPLAIN "
            len: 8,
        };
        assert_eq!(error_offset(explain, 3), None, "inside the EXPLAIN prefix");
        assert_eq!(error_offset(explain, 9), Some(0));
    }

    #[test]
    fn the_anchor_tracks_plain_and_explain_statements() {
        let mut b = connected();
        set_text(&mut b, "select 1;\n  SELEKT 2;");
        assert_eq!(
            b.anchor_for("SELEKT 2"),
            Some(Anchor {
                start: 12,
                prefix: 0,
                len: 8
            })
        );
        assert_eq!(
            b.anchor_for("EXPLAIN QUERY PLAN SELEKT 2;"),
            Some(Anchor {
                start: 12,
                prefix: 19,
                len: 8
            })
        );
        assert_eq!(
            b.anchor_for("SELECT 'substituted'"),
            None,
            "substituted bind parameters do not map back"
        );
    }

    #[test]
    fn a_positioned_error_moves_the_cursor_to_it() {
        // SQLite reports no position, so feed the error PostgreSQL would
        // send for `SELECT 1 FRM t` (position 10, the `FRM`).
        let mut b = connected();
        set_text(&mut b, "select 0;\nSELECT 1 FRM t");
        b.focus = Pane::Results;
        b.pending_query = Some(Pending {
            started: std::time::Instant::now(),
            sql: "SELECT 1 FRM t".into(),
            history_sql: "SELECT 1 FRM t".into(),
            kind: QueryKind::Run,
            anchor: b.anchor_for("SELECT 1 FRM t"),
            cancel: None,
        });
        let error = session::QueryError::other("syntax error at or near \"FRM\"");
        b.finish_stream_err(&error, Some(10));
        assert_eq!((b.query.row, b.query.col), (1, 9), "cursor on `FRM`");
        assert_eq!(b.focus, Pane::Editor);
        assert!(b.last_error.is_some(), "still a real error to fix");
    }

    #[test]
    fn an_error_without_a_position_leaves_the_cursor() {
        let mut b = connected();
        set_text(&mut b, "SELECT * FROM nope");
        b.query.row = 0;
        b.query.col = 3;
        b.execute();
        drain(&mut b);
        assert!(b.last_error.is_some());
        assert_eq!((b.query.row, b.query.col), (0, 3));
    }

    #[test]
    fn a_server_cancel_keeps_the_session_and_its_transaction() {
        let mut b = connected();
        let session = b.session.as_mut().unwrap();
        session.run("CREATE TEMP TABLE keep (a INTEGER)").unwrap();
        session.run("BEGIN").unwrap();
        // SQLite stands in for a server whose cancel is accepted: the handle
        // runs on a second connection, and the session must not restart.
        session.cancel_handle = Some(session::CancelHandle {
            url: "sqlite::memory:".into(),
            sql: "SELECT 1".into(),
        });
        b.tx = TxState::Open;
        set_text(&mut b, "SELECT count(*) FROM keep");
        b.execute();
        assert!(b.query_running());
        b.cancel_query();
        assert!(b.query_running(), "the statement is still awaited");
        assert!(!b.reconnect_running(), "no reconnect for a server cancel");
        drain(&mut b);
        assert!(!b.reconnect_running());
        assert_eq!(b.tx, TxState::Open, "the transaction survives");
        assert!(same_session(&mut b), "temporary tables survive");
    }

    #[test]
    fn a_cancelled_statement_in_a_transaction_marks_it_aborted() {
        let mut b = connected();
        b.tx = TxState::Open;
        b.pending_query = Some(Pending {
            started: std::time::Instant::now(),
            sql: "SELECT pg_sleep(60)".into(),
            history_sql: "SELECT pg_sleep(60)".into(),
            kind: QueryKind::Run,
            anchor: None,
            cancel: Some(ServerCancel { rx: None }),
        });
        let cancelled = session::QueryError {
            code: Some("57014".into()),
            ..session::QueryError::other("canceling statement due to user request")
        };
        b.finish_stream_err(&cancelled, None);
        assert_eq!(b.tx, TxState::Aborted, "TX! until the user rolls back");
        assert!(b.last_error.is_none(), "a cancel is not an error to fix");
        assert!(b.session.is_some(), "the session is kept");
    }

    #[test]
    fn a_failed_server_cancel_falls_back_to_reconnecting() {
        let mut b = connected();
        let session = b.session.as_mut().unwrap();
        session.run("CREATE TEMP TABLE keep (a INTEGER)").unwrap();
        session.cancel_handle = Some(session::CancelHandle {
            url: "sqlite::memory:".into(),
            sql: "NOT VALID SQL".into(),
        });
        b.tx = TxState::Open;
        // Keep the statement in flight until the cancel's reply is read:
        // nothing is polled between sending it and the cancel.
        set_text(&mut b, "SELECT count(*) FROM keep");
        b.execute();
        b.cancel_query();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !b.reconnect_running() && b.query_running() {
            // Only the cancel's reply is read here, not the rows, so the
            // fallback is taken regardless of how fast the query is.
            let _ = b.poll_server_cancel();
            assert!(std::time::Instant::now() < deadline, "cancel stalled");
            std::thread::yield_now();
        }
        assert!(b.reconnect_running(), "fell back to reconnecting");
        drain(&mut b);
        assert_eq!(b.tx, TxState::None, "the transaction is gone");
        assert!(b.pending_reconnect.is_none());
        assert!(
            !same_session(&mut b),
            "a fresh session: the temp table is gone"
        );
    }

    #[test]
    fn a_second_ctrl_c_escalates_to_reconnecting() {
        let mut b = connected();
        b.session.as_mut().unwrap().cancel_handle = Some(session::CancelHandle {
            url: "sqlite::memory:".into(),
            sql: "SELECT 1".into(),
        });
        set_text(&mut b, "SELECT 1");
        b.execute();
        b.cancel_query();
        assert!(!b.reconnect_running());
        b.cancel_query();
        assert!(!b.query_running(), "the statement is abandoned");
        assert!(b.reconnect_running(), "the second Ctrl+C reconnects");
        drain(&mut b);
        assert!(b.session.is_some());
    }

    #[test]
    fn sqlite_cancel_reconnects_and_reports_a_lost_transaction() {
        let mut b = connected();
        b.tx = TxState::Open;
        set_text(&mut b, "SELECT 1");
        b.execute();
        b.cancel_query();
        assert!(b.reconnect_running(), "no server cancel on SQLite");
        let lost = b.pending_reconnect.as_ref().is_some_and(|p| p.lost_tx);
        assert!(lost, "the reconnect knows a transaction was open");
        drain(&mut b);
        assert_eq!(b.tx, TxState::None);
    }
}
