//! The persistent database session: one live sqlx connection per workbench.
//!
//! Where the workbench once spawned a CLI client per statement, a [`Session`]
//! holds a single [`sqlx`] `Any` connection open for the life of the
//! workbench connection — so session state (temporary tables, `search_path`,
//! and above all `BEGIN` / `COMMIT` / `ROLLBACK` transactions) survives from
//! statement to statement.
//!
//! sqlx is async; the TUI is not. The connection lives on a dedicated worker
//! thread that owns a current-thread tokio runtime, and [`Session::run`]
//! exchanges one request for one reply over channels — same blocking
//! semantics the CLI bridge had, no async surface for the caller. Dropping
//! the `Session` closes the channel; the worker notices, closes the
//! connection cleanly, and exits.

use sqlx::{Column as _, Row as _};
use std::sync::mpsc;
use std::sync::Once;

/// A result set: column headers plus stringified rows.
pub type Table = (Vec<String>, Vec<Vec<String>>);

/// One statement for the worker.
enum Request {
    /// Execute the SQL and reply with its rows.
    Run(String),
}

/// The worker's answer to one [`Request`].
type Reply = Result<Table, String>;

/// Registers sqlx's `Any` drivers exactly once per process.
static DRIVERS: Once = Once::new();

/// A live database connection on its own worker thread.
#[derive(Debug)]
pub struct Session {
    /// Statements to the worker.
    req_tx: mpsc::Sender<Request>,
    /// Results back from the worker.
    reply_rx: mpsc::Receiver<Reply>,
}

impl Session {
    /// Open a connection to `url` (see [`crate::db::connect::url`]), blocking
    /// until the handshake finishes.
    ///
    /// # Errors
    ///
    /// Returns the driver's connect error (bad credentials, unreachable
    /// host, unreadable file, …) as a display-ready string.
    pub fn connect(url: &str) -> Result<Session, String> {
        DRIVERS.call_once(sqlx::any::install_default_drivers);
        let (req_tx, req_rx) = mpsc::channel::<Request>();
        let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let url = url.to_string();
        std::thread::Builder::new()
            .name("vix-db-session".into())
            .spawn(move || worker(&url, &ready_tx, &req_rx, &reply_tx))
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Session { req_tx, reply_rx })
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
        self.req_tx.send(Request::Run(sql.to_string())).map_err(|_| lost())?;
        self.reply_rx.recv().map_err(|_| lost())?
    }
}

/// The worker loop: connect, signal readiness, then serve one statement at a
/// time until the request channel closes.
fn worker(
    url: &str,
    ready_tx: &mpsc::Sender<Result<(), String>>,
    req_rx: &mpsc::Receiver<Request>,
    reply_tx: &mpsc::Sender<Reply>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
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
    let _ = ready_tx.send(Ok(()));
    while let Ok(Request::Run(sql)) = req_rx.recv() {
        let reply = rt.block_on(run_sql(&mut conn, &sql));
        if reply_tx.send(reply).is_err() {
            break;
        }
    }
    let _ = rt.block_on(sqlx::Connection::close(conn));
}

/// Execute one statement and stringify its result set. The workbench runs
/// exactly what the user typed, so `AssertSqlSafe` (sqlx's opt-in for
/// dynamic SQL) is the intended usage, not an injection hazard.
async fn run_sql(conn: &mut sqlx::AnyConnection, sql: &str) -> Reply {
    let sql = sqlx::AssertSqlSafe(sql.to_string());
    let rows = sqlx::raw_sql(sql).fetch_all(conn).await.map_err(|e| e.to_string())?;
    let headers: Vec<String> = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|row| (0..row.columns().len()).map(|i| cell(row, i)).collect())
        .collect();
    Ok((headers, data))
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
        return v.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Session {
        Session::connect("sqlite::memory:").expect("in-memory sqlite connects")
    }

    #[test]
    fn state_persists_across_statements() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        s.run("INSERT INTO t VALUES (1, 'ada'), (2, NULL)").unwrap();
        let (headers, rows) = s.run("SELECT a, b FROM t ORDER BY a").unwrap();
        assert_eq!(headers, vec!["a", "b"]);
        assert_eq!(rows, vec![vec!["1", "ada"], vec!["2", ""]], "NULL renders empty");
    }

    #[test]
    fn transactions_survive_between_run_calls() {
        let mut s = memory();
        s.run("CREATE TABLE t (a INTEGER)").unwrap();
        s.run("BEGIN").unwrap();
        s.run("INSERT INTO t VALUES (1)").unwrap();
        s.run("ROLLBACK").unwrap();
        let (_, rows) = s.run("SELECT count(*) FROM t").unwrap();
        assert_eq!(rows, vec![vec!["0"]], "the rollback really undid the insert");
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
        assert!(s.run("SELECT 1").is_ok(), "an error does not kill the session");
    }

    #[test]
    fn connect_failure_is_reported() {
        let err = Session::connect("sqlite:/definitely/missing/dir/x.db").unwrap_err();
        assert!(!err.is_empty());
    }
}
