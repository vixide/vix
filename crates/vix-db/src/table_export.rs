//! Whole-table export from one consistent snapshot (T784).
//!
//! The results-grid export (`e`, [`crate::export`]) writes the rows already
//! loaded. This module exports *whole tables* chosen in the schema tree: as
//! CSV, JSON or NDJSON with one file per table in a directory, or as one SQL
//! file of `INSERT` statements for every chosen table, ordered so parents come
//! before the children that reference them, with sequences / auto-increment
//! counters reset at the end.
//!
//! # How it runs
//!
//! - **A second connection.** The export opens its own connection from the
//!   workbench session's URL, read-only, on its own worker thread. The user's
//!   session — and any transaction open in it — is never touched, and the
//!   workbench stays usable while a large export runs. The export therefore
//!   sees committed data only (not the user's uncommitted changes), and an
//!   in-memory `SQLite` database, which a second connection cannot see, is
//!   refused.
//! - **One snapshot.** Every table is read inside one read-only transaction
//!   ([`snapshot_begin_sql`]): `REPEATABLE READ READ ONLY` on `PostgreSQL`,
//!   `WITH CONSISTENT SNAPSHOT, READ ONLY` on `MySQL`, a plain `BEGIN` on
//!   `SQLite`. It ends with `COMMIT` (or `ROLLBACK` after a failure).
//! - **Streamed.** Each table is one `SELECT` whose rows are written to the
//!   file as the driver yields them (no row cap, unlike the grid's
//!   [`crate::session::MAX_ROWS`]), so a large table never sits in memory;
//!   progress is reported every [`PROGRESS_EVERY`] rows. Files are written as
//!   `<name>.part` and renamed when complete.
//!
//! The pure pieces — foreign-key ordering ([`fk_order`]), the per-engine SQL
//! ([`snapshot_begin_sql`], [`column_names_sql`], [`sequences_sql`],
//! [`sequence_reset_sql`], [`insert_sql`]) and the row renderers — are unit
//! tested here; the worker is tested end to end on `SQLite`. Owning spec:
//! `crates/vix-db/spec/index.md` ("Table export").

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use futures_util::StreamExt as _;
use sqlx::Row as _;
use vix_input::{KeyCode, KeyEvent};

use crate::catalog::{Folder, quote_ident};
use crate::connect::Kind;
use crate::export::{Format, csv_cell};
use crate::{Browser, Outcome, View, catalog, connect, session};

/// The formats a table export offers, in the order the dialog cycles them.
pub const TABLE_FORMATS: &[Format] = &[Format::Csv, Format::Json, Format::Ndjson, Format::Sql];

/// Rows between two progress reports from the export worker.
pub const PROGRESS_EVERY: u64 = 512;

/// One exported cell, typed as the driver decoded it, so `NULL`, numbers and
/// booleans survive into JSON and SQL instead of becoming strings.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// SQL `NULL`.
    Null,
    /// An integer.
    Int(i64),
    /// A floating-point number.
    Float(f64),
    /// A boolean.
    Bool(bool),
    /// Text.
    Text(String),
    /// Binary data.
    Bytes(Vec<u8>),
}

impl Value {
    /// The cell as plain text (CSV, file names): `NULL` is empty, binary data
    /// is decoded as lossy UTF-8 (as the results grid shows it).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Text(s) => s.clone(),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }

    /// The cell as a JSON value: `null`, a number, a boolean or a string
    /// (binary data as lossy UTF-8; a non-finite float as its text).
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Int(i) => serde_json::Value::from(*i),
            Value::Float(f) => serde_json::Number::from_f64(*f)
                .map_or_else(|| serde_json::Value::from(f.to_string()), Into::into),
            Value::Bool(b) => serde_json::Value::from(*b),
            Value::Text(_) | Value::Bytes(_) => serde_json::Value::from(self.text()),
        }
    }
}

/// The order to write tables in, and whether the foreign keys among them
/// form a cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FkOrder {
    /// Indices into the table list, parents before children.
    pub order: Vec<usize>,
    /// The foreign keys form a cycle, so `order` is the given order.
    pub cycle: bool,
}

/// Order `tables` so every table comes after the tables it references.
///
/// `edges` are `(child, parent)` table names (as [`catalog::relationships_sql`]
/// lists them); edges to tables outside the list and self-references are
/// ignored. Among the tables that are ready, the earliest in the given order
/// goes first, so the result is stable. When the edges form a cycle there is
/// no valid order, and the given order is returned with `cycle` set.
///
/// # Examples
///
/// ```
/// use vix_db::table_export::fk_order;
/// let tables = ["orders".to_string(), "users".to_string()];
/// let edges = [("orders".to_string(), "users".to_string())];
/// let order = fk_order(&tables, &edges);
/// assert_eq!(order.order, [1, 0]); // users before orders
/// assert!(!order.cycle);
/// ```
#[must_use]
pub fn fk_order(tables: &[String], edges: &[(String, String)]) -> FkOrder {
    let n = tables.len();
    let index = |name: &str| tables.iter().position(|t| t == name);
    let mut waiting_on = vec![0usize; n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut seen = std::collections::HashSet::new();
    for (child, parent) in edges {
        let (Some(c), Some(p)) = (index(child), index(parent)) else {
            continue;
        };
        // A self-reference does not constrain the table order, and a
        // composite key lists one edge per column: count each pair once.
        if c == p || !seen.insert((c, p)) {
            continue;
        }
        waiting_on[c] += 1;
        children[p].push(c);
    }
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while order.len() < n {
        let Some(next) = (0..n).find(|&i| !done[i] && waiting_on[i] == 0) else {
            return FkOrder {
                order: (0..n).collect(),
                cycle: true,
            };
        };
        done[next] = true;
        order.push(next);
        for &c in &children[next] {
            waiting_on[c] -= 1;
        }
    }
    FkOrder {
        order,
        cycle: false,
    }
}

/// The statements that open the export's read-only snapshot transaction.
///
/// `MySQL` sets `REPEATABLE READ` for the next transaction first, since a
/// consistent snapshot needs it whatever the server default is.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::snapshot_begin_sql};
/// assert_eq!(
///     snapshot_begin_sql(Kind::Postgres),
///     ["BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"]
/// );
/// assert_eq!(snapshot_begin_sql(Kind::Sqlite), ["BEGIN"]);
/// ```
#[must_use]
pub fn snapshot_begin_sql(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Postgres => &["BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"],
        Kind::Mysql => &[
            "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
        ],
        Kind::Sqlite => &["BEGIN"],
    }
}

