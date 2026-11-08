//! Saved connections shared with MCP clients (T783): the read-only side of
//! `vix --mcp`'s `db_tables`, `db_describe` and `db_query` tools.
//!
//! The user marks a saved connection shared on the workbench's connection
//! form (stored by name in the `[db_mcp_shared]` settings table, like the
//! T781 environments). For such a connection this module:
//!
//! - **checks a query** before anything is opened ([`check_query`], pure):
//!   exactly one statement, starting with `SELECT`, `WITH`, `EXPLAIN`, `SHOW`
//!   or `VALUES`, with no write keyword ([`crate::editor::is_write_statement`]),
//!   no `ANALYZE`, and none of [`REFUSED_KEYWORDS`];
//! - **finds credentials without prompting**: the connection's
//!   `password_command`, then the OS keyring ([`crate::secret::resolve`]); a
//!   server connection with neither fails with [`ShareError::NeedsPassword`];
//! - **tunnels without prompting**: an SSH tunnel, when the connection has
//!   one, is opened with `BatchMode=yes` ([`crate::tunnel::open_batch`]);
//! - **reads inside a read-only transaction** ([`opening_sql`], then
//!   [`CLOSING_SQL`]): a read-only session and a read-only transaction with a
//!   30-second statement timeout, rolled back at the end, so even a write
//!   hidden in a function is refused by the engine and nothing persists;
//! - **caps the result** at [`ROW_LIMIT`] rows for a query and
//!   [`CATALOG_ROW_LIMIT`] for a catalog read, saying when rows were cut off.
//!
//! Every call opens its own connection and closes it afterwards; nothing is
//! kept between calls. Messages are English protocol text for an agent, not
//! UI (see `spec/mcp-server`), so they do not go through `t!` — except the
//! SSH tunnel's own errors, which are the workbench's translated ones.
//! Owning specs: `spec/mcp-server` and `crates/vix-db/spec/index.md`.

use futures_util::StreamExt as _;
use std::time::Duration;

use crate::connect::{self, Connection, Kind};
use crate::{catalog, editor, secret, session, tunnel};

/// The most rows `db_query` returns; more are cut off and flagged.
pub const ROW_LIMIT: usize = 100;

/// The most rows one catalog read (`db_tables`, `db_describe`) returns.
pub const CATALOG_ROW_LIMIT: usize = 10_000;

/// The statement timeout, in seconds. `PostgreSQL` and `MySQL` enforce it on
/// the server; for `SQLite` it is a client-side deadline (see [`opening_sql`]).
pub const STATEMENT_TIMEOUT_SECS: u64 = 30;

/// Extra time a server engine gets past [`STATEMENT_TIMEOUT_SECS`] before
/// the client gives up on its own, so the server's timeout fires first and
/// its error is the one reported.
const SERVER_GRACE: Duration = Duration::from_secs(5);

/// How long opening the connection may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// The first words a shared query may start with.
pub const READ_STATEMENTS: &[&str] = &["select", "with", "explain", "show", "values"];

/// Words refused anywhere in a shared query (outside strings and comments),
/// beyond the write keywords: `INTO` (`SELECT … INTO` creates a table, and
/// `MySQL`'s `INTO OUTFILE` writes a file), the locking clauses (`FOR SHARE`,
/// `LOCK IN SHARE MODE`; `FOR UPDATE` is already a write keyword), and
/// transaction control, which would end the read-only transaction early.
pub const REFUSED_KEYWORDS: &[&str] = &["into", "lock", "share", "commit", "rollback", "savepoint"];

/// The statement that ends every shared read: nothing is ever committed.
pub const CLOSING_SQL: &str = "ROLLBACK";

/// Whether a saved connection is shared with MCP clients: the value of the
/// connection form's **Share with MCP** row
/// ([`crate::Browser::form_mcp_sharing`]). Off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Sharing {
    /// Not shared: the connection does not exist for MCP clients.
    #[default]
    Private,
    /// Shared, read-only, with `vix --mcp`'s clients.
    Shared,
}

