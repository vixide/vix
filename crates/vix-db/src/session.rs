//! The persistent database session: one live sqlx connection per workbench.
//!
//! A [`Session`] holds a single [`sqlx`] `Any` connection open for the life of
//! the workbench connection — so session state (temporary tables,
//! `search_path`, and above all `BEGIN` / `COMMIT` / `ROLLBACK` transactions)
//! survives from statement to statement.
//!
//! sqlx is async; the TUI is not. The connection lives on a dedicated worker
//! thread that owns a current-thread tokio runtime, reached over channels.
//! [`Session::run`] blocks for one full result (used for fast internal catalog
//! and commit queries); [`Session::send`] + [`Session::poll`] are the
//! non-blocking counterpart that streams a statement's rows back as [`Chunk`]s
//! so the host can keep the UI responsive (M3/M4). On PostgreSQL and MySQL,
//! [`Session::cancel_handle`] asks the server to stop a running statement over
//! a second connection, leaving this one (and its transaction) alive (T774);
//! [`Session::restart`] reconnects on a fresh connection, the fallback for a
//! cancelled query that cannot be stopped that way. Dropping
//! the `Session` closes the channel; the worker notices, closes the connection
//! cleanly, and exits.
//!
//! # Invariants
//!
//! - **One statement at a time.** The worker serves requests strictly in
//!   order and a statement's chunks are complete (`Head`, `Rows`*, then
//!   `Done` or `Err`) before the next is read, so callers must drain one
//!   statement ([`Session::poll`] until `Done`/`Err`) before reading the
//!   reply of another.
//! - **Text-only results.** Every cell is decoded to a `String`; `NULL`
//!   becomes the empty string (see the private `cell`).
//! - **Bounded memory.** Rows stream in `BATCH`-sized chunks and stop at
//!   [`MAX_ROWS`], reported through `Chunk::Done(true)`.
//! - **No secrets in errors.** Connect failures are passed through
//!   `redact_url_secret` before reaching the UI.
//! - **Cancellation is cooperative.** A server-side cancel
//!   ([`CancelHandle::send`]) makes the statement fail on the server, and the
//!   worker carries on; [`Session::cancel`] only signals, and a hung worker
//!   is abandoned (and a fresh one started by [`Session::restart`]) rather
//!   than killed.
//!
//! The design is described in `crates/vix-db/spec/session.md`; the callers are
//! the `lifecycle` (connect) and `execute` (run/cancel) modules of this crate.

use futures_util::StreamExt as _;
use sqlx::{Column as _, Row as _};
use std::sync::Once;
use std::sync::mpsc;

/// A result set: column headers plus stringified rows.
pub type Table = (Vec<String>, Vec<Vec<String>>);

/// Rows sent per streamed batch (M4): the grid grows this many rows at a time.
/// A balance between channel traffic and how soon the first rows appear.
const BATCH: usize = 512;

/// Hard cap on streamed rows for one statement; beyond it the stream stops and
/// the result is flagged truncated so the workbench and memory stay bounded.
pub const MAX_ROWS: usize = 20_000;

/// One statement for the worker. An enum (rather than a bare `String`) so
/// further request kinds can be added without changing the channel type.
enum Request {
    /// Execute the SQL and stream its rows back as [`Chunk`]s.
    Run(String),
    /// Execute the SQL for its effect only and report how many rows it
    /// affected on the given one-shot channel (T777's exactly-one-row
    /// commit check), instead of streaming rows.
    Exec(String, mpsc::Sender<Result<u64, QueryError>>),
}

/// A piece of a streamed result: the header row, then batches of data rows,
/// then a terminal marker (or an error).
#[derive(Debug, Clone)]
pub enum Chunk {
    /// Column headers (sent once, before any rows; empty for no-row statements).
    Head(Vec<String>),
    /// A batch of stringified data rows.
    Rows(Vec<Vec<String>>),
    /// The statement finished; `true` if it was truncated at [`MAX_ROWS`].
    Done(bool),
    /// The statement failed.
    Err(QueryError),
}

/// A query's failure, classified enough to let the host react differently to
/// a lost connection, a constraint violation, or a plain database/other error
/// — instead of one opaque string (Run H, T538). `Display`s as `message`, so
/// every existing "just show the text" call site keeps working unchanged.
#[derive(Debug, Clone)]
pub struct QueryError {
    /// Display-ready message — what a bare `String` used to carry.
    pub message: String,
    /// A rough classification of what went wrong.
    pub kind: QueryErrorKind,
    /// The driver's raw error code, when it provides one (e.g. Postgres's
    /// SQLSTATE), for callers that want more than the coarse `kind`.
    pub code: Option<String>,
}

/// A rough classification of a [`QueryError`] — coarse buckets a host can
/// react to (offering a reconnect for a lost connection, highlighting a row
/// for a constraint violation, …) without parsing the display message. Own
/// copy of the constraint variants `sqlx::error::ErrorKind` already
/// distinguishes, rather than reusing that type directly, so this type (and
/// [`Chunk`], which carries it) can stay `Clone` — `sqlx::error::ErrorKind`
/// isn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryErrorKind {
    /// The connection itself is gone (an I/O error talking to the server).
    ConnectionLost,
    /// A unique/primary key constraint was violated.
    UniqueViolation,
    /// A foreign key constraint was violated.
    ForeignKeyViolation,
    /// A not-null constraint was violated.
    NotNullViolation,
    /// A check constraint was violated.
    CheckViolation,
    /// An exclusion constraint was violated.
    ExclusionViolation,
    /// The database rejected the statement for some other reason (a syntax
    /// error, an unknown column, …), or the driver didn't classify it.
    Database,
    /// Anything else (a disconnected channel, a non-`sqlx::Error` failure, …).
    Other,
}

