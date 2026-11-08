//! One database connection per engine, read as text (T787).
//!
//! The workbench used to open sqlx's runtime `Any` connection. `Any` maps a
//! row into its own value type before handing it over, and that mapping knows
//! only text, integers, floats, booleans and bytes: any other column type (a
//! PostgreSQL `timestamp`, `numeric`, `json`, `uuid` or array, a MySQL
//! `DATETIME`, `DECIMAL` or `TINYINT`, a SQLite column *declared* `BOOLEAN`,
//! `DATE` or `DATETIME`) fails the whole row with "Any driver does not
//! support …", before any of our code sees a value.
//!
//! [`Conn`] holds the engine's own connection instead
//! (`PgConnection`, `MySqlConnection`, `SqliteConnection`) and [`TextRow`]
//! the engine's own row. Every statement still goes through `raw_sql`, which
//! PostgreSQL runs with the simple query protocol and MySQL with `COM_QUERY`:
//! both send every value as the **server's own text**, whatever its type. So
//! a cell is read as those raw bytes, with no per-type decoding to get wrong:
//! timestamps as the server formats them, `numeric`/`DECIMAL` exactly, JSON as
//! JSON text, UUIDs canonical, arrays in PostgreSQL's literal form, enums by
//! label. `SQLite` values are read by their storage class (integer, real,
//! text or blob), which is what a `BOOLEAN` or `DATE` column really holds.
//!
//! The pure pieces — how a raw value becomes display text ([`pg_text`],
//! [`bytes_text`]) or a typed export [`Value`] ([`pg_value`],
//! [`mysql_value`]) — are unit-tested here without a server. Owning spec:
//! `crates/vix-db/spec/index.md` ("Column types").

use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use sqlx::{Column as _, Connection as _, Row as _, TypeInfo as _, ValueRef as _};

use crate::connect::Kind;
use crate::table_export::Value;

/// A live connection to one engine.
pub(crate) enum Conn {
    /// A PostgreSQL connection.
    Postgres(sqlx::postgres::PgConnection),
    /// A MySQL or MariaDB connection.
    Mysql(sqlx::mysql::MySqlConnection),
    /// A `SQLite` connection.
    Sqlite(sqlx::sqlite::SqliteConnection),
}

/// One result row, in the engine's own representation.
pub(crate) enum TextRow {
    /// A PostgreSQL row (text format).
    Postgres(sqlx::postgres::PgRow),
    /// A MySQL row (text protocol).
    Mysql(sqlx::mysql::MySqlRow),
    /// A `SQLite` row.
    Sqlite(sqlx::sqlite::SqliteRow),
}

impl Conn {
    /// Connect to `url`, choosing the engine from its scheme
    /// ([`crate::session::kind_of_url`]).
    ///
    /// # Errors
    ///
    /// The driver's connect error, or a configuration error for a scheme
    /// that names no supported engine.
    pub(crate) async fn connect(url: &str) -> Result<Conn, sqlx::Error> {
        match crate::session::kind_of_url(url) {
            Some(Kind::Postgres) => Ok(Conn::Postgres(
                sqlx::postgres::PgConnection::connect(url).await?,
            )),
            Some(Kind::Mysql) => Ok(Conn::Mysql(
                sqlx::mysql::MySqlConnection::connect(url).await?,
            )),
            Some(Kind::Sqlite) => Ok(Conn::Sqlite(
                sqlx::sqlite::SqliteConnection::connect(url).await?,
            )),
            None => Err(sqlx::Error::Configuration(
                "unsupported database URL scheme".into(),
            )),
        }
    }

    /// Execute `sql` (one or more statements) for its effect and return the
    /// rows it affected. `AssertSqlSafe` is the workbench's intended opt-in
    /// for SQL it runs as written.
    ///
    /// # Errors
    ///
    /// The driver's error.
    pub(crate) async fn execute(&mut self, sql: &str) -> Result<u64, sqlx::Error> {
        let query = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()));
        Ok(match self {
            Conn::Postgres(c) => query.execute(c).await?.rows_affected(),
            Conn::Mysql(c) => query.execute(c).await?.rows_affected(),
            Conn::Sqlite(c) => query.execute(c).await?.rows_affected(),
        })
    }

    /// Stream the rows `sql` returns.
    pub(crate) fn fetch<'c>(
        &'c mut self,
        sql: &str,
    ) -> BoxStream<'c, Result<TextRow, sqlx::Error>> {
        let query = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()));
        match self {
            Conn::Postgres(c) => query.fetch(c).map(|r| r.map(TextRow::Postgres)).boxed(),
            Conn::Mysql(c) => query.fetch(c).map(|r| r.map(TextRow::Mysql)).boxed(),
            Conn::Sqlite(c) => query.fetch(c).map(|r| r.map(TextRow::Sqlite)).boxed(),
        }
    }

    /// Run `sql` and collect every row as display text ([`TextRow::cells`]).
    ///
    /// # Errors
    ///
    /// The driver's error.
    pub(crate) async fn fetch_text(&mut self, sql: &str) -> Result<Vec<Vec<String>>, sqlx::Error> {
        let mut stream = self.fetch(sql);
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row?.cells());
        }
        Ok(rows)
    }

    /// Close the connection cleanly.
    ///
    /// # Errors
    ///
    /// The driver's error.
    pub(crate) async fn close(self) -> Result<(), sqlx::Error> {
        match self {
            Conn::Postgres(c) => c.close().await,
            Conn::Mysql(c) => c.close().await,
            Conn::Sqlite(c) => c.close().await,
        }
    }
}