impl Sharing {
    /// The other value (the form row's toggle).
    #[must_use]
    pub fn toggled(self) -> Sharing {
        match self {
            Sharing::Private => Sharing::Shared,
            Sharing::Shared => Sharing::Private,
        }
    }

    /// Whether this is [`Sharing::Shared`].
    #[must_use]
    pub fn is_shared(self) -> bool {
        self == Sharing::Shared
    }
}

/// Why [`check_query`] refused a query.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// The text holds no statement.
    Empty,
    /// The text holds this many statements; exactly one is allowed.
    Several(usize),
    /// The statement starts with this word, which is not a read.
    NotARead(String),
    /// `EXPLAIN ANALYZE` (or `ANALYZE` anywhere) runs the statement.
    Analyze,
    /// The statement holds this write keyword.
    Write(String),
    /// The statement holds this keyword from [`REFUSED_KEYWORDS`].
    Keyword(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("no SQL statement given"),
            Self::Several(n) => write!(
                f,
                "db_query runs exactly one statement; this text holds {n}"
            ),
            Self::NotARead(word) => write!(
                f,
                "db_query runs only SELECT, WITH, EXPLAIN, SHOW or VALUES statements, not {}",
                word.to_ascii_uppercase()
            ),
            Self::Analyze => {
                f.write_str("ANALYZE runs the statement, so it is refused; use plain EXPLAIN")
            }
            Self::Write(word) => write!(
                f,
                "refused: {} can write or change the schema, and shared connections are read-only",
                word.to_ascii_uppercase()
            ),
            Self::Keyword(word) => write!(
                f,
                "refused: {} is not allowed in a shared read-only query",
                word.to_ascii_uppercase()
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// Why a shared read failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum ShareError {
    /// [`check_query`] refused the statement; nothing was opened.
    Refused(Refusal),
    /// The server connection needs a password that only a person could type:
    /// it has no `password_command` that prints one and no keyring entry.
    NeedsPassword {
        /// The connection's name.
        connection: String,
    },
    /// The SSH tunnel could not be opened.
    Tunnel(tunnel::TunnelError),
    /// The connection could not be opened (credentials redacted).
    Connect(String),
    /// A statement failed on the server.
    Query(String),
    /// The read ran past the statement timeout.
    Timeout,
    /// No such table (or view) in the schema.
    NoSuchTable {
        /// The schema searched.
        schema: String,
        /// The table asked for.
        table: String,
    },
    /// The async runtime could not be started.
    Runtime(std::io::Error),
}

impl std::fmt::Display for ShareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(why) => why.fmt(f),
            Self::NeedsPassword { connection } => write!(
                f,
                "connection {connection} needs a password, and MCP clients cannot be asked for \
                 one: give it a password command, or connect once in the Vix DB workbench with \
                 Save to keyring on"
            ),
            Self::Tunnel(e) => write!(f, "SSH tunnel failed: {e}"),
            Self::Connect(message) => write!(f, "cannot connect: {message}"),
            Self::Query(message) => f.write_str(message),
            Self::Timeout => write!(
                f,
                "the statement ran longer than {STATEMENT_TIMEOUT_SECS} seconds and was stopped"
            ),
            Self::NoSuchTable { schema, table } => {
                if schema.is_empty() {
                    write!(f, "no table or view named {table}")
                } else {
                    write!(f, "no table or view named {table} in schema {schema}")
                }
            }
            Self::Runtime(e) => write!(f, "cannot start the database runtime: {e}"),
        }
    }
}

impl std::error::Error for ShareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(e) => Some(e),
            Self::Tunnel(e) => Some(e),
            Self::Runtime(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Refusal> for ShareError {
    fn from(e: Refusal) -> Self {
        Self::Refused(e)
    }
}

/// A capped result: column names, rows as text (`NULL` is empty, as in the
/// workbench grid), and whether rows were cut off at the limit.
///
/// Column names come from the first row, so an empty result has none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rows {
    /// Column names, in result order.
    pub columns: Vec<String>,
    /// The rows, each cell as text.
    pub rows: Vec<Vec<String>>,
    /// Whether the result had more rows than were returned.
    pub truncated: bool,
}

