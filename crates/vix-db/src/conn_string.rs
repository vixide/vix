//! Parsing a pasted connection string into a saved connection (T781).
//!
//! Two shapes are understood:
//!
//! - **URLs**: `postgres://` or `postgresql://` (PostgreSQL), `mysql://`
//!   (`MySQL`) and `sqlite:` (`SQLite`), with percent-decoding of every part,
//!   an optional port, the database as the path, and query options. The
//!   options the connection form has a row for are taken (`sslmode`,
//!   `ssl-mode`, `ssl_mode`, and on a server URL `host`, `port`, `user`,
//!   `password`, `dbname`/`database`); any other option is reported back in
//!   [`Parsed::ignored`] rather than silently dropped.
//! - **libpq `key=value` strings** (PostgreSQL): `host=db port=5432
//!   dbname=app user=me sslmode=require`, where a value may be single-quoted
//!   (`password='a b'`) and a backslash escapes the next character in either
//!   form. `hostaddr` is used when `host` is absent. Recognized keys the form
//!   has no row for (`connect_timeout`, `application_name`, …) and unknown
//!   keys go to [`Parsed::ignored`].
//!
//! A password found in the string is returned separately in
//! [`Parsed::password`] and is **never** part of [`Parsed::connection`], which
//! has no password field: the workbench holds it in memory for the next
//! connect, exactly like a typed password (see the credential waterfall in
//! `crates/vix-db/spec/index.md`).
//!
//! Everything here is pure and total: malformed input yields a
//! [`ParseError`], never a panic. Text that is not a connection string at all
//! (an ordinary paste into a form field) yields
//! [`ParseError::NotAConnectionString`], so the caller can fall back to
//! typing it.

use std::fmt;

use crate::connect::{Connection, Kind};

/// A parsed connection string: the connection it describes, plus the
/// password it carried (held in memory only, never saved) and the options
/// that were understood but have no place in the form.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Parsed {
    /// The connection described: engine, host, port, user, database, file
    /// and TLS mode. Its name is left empty for the caller to choose.
    pub connection: Connection,
    /// The password in the string, if any. Never written to disk.
    pub password: Option<String>,
    /// Option keys that were present but not applied, in input order.
    pub ignored: Vec<String>,
}

/// Why a connection string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The text is neither a supported URL nor a libpq `key=value` string;
    /// a caller should treat it as ordinary text.
    NotAConnectionString,
    /// A `%` escape is not followed by two hex digits, or decodes to
    /// invalid UTF-8.
    BadPercentEncoding,
    /// The port is not a number from 1 to 65535.
    BadPort(String),
    /// A single-quoted libpq value has no closing quote.
    UnterminatedQuote,
    /// A libpq token has no `=` or no key.
    MissingValue(String),
    /// More than one host (`h1,h2`), which the form cannot hold.
    MultipleHosts,
    /// The URL's structure is broken (an unclosed `[` around an IPv6 host,
    /// an `sqlite:` URL with no file).
    Malformed,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ParseError::NotAConnectionString => t!("msg.db_dsn_err_not"),
            ParseError::BadPercentEncoding => t!("msg.db_dsn_err_percent"),
            ParseError::BadPort(port) => t!("msg.db_dsn_err_port", port = port),
            ParseError::UnterminatedQuote => t!("msg.db_dsn_err_quote"),
            ParseError::MissingValue(token) => t!("msg.db_dsn_err_missing", token = token),
            ParseError::MultipleHosts => t!("msg.db_dsn_err_hosts"),
            ParseError::Malformed => t!("msg.db_dsn_err_malformed"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ParseError {}

/// libpq connection keywords: a token whose key is one of these marks the
/// text as a libpq string (so `x=1` pasted into a field is left alone).
const LIBPQ_KEYS: &[&str] = &[
    "host",
    "hostaddr",
    "port",
    "dbname",
    "user",
    "password",
    "passfile",
    "sslmode",
    "sslrootcert",
    "sslcert",
    "sslkey",
    "sslcrl",
    "sslpassword",
    "channel_binding",
    "connect_timeout",
    "client_encoding",
    "options",
    "application_name",
    "fallback_application_name",
    "keepalives",
    "keepalives_idle",
    "keepalives_interval",
    "keepalives_count",
    "tcp_user_timeout",
    "target_session_attrs",
    "load_balance_hosts",
    "gssencmode",
    "require_auth",
    "service",
];

/// Parse a pasted connection string. See the module docs for the accepted
/// shapes.
///
/// # Errors
///
/// [`ParseError::NotAConnectionString`] when `text` is neither shape, or the
/// specific [`ParseError`] for a string of a recognized shape that is
/// malformed.
///
/// # Examples
///
/// ```
/// use vix_db::conn_string::parse;
/// use vix_db::connect::Kind;
/// let p = parse("postgres://joel:s%40cret@db.example.com:5433/orders?sslmode=require").unwrap();
/// assert_eq!(p.connection.kind, Kind::Postgres);
/// assert_eq!(p.connection.host, "db.example.com");
/// assert_eq!(p.connection.port, "5433");
/// assert_eq!(p.connection.database, "orders");
/// assert_eq!(p.connection.sslmode, "require");
/// assert_eq!(p.password.as_deref(), Some("s@cret"));
///
/// let q = parse("host=db dbname='my app' user=me").unwrap();
/// assert_eq!(q.connection.database, "my app");
/// ```
pub fn parse(text: &str) -> Result<Parsed, ParseError> {
    let text = text.trim();
    if let Some((scheme, rest)) = split_scheme(text) {
        return match scheme.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" => server_url(Kind::Postgres, rest),
            "mysql" => server_url(Kind::Mysql, rest),
            "sqlite" => sqlite_url(rest),
            _ => Err(ParseError::NotAConnectionString),
        };
    }
    if looks_like_libpq(text) {
        return libpq(text);
    }
    Err(ParseError::NotAConnectionString)
}