/// The statement that ends a finished snapshot (a failed one is rolled back).
pub const SNAPSHOT_END_SQL: &str = "COMMIT";

/// Escape `s` for a single-quoted SQL literal (doubling `'`).
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The table a `SELECT` reads: schema-qualified on the server engines.
fn source(kind: Kind, schema: &str, table: &str) -> String {
    match kind {
        Kind::Sqlite => quote_ident(kind, table),
        Kind::Postgres | Kind::Mysql => {
            format!("{}.{}", quote_ident(kind, schema), quote_ident(kind, table))
        }
    }
}

/// The table an exported `INSERT` writes: schema-qualified on `PostgreSQL`
/// only. A `MySQL` schema is the database itself, so the dump stays loadable
/// into a database of another name.
fn target(kind: Kind, schema: &str, table: &str) -> String {
    match kind {
        Kind::Postgres => source(kind, schema, table),
        Kind::Sqlite | Kind::Mysql => quote_ident(kind, table),
    }
}

/// A query listing the column names of `schema`.`table` in declaration
/// order. With `insertable`, generated columns are left out (an `INSERT`
/// cannot set them).
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::column_names_sql};
/// let sql = column_names_sql(Kind::Sqlite, "main", "users", true);
/// assert!(sql.contains("pragma_table_xinfo('users')"));
/// ```
#[must_use]
pub fn column_names_sql(kind: Kind, schema: &str, table: &str, insertable: bool) -> String {
    let (s, t) = (literal(schema), literal(table));
    match kind {
        // `hidden` is 1 for a virtual table's hidden column, 2/3 for a
        // generated one.
        Kind::Sqlite => {
            let keep = if insertable { "= 0" } else { "<> 1" };
            format!("SELECT name FROM pragma_table_xinfo({t}) WHERE hidden {keep} ORDER BY cid;")
        }
        Kind::Postgres => {
            let generated = if insertable {
                " AND is_generated = 'NEVER'"
            } else {
                ""
            };
            format!(
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_schema = {s} AND table_name = {t}{generated} \
                 ORDER BY ordinal_position;"
            )
        }
        Kind::Mysql => {
            let generated = if insertable {
                " AND extra NOT LIKE '%GENERATED%'"
            } else {
                ""
            };
            format!(
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_schema = {s} AND table_name = {t}{generated} \
                 ORDER BY ordinal_position;"
            )
        }
    }
}

/// The `SELECT` that streams every row of `schema`.`table`.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::select_sql};
/// let cols = ["id".to_string(), "name".to_string()];
/// assert_eq!(
///     select_sql(Kind::Postgres, "public", "users", &cols),
///     "SELECT \"id\", \"name\" FROM \"public\".\"users\""
/// );
/// ```
#[must_use]
pub fn select_sql(kind: Kind, schema: &str, table: &str, columns: &[String]) -> String {
    let cols: Vec<String> = columns.iter().map(|c| quote_ident(kind, c)).collect();
    format!(
        "SELECT {} FROM {}",
        cols.join(", "),
        source(kind, schema, table)
    )
}

/// A query reading what the sequence reset of `schema`.`table` needs, run
/// inside the snapshot: on `PostgreSQL` the `(column, sequence)` of each
/// owned sequence (serial and identity columns), on `MySQL` the table's next
/// `AUTO_INCREMENT` value, on `SQLite` its `sqlite_sequence` counter (that
/// query fails when the database has no `AUTOINCREMENT` table, which simply
/// means there is nothing to reset).
#[must_use]
pub fn sequences_sql(kind: Kind, schema: &str, table: &str) -> String {
    match kind {
        Kind::Postgres => {
            let qualified = literal(&source(kind, schema, table));
            format!(
                "SELECT a.attname, pg_get_serial_sequence({qualified}, a.attname) \
                 FROM pg_attribute a WHERE a.attrelid = {qualified}::regclass \
                 AND a.attnum > 0 AND NOT a.attisdropped \
                 AND pg_get_serial_sequence({qualified}, a.attname) IS NOT NULL \
                 ORDER BY a.attnum;"
            )
        }
        Kind::Mysql => format!(
            "SELECT AUTO_INCREMENT FROM information_schema.tables \
             WHERE table_schema = {} AND table_name = {} AND AUTO_INCREMENT IS NOT NULL;",
            literal(schema),
            literal(table)
        ),
        Kind::Sqlite => format!(
            "SELECT seq FROM sqlite_sequence WHERE name = {};",
            literal(table)
        ),
    }
}

