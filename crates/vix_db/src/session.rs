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
//! so the host can keep the UI responsive (M3/M4). [`Session::restart`]
//! reconnects on a fresh connection to recover from a cancelled query. Dropping
//! the `Session` closes the channel; the worker notices, closes the connection
//! cleanly, and exits.

use futures_util::StreamExt as _;
use sqlx::{Column as _, Row as _};
use std::sync::Once;
use std::sync::mpsc;

/// A result set: column headers plus stringified rows.
pub type Table = (Vec<String>, Vec<Vec<String>>);

/// Rows sent per streamed batch (M4): the grid grows this many rows at a time.
const BATCH: usize = 512;

/// Hard cap on streamed rows for one statement; beyond it the stream stops and
/// the result is flagged truncated so the workbench and memory stay bounded.
pub const MAX_ROWS: usize = 20_000;

/// One statement for the worker.
enum Request {
    /// Execute the SQL and stream its rows back as [`Chunk`]s.
    Run(String),
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
    /// The statement failed, with a display-ready message.
    Err(String),
}

/// Registers sqlx's `Any` drivers exactly once per process.
static DRIVERS: Once = Once::new();

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
}

impl Session {
    /// Open a connection to `url` (see [`crate::connect::url`]), run each
    /// `setup` statement (e.g. a read-only pragma), and block until the
    /// connection is ready.
    ///
    /// # Errors
    ///
    /// Returns the driver's connect error (bad credentials, unreachable
    /// host, unreadable file, …) or a failing setup statement as a
    /// display-ready string.
    pub fn connect(url: &str, setup: &[String]) -> Result<Session, String> {
        DRIVERS.call_once(sqlx::any::install_default_drivers);
        let (req_tx, req_rx) = mpsc::channel::<Request>();
        let (reply_tx, reply_rx) = mpsc::channel::<Chunk>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let url = url.to_string();
        let setup = setup.to_vec();
        let (worker_url, worker_setup) = (url.clone(), setup.clone());
        std::thread::Builder::new()
            .name("vix-db-session".into())
            .spawn(move || worker(&worker_url, &worker_setup, &ready_tx, &req_rx, &reply_tx))
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Session {
            req_tx,
            reply_rx,
            url,
            setup,
        })
    }

    /// Hand the worker a statement without waiting for its reply — the async
    /// counterpart of [`run`](Session::run), drained by [`poll`](Session::poll).
    ///
    /// # Errors
    ///
    /// Returns a disconnected message if the worker died.
    pub fn send(&self, sql: &str) -> Result<(), String> {
        let lost = || t!("msg.db_not_connected").to_string();
        self.req_tx
            .send(Request::Run(sql.to_string()))
            .map_err(|_| lost())
    }

    /// The next streamed [`Chunk`] from a [`send`](Session::send), or `None`
    /// while the worker has produced nothing new this moment.
    #[must_use]
    pub fn poll(&self) -> Option<Chunk> {
        match self.reply_rx.try_recv() {
            Ok(chunk) => Some(chunk),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Chunk::Err(t!("msg.db_not_connected").to_string()))
            }
        }
    }

    /// Abandon the current worker (its in-flight statement's result is dropped
    /// when it finally arrives) and reconnect on a fresh connection — how a
    /// cancelled query returns control to the UI. Transaction state is lost.
    ///
    /// # Errors
    ///
    /// Returns the reconnect error if the new connection cannot be opened.
    pub fn restart(&mut self) -> Result<(), String> {
        let (url, setup) = (self.url.clone(), self.setup.clone());
        *self = Session::connect(&url, &setup)?;
        Ok(())
    }

    /// Execute `sql` on the held connection and return its rows (headers are
    /// empty when the statement returns no rows).
    ///
    /// # Errors
    ///
    /// Returns the database error as a display-ready string, or a
    /// disconnected message if the worker died.
    pub fn run(&mut self, sql: &str) -> Result<Table, String> {
        let lost = || t!("msg.db_not_connected").to_string();
        self.send(sql)?;
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        loop {
            match self.reply_rx.recv().map_err(|_| lost())? {
                Chunk::Head(h) => headers = h,
                Chunk::Rows(mut batch) => rows.append(&mut batch),
                Chunk::Done(_) => return Ok((headers, rows)),
                Chunk::Err(e) => return Err(e),
            }
        }
    }
}

/// The worker loop: connect, signal readiness, then serve one statement at a
/// time until the request channel closes.
fn worker(
    url: &str,
    setup: &[String],
    ready_tx: &mpsc::Sender<Result<(), String>>,
    req_rx: &mpsc::Receiver<Request>,
    reply_tx: &mpsc::Sender<Chunk>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = ready_tx.send(Err(e.to_string()));
            return;
        }
    };
    let mut conn = match rt.block_on(<sqlx::AnyConnection as sqlx::Connection>::connect(url)) {
        Ok(conn) => conn,
        Err(e) => {
            let _ = ready_tx.send(Err(e.to_string()));
            return;
        }
    };
    for stmt in setup {
        if let Err(e) = rt.block_on(exec_one(&mut conn, stmt)) {
            let _ = ready_tx.send(Err(e));
            let _ = rt.block_on(sqlx::Connection::close(conn));
            return;
        }
    }
    let _ = ready_tx.send(Ok(()));
    while let Ok(Request::Run(sql)) = req_rx.recv() {
        if !rt.block_on(stream_sql(&mut conn, &sql, reply_tx)) {
            break; // the consumer went away (a cancel abandoned this worker)
        }
    }
    let _ = rt.block_on(sqlx::Connection::close(conn));
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

/// Stream one statement's rows back as [`Chunk`]s: a [`Chunk::Head`], then
/// [`Chunk::Rows`] batches of up to [`BATCH`], then [`Chunk::Done`] (flagged
/// truncated past [`MAX_ROWS`]) — or a [`Chunk::Err`]. Returns `false` when the
/// consumer's channel has closed, so the worker can stop. `AssertSqlSafe` is
/// the intended opt-in for the workbench's user SQL, not an injection hazard.
async fn stream_sql(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
    reply_tx: &mpsc::Sender<Chunk>,
) -> bool {
    let query = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()));
    let mut stream = query.fetch(conn);
    let mut head_sent = false;
    let mut batch: Vec<Vec<String>> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    while let Some(item) = stream.next().await {
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
            Err(e) => return reply_tx.send(Chunk::Err(e.to_string())).is_ok(),
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
    fn errors_come_back_as_strings_and_the_session_survives() {
        let mut s = memory();
        let err = s.run("SELECT * FROM nope").unwrap_err();
        assert!(err.contains("nope"), "driver error is surfaced: {err}");
        assert!(
            s.run("SELECT 1").is_ok(),
            "an error does not kill the session"
        );
    }

    #[test]
    fn connect_failure_is_reported() {
        let err = Session::connect("sqlite:/definitely/missing/dir/x.db", &[]).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn setup_pragma_makes_the_session_read_only() {
        let setup = vec!["PRAGMA query_only = ON".to_string()];
        let mut s = Session::connect("sqlite::memory:", &setup).expect("connects read-only");
        let err = s.run("CREATE TABLE t (a INTEGER)").unwrap_err();
        assert!(
            !err.is_empty(),
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
    fn a_failing_setup_statement_fails_the_connect() {
        let err = Session::connect("sqlite::memory:", &["NOT VALID SQL".to_string()]).unwrap_err();
        assert!(
            !err.is_empty(),
            "a bad setup statement surfaces as a connect error"
        );
    }
}