/// Why a [`Session`] could not connect, send or run a statement (T610).
///
/// `Display` text matches the earlier `String` errors; the not-connected
/// message is translated (`msg.db_not_connected`).
#[derive(Debug)]
#[non_exhaustive]
pub enum SessionError {
    /// The worker thread could not be started.
    Spawn(std::io::Error),
    /// The connection could not be opened (credentials already redacted), or
    /// the worker exited before reporting readiness.
    Connect {
        /// Display-ready driver message.
        message: String,
    },
    /// A setup statement (e.g. a read-only pragma) failed.
    Setup {
        /// Display-ready driver message.
        message: String,
    },
    /// The worker is gone, or no session is open.
    NotConnected,
    /// The statement failed.
    Query(QueryError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => e.fmt(f),
            Self::Connect { message } | Self::Setup { message } => f.write_str(message),
            Self::NotConnected => f.write_str(&t!("msg.db_not_connected")),
            Self::Query(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(e) => Some(e),
            Self::Connect { .. } | Self::Setup { .. } | Self::NotConnected | Self::Query(_) => None,
        }
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl QueryError {
    /// Classify a driver error and capture its display message in one step.
    fn from_sqlx(e: &sqlx::Error) -> QueryError {
        let (kind, code) = match e {
            sqlx::Error::Io(_) => (QueryErrorKind::ConnectionLost, None),
            sqlx::Error::Database(db_err) => {
                let kind = match db_err.kind() {
                    sqlx::error::ErrorKind::UniqueViolation => QueryErrorKind::UniqueViolation,
                    sqlx::error::ErrorKind::ForeignKeyViolation => {
                        QueryErrorKind::ForeignKeyViolation
                    }
                    sqlx::error::ErrorKind::NotNullViolation => QueryErrorKind::NotNullViolation,
                    sqlx::error::ErrorKind::CheckViolation => QueryErrorKind::CheckViolation,
                    sqlx::error::ErrorKind::ExclusionViolation => {
                        QueryErrorKind::ExclusionViolation
                    }
                    _ => QueryErrorKind::Database,
                };
                (kind, db_err.code().map(std::borrow::Cow::into_owned))
            }
            _ => (QueryErrorKind::Other, None),
        };
        QueryError {
            message: e.to_string(),
            kind,
            code,
        }
    }

    /// A disconnected-channel or other non-`sqlx::Error` failure, with no
    /// finer classification available.
    pub(crate) fn other(message: impl Into<String>) -> QueryError {
        QueryError {
            message: message.into(),
            kind: QueryErrorKind::Other,
            code: None,
        }
    }

    /// Whether this is the server reporting that the statement was
    /// cancelled on request (T774): SQLSTATE `57014` (PostgreSQL
    /// `query_canceled`) or `70100` (MySQL's "query execution was
    /// interrupted" after `KILL QUERY`).
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        matches!(self.code.as_deref(), Some("57014" | "70100"))
    }
}

/// Where in the statement the server says `e` is (T775): a 1-based character
/// position into the SQL text that was sent. Only PostgreSQL reports one (the
/// `P` field of its error response). The `Any` driver passes the engine's own
/// error through boxed, so it survives a downcast to
/// [`sqlx::postgres::PgDatabaseError`]. A position into an internally
/// generated query (a PL/pgSQL body) is dropped: it does not point into the
/// editor's text.
fn error_position_of(e: &sqlx::Error) -> Option<usize> {
    let sqlx::Error::Database(db_err) = e else {
        return None;
    };
    match db_err
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()?
        .position()?
    {
        sqlx::postgres::PgErrorPosition::Original(at) => Some(at),
        sqlx::postgres::PgErrorPosition::Internal { .. } => None,
    }
}

/// Registers sqlx's `Any` drivers exactly once per process. sqlx's `Any`
/// connection panics on an unregistered scheme, so [`Session::connect`] calls
/// this before every connect; `Once` makes that cheap and thread-safe.
static DRIVERS: Once = Once::new();

/// How long a server-side cancel (T774) may take, connect included, before
/// it counts as failed and the caller falls back to reconnecting.
const CANCEL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The engine a connection URL is for, from its scheme: `postgres://` or
/// `postgresql://`, `mysql://`, or `sqlite:`. `None` for anything else.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, session::kind_of_url};
/// assert_eq!(kind_of_url("postgresql://u@h/d"), Some(Kind::Postgres));
/// assert_eq!(kind_of_url("mysql://u@h/d"), Some(Kind::Mysql));
/// assert_eq!(kind_of_url("sqlite::memory:"), Some(Kind::Sqlite));
/// assert_eq!(kind_of_url("mssql://h"), None);
/// ```
#[must_use]
pub fn kind_of_url(url: &str) -> Option<crate::connect::Kind> {
    use crate::connect::Kind;
    let scheme = url.split(':').next()?.to_ascii_lowercase();
    match scheme.as_str() {
        "postgres" | "postgresql" => Some(Kind::Postgres),
        "mysql" | "mariadb" => Some(Kind::Mysql),
        "sqlite" => Some(Kind::Sqlite),
        _ => None,
    }
}

/// The query that reports the server's id for the current connection (T774),
/// run once when a session connects: `pg_backend_pid()` on PostgreSQL,
/// `CONNECTION_ID()` (cast to a signed integer) on MySQL. `SQLite` runs in-process and has none.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, session::backend_id_sql};
/// assert_eq!(backend_id_sql(Kind::Postgres), Some("SELECT pg_backend_pid()"));
/// assert_eq!(backend_id_sql(Kind::Sqlite), None);
/// ```
#[must_use]
pub fn backend_id_sql(kind: crate::connect::Kind) -> Option<&'static str> {
    use crate::connect::Kind;
    match kind {
        Kind::Postgres => Some("SELECT pg_backend_pid()"),
        // `CONNECTION_ID()` is `BIGINT UNSIGNED`, which sqlx's `Any` row
        // mapping rejects (its MySQL integer decoding is signed only).
        Kind::Mysql => Some("SELECT CAST(CONNECTION_ID() AS SIGNED)"),
        Kind::Sqlite => None,
    }
}