/// Split `scheme:rest` when `text` starts with a URL scheme followed by `://`
/// (or, for `sqlite`, by a bare `:`). Returns the scheme and what follows
/// the `://` (or the `:` for `sqlite:`).
fn split_scheme(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let scheme = &text[..colon];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let after = &text[colon + 1..];
    if scheme.eq_ignore_ascii_case("sqlite") {
        return Some((scheme, after));
    }
    after.strip_prefix("//").map(|rest| (scheme, rest))
}

/// Whether `text` starts with a `key=` token whose key is a libpq keyword.
fn looks_like_libpq(text: &str) -> bool {
    let Some(eq) = text.find('=') else {
        return false;
    };
    let key = text[..eq].trim();
    !key.contains(char::is_whitespace) && LIBPQ_KEYS.contains(&key.to_ascii_lowercase().as_str())
}

/// Decode `%XX` escapes strictly: a `%` must be followed by two hex digits,
/// and the result must be UTF-8. `+` is left alone (it is not a space in a
/// connection URL).
fn decode(s: &str) -> Result<String, ParseError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = |b: u8| char::from(b).to_digit(16);
            let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            ) else {
                return Err(ParseError::BadPercentEncoding);
            };
            // Two hex digits always fit a byte.
            out.push(u8::try_from(hi * 16 + lo).map_err(|_| ParseError::BadPercentEncoding)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ParseError::BadPercentEncoding)
}

/// Validate a port: digits only, 1 to 65535. Empty means "the default".
fn port(value: &str) -> Result<String, ParseError> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.contains(',') {
        return Err(ParseError::MultipleHosts);
    }
    match value.parse::<u16>() {
        Ok(n) if n > 0 && value.chars().all(|c| c.is_ascii_digit()) => Ok(value.to_string()),
        _ => Err(ParseError::BadPort(value.to_string())),
    }
}

/// Reject a comma-separated host list.
fn single_host(host: String) -> Result<String, ParseError> {
    if host.contains(',') {
        Err(ParseError::MultipleHosts)
    } else {
        Ok(host)
    }
}