/// What `db_tables` reports: one schema's objects and every schema's name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tables {
    /// The schema listed (the one asked for, or the default).
    pub schema: String,
    /// Every schema that holds an object, in catalog order.
    pub schemas: Vec<String>,
    /// `(name, kind)` of each object in `schema`; `kind` is `table`, `view`
    /// or `function`.
    pub objects: Vec<(String, String)>,
    /// Whether the catalog had more rows than [`CATALOG_ROW_LIMIT`].
    pub truncated: bool,
}

/// What `db_describe` reports about one table, as the engine's own catalog
/// reports (the same ones as the workbench's table details).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Description {
    /// The schema (empty for `MySQL`, whose reports use the current database).
    pub schema: String,
    /// The table or view.
    pub table: String,
    /// Columns: names, types, nullability, defaults.
    pub columns: Rows,
    /// The primary-key column names, in key order.
    pub primary_key: Rows,
    /// Indexes.
    pub indexes: Rows,
    /// Foreign keys leaving the table.
    pub foreign_keys: Rows,
    /// Constraints (`SQLite`: the table's `CREATE` statement).
    pub constraints: Rows,
}

/// Check that `sql` is one read-only statement a shared connection may run,
/// returning that statement (trimmed, without its `;`).
///
/// A piece that is only a comment does not count as a statement, so
/// `SELECT 1; -- done` is one statement. The checks are the workbench's
/// whole-word scans outside strings and comments, so they are conservative:
/// `SELECT replace(a, 'x', 'y')` is refused because `REPLACE` is a write
/// keyword in `MySQL` and `SQLite`.
///
/// # Errors
///
/// Returns the [`Refusal`] that applies first.
///
/// # Examples
///
/// ```
/// use vix_db::mcp_share::{check_query, Refusal};
/// assert_eq!(check_query("select 1;").as_deref(), Ok("select 1"));
/// assert_eq!(
///     check_query("WITH x AS (SELECT 1) DELETE FROM t"),
///     Err(Refusal::Write("delete".into()))
/// );
/// assert_eq!(check_query("select 1; select 2"), Err(Refusal::Several(2)));
/// ```
pub fn check_query(sql: &str) -> Result<String, Refusal> {
    let chars: Vec<char> = sql.chars().collect();
    let statements: Vec<String> = editor::statement_spans(sql)
        .into_iter()
        .map(|(start, end)| chars[start..end].iter().collect::<String>())
        .filter(|piece| first_word(piece).is_some())
        .collect();
    let statement = match statements.as_slice() {
        [] => return Err(Refusal::Empty),
        [one] => one.trim().to_string(),
        many => return Err(Refusal::Several(many.len())),
    };
    let first = first_word(&statement).unwrap_or_default();
    if !READ_STATEMENTS.contains(&first.as_str()) {
        return Err(Refusal::NotARead(first));
    }
    if editor::has_keyword(&statement, &["analyze", "analyse"]) {
        return Err(Refusal::Analyze);
    }
    if let Some(word) = editor::WRITE_KEYWORDS
        .iter()
        .find(|w| editor::has_keyword(&statement, &[w]))
    {
        return Err(Refusal::Write((*word).to_string()));
    }
    if let Some(word) = REFUSED_KEYWORDS
        .iter()
        .find(|w| editor::has_keyword(&statement, &[w]))
    {
        return Err(Refusal::Keyword((*word).to_string()));
    }
    Ok(statement)
}

/// The first word of `sql`, lowercased, after whitespace, comments and
/// opening parentheses; `None` when there is none (a comment-only piece).
fn first_word(sql: &str) -> Option<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '(' {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else {
            let word: String = chars[i..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                .collect();
            // Anything else (a quote, a symbol) starts no keyword; report it
            // as itself so the refusal names what was there.
            return Some(if word.is_empty() {
                c.to_string()
            } else {
                word.to_ascii_lowercase()
            });
        }
    }
    None
}