/// The statement that asks the server to stop whatever connection `id` is
/// running, without closing that connection (T774). Sent over a second,
/// short-lived connection, since the first one is busy.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, session::cancel_sql};
/// assert_eq!(
///     cancel_sql(Kind::Postgres, 4242).as_deref(),
///     Some("SELECT pg_cancel_backend(4242)")
/// );
/// assert_eq!(cancel_sql(Kind::Mysql, 17).as_deref(), Some("KILL QUERY 17"));
/// assert_eq!(cancel_sql(Kind::Sqlite, 1), None);
/// ```
#[must_use]
pub fn cancel_sql(kind: crate::connect::Kind, id: u64) -> Option<String> {
    use crate::connect::Kind;
    match kind {
        Kind::Postgres => Some(format!("SELECT pg_cancel_backend({id})")),
        Kind::Mysql => Some(format!("KILL QUERY {id}")),
        Kind::Sqlite => None,
    }
}

/// Whether the reply to a [`cancel_sql`] statement means the server took
/// the cancel: `pg_cancel_backend` returns one boolean cell (`false` when
/// the backend could not be signalled); `KILL QUERY` returns no rows and
/// reports failure as an error instead.
fn cancel_reply_ok(rows: &[Vec<String>]) -> bool {
    match rows.first().and_then(|r| r.first()) {
        Some(v) => !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "f" | "0"),
        None => true,
    }
}

/// What it takes to ask the server to stop a [`Session`]'s running
/// statement (T774): the session's own connection URL and the cancel
/// statement for its backend id. `Clone + Send`, so the host can send the
/// cancel from a background thread while the session itself keeps waiting
/// for the statement's (now failing) reply.
#[derive(Clone)]
pub struct CancelHandle {
    /// The URL the session connected with (credentials included; never
    /// shown — errors pass through `redact_url_secret`).
    pub(crate) url: String,
    /// The engine's cancel statement, from [`cancel_sql`].
    pub(crate) sql: String,
}

impl std::fmt::Debug for CancelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The URL can carry a password; keep it out of debug output.
        f.debug_struct("CancelHandle")
            .field("sql", &self.sql)
            .finish_non_exhaustive()
    }
}

impl CancelHandle {
    /// Open a second connection with the same URL, send the cancel
    /// statement, and close it again. Blocks (bounded by a 10-second
    /// timeout, connect included), so call it off the UI thread.
    ///
    /// Returns `Ok(true)` when the server accepted the cancel and
    /// `Ok(false)` when it answered that it could not signal the backend.
    ///
    /// # Errors
    ///
    /// [`SessionError::Connect`] when the second connection cannot be
    /// opened (or the whole attempt timed out), [`SessionError::Query`]
    /// when the server rejected the cancel statement itself (for example
    /// missing privileges for `KILL QUERY`), or [`SessionError::Spawn`]
    /// when no runtime could be built.
    pub fn send(&self) -> Result<bool, SessionError> {
        DRIVERS.call_once(sqlx::any::install_default_drivers);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(SessionError::Spawn)?;
        let url = self.url.clone();
        let sql = self.sql.clone();
        let attempt = async move {
            let mut conn = <sqlx::AnyConnection as sqlx::Connection>::connect(&url)
                .await
                .map_err(|e| SessionError::Connect {
                    message: redact_url_secret(&e.to_string(), &url),
                })?;
            let reply = query_text(&mut conn, &sql).await;
            let _ = sqlx::Connection::close(conn).await;
            reply
                .map(|rows| cancel_reply_ok(&rows))
                .map_err(|e| SessionError::Query(QueryError::from_sqlx(&e)))
        };
        rt.block_on(async { tokio::time::timeout(CANCEL_TIMEOUT, attempt).await })
            .unwrap_or_else(|_| {
                Err(SessionError::Connect {
                    message: std::io::Error::from(std::io::ErrorKind::TimedOut).to_string(),
                })
            })
    }
}