/// A `postgres://` / `mysql://` URL body (everything after `://`).
fn server_url(kind: Kind, rest: &str) -> Result<Parsed, ParseError> {
    let rest = rest.split('#').next().unwrap_or("");
    let (main, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = main.split_once('/').unwrap_or((main, ""));
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    let mut parsed = Parsed {
        connection: Connection {
            kind,
            ..Connection::default()
        },
        ..Parsed::default()
    };
    if let Some(info) = userinfo {
        let (user, password) = match info.split_once(':') {
            Some((u, p)) => (u, Some(p)),
            None => (info, None),
        };
        parsed.connection.user = decode(user)?;
        if let Some(p) = password {
            let p = decode(p)?;
            parsed.password = (!p.is_empty()).then_some(p);
        }
    }
    let (host, port_text) = if let Some(inner) = hostport.strip_prefix('[') {
        let Some((v6, after)) = inner.split_once(']') else {
            return Err(ParseError::Malformed);
        };
        let port_text = match after.strip_prefix(':') {
            Some(p) => p,
            None if after.is_empty() => "",
            None => return Err(ParseError::Malformed),
        };
        (v6.to_string(), port_text)
    } else {
        match hostport.split_once(':') {
            Some((h, p)) => (decode(h)?, p),
            None => (decode(hostport)?, ""),
        }
    };
    parsed.connection.host = single_host(host)?;
    parsed.connection.port = port(port_text)?;
    parsed.connection.database = decode(path)?;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode(key)?;
        let value = decode(value)?;
        match key.to_ascii_lowercase().as_str() {
            "sslmode" | "ssl-mode" | "ssl_mode" => parsed.connection.sslmode = value,
            "host" => parsed.connection.host = single_host(value)?,
            "port" => parsed.connection.port = port(&value)?,
            "user" => parsed.connection.user = value,
            "password" => parsed.password = (!value.is_empty()).then_some(value),
            "dbname" | "database" => parsed.connection.database = value,
            _ => parsed.ignored.push(key),
        }
    }
    Ok(parsed)
}

/// An `sqlite:` URL body (everything after `sqlite:`): `sqlite:app.db`,
/// `sqlite://app.db`, `sqlite:///abs/app.db` or `sqlite::memory:`, with
/// optional `?…` options, which are reported as ignored (Vix opens the file
/// with its own mode).
fn sqlite_url(rest: &str) -> Result<Parsed, ParseError> {
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let file = decode(path)?;
    if file.is_empty() {
        return Err(ParseError::Malformed);
    }
    let mut parsed = Parsed {
        connection: Connection {
            kind: Kind::Sqlite,
            file,
            ..Connection::default()
        },
        ..Parsed::default()
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let key = pair.split_once('=').map_or(pair, |(k, _)| k);
        parsed.ignored.push(decode(key)?);
    }
    Ok(parsed)
}

/// Split a libpq string into `(key, value)` pairs, handling single quotes and
/// backslash escapes.
fn libpq_pairs(text: &str) -> Result<Vec<(String, String)>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut pairs = Vec::new();
    let mut i = 0;
    let skip_ws = |i: &mut usize| {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
    };
    loop {
        skip_ws(&mut i);
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let key: String = chars[start..i].iter().collect();
        skip_ws(&mut i);
        if key.is_empty() || chars.get(i) != Some(&'=') {
            return Err(ParseError::MissingValue(if key.is_empty() {
                "=".to_string()
            } else {
                key
            }));
        }
        i += 1; // the '='
        skip_ws(&mut i);
        let mut value = String::new();
        if chars.get(i) == Some(&'\'') {
            i += 1;
            let mut closed = false;
            while i < chars.len() {
                match chars[i] {
                    '\\' if i + 1 < chars.len() => {
                        value.push(chars[i + 1]);
                        i += 2;
                    }
                    '\'' => {
                        closed = true;
                        i += 1;
                        break;
                    }
                    c => {
                        value.push(c);
                        i += 1;
                    }
                }
            }
            if !closed {
                return Err(ParseError::UnterminatedQuote);
            }
        } else {
            while i < chars.len() && !chars[i].is_whitespace() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    value.push(chars[i + 1]);
                    i += 2;
                } else {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        pairs.push((key, value));
    }
    Ok(pairs)
}