/// The statements run on a fresh shared connection before the caller's:
/// a read-only session, then a read-only transaction with the
/// [`STATEMENT_TIMEOUT_SECS`] timeout. [`CLOSING_SQL`] ends it.
///
/// - `SQLite`: `PRAGMA query_only = ON` and `BEGIN` (the file is also opened
///   with `mode=ro`). `SQLite` has no statement timeout and sqlx
///   cannot interrupt a running statement, so the timeout is a
///   client-side deadline: the tool call fails at 30 seconds and the
///   connection is dropped, while the engine may finish the abandoned
///   (read-only) statement in the background.
/// - `PostgreSQL`: `SET default_transaction_read_only = on`,
///   `START TRANSACTION READ ONLY`, `SET LOCAL statement_timeout = '30s'`.
/// - `MySQL`: `SET SESSION TRANSACTION READ ONLY`,
///   `START TRANSACTION READ ONLY`, `SET SESSION MAX_EXECUTION_TIME = 30000`
///   (milliseconds; `MySQL` applies it to `SELECT` statements).
///
/// # Examples
///
/// ```
/// use vix_db::connect::Kind;
/// use vix_db::mcp_share::opening_sql;
/// assert!(opening_sql(Kind::Postgres).contains(&"SET LOCAL statement_timeout = '30s'"));
/// ```
#[must_use]
pub fn opening_sql(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Sqlite => &["PRAGMA query_only = ON", "BEGIN"],
        Kind::Postgres => &[
            "SET default_transaction_read_only = on",
            "START TRANSACTION READ ONLY",
            "SET LOCAL statement_timeout = '30s'",
        ],
        Kind::Mysql => &[
            "SET SESSION TRANSACTION READ ONLY",
            "START TRANSACTION READ ONLY",
            "SET SESSION MAX_EXECUTION_TIME = 30000",
        ],
    }
}

/// The URL a shared read opens: a `SQLite` file read-only (`mode=ro`, never
/// created), or the server URL — through the tunnel's local port when there
/// is one. The password lives only in this string.
#[must_use]
pub fn connection_url(conn: &Connection, password: &str, tunnel_port: Option<u16>) -> String {
    match (conn.kind, tunnel_port) {
        (Kind::Sqlite, _) => catalog::sqlite_read_only_url(std::path::Path::new(&conn.file)),
        (_, Some(port)) => connect::url_via_local(conn, password, port),
        (_, None) => connect::url(conn, password),
    }
}

/// The password for a shared connection, without prompting: none for
/// `SQLite`, else the `password_command`, else the OS keyring.
///
/// # Errors
///
/// [`ShareError::NeedsPassword`] when neither yields one.
pub fn password(conn: &Connection) -> Result<String, ShareError> {
    if !conn.needs_password() {
        return Ok(String::new());
    }
    secret::resolve(conn).ok_or_else(|| ShareError::NeedsPassword {
        connection: conn.name.clone(),
    })
}

/// The schema `db_tables` and `db_describe` use when none is given:
/// `main` for `SQLite`, `public` for `PostgreSQL` when it has objects (else
/// the first schema), and the current database for `MySQL` (the first, and
/// only, schema its catalog query reports).
#[must_use]
pub fn default_schema(kind: Kind, schemas: &[String]) -> String {
    match kind {
        Kind::Sqlite => "main".to_string(),
        Kind::Postgres if schemas.iter().any(|s| s == "public") || schemas.is_empty() => {
            "public".to_string()
        }
        _ => schemas.first().cloned().unwrap_or_default(),
    }
}

/// `db_tables`: the objects of `schema` (or the default schema) and the
/// names of every schema, read inside a read-only transaction.
///
/// # Errors
///
/// Any [`ShareError`] from opening the connection or reading the catalog.
pub fn tables(conn: &Connection, schema: Option<&str>) -> Result<Tables, ShareError> {
    let mut read = ReadOnly::open(conn)?;
    let rows = read.query(catalog::objects_sql(conn.kind), CATALOG_ROW_LIMIT)?;
    read.finish();
    let mut schemas: Vec<String> = Vec::new();
    for row in &rows.rows {
        if let Some(name) = row.first()
            && !schemas.contains(name)
        {
            schemas.push(name.clone());
        }
    }
    let schema = schema.map_or_else(|| default_schema(conn.kind, &schemas), str::to_string);
    let objects = rows
        .rows
        .iter()
        .filter(|row| row.first() == Some(&schema))
        .filter_map(|row| Some((row.get(1)?.clone(), row.get(2)?.clone())))
        .collect();
    Ok(Tables {
        schema,
        schemas,
        objects,
        truncated: rows.truncated,
    })
}