impl TextRow {
    /// How many columns the row has.
    pub(crate) fn len(&self) -> usize {
        match self {
            TextRow::Postgres(r) => r.len(),
            TextRow::Mysql(r) => r.len(),
            TextRow::Sqlite(r) => r.len(),
        }
    }

    /// The column names, in order.
    pub(crate) fn names(&self) -> Vec<String> {
        match self {
            TextRow::Postgres(r) => r.columns().iter().map(|c| c.name().to_string()).collect(),
            TextRow::Mysql(r) => r.columns().iter().map(|c| c.name().to_string()).collect(),
            TextRow::Sqlite(r) => r.columns().iter().map(|c| c.name().to_string()).collect(),
        }
    }

    /// Every cell as display text, `NULL` as the empty string (the grid's
    /// text-only model).
    pub(crate) fn cells(&self) -> Vec<String> {
        (0..self.len())
            .map(|i| self.text(i).unwrap_or_default())
            .collect()
    }

    /// Cell `i` as display text, or `None` for `NULL`.
    pub(crate) fn text(&self, i: usize) -> Option<String> {
        match self {
            TextRow::Postgres(r) => {
                let value = r.try_get_raw(i).ok()?;
                if value.is_null() {
                    return None;
                }
                let raw = value.as_bytes().ok()?;
                Some(match value.format() {
                    sqlx::postgres::PgValueFormat::Text => {
                        pg_text(r.column(i).type_info().name(), raw)
                    }
                    // Not produced by `raw_sql` (simple query protocol), but
                    // never render binary as if it were text.
                    sqlx::postgres::PgValueFormat::Binary => hex(raw),
                })
            }
            TextRow::Mysql(r) => {
                let raw = r.try_get_unchecked::<Option<&[u8]>, _>(i).ok()??;
                Some(mysql_text(r.column(i).type_info().name(), raw))
            }
            TextRow::Sqlite(_) => match self.value(i) {
                Value::Null => None,
                Value::Bytes(b) => Some(bytes_text(&b)),
                other => Some(other.text()),
            },
        }
    }

    /// Cell `i` as a typed export [`Value`]: `NULL`, numbers and booleans
    /// keep their type so JSON and SQL exports can write them unquoted.
    pub(crate) fn value(&self, i: usize) -> Value {
        match self {
            TextRow::Postgres(r) => {
                let Ok(value) = r.try_get_raw(i) else {
                    return Value::Null;
                };
                if value.is_null() {
                    return Value::Null;
                }
                match (value.format(), value.as_bytes()) {
                    (sqlx::postgres::PgValueFormat::Text, Ok(raw)) => {
                        pg_value(r.column(i).type_info().name(), raw)
                    }
                    (sqlx::postgres::PgValueFormat::Binary, Ok(raw)) => Value::Bytes(raw.to_vec()),
                    (_, Err(_)) => Value::Null,
                }
            }
            TextRow::Mysql(r) => match r.try_get_unchecked::<Option<&[u8]>, _>(i) {
                Ok(Some(raw)) => mysql_value(r.column(i).type_info().name(), raw),
                _ => Value::Null,
            },
            TextRow::Sqlite(r) => {
                let Ok(value) = r.try_get_raw(i) else {
                    return Value::Null;
                };
                if value.is_null() {
                    return Value::Null;
                }
                // The value's storage class, not the declared column type:
                // a `BOOLEAN` column holds integers, a `DATE` column text.
                let class = value.type_info().name().to_string();
                let decoded = match class.as_str() {
                    "INTEGER" => r.try_get_unchecked::<i64, _>(i).map(Value::Int),
                    "REAL" => r.try_get_unchecked::<f64, _>(i).map(Value::Float),
                    "BLOB" => r.try_get_unchecked::<Vec<u8>, _>(i).map(Value::Bytes),
                    _ => r.try_get_unchecked::<String, _>(i).map(Value::Text),
                };
                decoded.unwrap_or(Value::Null)
            }
        }
    }
}