/// A live database connection on its own worker thread.
#[derive(Debug)]
pub struct Session {
    /// Statements to the worker.
    req_tx: mpsc::Sender<Request>,
    /// Streamed result chunks back from the worker.
    reply_rx: mpsc::Receiver<Chunk>,
    /// The connection URL, kept so a cancelled query can [`restart`] the
    /// worker on a fresh connection.
    ///
    /// [`restart`]: Session::restart
    url: String,
    /// Setup statements re-applied on [`restart`](Session::restart).
    setup: Vec<String>,
    /// Signals the worker to abandon an in-flight statement and exit as
    /// soon as it can (Run H, T532) — see [`Self::cancel`].
    cancel_tx: tokio::sync::watch::Sender<bool>,
    /// How to ask the server to stop the running statement (T774): `Some`
    /// when the engine supports it and the backend id was read at connect.
    pub(crate) cancel_handle: Option<CancelHandle>,
    /// The server-reported error position of the latest failed statement
    /// (T775), written by the worker before it sends the `Chunk::Err`; `0`
    /// means none (positions are 1-based). A side slot rather than a
    /// `QueryError` field, because adding a field to that public,
    /// literal-constructible struct would be a semver break.
    error_position: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Session {
    /// Open a connection to `url` (see [`crate::connect::url`]), run each
    /// `setup` statement (e.g. a read-only pragma), and block until the
    /// connection is ready.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Connect`] for the driver's connect error (bad
    /// credentials, unreachable host, unreadable file, …) or
    /// [`SessionError::Setup`] for a failing setup statement.
    pub fn connect(url: &str, setup: &[String]) -> Result<Session, SessionError> {
        DRIVERS.call_once(sqlx::any::install_default_drivers);
        let (req_tx, req_rx) = mpsc::channel::<Request>();
        let (reply_tx, reply_rx) = mpsc::channel::<Chunk>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Option<u64>, SessionError>>();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let url = url.to_string();
        let setup = setup.to_vec();
        let (worker_url, worker_setup) = (url.clone(), setup.clone());
        let error_position = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_position = std::sync::Arc::clone(&error_position);
        std::thread::Builder::new()
            .name("vix-db-session".into())
            .spawn(move || {
                worker(
                    &worker_url,
                    &worker_setup,
                    &ready_tx,
                    &req_rx,
                    &reply_tx,
                    cancel_rx,
                    &worker_position,
                );
            })
            .map_err(SessionError::Spawn)?;
        let backend_id = ready_rx.recv().map_err(|e| SessionError::Connect {
            message: e.to_string(),
        })??;
        let cancel_handle = backend_id
            .zip(kind_of_url(&url))
            .and_then(|(id, kind)| cancel_sql(kind, id))
            .map(|sql| CancelHandle {
                url: url.clone(),
                sql,
            });
        Ok(Session {
            req_tx,
            reply_rx,
            url,
            setup,
            cancel_tx,
            cancel_handle,
            error_position,
        })
    }

    /// Where the server says the error of the statement that last came back
    /// as `Chunk::Err` is (T775): a 1-based character position into the SQL
    /// that was sent. `None` when the engine reports none (only PostgreSQL
    /// does) or the latest statement succeeded. Read it right after
    /// receiving the `Chunk::Err`, before sending another statement.
    #[must_use]
    pub fn error_position(&self) -> Option<usize> {
        match self
            .error_position
            .load(std::sync::atomic::Ordering::Acquire)
        {
            0 => None,
            at => Some(at),
        }
    }

    /// How to cancel this session's running statement on the server
    /// (T774), or `None` when that is not possible (`SQLite`, or the
    /// backend id could not be read at connect) and the only way out of a
    /// running statement is [`restart`](Session::restart).
    #[must_use]
    pub fn cancel_handle(&self) -> Option<CancelHandle> {
        self.cancel_handle.clone()
    }

    /// Tell the worker to abandon whatever statement it's mid-flight on and
    /// exit as soon as it can, rather than run to completion (which, for a
    /// genuinely hung network read, may be never) — Run H, T532. The worker
    /// notices even while blocked inside a single `.await` on the stream
    /// (unlike dropping the request channel alone, which only unblocks a
    /// worker that's back at `req_rx.recv()` between statements).
    ///
    /// A one-way, **sticky** signal meant to precede giving up on the
    /// session entirely ([`restart`](Session::restart), or the host
    /// disconnecting), not a resettable pause: once sent, the *next*
    /// statement handed to this same worker (if any) is abandoned
    /// immediately too, even though nothing was in flight when `cancel` was
    /// called. Harmless to call more than once — but callers that still
    /// want to use the session afterward should call
    /// [`restart`](Session::restart) instead of calling this directly.
    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    /// Hand the worker a statement without waiting for its reply — the async
    /// counterpart of [`run`](Session::run), drained by [`poll`](Session::poll).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotConnected`] if the worker died.
    pub fn send(&self, sql: &str) -> Result<(), SessionError> {
        self.req_tx
            .send(Request::Run(sql.to_string()))
            .map_err(|_| SessionError::NotConnected)
    }

    /// The next streamed [`Chunk`] from a [`send`](Session::send), or `None`
    /// while the worker has produced nothing new this moment.
    #[must_use]
    pub fn poll(&self) -> Option<Chunk> {
        match self.reply_rx.try_recv() {
            Ok(chunk) => Some(chunk),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Chunk::Err(QueryError::other(t!("msg.db_not_connected"))))
            }
        }
    }

    /// Abandon the current worker and reconnect on a fresh connection — how
    /// a cancelled query returns control to the UI. Transaction state is
    /// lost. [`cancel`](Self::cancel)s the old worker first (T532) so it
    /// stops as soon as it notices, rather than only once the *new*
    /// connection below happens to finish — `*self` isn't overwritten (and
    /// so the old `Session`, including its own `cancel_tx`, isn't dropped)
    /// until then, which would otherwise leave the signal until too late to
    /// matter for how promptly the old worker exits.
    ///
    /// # Errors
    ///
    /// Returns the reconnect error if the new connection cannot be opened.
    pub fn restart(&mut self) -> Result<(), SessionError> {
        self.cancel();
        let (url, setup) = (self.url.clone(), self.setup.clone());
        *self = Session::connect(&url, &setup)?;
        Ok(())
    }

    /// Execute `sql` on the held connection and return its rows (headers are
    /// empty when the statement returns no rows).
    ///
    /// Blocks the calling thread until the statement completes, so it is meant
    /// for short internal queries (catalog, commit); user statements go through
    /// [`Session::send`] / [`Session::poll`]. A truncated result (past
    /// [`MAX_ROWS`]) is returned as-is without any indication here.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Query`] for the database error, or
    /// [`SessionError::NotConnected`] if the worker died.
    pub fn run(&mut self, sql: &str) -> Result<Table, SessionError> {
        self.send(sql)?;
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        loop {
            match self
                .reply_rx
                .recv()
                .map_err(|_| SessionError::NotConnected)?
            {
                Chunk::Head(h) => headers = h,
                Chunk::Rows(mut batch) => rows.append(&mut batch),
                Chunk::Done(_) => return Ok((headers, rows)),
                Chunk::Err(e) => return Err(SessionError::Query(e)),
            }
        }
    }

    /// Execute `sql` for its effect only and return the number of rows it
    /// affected, as the driver reports it (`MySQL` counts *matched* rows,
    /// since sqlx connects with `CLIENT_FOUND_ROWS`). Blocks like
    /// [`run`](Session::run). Used by the staged-change commit, which needs
    /// every statement to touch exactly one row (T777).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Query`] for the database error, or
    /// [`SessionError::NotConnected`] if the worker died.
    pub fn execute(&mut self, sql: &str) -> Result<u64, SessionError> {
        let (done_tx, done_rx) = mpsc::channel();
        self.req_tx
            .send(Request::Exec(sql.to_string(), done_tx))
            .map_err(|_| SessionError::NotConnected)?;
        done_rx
            .recv()
            .map_err(|_| SessionError::NotConnected)?
            .map_err(SessionError::Query)
    }
}

