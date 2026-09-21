//! Running SQL: the write/DDL confirmation and bind-parameter prompts,
//! executing statements (synchronously via [`Browser::run_sql`]/
//! [`Browser::run_catalog`] for internal work, or streamed asynchronously
//! for user-initiated ones), client-side transaction tracking, and
//! cancelling a hung query. Extracted from `lib.rs` (T516): the one
//! cohesive slice of [`Browser`]'s methods concerned with *running a
//! statement*, as opposed to connecting or editing results.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    Browser, Outcome, Pane, ParamPrompt, TxState, View, catalog, connect, editor, params, session,
    store,
};

/// An execution awaiting write/DDL confirmation.
#[derive(Debug, Clone)]
pub(crate) enum PendingRun {
    /// One statement (possibly EXPLAIN-wrapped).
    One(String),
    /// Every statement in the buffer, in order.
    All(Vec<String>),
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
pub(crate) struct Pending {
    /// When the query was sent (for the elapsed indicator).
    pub(super) started: std::time::Instant,
    /// The statement (for logging / transaction tracking) — the exact SQL sent.
    sql: String,
    /// The text to persist in history — the bind-parameter template when the
    /// statement came from substitution, otherwise identical to `sql`. Keeping
    /// this separate stops prompted secret values from being written to disk.
    history_sql: String,
    /// How to apply the reply.
    kind: QueryKind,
}

/// A cancelled query's reconnect running on a background thread, awaited by
/// [`Browser::poll_reconnect`] (Run H, T531/T532).
pub(crate) struct PendingReconnect {
    /// The eventual reconnect result: the fresh session, or the reconnect's
    /// own error (in which case the workbench gives up and disconnects,
    /// exactly like the old synchronous path did).
    rx: std::sync::mpsc::Receiver<Result<session::Session, String>>,
}

impl Browser {
    /// Run a user-initiated `sql` on the live session, timing and logging it.
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is
    /// open.
    pub(super) fn run_sql(&mut self, sql: &str) -> Result<session::Table, String> {
        self.run_traced(sql, store::Origin::User)
    }

    /// Run a background workbench `sql` (catalog, preview, ERD), logged as
    /// [`store::Origin::App`].
    ///
    /// # Errors
    ///
    /// The driver's error, or a not-connected message when no session is open.
    pub(super) fn run_catalog(&mut self, sql: &str) -> Result<session::Table, String> {
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

    /// Keys on the write/DDL confirmation.
    pub(super) fn key_confirm(&mut self, key: KeyEvent) -> Outcome {
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

    /// Keys on the bind-parameter prompt: each `Enter` records the value for the
    /// current placeholder; once all are filled the substituted statement runs.
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
                // This attempt never reaches `start_query`, so drop any staged
                // history template rather than let it attach to a later query.
                self.history_override = None;
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
        // Consume any staged history template up front so it applies to exactly
        // this query and can never leak onto a later one, even if we bail below.
        let history_sql = self.history_override.take().unwrap_or_else(|| sql.clone());
        let Some(session) = self.session.as_ref() else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        if let Err(e) = session.send(&sql) {
            self.message = Some(e);
            return;
        }
        self.message = Some(t!("msg.db_running").to_string());
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

    /// Cancel the in-flight query: abandon the worker (its result is
    /// discarded when/if it ever arrives) and reconnect, in the background
    /// (Run H, T531), so the UI is usable again. Transaction state is lost.
    /// [`Self::poll_reconnect`] finishes it.
    ///
    /// The old synchronous `Session::restart` here was the designed escape
    /// hatch from a hung query — but reconnecting calls `Session::connect`
    /// again, so if the network condition that caused the hang was still
    /// present, cancelling a hung query could itself hang. Backgrounding it
    /// fixes that; actually cancelling the *old* worker's stuck read is a
    /// separate, deeper fix (T532).
    pub fn cancel_query(&mut self) {
        if self.pending_query.take().is_none() {
            return;
        }
        let Some(mut session) = self.session.take() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = session.restart().map(|()| session);
            let _ = tx.send(result);
        });
        self.tx = TxState::None;
        self.message = Some(t!("msg.db_cancelling").to_string());
        self.pending_reconnect = Some(PendingReconnect { rx });
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
                Err(t!("msg.db_not_connected").to_string())
            }
        };
        self.pending_reconnect = None;
        match result {
            Ok(session) => {
                self.session = Some(session);
                self.message = Some(t!("msg.db_cancelled").to_string());
            }
            Err(e) => {
                self.message = Some(e);
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
                let insight = catalog::scan_insight(kind, &self.grid.rows);
                self.message = Some(if insight {
                    t!("msg.db_insight_scan").to_string()
                } else {
                    t!("msg.db_rows", count = rows).to_string()
                });
            }
        }
    }

    /// Finalize a streamed statement that errored partway. `last_error`
    /// (fed to "fix the last error" AI prompts) always keeps the driver's
    /// own raw message; `message` (the status line shown to the user) gets
    /// a clarifying hint on top of it when the error's [`session::
    /// QueryErrorKind`] (Run H, T538) says the connection itself is gone —
    /// a plain I/O error's text alone doesn't make that obvious.
    fn finish_stream_err(&mut self, error: &session::QueryError) {
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
        self.last_error = Some((pending.sql.clone(), error.message.clone()));
        self.message = Some(if error.kind == session::QueryErrorKind::ConnectionLost {
            t!("msg.db_connection_lost", error = &error.message).to_string()
        } else {
            error.message.clone()
        });
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
}