/// The statements that reset `schema`.`table`'s sequences after its rows are
/// loaded, from the rows [`sequences_sql`] returned.
///
/// `PostgreSQL` sets each owned sequence to the loaded column's maximum
/// (`setval`, so the next value follows it; an empty table restarts at 1).
/// `MySQL` restores the source's `AUTO_INCREMENT`, and `SQLite` its
/// `sqlite_sequence` counter, which may be higher than the largest loaded id.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::sequence_reset_sql};
/// let rows = vec![vec!["42".to_string()]];
/// assert_eq!(
///     sequence_reset_sql(Kind::Mysql, "app", "users", &rows),
///     ["ALTER TABLE `users` AUTO_INCREMENT = 42;"]
/// );
/// ```
#[must_use]
pub fn sequence_reset_sql(
    kind: Kind,
    schema: &str,
    table: &str,
    rows: &[Vec<String>],
) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        match kind {
            Kind::Postgres => {
                let (Some(column), Some(sequence)) = (row.first(), row.get(1)) else {
                    continue;
                };
                if sequence.is_empty() {
                    continue;
                }
                let col = quote_ident(kind, column);
                out.push(format!(
                    "SELECT setval({}, COALESCE(MAX({col}), 1), MAX({col}) IS NOT NULL) FROM {};",
                    literal(sequence),
                    target(kind, schema, table)
                ));
            }
            Kind::Mysql => {
                let Some(next) = row.first().and_then(|v| v.trim().parse::<u64>().ok()) else {
                    continue;
                };
                out.push(format!(
                    "ALTER TABLE {} AUTO_INCREMENT = {next};",
                    target(kind, schema, table)
                ));
            }
            Kind::Sqlite => {
                let Some(seq) = row.first().and_then(|v| v.trim().parse::<i64>().ok()) else {
                    continue;
                };
                let name = literal(table);
                out.push(format!("DELETE FROM sqlite_sequence WHERE name = {name};"));
                out.push(format!(
                    "INSERT INTO sqlite_sequence (name, seq) VALUES ({name}, {seq});"
                ));
            }
        }
    }
    out
}

/// `value` as a SQL literal for `kind`.
///
/// Text is single-quoted with `'` doubled (and `\` doubled on `MySQL`, where a
/// backslash escapes in string literals by default); binary data is a hex
/// literal (`'\x…'::bytea` on `PostgreSQL`, `X'…'` elsewhere); a non-finite
/// float is quoted text (`'NaN'`, `'inf'`), which `PostgreSQL` reads back.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::{sql_literal, Value}};
/// assert_eq!(sql_literal(Kind::Sqlite, &Value::Null), "NULL");
/// assert_eq!(sql_literal(Kind::Sqlite, &Value::Text("O'Brien".into())), "'O''Brien'");
/// assert_eq!(sql_literal(Kind::Mysql, &Value::Text("a\\b".into())), "'a\\\\b'");
/// assert_eq!(sql_literal(Kind::Sqlite, &Value::Bytes(vec![0xab, 1])), "X'ab01'");
/// ```
#[must_use]
pub fn sql_literal(kind: Kind, value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.is_finite() => f.to_string(),
        Value::Float(f) => literal(&f.to_string()),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Value::Text(s) if kind == Kind::Mysql => literal(&s.replace('\\', "\\\\")),
        Value::Text(s) => literal(s),
        Value::Bytes(b) => {
            let hex = b.iter().fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
            match kind {
                Kind::Postgres => format!("'\\x{hex}'::bytea"),
                Kind::Sqlite | Kind::Mysql => format!("X'{hex}'"),
            }
        }
    }
}

/// One `INSERT` statement for a row of `schema`.`table`.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, table_export::{insert_sql, Value}};
/// let cols = ["id".to_string(), "name".to_string()];
/// let row = [Value::Int(1), Value::Null];
/// assert_eq!(
///     insert_sql(Kind::Sqlite, "main", "users", &cols, &row),
///     "INSERT INTO \"users\" (\"id\", \"name\") VALUES (1, NULL);"
/// );
/// ```
#[must_use]
pub fn insert_sql(
    kind: Kind,
    schema: &str,
    table: &str,
    columns: &[String],
    values: &[Value],
) -> String {
    let cols: Vec<String> = columns.iter().map(|c| quote_ident(kind, c)).collect();
    let vals: Vec<String> = values.iter().map(|v| sql_literal(kind, v)).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        target(kind, schema, table),
        cols.join(", "),
        vals.join(", ")
    )
}

/// One CSV line (RFC 4180 quoting, `NULL` as an empty field), without the
/// line break.
#[must_use]
pub fn csv_line(values: &[Value]) -> String {
    let cells: Vec<String> = values.iter().map(|v| csv_cell(&v.text())).collect();
    cells.join(",")
}

/// One row as a JSON object keyed by column name.
#[must_use]
pub fn json_object(columns: &[String], values: &[Value]) -> serde_json::Value {
    columns
        .iter()
        .zip(values)
        .map(|(c, v)| (c.clone(), v.json()))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into()
}

/// The file name a per-table export writes for `schema`.`table`:
/// `table.ext`, or `schema.table.ext` when the export spans schemas
/// (`qualify`). Characters other than letters, digits, `-`, `_` and `.` become
/// `_`, so a name can never leave the chosen directory.
///
/// # Examples
///
/// ```
/// use vix_db::{export::Format, table_export::file_name};
/// assert_eq!(file_name("public", "users", false, Format::Csv), "users.csv");
/// assert_eq!(file_name("public", "a/b", true, Format::Ndjson), "public.a_b.ndjson");
/// ```
#[must_use]
pub fn file_name(schema: &str, table: &str, qualify: bool, format: Format) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    let stem = if qualify {
        format!("{}.{}", clean(schema), clean(table))
    } else {
        clean(table)
    };
    format!("{stem}.{}", format.label())
}

/// The table-export dialog: the tables chosen in the tree, the format and
/// the destination (a directory for the per-table formats, a file for SQL).
/// Shown on [`View::Export`] while [`Browser::table_export`] is `Some`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Dialog {
    /// The chosen `(schema, table)` pairs, in tree order.
    pub tables: Vec<(String, String)>,
    /// Selected index into [`TABLE_FORMATS`].
    pub format: usize,
    /// The destination being typed.
    pub path: String,
}

impl Dialog {
    /// The selected format.
    #[must_use]
    pub fn format(&self) -> Format {
        TABLE_FORMATS[self.format % TABLE_FORMATS.len()]
    }