/// The worker loop: connect, signal readiness, then serve one statement at a
/// time until the request channel closes or [`Session::cancel`] fires.
fn worker(
    url: &str,
    setup: &[String],
    ready_tx: &mpsc::Sender<Result<Option<u64>, SessionError>>,
    req_rx: &mpsc::Receiver<Request>,
    reply_tx: &mpsc::Sender<Chunk>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
    error_position: &std::sync::atomic::AtomicUsize,
) {
    // A current-thread runtime is enough: the worker serves one statement at
    // a time, and it keeps the thread count at one per connection.
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = ready_tx.send(Err(SessionError::Spawn(e)));
            return;
        }
    };
    let mut conn = match rt.block_on(<sqlx::AnyConnection as sqlx::Connection>::connect(url)) {
        Ok(conn) => conn,
        Err(e) => {
            let _ = ready_tx.send(Err(SessionError::Connect {
                message: redact_url_secret(&e.to_string(), url),
            }));
            return;
        }
    };
    for stmt in setup {
        if let Err(e) = rt.block_on(exec_one(&mut conn, stmt)) {
            let _ = ready_tx.send(Err(SessionError::Setup { message: e }));
            let _ = rt.block_on(sqlx::Connection::close(conn));
            return;
        }
    }
    // The server's id for this connection, so a cancel can name it (T774).
    // Best effort: without it, cancelling falls back to reconnecting.
    let backend_id = match kind_of_url(url).and_then(backend_id_sql) {
        Some(sql) => rt
            .block_on(query_text(&mut conn, sql))
            .ok()
            .and_then(|rows| rows.first()?.first()?.trim().parse::<u64>().ok()),
        None => None,
    };
    // Readiness is reported only after setup succeeds, so `Session::connect`
    // never hands back a session whose read-only pragma failed to apply.
    let _ = ready_tx.send(Ok(backend_id));
    while let Ok(request) = req_rx.recv() {
        let keep_going = match request {
            Request::Run(sql) => rt.block_on(stream_sql(
                &mut conn,
                &sql,
                reply_tx,
                &mut cancel_rx,
                error_position,
            )),
            Request::Exec(sql, done) => {
                rt.block_on(exec_counted(&mut conn, &sql, &done, &mut cancel_rx))
            }
        };
        if !keep_going {
            break; // the consumer went away, or cancelled (T532), and abandoned this worker
        }
    }
    // A genuinely dead connection's close() could itself hang waiting on the
    // network (T532's same concern, one step later) -- bounded so a worker
    // that already gave up on its query doesn't then get stuck saying
    // goodbye to it.
    let _ = rt.block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::Connection::close(conn),
        )
        .await
    });
}

/// Remove any credential from a connection error before it is shown in the UI.
///
/// sqlx connect/config errors are stringified for display and, depending on the
/// error variant, can echo the DSN — which embeds `user:password@host`. This
/// replaces the full URL and, defensively, the bare password substring with
/// placeholders so a password can never surface in a status message or log.
fn redact_url_secret(msg: &str, url: &str) -> String {
    let mut out = msg.replace(url, "<database-url>");
    if let Some(pw) = password_of(url).filter(|p| !p.is_empty()) {
        out = out.replace(pw, "<redacted>");
    }
    out
}

/// The password component of a `scheme://user:password@host/…` URL, if present.
/// Splits on the *last* `@` so an `@` inside the (percent-encoded) userinfo
/// cannot be mistaken for the host separator. The result is still
/// percent-encoded, exactly as it appears in the URL.
fn password_of(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let authority = after_scheme.split(['/', '?']).next()?;
    let userinfo = authority.rsplit_once('@')?.0;
    userinfo.split_once(':').map(|(_, pw)| pw)
}

/// Execute one statement for its effect only (setup pragmas), discarding rows.
async fn exec_one(conn: &mut sqlx::AnyConnection, sql: &str) -> Result<(), String> {
    let sql = sqlx::AssertSqlSafe(sql.to_string());
    sqlx::raw_sql(sql)
        .execute(conn)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Run one short statement and collect every row as text (the backend-id
/// lookup and the cancel statement, T774).
async fn query_text(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
) -> Result<Vec<Vec<String>>, sqlx::Error> {
    let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_all(conn)
        .await?;
    Ok(rows
        .iter()
        .map(|row| (0..row.columns().len()).map(|i| cell(row, i)).collect())
        .collect())
}

/// Execute one statement for its effect and send the number of rows it
/// affected (or its error) on `done`. Returns `false` when cancelled (T532),
/// like [`stream_sql`], so the worker stops; a closed `done` channel is not a
/// reason to stop, since the next request may still be read.
async fn exec_counted(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
    done: &mpsc::Sender<Result<u64, QueryError>>,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    let query = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()));
    let result = tokio::select! {
        biased;
        _ = cancel_rx.changed() => return false,
        result = query.execute(conn) => result,
    };
    let reply = result
        .map(|r| r.rows_affected())
        .map_err(|e| QueryError::from_sqlx(&e));
    let _ = done.send(reply);
    true
}