/// `db_describe`: the columns, primary key, indexes, foreign keys and
/// constraints of `table` in `schema` (or the default schema), read inside a
/// read-only transaction.
///
/// # Errors
///
/// [`ShareError::NoSuchTable`] when the table has no columns, or any
/// [`ShareError`] from opening the connection or reading the catalog.
pub fn describe(
    conn: &Connection,
    schema: Option<&str>,
    table: &str,
) -> Result<Description, ShareError> {
    let kind = conn.kind;
    let schema = match (kind, schema) {
        (Kind::Mysql, _) => String::new(),
        (_, Some(s)) => s.to_string(),
        (_, None) => default_schema(kind, &[]),
    };
    let mut read = ReadOnly::open(conn)?;
    let detail = |d| catalog::detail_sql(kind, d, &schema, table);
    let columns = read.query(&detail(catalog::Detail::Columns), CATALOG_ROW_LIMIT)?;
    if columns.rows.is_empty() {
        read.finish();
        return Err(ShareError::NoSuchTable {
            schema,
            table: table.to_string(),
        });
    }
    let primary_key = read.query(
        &catalog::primary_key_sql(kind, &schema, table),
        CATALOG_ROW_LIMIT,
    )?;
    let indexes = read.query(&detail(catalog::Detail::Indexes), CATALOG_ROW_LIMIT)?;
    let foreign_keys = read.query(&detail(catalog::Detail::ForeignKeys), CATALOG_ROW_LIMIT)?;
    let constraints = read.query(&detail(catalog::Detail::Constraints), CATALOG_ROW_LIMIT)?;
    read.finish();
    Ok(Description {
        schema,
        table: table.to_string(),
        columns,
        primary_key,
        indexes,
        foreign_keys,
        constraints,
    })
}

/// `db_query`: [`check_query`], then run the statement inside a read-only
/// transaction and return at most [`ROW_LIMIT`] rows.
///
/// # Errors
///
/// [`ShareError::Refused`] before anything is opened, or any [`ShareError`]
/// from opening the connection or running the statement.
pub fn query(conn: &Connection, sql: &str) -> Result<Rows, ShareError> {
    let statement = check_query(sql)?;
    let mut read = ReadOnly::open(conn)?;
    let rows = read.query(&statement, ROW_LIMIT)?;
    read.finish();
    Ok(rows)
}

/// One open read-only transaction on a shared connection.
///
/// Field order matters: the connection is dropped before the tunnel it
/// runs through.
struct ReadOnly {
    /// The live connection; `None` once it is closed or abandoned.
    conn: Option<crate::conn::Conn>,
    /// The SSH forward, held while the connection is open.
    _tunnel: Option<tunnel::Tunnel>,
    /// A current-thread runtime: one connection, one statement at a time.
    rt: tokio::runtime::Runtime,
    /// When the client gives up on the transaction.
    deadline: tokio::time::Instant,
}