    /// Whether the destination is a single file (SQL) rather than a
    /// directory of per-table files.
    #[must_use]
    pub fn single_file(&self) -> bool {
        self.format() == Format::Sql
    }
}

/// The default destination for `format`.
fn default_path(format: Format) -> String {
    if format == Format::Sql {
        "vix-export.sql".to_string()
    } else {
        "vix-export".to_string()
    }
}

/// Everything the worker needs, moved onto its thread.
struct Job {
    /// The engine.
    kind: Kind,
    /// The workbench session's URL (holds the in-memory password).
    url: String,
    /// Setup statements (the read-only pragma).
    setup: Vec<String>,
    /// The chosen `(schema, table)` pairs.
    tables: Vec<(String, String)>,
    /// The output format.
    format: Format,
    /// The destination file (SQL) or directory.
    dest: PathBuf,
}

/// Progress reported by the worker.
enum Event {
    /// Started table `index` (zero-based) of the export.
    Table {
        /// Zero-based position in the write order.
        index: usize,
        /// The table's name.
        name: String,
    },
    /// Rows written so far, across all tables.
    Rows(u64),
    /// The export finished.
    Done {
        /// Rows written.
        rows: u64,
        /// The foreign keys formed a cycle (SQL only).
        cycle: bool,
    },
    /// The export failed; the message is display-ready.
    Failed(String),
}

/// Why the worker stopped early.
#[derive(Debug)]
enum ExportError {
    /// A database error (credentials already redacted).
    Db(String),
    /// Writing the file failed.
    Io(std::io::Error),
    /// The table has no columns the export can read.
    NoColumns(String),
    /// The workbench went away (the receiver was dropped).
    Cancelled,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(message) => f.write_str(message),
            Self::Io(e) => e.fmt(f),
            Self::NoColumns(table) => {
                f.write_str(&t!("msg.db_table_export_no_columns", table = table))
            }
            Self::Cancelled => f.write_str(&t!("msg.db_cancelled")),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<sqlx::Error> for ExportError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

/// A running export, polled by [`Browser::poll_table_export`].
pub(crate) struct Pending {
    /// Worker events.
    rx: mpsc::Receiver<Event>,
    /// The destination, for the messages.
    dest: String,
    /// How many tables are being exported.
    total: usize,
    /// The table being written, and its one-based position.
    current: (String, usize),
    /// Rows written so far.
    rows: u64,
}

/// Start the export on its own thread.
fn spawn(job: Job) -> std::io::Result<mpsc::Receiver<Event>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("vix-db-export".into())
        .spawn(move || {
            let result = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(run(&job, &tx)),
                Err(e) => Err(ExportError::Io(e)),
            };
            let _ = tx.send(match result {
                Ok((rows, cycle)) => Event::Done { rows, cycle },
                Err(e) => Event::Failed(session::redact_url_secret(&e.to_string(), &job.url)),
            });
        })?;
    Ok(rx)
}

/// Execute one statement for its effect.
async fn exec(conn: &mut sqlx::AnyConnection, sql: &str) -> Result<(), ExportError> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(conn)
        .await?;
    Ok(())
}

/// Run a small helper query and return its rows as text.
async fn rows_of(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
) -> Result<Vec<Vec<String>>, ExportError> {
    let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_all(conn)
        .await?;
    Ok(rows
        .iter()
        .map(|row| (0..row.len()).map(|i| value(row, i).text()).collect())
        .collect())
}

/// Decode one cell, trying the `Any` driver's types widest-first (as the
/// results grid does), but keeping `NULL` and the type.
fn value(row: &sqlx::any::AnyRow, i: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return v.map_or(Value::Null, Value::Int);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(i) {
        return v.map_or(Value::Null, Value::Float);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return v.map_or(Value::Null, Value::Text);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return v.map_or(Value::Null, Value::Bool);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(i) {
        return v.map_or(Value::Null, Value::Bytes);
    }
    Value::Null
}

/// Connect, open the snapshot, write every table, and end the snapshot.
/// Returns the rows written and whether the foreign keys formed a cycle.
async fn run(job: &Job, tx: &mpsc::Sender<Event>) -> Result<(u64, bool), ExportError> {
    session::install_drivers();
    let mut conn = <sqlx::AnyConnection as sqlx::Connection>::connect(&job.url).await?;
    for stmt in &job.setup {
        exec(&mut conn, stmt).await?;
    }
    for stmt in snapshot_begin_sql(job.kind) {
        exec(&mut conn, stmt).await?;
    }
    let outcome = write_all(&mut conn, job, tx).await;
    // A read-only snapshot has nothing to undo; ROLLBACK after a failure
    // simply ends it (on PostgreSQL a failed statement aborted it anyway).
    let end = if outcome.is_ok() {
        SNAPSHOT_END_SQL
    } else {
        "ROLLBACK"
    };
    let ended = exec(&mut conn, end).await;
    let _ = sqlx::Connection::close(conn).await;
    let result = outcome?;
    ended?;
    Ok(result)
}

/// Where a file is written until it is complete.
fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

/// Write every chosen table in the job's format.
async fn write_all(
    conn: &mut sqlx::AnyConnection,
    job: &Job,
    tx: &mpsc::Sender<Event>,
) -> Result<(u64, bool), ExportError> {
    let mut rows = 0u64;
    if job.format == Format::Sql {
        let cycle = write_sql(conn, job, tx, &mut rows).await?;
        return Ok((rows, cycle));
    }
    std::fs::create_dir_all(&job.dest)?;
    let qualify = job.tables.iter().any(|(s, _)| *s != job.tables[0].0);
    for (index, (schema, table)) in job.tables.iter().enumerate() {
        tx.send(Event::Table {
            index,
            name: table.clone(),
        })
        .map_err(|_| ExportError::Cancelled)?;
        let path = job.dest.join(file_name(schema, table, qualify, job.format));
        let part = part_path(&path);
        let written = write_table_file(conn, job, schema, table, &part, tx, &mut rows).await;
        if let Err(e) = written {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, &path)?;
    }
    Ok((rows, false))
}

/// The column names of a table, failing when there are none.
async fn columns(
    conn: &mut sqlx::AnyConnection,
    kind: Kind,
    schema: &str,
    table: &str,
    insertable: bool,
) -> Result<Vec<String>, ExportError> {
    let names: Vec<String> = rows_of(conn, &column_names_sql(kind, schema, table, insertable))
        .await?
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .collect();
    if names.is_empty() {
        return Err(ExportError::NoColumns(table.to_string()));
    }
    Ok(names)
}

/// Stream every row of `sql`, handing each to `each`, counting into `rows`
/// and reporting progress every [`PROGRESS_EVERY`] rows.
async fn stream_rows(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
    tx: &mpsc::Sender<Event>,
    rows: &mut u64,
    mut each: impl FnMut(&[Value]) -> std::io::Result<()>,
) -> Result<(), ExportError> {
    let mut stream = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string())).fetch(conn);
    while let Some(row) = stream.next().await {
        let row = row?;
        let values: Vec<Value> = (0..row.len()).map(|i| value(&row, i)).collect();
        each(&values)?;
        *rows += 1;
        if (*rows).is_multiple_of(PROGRESS_EVERY) {
            tx.send(Event::Rows(*rows))
                .map_err(|_| ExportError::Cancelled)?;
        }
    }
    Ok(())
}