/// Stream one statement's rows back as [`Chunk`]s: a [`Chunk::Head`], then
/// [`Chunk::Rows`] batches of up to [`BATCH`], then [`Chunk::Done`] (flagged
/// truncated past [`MAX_ROWS`]) — or a [`Chunk::Err`]. Returns `false` when the
/// consumer's channel has closed, or [`Session::cancel`] fired (T532), so the
/// worker can stop either way. `AssertSqlSafe` is the intended opt-in for the
/// workbench's user SQL, not an injection hazard.
async fn stream_sql(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
    reply_tx: &mpsc::Sender<Chunk>,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
    error_position: &std::sync::atomic::AtomicUsize,
) -> bool {
    error_position.store(0, std::sync::atomic::Ordering::Release);
    let query = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()));
    let mut stream = query.fetch(conn);
    let mut head_sent = false;
    let mut batch: Vec<Vec<String>> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    loop {
        let item = tokio::select! {
            // `biased`: when a row and a cancel are both ready, cancel wins.
            // Without it `select!` picks a ready branch at random, so a
            // cancelled statement could still run to completion (a CI flake
            // in `cancel_is_a_one_way_signal_the_next_statement_also_notices`).
            biased;
            // Cancelled (or the Session, and its cancel_tx, was simply
            // dropped -- `changed()` also errors then): give up on this
            // statement immediately rather than wait on a network read
            // that may never produce another item, so the worker can move
            // on to closing the connection and exiting (T532).
            _ = cancel_rx.changed() => return false,
            item = stream.next() => item,
        };
        let Some(item) = item else {
            break;
        };
        match item {
            Ok(row) => {
                if !head_sent {
                    let headers = row.columns().iter().map(|c| c.name().to_string()).collect();
                    if reply_tx.send(Chunk::Head(headers)).is_err() {
                        return false;
                    }
                    head_sent = true;
                }
                let cells = (0..row.columns().len()).map(|i| cell(&row, i)).collect();
                batch.push(cells);
                total += 1;
                if batch.len() >= BATCH
                    && reply_tx
                        .send(Chunk::Rows(std::mem::take(&mut batch)))
                        .is_err()
                {
                    return false;
                }
                if total >= MAX_ROWS {
                    truncated = true;
                    break;
                }
            }
            Err(e) => {
                // Stored before the chunk is sent, so the consumer that
                // receives the error can read the position (T775).
                error_position.store(
                    error_position_of(&e).unwrap_or(0),
                    std::sync::atomic::Ordering::Release,
                );
                return reply_tx.send(Chunk::Err(QueryError::from_sqlx(&e))).is_ok();
            }
        }
    }
    drop(stream);
    if !head_sent && reply_tx.send(Chunk::Head(Vec::new())).is_err() {
        return false;
    }
    if !batch.is_empty() && reply_tx.send(Chunk::Rows(batch)).is_err() {
        return false;
    }
    reply_tx.send(Chunk::Done(truncated)).is_ok()
}