impl ReadOnly {
    /// Resolve credentials, open the tunnel and the connection, and run
    /// [`opening_sql`].
    fn open(conn: &Connection) -> Result<ReadOnly, ShareError> {
        let password = password(conn)?;
        let tunnel = tunnel::open_batch(conn).map_err(ShareError::Tunnel)?;
        let url = connection_url(conn, &password, tunnel.as_ref().map(|t| t.local_port));
        // Every timer below is created inside `block_on`: a `Sleep` needs the
        // runtime's reactor.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ShareError::Runtime)?;
        let connect = crate::conn::Conn::connect(&url);
        let mut live =
            match rt.block_on(async { tokio::time::timeout(CONNECT_TIMEOUT, connect).await }) {
                Ok(Ok(live)) => live,
                Ok(Err(e)) => {
                    return Err(ShareError::Connect(session::redact_url_secret(
                        &e.to_string(),
                        &url,
                    )));
                }
                Err(_) => return Err(ShareError::Connect("timed out".to_string())),
            };
        let grace = if conn.kind == Kind::Sqlite {
            Duration::ZERO
        } else {
            SERVER_GRACE
        };
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(STATEMENT_TIMEOUT_SECS) + grace;
        for sql in opening_sql(conn.kind) {
            let run = live.execute(sql);
            match rt.block_on(async { tokio::time::timeout_at(deadline, run).await }) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(ShareError::Query(e.to_string())),
                Err(_) => return Err(ShareError::Timeout),
            }
        }
        Ok(ReadOnly {
            conn: Some(live),
            _tunnel: tunnel,
            rt,
            deadline,
        })
    }

    /// Run one statement and collect at most `limit` rows, flagging more.
    /// On a timeout the connection is abandoned (closing it ends the
    /// transaction on the server).
    fn query(&mut self, sql: &str, limit: usize) -> Result<Rows, ShareError> {
        let Some(live) = self.conn.as_mut() else {
            return Err(ShareError::Timeout);
        };
        let collect = async {
            let mut stream = live.fetch(sql);
            let mut out = Rows::default();
            while let Some(row) = stream.next().await {
                let row = row.map_err(|e| ShareError::Query(e.to_string()))?;
                if out.rows.len() == limit {
                    out.truncated = true;
                    break;
                }
                if out.columns.is_empty() {
                    out.columns = row.names();
                }
                out.rows.push(row.cells());
            }
            Ok(out)
        };
        let deadline = self.deadline;
        let outcome = self
            .rt
            .block_on(async { tokio::time::timeout_at(deadline, collect).await });
        outcome.unwrap_or_else(|_| {
            // The connection is mid-statement: drop it rather than wait.
            self.conn = None;
            Err(ShareError::Timeout)
        })
    }

    /// Roll back and close (best effort, bounded: the transaction ends with
    /// the connection anyway).
    fn finish(mut self) {
        if let Some(mut live) = self.conn.take() {
            let rt = &self.rt;
            let _ = rt.block_on(async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    let _ = live.execute(CLOSING_SQL).await;
                    let _ = live.close().await;
                })
                .await
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pass_and_come_back_trimmed() {
        for (sql, statement) in [
            ("select 1", "select 1"),
            ("  SELECT * FROM t;  ", "SELECT * FROM t"),
            (
                "with x as (select 1) select * from x",
                "with x as (select 1) select * from x",
            ),
            ("EXPLAIN SELECT 1", "EXPLAIN SELECT 1"),
            ("explain query plan select 1", "explain query plan select 1"),
            ("SHOW search_path", "SHOW search_path"),
            ("VALUES (1), (2)", "VALUES (1), (2)"),
            ("(select 1) union (select 2)", "(select 1) union (select 2)"),
            ("-- why\nselect 1; -- done", "-- why\nselect 1"),
            (
                "/* c */ select 'drop table t; delete'",
                "/* c */ select 'drop table t; delete'",
            ),
            (
                "select updated_at, into_x from t",
                "select updated_at, into_x from t",
            ),
        ] {
            assert_eq!(check_query(sql).as_deref(), Ok(statement), "{sql}");
        }
    }

    #[test]
    fn everything_else_is_refused_with_a_reason() {
        let cases: &[(&str, Refusal)] = &[
            ("", Refusal::Empty),
            ("  ;  -- nothing", Refusal::Empty),
            ("select 1; select 2", Refusal::Several(2)),
            ("select 1; delete from t", Refusal::Several(2)),
            ("delete from t", Refusal::NotARead("delete".into())),
            ("pragma table_info(t)", Refusal::NotARead("pragma".into())),
            ("call p()", Refusal::NotARead("call".into())),
            ("'select'", Refusal::NotARead("'".into())),
            ("explain analyze select 1", Refusal::Analyze),
            ("EXPLAIN (ANALYZE, BUFFERS) SELECT 1", Refusal::Analyze),
            ("explain analyse select 1", Refusal::Analyze),
            (
                "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d",
                Refusal::Write("delete".into()),
            ),
            (
                "with x as (select 1) delete from t",
                Refusal::Write("delete".into()),
            ),
            (
                "select * from t for update",
                Refusal::Write("update".into()),
            ),
            (
                "select * into backup from t",
                Refusal::Keyword("into".into()),
            ),
            (
                "select * from t into outfile '/tmp/x'",
                Refusal::Keyword("into".into()),
            ),
            (
                "select * from t for share",
                Refusal::Keyword("share".into()),
            ),
            (
                "select * from t lock in share mode",
                Refusal::Keyword("lock".into()),
            ),
            ("select 1 /* ; */ ; commit", Refusal::Several(2)),
        ];
        for (sql, why) in cases {
            assert_eq!(check_query(sql).as_ref(), Err(why), "{sql}");
            assert_ne!(why.to_string(), "");
        }
    }

    #[test]
    fn every_engine_opens_read_only_with_a_thirty_second_timeout() {
        assert_eq!(
            opening_sql(Kind::Sqlite),
            ["PRAGMA query_only = ON", "BEGIN"]
        );
        assert_eq!(
            opening_sql(Kind::Postgres),
            [
                "SET default_transaction_read_only = on",
                "START TRANSACTION READ ONLY",
                "SET LOCAL statement_timeout = '30s'",
            ]
        );
        assert_eq!(
            opening_sql(Kind::Mysql),
            [
                "SET SESSION TRANSACTION READ ONLY",
                "START TRANSACTION READ ONLY",
                "SET SESSION MAX_EXECUTION_TIME = 30000",
            ]
        );
        assert_eq!(CLOSING_SQL, "ROLLBACK");
        assert_eq!(STATEMENT_TIMEOUT_SECS, 30);
        // Both server timeouts say the same thing.
        assert_eq!(STATEMENT_TIMEOUT_SECS * 1000, 30_000);
    }

    #[test]
    fn urls_open_sqlite_read_only_and_servers_through_the_tunnel() {
        let sqlite = Connection {
            kind: Kind::Sqlite,
            file: "/data/app.db".into(),
            ..Connection::default()
        };
        assert_eq!(
            connection_url(&sqlite, "", None),
            "sqlite:/data/app.db?mode=ro"
        );
        let pg = Connection {
            kind: Kind::Postgres,
            host: "db.example.com".into(),
            user: "u".into(),
            database: "d".into(),
            ..Connection::default()
        };
        assert_eq!(
            connection_url(&pg, "pw", None),
            "postgres://u:pw@db.example.com:5432/d"
        );
        assert_eq!(
            connection_url(&pg, "pw", Some(6000)),
            "postgres://u:pw@127.0.0.1:6000/d"
        );
    }

    #[test]
    fn a_server_without_a_stored_password_is_refused_not_prompted() {
        // Tests never opt into the OS keyring (T639), so this finds nothing.
        let pg = Connection {
            name: "vix-test-t783-no-password".into(),
            kind: Kind::Postgres,
            host: "db.invalid".into(),
            ..Connection::default()
        };
        assert!(matches!(
            password(&pg),
            Err(ShareError::NeedsPassword { connection }) if connection == pg.name
        ));
        assert!(matches!(
            query(&pg, "select 1"),
            Err(ShareError::NeedsPassword { .. })
        ));
        // A refusal comes first: nothing is opened for a write.
        assert!(matches!(
            query(&pg, "delete from t"),
            Err(ShareError::Refused(_))
        ));
        let with_command = Connection {
            password_command: "printf s3cret".into(),
            ..pg
        };
        assert_eq!(password(&with_command).unwrap(), "s3cret");
    }

    #[test]
    fn default_schemas_per_engine() {
        let names = |v: &[&str]| v.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(default_schema(Kind::Sqlite, &[]), "main");
        assert_eq!(
            default_schema(Kind::Postgres, &names(&["app", "public"])),
            "public"
        );
        assert_eq!(default_schema(Kind::Postgres, &names(&["app"])), "app");
        assert_eq!(default_schema(Kind::Postgres, &[]), "public");
        assert_eq!(default_schema(Kind::Mysql, &names(&["shop"])), "shop");
    }

    /// A scratch `SQLite` file with `n` rows in `items`.
    fn sqlite(name: &str, n: usize) -> Connection {
        let dir = std::env::temp_dir().join(format!("vix-db-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.db");
        let url = format!("sqlite:{}?mode=rwc", file.display());
        let mut seed = session::Session::connect(&url, &[]).unwrap();
        seed.run("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
            .unwrap();
        seed.run("CREATE INDEX items_name ON items(name);").unwrap();
        seed.run("CREATE VIEW names AS SELECT name FROM items;")
            .unwrap();
        for i in 0..n {
            seed.run(&format!("INSERT INTO items (name) VALUES ('item {i}');"))
                .unwrap();
        }
        drop(seed);
        Connection {
            name: "app".into(),
            kind: Kind::Sqlite,
            file: file.to_string_lossy().into_owned(),
            ..Connection::default()
        }
    }

    #[test]
    fn sqlite_reads_are_capped_and_writes_never_land() {
        let conn = sqlite("cap", ROW_LIMIT + 5);
        let before = std::fs::read(&conn.file).unwrap();
        let rows = query(&conn, "select id, name from items order by id").unwrap();
        assert_eq!(rows.columns, ["id", "name"]);
        assert_eq!(rows.rows.len(), ROW_LIMIT);
        assert!(rows.truncated);
        assert_eq!(rows.rows[0], ["1", "item 0"]);
        let few = query(&conn, "select count(*) as n from items").unwrap();
        assert_eq!(few.rows, [["105"]]);
        assert!(!few.truncated);
        // Bypassing the allowlist, the engine still refuses a write.
        let mut read = ReadOnly::open(&conn).unwrap();
        assert!(matches!(
            read.query("DELETE FROM items", ROW_LIMIT),
            Err(ShareError::Query(_))
        ));
        read.finish();
        assert_eq!(std::fs::read(&conn.file).unwrap(), before);
        // A missing file is not created.
        let missing = Connection {
            file: format!("{}.missing", conn.file),
            ..conn
        };
        assert!(matches!(
            query(&missing, "select 1"),
            Err(ShareError::Connect(_))
        ));
        assert!(!std::path::Path::new(&missing.file).exists());
    }

    /// `db_query` reads columns `Any` used to reject by declared type (T787).
    #[test]
    fn sqlite_query_reads_boolean_date_and_json_columns() {
        let conn = sqlite("typed", 0);
        let mut seed =
            session::Session::connect(&format!("sqlite:{}?mode=rwc", conn.file), &[]).unwrap();
        seed.run("CREATE TABLE typed (done BOOLEAN, at DATETIME, doc JSON);")
            .unwrap();
        seed.run("INSERT INTO typed VALUES (TRUE, '2026-11-08 10:15:00', '[1]');")
            .unwrap();
        drop(seed);
        let rows = query(&conn, "select done, at, doc from typed").unwrap();
        assert_eq!(rows.rows, [["1", "2026-11-08 10:15:00", "[1]"]]);
    }

    #[test]
    fn sqlite_tables_and_describe_read_the_catalog() {
        let conn = sqlite("catalog", 1);
        let t = tables(&conn, None).unwrap();
        assert_eq!(t.schema, "main");
        assert_eq!(t.schemas, ["main"]);
        assert_eq!(
            t.objects,
            [
                ("items".to_string(), "table".to_string()),
                ("names".to_string(), "view".to_string())
            ]
        );
        let other = tables(&conn, Some("elsewhere")).unwrap();
        assert_eq!(other.objects.len(), 0);
        let d = describe(&conn, None, "items").unwrap();
        assert_eq!(d.columns.rows.len(), 2);
        assert_eq!(d.primary_key.rows, [["id"]]);
        assert_eq!(d.indexes.rows.len(), 1);
        assert!(d.constraints.rows[0][1].contains("CREATE TABLE items"));
        assert!(matches!(
            describe(&conn, None, "nope"),
            Err(ShareError::NoSuchTable { .. })
        ));
    }
}