/// Write one table as CSV, JSON or NDJSON to `path`.
async fn write_table_file(
    conn: &mut sqlx::AnyConnection,
    job: &Job,
    schema: &str,
    table: &str,
    path: &Path,
    tx: &mpsc::Sender<Event>,
    rows: &mut u64,
) -> Result<(), ExportError> {
    let cols = columns(conn, job.kind, schema, table, false).await?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    let sql = select_sql(job.kind, schema, table, &cols);
    match job.format {
        Format::Json => {
            out.write_all(b"[")?;
            let mut first = true;
            stream_rows(conn, &sql, tx, rows, |values| {
                let sep: &[u8] = if first { b"\n  " } else { b",\n  " };
                first = false;
                out.write_all(sep)?;
                serde_json::to_writer(&mut out, &json_object(&cols, values))?;
                Ok(())
            })
            .await?;
            out.write_all(if first { b"]\n" } else { b"\n]\n" })?;
        }
        Format::Ndjson => {
            stream_rows(conn, &sql, tx, rows, |values| {
                serde_json::to_writer(&mut out, &json_object(&cols, values))?;
                out.write_all(b"\n")
            })
            .await?;
        }
        _ => {
            let header: Vec<String> = cols.iter().map(|c| csv_cell(c)).collect();
            writeln!(out, "{}", header.join(","))?;
            stream_rows(conn, &sql, tx, rows, |values| {
                writeln!(out, "{}", csv_line(values))
            })
            .await?;
        }
    }
    out.flush()?;
    Ok(())
}

/// Write every table as `INSERT`s to one SQL file, in foreign-key order,
/// with the sequence resets at the end. Returns whether the foreign keys
/// formed a cycle.
async fn write_sql(
    conn: &mut sqlx::AnyConnection,
    job: &Job,
    tx: &mpsc::Sender<Event>,
    rows: &mut u64,
) -> Result<bool, ExportError> {
    let kind = job.kind;
    let names: Vec<String> = job.tables.iter().map(|(_, t)| t.clone()).collect();
    let edges: Vec<(String, String)> = rows_of(conn, catalog::relationships_sql(kind))
        .await?
        .into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| (r[0].clone(), r[2].clone()))
        .collect();
    let order = fk_order(&names, &edges);
    let part = part_path(&job.dest);
    let written = write_sql_file(conn, job, &order, &part, tx, rows).await;
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &job.dest)?;
    Ok(order.cycle)
}

/// The body of [`write_sql`]: header, one block of `INSERT`s per table, the
/// sequence resets, inside `BEGIN` / `COMMIT`. The comments are fixed English,
/// like the SQL around them, so a dump is the same whatever the UI language.
async fn write_sql_file(
    conn: &mut sqlx::AnyConnection,
    job: &Job,
    order: &FkOrder,
    path: &Path,
    tx: &mpsc::Sender<Event>,
    rows: &mut u64,
) -> Result<(), ExportError> {
    let kind = job.kind;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(
        out,
        "-- Vix table export: {} table(s), read from one snapshot.",
        job.tables.len()
    )?;
    if order.cycle {
        writeln!(
            out,
            "-- The foreign keys among these tables form a cycle, so they are written\n\
             -- in the order chosen. Load with foreign-key checks off."
        )?;
    } else {
        writeln!(
            out,
            "-- Tables are ordered so parents come before children."
        )?;
    }
    writeln!(out, "BEGIN;")?;
    let mut resets = Vec::new();
    for (index, &i) in order.order.iter().enumerate() {
        let (schema, table) = &job.tables[i];
        tx.send(Event::Table {
            index,
            name: table.clone(),
        })
        .map_err(|_| ExportError::Cancelled)?;
        let cols = columns(conn, kind, schema, table, true).await?;
        writeln!(out, "\n-- {}", target(kind, schema, table))?;
        let sql = select_sql(kind, schema, table, &cols);
        stream_rows(conn, &sql, tx, rows, |values| {
            writeln!(out, "{}", insert_sql(kind, schema, table, &cols, values))
        })
        .await?;
        let seq = match rows_of(conn, &sequences_sql(kind, schema, table)).await {
            Ok(seq) => seq,
            // No `sqlite_sequence` table: no AUTOINCREMENT counter to reset.
            Err(_) if kind == Kind::Sqlite => Vec::new(),
            Err(e) => return Err(e),
        };
        resets.extend(sequence_reset_sql(kind, schema, table, &seq));
    }
    if !resets.is_empty() {
        writeln!(out, "\n-- Sequences")?;
        for stmt in &resets {
            writeln!(out, "{stmt}")?;
        }
    }
    writeln!(out, "\nCOMMIT;")?;
    out.flush()?;
    Ok(())
}