/// A libpq `key=value` string (always PostgreSQL).
fn libpq(text: &str) -> Result<Parsed, ParseError> {
    let mut parsed = Parsed {
        connection: Connection {
            kind: Kind::Postgres,
            ..Connection::default()
        },
        ..Parsed::default()
    };
    let mut hostaddr = String::new();
    for (key, value) in libpq_pairs(text)? {
        match key.to_ascii_lowercase().as_str() {
            "host" => parsed.connection.host = single_host(value)?,
            "hostaddr" => hostaddr = single_host(value)?,
            "port" => parsed.connection.port = port(&value)?,
            "dbname" => parsed.connection.database = value,
            "user" => parsed.connection.user = value,
            "password" => parsed.password = (!value.is_empty()).then_some(value),
            "sslmode" => parsed.connection.sslmode = value,
            _ => parsed.ignored.push(key),
        }
    }
    if parsed.connection.host.is_empty() {
        parsed.connection.host = hostaddr;
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Parsed {
        parse(text).unwrap_or_else(|e| panic!("{text:?} did not parse: {e:?}"))
    }

    #[test]
    fn postgres_url_with_everything() {
        let p = ok(
            "postgresql://ad%20min:p%3Aw%40rd@db.example.com:6543/my%2Fdb?sslmode=verify-full&application_name=vix#frag",
        );
        assert_eq!(p.connection.kind, Kind::Postgres);
        assert_eq!(p.connection.user, "ad min");
        assert_eq!(p.password.as_deref(), Some("p:w@rd"));
        assert_eq!(p.connection.host, "db.example.com");
        assert_eq!(p.connection.port, "6543");
        assert_eq!(p.connection.database, "my/db");
        assert_eq!(p.connection.sslmode, "verify-full");
        assert_eq!(p.ignored, vec!["application_name".to_string()]);
    }

    #[test]
    fn postgres_scheme_aliases_and_minimal_urls() {
        let p = ok("postgres://localhost");
        assert_eq!(p.connection.kind, Kind::Postgres);
        assert_eq!(p.connection.host, "localhost");
        assert_eq!(p.connection.port, "", "empty port means the default");
        assert_eq!(p.connection.database, "");
        assert_eq!(p.password, None);
        let p = ok("  POSTGRES://u@h/d  ");
        assert_eq!(
            (p.connection.user.as_str(), p.connection.host.as_str()),
            ("u", "h")
        );
        assert_eq!(p.connection.database, "d");
        // An empty password is no password.
        assert_eq!(ok("postgres://u:@h/d").password, None);
    }

    #[test]
    fn query_options_override_the_authority() {
        let p = ok(
            "postgres:///app?host=%2Fvar%2Frun%2Fpostgresql&port=5433&user=me&password=pw&dbname=other",
        );
        assert_eq!(p.connection.host, "/var/run/postgresql");
        assert_eq!(p.connection.port, "5433");
        assert_eq!(p.connection.user, "me");
        assert_eq!(p.password.as_deref(), Some("pw"));
        assert_eq!(p.connection.database, "other");
    }

    #[test]
    fn ipv6_hosts_in_brackets() {
        let p = ok("postgres://u@[::1]:5432/d");
        assert_eq!(p.connection.host, "::1");
        assert_eq!(p.connection.port, "5432");
        assert_eq!(ok("postgres://[fe80::1]/d").connection.host, "fe80::1");
        assert_eq!(parse("postgres://[::1/d"), Err(ParseError::Malformed));
        assert_eq!(parse("postgres://[::1]x/d"), Err(ParseError::Malformed));
    }

    #[test]
    fn mysql_url_reads_ssl_mode_spellings() {
        let p = ok("mysql://root:pw@127.0.0.1:3307/shop?ssl-mode=REQUIRED&charset=utf8mb4");
        assert_eq!(p.connection.kind, Kind::Mysql);
        assert_eq!(p.connection.port, "3307");
        assert_eq!(p.connection.database, "shop");
        assert_eq!(p.connection.sslmode, "REQUIRED");
        assert_eq!(p.ignored, vec!["charset".to_string()]);
        assert_eq!(
            ok("mysql://h/d?sslmode=DISABLED").connection.sslmode,
            "DISABLED"
        );
        assert_eq!(
            ok("mysql://h/d?ssl_mode=PREFERRED").connection.sslmode,
            "PREFERRED"
        );
    }

    #[test]
    fn sqlite_urls_in_every_spelling() {
        assert_eq!(ok("sqlite:app.db").connection.file, "app.db");
        assert_eq!(ok("sqlite://app.db").connection.file, "app.db");
        assert_eq!(ok("sqlite:///tmp/app.db").connection.file, "/tmp/app.db");
        assert_eq!(ok("sqlite::memory:").connection.file, ":memory:");
        let p = ok("sqlite:/tmp/my%20app.db?mode=rwc&cache=shared");
        assert_eq!(p.connection.kind, Kind::Sqlite);
        assert_eq!(p.connection.file, "/tmp/my app.db");
        assert_eq!(p.ignored, vec!["mode".to_string(), "cache".to_string()]);
        assert_eq!(p.password, None);
        assert_eq!(parse("sqlite:"), Err(ParseError::Malformed));
        assert_eq!(parse("sqlite://"), Err(ParseError::Malformed));
    }

    #[test]
    fn libpq_strings_with_quotes_and_escapes() {
        let p = ok(
            r"host=db.internal port = 5433 dbname='my app' user=o\'brien password='it\'s \\ secret' sslmode=require connect_timeout=10",
        );
        assert_eq!(p.connection.kind, Kind::Postgres);
        assert_eq!(p.connection.host, "db.internal");
        assert_eq!(p.connection.port, "5433");
        assert_eq!(p.connection.database, "my app");
        assert_eq!(p.connection.user, "o'brien");
        assert_eq!(p.password.as_deref(), Some(r"it's \ secret"));
        assert_eq!(p.connection.sslmode, "require");
        assert_eq!(p.ignored, vec!["connect_timeout".to_string()]);
        // hostaddr stands in for a missing host; an empty quoted value is fine.
        let q = ok("hostaddr=10.0.0.5 dbname='' password=''");
        assert_eq!(q.connection.host, "10.0.0.5");
        assert_eq!(q.connection.database, "");
        assert_eq!(q.password, None);
        // host wins over hostaddr.
        assert_eq!(ok("hostaddr=1.2.3.4 host=h").connection.host, "h");
    }

    #[test]
    fn malformed_strings_are_errors_not_panics() {
        assert_eq!(
            parse("postgres://u:p%zz@h/d"),
            Err(ParseError::BadPercentEncoding)
        );
        assert_eq!(parse("postgres://h/%"), Err(ParseError::BadPercentEncoding));
        assert_eq!(
            parse("postgres://h/%e9"),
            Err(ParseError::BadPercentEncoding)
        );
        assert_eq!(
            parse("postgres://h:99999/d"),
            Err(ParseError::BadPort("99999".into()))
        );
        assert_eq!(
            parse("postgres://h:0/d"),
            Err(ParseError::BadPort("0".into()))
        );
        assert_eq!(
            parse("postgres://h:+5/d"),
            Err(ParseError::BadPort("+5".into()))
        );
        assert_eq!(
            parse("mysql://h:abc/d"),
            Err(ParseError::BadPort("abc".into()))
        );
        assert_eq!(parse("postgres://h1,h2/d"), Err(ParseError::MultipleHosts));
        assert_eq!(parse("host=h1,h2"), Err(ParseError::MultipleHosts));
        assert_eq!(
            parse("host=h port=5432,5433"),
            Err(ParseError::MultipleHosts)
        );
        assert_eq!(
            parse("host=h password='open"),
            Err(ParseError::UnterminatedQuote)
        );
        assert_eq!(
            parse("host=h dbname"),
            Err(ParseError::MissingValue("dbname".into()))
        );
        assert_eq!(
            parse("host=h =x"),
            Err(ParseError::MissingValue("=".into()))
        );
        assert_eq!(parse("host=h port=x"), Err(ParseError::BadPort("x".into())));
    }

    #[test]
    fn ordinary_text_is_not_a_connection_string() {
        for text in [
            "",
            "   ",
            "my connection",
            "pass show db/prod",
            "op read op://vault/db/password",
            "https://example.com/db",
            "x=1",
            "a = b",
            "localhost:5432",
            "C:\\data\\app.db",
            "://nothing",
        ] {
            assert_eq!(
                parse(text),
                Err(ParseError::NotAConnectionString),
                "{text:?}"
            );
        }
    }

    #[test]
    fn errors_display_something() {
        for e in [
            ParseError::NotAConnectionString,
            ParseError::BadPercentEncoding,
            ParseError::BadPort("x".into()),
            ParseError::UnterminatedQuote,
            ParseError::MissingValue("k".into()),
            ParseError::MultipleHosts,
            ParseError::Malformed,
        ] {
            assert_ne!(e.to_string(), "");
        }
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        // A cheap stand-in for the fuzz target: every prefix of some nasty
        // inputs parses or errors, never panics.
        for text in [
            "postgres://[::1]:%zz/%ff?%=&=%&",
            "host='\\",
            "host=\\",
            "sqlite:%",
            "mysql://@:/?",
            "postgresql://ü:ö@ä:1/ß",
            "user=é password='日本\\'語'",
        ] {
            for end in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
                let _ = parse(&text[..end]);
            }
        }
    }
}