/// Decode one cell as display text. The `Any` driver types values at
/// runtime, so try the common decodings widest-first; `NULL` renders as an
/// empty string (matching the old CLI output).
///
/// Binary values are shown via lossy UTF-8, so non-text blobs display
/// replacement characters; a value no decoding accepts also renders empty.
fn cell(row: &sqlx::any::AnyRow, i: usize) -> String {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map(|x| x.to_string()).unwrap_or_default();
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map(|x| x.to_string()).unwrap_or_default();
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.unwrap_or_default();
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return v.map(|x| x.to_string()).unwrap_or_default();
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Session {
        Session::connect("sqlite::memory:", &[]).expect("in-memory sqlite connects")
    }

    /// Send `sql` (expected to fail) and return its classified error, via
    /// the real async `send`/`poll` path — the actual integration point
    /// `QueryError::from_sqlx` (Run H, T538) is exercised through, not just
    /// the classifier called directly on a synthetic `sqlx::Error`.
    fn send_and_expect_err(s: &mut Session, sql: &str) -> QueryError {
        s.send(sql).unwrap();
        loop {
            match s.poll() {
                Some(Chunk::Err(e)) => return e,
                Some(Chunk::Head(_) | Chunk::Rows(_) | Chunk::Done(_)) => {
                    panic!("expected {sql:?} to fail")
                }
                None => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn password_is_redacted_from_error_text() {
        let url = "postgres://joel:sup3rsecret@db.internal:5432/app";
        // An error that echoes the whole DSN.
        let msg = format!("error connecting to {url}: timeout");
        let red = redact_url_secret(&msg, url);
        assert!(!red.contains("sup3rsecret"), "password leaked: {red}");
        assert!(!red.contains(url), "full DSN leaked: {red}");
        // An error that echoes only the password (e.g. an auth failure detail).
        let msg2 = "password authentication failed for password sup3rsecret";
        let red2 = redact_url_secret(msg2, url);
        assert!(
            !red2.contains("sup3rsecret"),
            "bare password leaked: {red2}"
        );
    }

    #[test]
    fn password_of_extracts_only_the_password() {
        assert_eq!(password_of("mysql://u:p%40ss@h:3306/d"), Some("p%40ss"));
        assert_eq!(password_of("postgres://user@host/db"), None); // no password
        assert_eq!(password_of("sqlite::memory:"), None); // no authority
    }

    #[test]
    fn execute_reports_rows_affected() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER)").unwrap();
        assert_eq!(s.execute("INSERT INTO t VALUES (1), (2), (3)").unwrap(), 3);
        assert_eq!(s.execute("UPDATE t SET a = a + 10 WHERE a = 1").unwrap(), 1);
        assert_eq!(s.execute("DELETE FROM t WHERE a = 99").unwrap(), 0);
        assert!(matches!(
            s.execute("DELETE FROM missing"),
            Err(SessionError::Query(_))
        ));
        // The session still serves streamed queries afterwards.
        let (_, rows) = s.run("SELECT count(*) FROM t").unwrap();
        assert_eq!(rows, vec![vec!["3"]]);
    }

    #[test]
    fn state_persists_across_statements() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        s.run("INSERT INTO t VALUES (1, 'ada'), (2, NULL)").unwrap();
        let (headers, rows) = s.run("SELECT a, b FROM t ORDER BY a").unwrap();
        assert_eq!(headers, vec!["a", "b"]);
        assert_eq!(
            rows,
            vec![vec!["1", "ada"], vec!["2", ""]],
            "NULL renders empty"
        );
    }

    #[test]
    fn transactions_survive_between_run_calls() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER)").unwrap();
        s.run("BEGIN").unwrap();
        s.run("INSERT INTO t VALUES (1)").unwrap();
        s.run("ROLLBACK").unwrap();
        let (_, rows) = s.run("SELECT count(*) FROM t").unwrap();
        assert_eq!(
            rows,
            vec![vec!["0"]],
            "the rollback really undid the insert"
        );
        s.run("BEGIN").unwrap();
        s.run("INSERT INTO t VALUES (2)").unwrap();
        s.run("COMMIT").unwrap();
        let (_, rows) = s.run("SELECT count(*) FROM t").unwrap();
        assert_eq!(rows, vec![vec!["1"]]);
    }

    #[test]
    fn errors_come_back_typed_and_the_session_survives() {
        let mut s = memory();
        let err = s.run("SELECT * FROM nope").unwrap_err();
        assert!(matches!(err, SessionError::Query(_)), "{err:?}");
        assert!(
            err.to_string().contains("nope"),
            "driver error is surfaced: {err}"
        );
        assert!(
            s.run("SELECT 1").is_ok(),
            "an error does not kill the session"
        );
    }

    #[test]
    fn connect_failure_is_reported() {
        let err = Session::connect("sqlite:/definitely/missing/dir/x.db", &[]).unwrap_err();
        assert!(matches!(err, SessionError::Connect { .. }), "{err:?}");
        assert_ne!(err.to_string(), "");
    }

    #[test]
    fn display_matches_the_old_string_text() {
        let query = SessionError::Query(QueryError::other("boom"));
        assert_eq!(query.to_string(), "boom");
        let connect = SessionError::Connect {
            message: "refused".into(),
        };
        assert_eq!(connect.to_string(), "refused");
        let setup = SessionError::Setup {
            message: "bad pragma".into(),
        };
        assert_eq!(setup.to_string(), "bad pragma");
        let spawn = SessionError::Spawn(std::io::Error::other("no threads"));
        assert_eq!(spawn.to_string(), "no threads");
        assert!(std::error::Error::source(&spawn).is_some());
        assert!(std::error::Error::source(&connect).is_none());
        assert_ne!(SessionError::NotConnected.to_string(), "");
    }

    #[test]
    fn setup_pragma_makes_the_session_read_only() {
        let setup = vec!["PRAGMA query_only = ON".to_string()];
        let mut s = Session::connect("sqlite::memory:", &setup).expect("connects read-only");
        let err = s.run("CREATE TABLE t (a INTEGER)").unwrap_err();
        assert!(
            matches!(err, SessionError::Query(_)),
            "a write is rejected at the database level: {err}"
        );
        assert!(s.run("SELECT 1").is_ok(), "reads still work read-only");
    }

    #[test]
    fn send_and_poll_stream_a_query_without_blocking() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER)").unwrap();
        s.run("INSERT INTO t VALUES (7),(8)").unwrap();
        s.send("SELECT a FROM t ORDER BY a").unwrap();
        let mut headers = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let truncated = loop {
            match s.poll() {
                Some(Chunk::Head(h)) => headers = h,
                Some(Chunk::Rows(mut b)) => rows.append(&mut b),
                Some(Chunk::Done(t)) => break t,
                Some(Chunk::Err(e)) => panic!("stream error: {e}"),
                None => std::thread::yield_now(),
            }
        };
        assert_eq!(headers, vec!["a"]);
        assert_eq!(rows, vec![vec!["7".to_string()], vec!["8".to_string()]]);
        assert!(!truncated);
    }

    #[test]
    fn restart_gives_a_working_connection() {
        // A file DB so state would survive if it were the same connection; the
        // point here is that restart yields a usable session.
        let dir = std::env::temp_dir().join(format!("vix-sess-restart-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("r.db");
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let mut s = Session::connect(&url, &[]).expect("connect");
        s.run("CREATE TABLE t (a INTEGER)").unwrap();
        s.run("INSERT INTO t VALUES (1)").unwrap();
        s.restart().expect("restart reconnects");
        let (_, rows) = s
            .run("SELECT count(*) FROM t")
            .expect("query after restart");
        assert_eq!(rows, vec![vec!["1"]], "committed data is still there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_is_a_one_way_signal_the_next_statement_also_notices() {
        let mut s = memory();
        s.cancel(); // nothing in flight yet -- calling it twice is still fine
        s.cancel();
        assert!(
            s.run("SELECT 1").is_err(),
            "cancel is sticky: the next statement is abandoned too, not just \
             whatever was in flight when it was called -- callers that still \
             want to use the session afterward must restart() it instead"
        );
    }

    /// The exact mechanism [`stream_sql`] relies on (Run H, T532): a
    /// `tokio::select!` between the stream and `cancel_rx.changed()` must
    /// resolve via cancellation even when the other branch would otherwise
    /// never complete on its own -- standing in for a genuinely hung network
    /// read, which sqlite has no way to simulate for an end-to-end test.
    #[test]
    fn cancel_unblocks_a_select_that_would_otherwise_wait_forever() {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        std::thread::spawn(move || {
            // Give the select below a moment to actually start waiting --
            // not required for correctness (`changed()` also resolves
            // immediately if the version already moved on before the
            // receiver was ever polled), just makes the interrupt-in-
            // progress case this test means to exercise the likely one.
            std::thread::sleep(std::time::Duration::from_millis(20));
            let _ = cancel_tx.send(true);
        });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let cancelled = rt.block_on(async {
            tokio::select! {
                () = std::future::pending::<()>() => false,
                _ = cancel_rx.changed() => true,
            }
        });
        assert!(
            cancelled,
            "cancel() unblocked a select that would otherwise wait forever"
        );
    }

    #[test]
    fn a_failing_setup_statement_fails_the_connect() {
        let err = Session::connect("sqlite::memory:", &["NOT VALID SQL".to_string()]).unwrap_err();
        assert!(
            matches!(err, SessionError::Setup { .. }),
            "a bad setup statement surfaces as a setup error: {err}"
        );
    }

    #[test]
    fn a_unique_constraint_violation_is_classified_not_just_stringified() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        s.run("INSERT INTO t VALUES (1)").unwrap();
        let err = send_and_expect_err(&mut s, "INSERT INTO t VALUES (1)");
        assert_eq!(
            err.kind,
            QueryErrorKind::UniqueViolation,
            "message: {}",
            err.message
        );
    }

    #[test]
    fn a_plain_syntax_error_is_classified_as_database_not_a_constraint() {
        let mut s = memory();
        let err = send_and_expect_err(&mut s, "SELEKT 1");
        assert_eq!(
            err.kind,
            QueryErrorKind::Database,
            "message: {}",
            err.message
        );
    }

    #[test]
    fn engine_specific_cancel_sql_is_built_per_engine() {
        use crate::connect::Kind;
        assert_eq!(kind_of_url("postgres://u:p@h:5432/d"), Some(Kind::Postgres));
        assert_eq!(kind_of_url("POSTGRESQL://h/d"), Some(Kind::Postgres));
        assert_eq!(kind_of_url("mysql://u@h/d"), Some(Kind::Mysql));
        assert_eq!(kind_of_url("sqlite:/x.db?mode=rwc"), Some(Kind::Sqlite));
        assert_eq!(kind_of_url("nonsense"), None);
        assert_eq!(
            backend_id_sql(Kind::Mysql),
            Some("SELECT CAST(CONNECTION_ID() AS SIGNED)")
        );
        assert_eq!(
            cancel_sql(Kind::Postgres, 99).as_deref(),
            Some("SELECT pg_cancel_backend(99)")
        );
        assert_eq!(
            cancel_sql(Kind::Mysql, 99).as_deref(),
            Some("KILL QUERY 99")
        );
        assert_eq!(backend_id_sql(Kind::Sqlite), None);
        assert_eq!(cancel_sql(Kind::Sqlite, 99), None);
    }

    #[test]
    fn cancel_reply_reads_pg_booleans_and_mysql_ok() {
        let one = |v: &str| vec![vec![v.to_string()]];
        assert!(cancel_reply_ok(&one("true")));
        assert!(cancel_reply_ok(&one("t")));
        assert!(cancel_reply_ok(&one("1")));
        assert!(!cancel_reply_ok(&one("false")));
        assert!(!cancel_reply_ok(&one("f")));
        assert!(!cancel_reply_ok(&one("0")));
        assert!(cancel_reply_ok(&[]), "KILL QUERY answers with no rows");
    }

    #[test]
    fn sqlite_sessions_have_no_server_cancel() {
        let s = memory();
        assert!(
            s.cancel_handle().is_none(),
            "SQLite runs in-process: no backend id, so cancel reconnects"
        );
    }

    #[test]
    fn a_cancel_handle_uses_a_second_short_lived_connection() {
        // SQLite stands in for the server: the handle's statement runs on a
        // fresh connection with the session's URL, and its reply decides.
        let dir = std::env::temp_dir().join(format!("vix-sess-cancel-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let url = format!("sqlite:{}?mode=rwc", dir.join("c.db").display());
        let handle = |sql: &str| CancelHandle {
            url: url.clone(),
            sql: sql.to_string(),
        };
        assert!(matches!(handle("SELECT 1").send(), Ok(true)));
        assert!(
            matches!(handle("SELECT 0").send(), Ok(false)),
            "pg_cancel_backend answering false is a refused cancel"
        );
        assert!(matches!(
            handle("NOT VALID SQL").send(),
            Err(SessionError::Query(_))
        ));
        let missing = CancelHandle {
            url: "sqlite:/definitely/missing/dir/x.db".into(),
            sql: "SELECT 1".into(),
        };
        assert!(matches!(missing.send(), Err(SessionError::Connect { .. })));
        let shown = format!(
            "{:?}",
            CancelHandle {
                url: "postgres://u:sup3rsecret@h/d".into(),
                sql: "SELECT 1".into(),
            }
        );
        assert!(
            !shown.contains("sup3rsecret"),
            "debug leaks the URL: {shown}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_statements_are_recognised_by_sqlstate() {
        let with_code = |code: Option<&str>| QueryError {
            code: code.map(str::to_string),
            ..QueryError::other("x")
        };
        assert!(
            with_code(Some("57014")).is_cancelled(),
            "PostgreSQL query_canceled"
        );
        assert!(with_code(Some("70100")).is_cancelled(), "MySQL KILL QUERY");
        assert!(!with_code(Some("42601")).is_cancelled());
        assert!(!with_code(None).is_cancelled());
    }

    #[test]
    fn sqlite_errors_carry_no_position() {
        let mut s = memory();
        let _ = send_and_expect_err(&mut s, "SELECT * FROM nope");
        assert_eq!(
            s.error_position(),
            None,
            "only PostgreSQL reports a position"
        );
        assert_eq!(
            error_position_of(&sqlx::Error::RowNotFound),
            None,
            "not a database error"
        );
    }

    #[test]
    fn query_error_displays_as_its_message() {
        // Every existing "just show the text" call site relies on this.
        let err = QueryError::other("boom");
        assert_eq!(err.to_string(), "boom");
    }
}