/// Whether `conn` is an in-memory `SQLite` database, which a second
/// connection would see as a different, empty database.
fn in_memory(conn: &connect::Connection) -> bool {
    let file = conn.file.trim();
    conn.kind == Kind::Sqlite
        && (file.is_empty() || file.contains(":memory:") || file.contains("mode=memory"))
}

impl Browser {
    /// Mark or unmark the table under the tree cursor for a table export
    /// (`m` in the tree). Views and functions are not exported.
    pub fn toggle_export_mark(&mut self) {
        let Some((schema, table, Folder::Tables)) = self.tree.selected_object() else {
            self.message = Some(t!("msg.db_table_export_none").to_string());
            return;
        };
        let key = (schema, table);
        if !self.export_marks.remove(&key) {
            self.export_marks.insert(key);
        }
        self.message =
            Some(t!("msg.db_tables_marked", count = self.export_marks.len()).to_string());
    }

    /// Open the table-export dialog (`e` in the tree) for the marked tables,
    /// or the table under the cursor when none is marked.
    pub fn open_table_export(&mut self) {
        if self.conn.is_none() {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        }
        let tables: Vec<(String, String)> = if self.export_marks.is_empty() {
            match self.tree.selected_object() {
                Some((schema, table, Folder::Tables)) => vec![(schema, table)],
                _ => Vec::new(),
            }
        } else {
            self.export_marks.iter().cloned().collect()
        };
        if tables.is_empty() {
            self.message = Some(t!("msg.db_table_export_none").to_string());
            return;
        }
        let format = self
            .table_export
            .as_ref()
            .map_or(0, |d| d.format % TABLE_FORMATS.len());
        self.table_export = Some(Dialog {
            tables,
            format,
            path: default_path(TABLE_FORMATS[format]),
        });
        self.view = View::Export;
    }

    /// Keys on the table-export dialog: Left/Right choose the format (the
    /// path follows while it is still a default), characters edit the path,
    /// Enter starts the export, Esc closes the dialog.
    pub(crate) fn key_table_export(&mut self, key: KeyEvent) -> Outcome {
        let Some(dialog) = self.table_export.as_mut() else {
            self.view = View::Workbench;
            return Outcome::Consumed;
        };
        let n = TABLE_FORMATS.len();
        match key.code {
            KeyCode::Left | KeyCode::Right => {
                let was_default = dialog.path == default_path(dialog.format());
                dialog.format = if key.code == KeyCode::Left {
                    (dialog.format + n - 1) % n
                } else {
                    (dialog.format + 1) % n
                };
                if was_default {
                    dialog.path = default_path(dialog.format());
                }
            }
            KeyCode::Char(c) => dialog.path.push(c),
            KeyCode::Backspace => {
                dialog.path.pop();
            }
            KeyCode::Enter => self.start_table_export(),
            KeyCode::Esc => {
                self.table_export = None;
                self.view = View::Workbench;
            }
            _ => {}
        }
        Outcome::Consumed
    }