/// A PostgreSQL text-format value as display text: the server's own text,
/// except that `bool`'s `t` / `f` read `true` / `false` (as the grid always
/// showed them, and as the boolean pick list offers them).
///
/// ```text
/// pg_text("BOOL", b"t")          == "true"
/// pg_text("TIMESTAMPTZ", b"2026-11-08 10:00:00+00") == "2026-11-08 10:00:00+00"
/// ```
pub(crate) fn pg_text(type_name: &str, raw: &[u8]) -> String {
    match (type_name, raw) {
        ("BOOL", b"t") => "true".to_string(),
        ("BOOL", b"f") => "false".to_string(),
        _ => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// A PostgreSQL text-format value as an export [`Value`], by the column's
/// type name: integers and floats as numbers, `bool` as a boolean, `bytea`
/// (`\x…` hex) as bytes, everything else — `numeric` included, so it stays
/// exact — as text.
pub(crate) fn pg_value(type_name: &str, raw: &[u8]) -> Value {
    let text = String::from_utf8_lossy(raw);
    let parsed = match type_name {
        "INT2" | "INT4" | "INT8" | "OID" => text.parse().ok().map(Value::Int),
        "FLOAT4" | "FLOAT8" => text.parse().ok().map(Value::Float),
        "BOOL" => match raw {
            b"t" => Some(Value::Bool(true)),
            b"f" => Some(Value::Bool(false)),
            _ => None,
        },
        "BYTEA" => text.strip_prefix("\\x").and_then(unhex).map(Value::Bytes),
        _ => None,
    };
    parsed.unwrap_or_else(|| Value::Text(text.into_owned()))
}

/// Whether a MySQL column type (as sqlx names it) carries binary data rather
/// than text: binary strings, blobs, `BIT` and spatial values.
fn mysql_is_binary(type_name: &str) -> bool {
    matches!(
        type_name,
        "BINARY"
            | "VARBINARY"
            | "TINYBLOB"
            | "BLOB"
            | "MEDIUMBLOB"
            | "LONGBLOB"
            | "BIT"
            | "GEOMETRY"
    )
}

/// A MySQL text-protocol value as display text: the server's text, and
/// binary columns through [`bytes_text`].
pub(crate) fn mysql_text(type_name: &str, raw: &[u8]) -> String {
    if mysql_is_binary(type_name) {
        bytes_text(raw)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    }
}

/// A MySQL text-protocol value as an export [`Value`], by the column's type
/// name: integer types (signed or unsigned, `BOOLEAN` is `TINYINT(1)`, `YEAR`)
/// as integers when they fit an `i64`, `FLOAT` / `DOUBLE` as floats, binary
/// columns as bytes, everything else — `DECIMAL` included — as text.
pub(crate) fn mysql_value(type_name: &str, raw: &[u8]) -> Value {
    if mysql_is_binary(type_name) {
        return Value::Bytes(raw.to_vec());
    }
    let text = String::from_utf8_lossy(raw);
    let base = type_name.split(' ').next().unwrap_or_default();
    let parsed = match base {
        "BOOLEAN" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "YEAR" => {
            text.parse().ok().map(Value::Int)
        }
        "FLOAT" | "DOUBLE" => text.parse().ok().map(Value::Float),
        _ => None,
    };
    parsed.unwrap_or_else(|| Value::Text(text.into_owned()))
}

/// Binary data as display text: as-is when it is valid UTF-8 (a blob that
/// holds text reads as that text), otherwise PostgreSQL-style `\x` hex, so
/// binary data never turns into replacement characters.
pub(crate) fn bytes_text(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => hex(raw),
    }
}

/// `\x` followed by the bytes in lowercase hex.
fn hex(raw: &[u8]) -> String {
    use std::fmt::Write as _;
    raw.iter().fold(String::from("\\x"), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Parse an even-length hex string into bytes.
fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_text_is_the_servers_own() {
        for (ty, raw) in [
            ("TIMESTAMPTZ", "2026-11-08 10:15:00.123+01"),
            ("TIMESTAMP", "2026-11-08 10:15:00"),
            ("DATE", "2026-11-08"),
            ("TIME", "10:15:00"),
            ("INTERVAL", "1 day 02:00:00"),
            ("NUMERIC", "12345678901234567890.000000001"),
            ("JSONB", r#"{"a": [1, 2], "b": null}"#),
            ("UUID", "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"),
            ("INT4[]", "{1,2,3}"),
            ("TEXT[]", r#"{"a b",NULL,c}"#),
            ("INET", "192.168.0.1/24"),
            ("?", "happy"), // an enum: an OID sqlx does not know
            ("BYTEA", "\\x00ff"),
            ("FLOAT8", "NaN"),
        ] {
            assert_eq!(pg_text(ty, raw.as_bytes()), raw, "{ty}");
        }
        assert_eq!(pg_text("BOOL", b"t"), "true");
        assert_eq!(pg_text("BOOL", b"f"), "false");
    }

    #[test]
    fn postgres_values_keep_numbers_booleans_and_bytes() {
        assert_eq!(pg_value("INT8", b"-42"), Value::Int(-42));
        assert_eq!(pg_value("INT2", b"7"), Value::Int(7));
        assert_eq!(pg_value("FLOAT8", b"1.5"), Value::Float(1.5));
        assert_eq!(pg_value("BOOL", b"t"), Value::Bool(true));
        assert_eq!(pg_value("BOOL", b"f"), Value::Bool(false));
        assert_eq!(
            pg_value("BYTEA", b"\\x00ff10"),
            Value::Bytes(vec![0, 255, 16])
        );
        // Numeric stays exact text; so do timestamps, JSON and arrays.
        assert_eq!(
            pg_value("NUMERIC", b"0.10000000000000000001"),
            Value::Text("0.10000000000000000001".into())
        );
        assert_eq!(pg_value("JSON", b"[1]"), Value::Text("[1]".into()));
        assert_eq!(
            pg_value("TIMESTAMP", b"2026-01-02 03:04:05"),
            Value::Text("2026-01-02 03:04:05".into())
        );
        // Odd input falls back to text rather than losing the value.
        assert_eq!(pg_value("BYTEA", b"\\x0"), Value::Text("\\x0".into()));
        assert_eq!(pg_value("INT4", b"x"), Value::Text("x".into()));
    }

    #[test]
    fn mysql_values_by_type_name() {
        assert_eq!(mysql_value("BIGINT", b"-9"), Value::Int(-9));
        assert_eq!(mysql_value("BOOLEAN", b"1"), Value::Int(1));
        assert_eq!(mysql_value("YEAR", b"2026"), Value::Int(2026));
        assert_eq!(
            mysql_value("INT UNSIGNED", b"4000000000"),
            Value::Int(4_000_000_000)
        );
        // Past i64: kept exactly as text.
        assert_eq!(
            mysql_value("BIGINT UNSIGNED", b"18446744073709551615"),
            Value::Text("18446744073709551615".into())
        );
        assert_eq!(mysql_value("DOUBLE", b"2.25"), Value::Float(2.25));
        assert_eq!(
            mysql_value("DECIMAL", b"10.50"),
            Value::Text("10.50".into())
        );
        assert_eq!(
            mysql_value("DATETIME", b"2026-11-08 10:00:00"),
            Value::Text("2026-11-08 10:00:00".into())
        );
        assert_eq!(
            mysql_value("JSON", br#"{"a": 1}"#),
            Value::Text(r#"{"a": 1}"#.into())
        );
        assert_eq!(mysql_value("BIT", &[1]), Value::Bytes(vec![1]));
        assert_eq!(mysql_value("VARBINARY", &[0xff]), Value::Bytes(vec![0xff]));
    }

    #[test]
    fn mysql_text_and_binary_display() {
        assert_eq!(mysql_text("DECIMAL", b"10.50"), "10.50");
        assert_eq!(
            mysql_text("TIMESTAMP", b"2026-11-08 10:00:00"),
            "2026-11-08 10:00:00"
        );
        assert_eq!(mysql_text("BLOB", b"plain"), "plain");
        assert_eq!(mysql_text("BIT", &[0x01, 0xfe]), "\\x01fe");
    }

    #[test]
    fn bytes_show_as_text_or_hex() {
        assert_eq!(bytes_text(b"hello"), "hello");
        assert_eq!(bytes_text(&[0xde, 0xad, 0xbe, 0xef]), "\\xdeadbeef");
        assert_eq!(bytes_text(&[]), "");
        assert_eq!(unhex("00ff"), Some(vec![0, 255]));
        assert_eq!(unhex("0g"), None);
    }

    #[test]
    fn unknown_scheme_is_a_configuration_error() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(Conn::connect("mssql://h/d")).err().unwrap();
        assert!(matches!(err, sqlx::Error::Configuration(_)));
    }
}