    /// Start the export the dialog describes, on a second connection.
    fn start_table_export(&mut self) {
        if self.pending_export.is_some() {
            self.message = Some(t!("msg.db_table_export_busy").to_string());
            return;
        }
        let (Some(conn), Some(session)) = (self.conn.as_ref(), self.session.as_ref()) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        if in_memory(conn) {
            self.message = Some(t!("msg.db_table_export_memory").to_string());
            return;
        }
        let Some(dialog) = self.table_export.as_ref() else {
            return;
        };
        let path = dialog.path.trim().to_string();
        if path.is_empty() {
            self.message = Some(t!("msg.db_name_required").to_string());
            return;
        }
        let job = Job {
            kind: conn.kind,
            url: session.url().to_string(),
            setup: connect::read_only_sql(conn.kind, true)
                .into_iter()
                .collect(),
            tables: dialog.tables.clone(),
            format: dialog.format(),
            dest: PathBuf::from(&path),
        };
        let total = job.tables.len();
        match spawn(job) {
            Ok(rx) => {
                self.pending_export = Some(Pending {
                    rx,
                    dest: path,
                    total,
                    current: (String::new(), 0),
                    rows: 0,
                });
                self.table_export = None;
                self.view = View::Workbench;
                self.message = Some(t!("msg.db_table_export_started", count = total).to_string());
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    /// Whether a table export is running in the background.
    #[must_use]
    pub fn table_export_running(&self) -> bool {
        self.pending_export.is_some()
    }

    /// Drain the running table export's progress into the status line, and
    /// finish it when the worker is done. Called by the host each tick;
    /// cheap when nothing runs.
    pub fn poll_table_export(&mut self) {
        let Some(pending) = self.pending_export.as_mut() else {
            return;
        };
        let mut progressed = false;
        loop {
            match pending.rx.try_recv() {
                Ok(Event::Table { index, name }) => {
                    pending.current = (name, index + 1);
                    progressed = true;
                }
                Ok(Event::Rows(rows)) => {
                    pending.rows = rows;
                    progressed = true;
                }
                Ok(Event::Done { rows, cycle }) => {
                    let key = if cycle {
                        "msg.db_tables_exported_cycle"
                    } else {
                        "msg.db_tables_exported"
                    };
                    self.message = Some(
                        t!(
                            key,
                            rows = rows,
                            count = pending.total,
                            path = &pending.dest
                        )
                        .to_string(),
                    );
                    self.pending_export = None;
                    return;
                }
                Ok(Event::Failed(error)) => {
                    self.message =
                        Some(t!("msg.db_table_export_failed", error = error).to_string());
                    self.pending_export = None;
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.message = Some(
                        t!(
                            "msg.db_table_export_failed",
                            error = t!("msg.db_not_connected")
                        )
                        .to_string(),
                    );
                    self.pending_export = None;
                    return;
                }
            }
        }
        if progressed {
            self.message = Some(
                t!(
                    "msg.db_table_export_progress",
                    table = &pending.current.0,
                    index = pending.current.1,
                    count = pending.total,
                    rows = pending.rows
                )
                .to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn edge(child: &str, parent: &str) -> (String, String) {
        (child.to_string(), parent.to_string())
    }

    #[test]
    fn fk_order_puts_parents_first_and_is_stable() {
        let tables = names(&["line_items", "orders", "users", "tags"]);
        let edges = [
            edge("line_items", "orders"),
            edge("orders", "users"),
            // a composite key repeats the pair, a self-reference is ignored
            edge("orders", "users"),
            edge("users", "users"),
            // an edge to a table outside the export is ignored
            edge("orders", "regions"),
        ];
        let order = fk_order(&tables, &edges);
        assert!(!order.cycle);
        let named: Vec<&str> = order.order.iter().map(|&i| tables[i].as_str()).collect();
        assert_eq!(named, ["users", "orders", "line_items", "tags"]);
    }

    #[test]
    fn fk_order_falls_back_to_the_given_order_on_a_cycle() {
        let tables = names(&["a", "b", "c"]);
        let edges = [edge("a", "b"), edge("b", "c"), edge("c", "a")];
        let order = fk_order(&tables, &edges);
        assert!(order.cycle);
        assert_eq!(order.order, [0, 1, 2]);
    }

    #[test]
    fn fk_order_of_unrelated_tables_keeps_their_order() {
        let tables = names(&["b", "a"]);
        assert_eq!(fk_order(&tables, &[]).order, [0, 1]);
        assert_eq!(fk_order(&[], &[]).order, [] as [usize; 0]);
    }

    #[test]
    fn snapshot_sql_per_engine() {
        assert_eq!(
            snapshot_begin_sql(Kind::Mysql),
            [
                "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
                "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY"
            ]
        );
        assert_eq!(
            snapshot_begin_sql(Kind::Postgres),
            ["BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"]
        );
        assert_eq!(snapshot_begin_sql(Kind::Sqlite), ["BEGIN"]);
        assert_eq!(SNAPSHOT_END_SQL, "COMMIT");
    }

    #[test]
    fn inserts_quote_per_engine() {
        let cols = names(&["id", "note", "blob", "ok"]);
        let row = [
            Value::Int(7),
            Value::Text("it's \\ fine".into()),
            Value::Bytes(vec![0, 255]),
            Value::Bool(true),
        ];
        assert_eq!(
            insert_sql(Kind::Postgres, "app", "t", &cols, &row),
            "INSERT INTO \"app\".\"t\" (\"id\", \"note\", \"blob\", \"ok\") \
             VALUES (7, 'it''s \\ fine', '\\x00ff'::bytea, TRUE);"
        );
        assert_eq!(
            insert_sql(Kind::Mysql, "app", "t", &cols, &row),
            "INSERT INTO `t` (`id`, `note`, `blob`, `ok`) \
             VALUES (7, 'it''s \\\\ fine', X'00ff', TRUE);"
        );
        assert_eq!(
            insert_sql(Kind::Sqlite, "main", "t", &cols[..1], &[Value::Float(1.5)]),
            "INSERT INTO \"t\" (\"id\") VALUES (1.5);"
        );
        assert_eq!(
            sql_literal(Kind::Postgres, &Value::Float(f64::NAN)),
            "'NaN'"
        );
    }

    #[test]
    fn sequence_resets_per_engine() {
        let pg = sequence_reset_sql(
            Kind::Postgres,
            "public",
            "users",
            &[vec!["id".into(), "public.users_id_seq".into()]],
        );
        assert_eq!(
            pg,
            [
                "SELECT setval('public.users_id_seq', COALESCE(MAX(\"id\"), 1), \
              MAX(\"id\") IS NOT NULL) FROM \"public\".\"users\";"
            ]
        );
        let lite = sequence_reset_sql(Kind::Sqlite, "main", "o'k", &[vec!["9".into()]]);
        assert_eq!(
            lite,
            [
                "DELETE FROM sqlite_sequence WHERE name = 'o''k';",
                "INSERT INTO sqlite_sequence (name, seq) VALUES ('o''k', 9);"
            ]
        );
        assert_eq!(
            sequence_reset_sql(Kind::Mysql, "a", "t", &[vec![String::new()]]),
            Vec::<String>::new()
        );
        assert!(
            sequences_sql(Kind::Postgres, "public", "users")
                .contains("'\"public\".\"users\"'::regclass")
        );
    }

    #[test]
    fn column_queries_leave_out_generated_columns_for_inserts() {
        assert!(
            column_names_sql(Kind::Postgres, "s", "t", true).contains("is_generated = 'NEVER'")
        );
        assert!(!column_names_sql(Kind::Postgres, "s", "t", false).contains("is_generated"));
        assert!(column_names_sql(Kind::Mysql, "s", "t", true).contains("GENERATED"));
        assert!(column_names_sql(Kind::Sqlite, "main", "t", true).contains("hidden = 0"));
        assert!(column_names_sql(Kind::Sqlite, "main", "t", false).contains("hidden <> 1"));
    }

    #[test]
    fn rows_render_as_csv_and_json() {
        let cols = names(&["id", "name", "score"]);
        let row = [Value::Int(1), Value::Text("a,b".into()), Value::Null];
        assert_eq!(csv_line(&row), "1,\"a,b\",");
        assert_eq!(
            json_object(&cols, &row).to_string(),
            r#"{"id":1,"name":"a,b","score":null}"#
        );
        assert_eq!(
            Value::Float(f64::INFINITY).json(),
            serde_json::Value::from("inf")
        );
    }

    #[test]
    fn file_names_cannot_escape_the_directory() {
        assert_eq!(file_name("s", "../x", false, Format::Csv), ".._x.csv");
        assert_eq!(
            file_name("my schema", "t", true, Format::Json),
            "my_schema.t.json"
        );
    }

    /// Seed a SQLite file, returning its path and connection URL.
    fn seeded(name: &str, statements: &[&str]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-tx-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("db.sqlite");
        let mut s =
            session::Session::connect(&format!("sqlite:{}?mode=rwc", file.display()), &[]).unwrap();
        for sql in statements {
            s.run(sql).unwrap();
        }
        (dir, file)
    }

    fn run_job(file: &Path, tables: &[&str], format: Format, dest: PathBuf) -> (u64, bool) {
        let job = Job {
            kind: Kind::Sqlite,
            url: format!("sqlite:{}?mode=rwc", file.display()),
            setup: vec!["PRAGMA query_only = ON".into()],
            tables: tables
                .iter()
                .map(|t| ("main".to_string(), (*t).to_string()))
                .collect(),
            format,
            dest,
        };
        let rx = spawn(job).unwrap();
        let mut progress = 0;
        loop {
            match rx.recv().unwrap() {
                Event::Rows(_) => progress += 1,
                Event::Table { .. } => {}
                Event::Done { rows, cycle } => {
                    assert!(progress > 0 || rows < PROGRESS_EVERY, "batched progress");
                    return (rows, cycle);
                }
                Event::Failed(e) => panic!("export failed: {e}"),
            }
        }
    }

    const CHAIN: &[&str] = &[
        "PRAGMA foreign_keys = ON",
        "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)",
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id), \
         total REAL)",
        "CREATE TABLE items (id INTEGER PRIMARY KEY, order_id INTEGER REFERENCES orders(id), \
         sku TEXT, data BLOB)",
        "INSERT INTO users (name) VALUES ('ada'), (NULL), ('o''hara')",
        "DELETE FROM users WHERE id = 3",
        "INSERT INTO users (name) VALUES ('grace')",
        "INSERT INTO orders VALUES (1, 1, 9.5), (2, 2, NULL)",
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 25000) \
         INSERT INTO items SELECT i, 1 + i % 2, 'sku-' || i, X'00ff' FROM n",
    ];

    #[test]
    fn sql_export_orders_by_foreign_key_streams_every_row_and_reloads() {
        let (dir, file) = seeded("sql", CHAIN);
        let dump = dir.join("dump.sql");
        // Children first in the chosen order: the export must reorder them.
        let (rows, cycle) = run_job(
            &file,
            &["items", "orders", "users"],
            Format::Sql,
            dump.clone(),
        );
        assert!(!cycle);
        assert_eq!(rows, 25_000 + 2 + 3, "no row cap on a whole-table export");
        let text = std::fs::read_to_string(&dump).unwrap();
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("{needle} missing"))
        };
        assert!(at("-- \"users\"") < at("-- \"orders\""));
        assert!(at("-- \"orders\"") < at("-- \"items\""));
        assert!(text.contains("VALUES (2, NULL);"), "NULL survives");
        assert!(text.contains("DELETE FROM sqlite_sequence WHERE name = 'users';"));
        assert!(text.contains("VALUES ('users', 4);"));
        assert!(!dir.join("dump.sql.part").exists());

        // The dump loads into an empty database with the same schema.
        let target = dir.join("copy.sqlite");
        let mut copy =
            session::Session::connect(&format!("sqlite:{}?mode=rwc", target.display()), &[])
                .unwrap();
        for ddl in &CHAIN[1..4] {
            copy.run(ddl).unwrap();
        }
        copy.run("PRAGMA foreign_keys = ON").unwrap();
        copy.run(&text).unwrap();
        let (_, n) = copy.run("SELECT count(*) FROM items").unwrap();
        assert_eq!(n, vec![vec!["25000"]]);
        let (_, seq) = copy
            .run("SELECT seq FROM sqlite_sequence WHERE name = 'users'")
            .unwrap();
        assert_eq!(
            seq,
            vec![vec!["4"]],
            "the counter is restored, past the deleted id"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn per_table_files_in_csv_json_and_ndjson() {
        let (dir, file) = seeded("files", CHAIN);
        let out = dir.join("out");
        let (rows, _) = run_job(&file, &["users", "orders"], Format::Csv, out.clone());
        assert_eq!(rows, 5);
        assert_eq!(
            std::fs::read_to_string(out.join("users.csv")).unwrap(),
            "id,name\n1,ada\n2,\n4,grace\n"
        );
        run_job(&file, &["orders"], Format::Json, out.clone());
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(out.join("orders.json")).unwrap())
                .unwrap();
        assert_eq!(json[0]["total"], 9.5);
        assert_eq!(json[1]["total"], serde_json::Value::Null);
        run_job(&file, &["items"], Format::Ndjson, out.clone());
        let nd = std::fs::read_to_string(out.join("items.ndjson")).unwrap();
        assert_eq!(nd.lines().count(), 25_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_foreign_key_cycle_keeps_the_given_order_and_says_so() {
        let (dir, file) = seeded(
            "cycle",
            &[
                "CREATE TABLE a (id INTEGER PRIMARY KEY, b_id INTEGER REFERENCES b(id))",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, a_id INTEGER REFERENCES a(id))",
                "INSERT INTO a VALUES (1, NULL)",
            ],
        );
        let dump = dir.join("cycle.sql");
        let (_, cycle) = run_job(&file, &["b", "a"], Format::Sql, dump.clone());
        assert!(cycle);
        let text = std::fs::read_to_string(&dump).unwrap();
        assert!(text.contains("form a cycle"));
        assert!(text.find("-- \"b\"").unwrap() < text.find("-- \"a\"").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
