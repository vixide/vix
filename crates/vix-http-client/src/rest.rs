//! The "REST Client" layer over the single-request parser: several requests in
//! one buffer, `{{variables}}`, named-response chaining and `curl` export.
//!
//! A buffer holds any number of requests separated by lines starting with
//! `###`. [`block_at`] finds the one under a line, [`file_variables`] collects
//! the `@name = value` definitions, and [`expand`] substitutes every `{{...}}`
//! in a block before it is handed to [`crate::parse_request`]. Everything here
//! is pure apart from the clock, the random source and the environment, which
//! only the system variables read.

use crate::{Request, Response};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// A request's line range in the buffer: `start..end`, end exclusive. The
/// `###` separator lines belong to no block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// First line of the block.
    pub start: usize,
    /// One past the last line of the block.
    pub end: usize,
}

/// Split `text` into request blocks at `###` separator lines. Blocks with no
/// content (only blanks, comments or variable definitions) are dropped.
#[must_use]
pub fn blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..=lines.len() {
        if i == lines.len() || lines[i].trim_start().starts_with("###") {
            if has_request(&lines[start..i]) {
                out.push(Block { start, end: i });
            }
            start = i + 1;
        }
    }
    out
}

/// Whether `lines` hold a request line (a non-blank line that is not a comment
/// or a variable definition).
fn has_request(lines: &[&str]) -> bool {
    lines.iter().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#') && !t.starts_with("//") && !t.starts_with('@')
    })
}

/// The block containing `line`, or the nearest block before it when `line`
/// falls on a separator, a blank gap or a variable definition. Falls back to
/// the first block after `line` when nothing precedes it.
#[must_use]
pub fn block_at(text: &str, line: usize) -> Option<Block> {
    let all = blocks(text);
    all.iter()
        .find(|b| (b.start..b.end).contains(&line))
        .or_else(|| all.iter().rev().find(|b| b.end <= line))
        .or_else(|| all.first())
        .copied()
}

/// The text of `block` within `text`, lines joined by `\n`.
#[must_use]
pub fn block_text(text: &str, block: Block) -> String {
    text.lines()
        .skip(block.start)
        .take(block.end - block.start)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `@name = value` file variable in `text`, in order. Later definitions
/// win when resolved. A value may itself contain `{{...}}`.
#[must_use]
pub fn file_variables(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix('@')?;
            let (name, value) = rest.split_once('=')?;
            let name = name.trim();
            (!name.is_empty() && !name.contains(char::is_whitespace))
                .then(|| (name.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// The name from a `# @name foo` (or `// @name foo`) line in `block`.
#[must_use]
pub fn request_name(block: &str) -> Option<String> {
    block.lines().find_map(|l| {
        let t = l
            .trim()
            .trim_start_matches('#')
            .trim_start_matches('/')
            .trim();
        let rest = t.strip_prefix("@name")?;
        let name = rest.trim_start_matches(['=', ' ', '\t']).trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// Why a `{{...}}` could not be expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VarError {
    /// No file variable, system variable or named request has this name.
    Unknown(String),
    /// A response reference names a request that has not been sent yet.
    NotSent(String),
    /// A response reference could not be resolved (bad path, not JSON, no
    /// such header).
    BadReference(String),
    /// A system variable was given bad arguments.
    BadArguments(String),
    /// Variables refer to each other too deeply (probably a cycle).
    TooDeep(String),
    /// `{{$aadToken}}` needs the Azure CLI (`az`), which is not installed.
    AzureCliMissing,
    /// The Azure CLI ran but gave no token (not signed in, bad resource).
    /// Deliberately carries no detail: nothing from the CLI is echoed.
    AzureTokenFailed,
    /// A `{{` with no closing `}}`.
    Unterminated,
}

impl std::fmt::Display for VarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(n) => write!(f, "unknown variable {{{{{n}}}}}"),
            Self::NotSent(n) => write!(f, "request `{n}` has not been sent yet"),
            Self::BadReference(n) => write!(f, "cannot resolve {{{{{n}}}}}"),
            Self::BadArguments(n) => write!(f, "bad arguments for {{{{{n}}}}}"),
            Self::TooDeep(n) => write!(f, "variable {{{{{n}}}}} nests too deeply"),
            Self::AzureCliMissing => f.write_str("the Azure CLI (az) is not installed"),
            Self::AzureTokenFailed => f.write_str("the Azure CLI could not get a token"),
            Self::Unterminated => f.write_str("unterminated {{ in request"),
        }
    }
}

impl std::error::Error for VarError {}

/// What [`expand`] needs to resolve a variable.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// `@name = value` definitions (see [`file_variables`]).
    pub vars: &'a [(String, String)],
    /// Responses of named requests already sent.
    pub named: &'a HashMap<String, Response>,
    /// Directory of the `.http` file, for `$dotenv`.
    pub base_dir: Option<&'a Path>,
    /// Current time, milliseconds since the Unix epoch.
    pub now_ms: i64,
    /// The local zone's offset from UTC in minutes (east positive), for
    /// `{{$localDatetime}}`.
    pub local_offset_min: i32,
}

/// Deepest chain of variables referring to variables.
const MAX_DEPTH: usize = 8;

/// Replace every `{{...}}` in `text`.
///
/// # Errors
/// A [`VarError`] for the first reference that cannot be resolved.
pub fn expand(text: &str, ctx: &Context<'_>) -> Result<String, VarError> {
    expand_depth(text, ctx, 0)
}

fn expand_depth(text: &str, ctx: &Context<'_>, depth: usize) -> Result<String, VarError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let close = after.find("}}").ok_or(VarError::Unterminated)?;
        let name = after[..close].trim();
        if depth >= MAX_DEPTH {
            return Err(VarError::TooDeep(name.to_string()));
        }
        out.push_str(&resolve(name, ctx, depth)?);
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn resolve(name: &str, ctx: &Context<'_>, depth: usize) -> Result<String, VarError> {
    if let Some(sys) = name.strip_prefix('$') {
        return system_variable(sys, name, ctx);
    }
    if let Some((req, tail)) = name.split_once(".response.") {
        let resp = ctx
            .named
            .get(req)
            .ok_or_else(|| VarError::NotSent(req.to_string()))?;
        return response_part(resp, tail).ok_or_else(|| VarError::BadReference(name.to_string()));
    }
    // The last definition of a name wins.
    let value = ctx
        .vars
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| VarError::Unknown(name.to_string()))?;
    expand_depth(value, ctx, depth + 1)
}

/// `body`, `body.$.path` (JSON), `body.*`, or `headers.Name`.
fn response_part(resp: &Response, tail: &str) -> Option<String> {
    if let Some(h) = tail.strip_prefix("headers.") {
        return resp
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(h))
            .map(|(_, v)| v.clone());
    }
    let body = tail.strip_prefix("body")?;
    match body {
        "" | ".*" => Some(resp.body.clone()),
        _ => {
            let path = body.strip_prefix('.')?;
            if path.starts_with('/') {
                crate::xpath::query(&resp.body, path)
            } else {
                json_path(&resp.body, path)
            }
        }
    }
}

/// Evaluate a JSON path such as `$.data.items[0].id` against `body`. Strings
/// come back unquoted; other values as JSON text.
#[must_use]
pub fn json_path(body: &str, path: &str) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_str(body).ok()?;
    let path = path.strip_prefix('$')?;
    let mut rest = path;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            let (key, tail) = r.split_at(end);
            value = value.get(key)?.clone();
            rest = tail;
        } else {
            let r = rest.strip_prefix('[')?;
            let close = r.find(']')?;
            let idx: usize = r[..close].trim().parse().ok()?;
            value = value.get(idx)?.clone();
            rest = &r[close + 1..];
        }
    }
    Some(match value {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

fn system_variable(sys: &str, full: &str, ctx: &Context<'_>) -> Result<String, VarError> {
    let mut parts = sys.split_whitespace();
    let bad = || VarError::BadArguments(full.to_string());
    match parts.next().unwrap_or("") {
        "guid" => Ok(uuid::Uuid::new_v4().to_string()),
        "randomInt" => {
            let min: i64 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
            let max: i64 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
            if max <= min {
                return Err(bad());
            }
            let span = u64::try_from(max - min).map_err(|_| bad())?;
            Ok((min + i64::try_from(random_u64() % span).map_err(|_| bad())?).to_string())
        }
        "timestamp" => {
            let ms = ctx.now_ms + offset_ms(parts.next(), parts.next()).ok_or_else(bad)?;
            Ok((ms / 1000).to_string())
        }
        "datetime" => {
            let format = parts.next().ok_or_else(bad)?;
            let ms = ctx.now_ms + offset_ms(parts.next(), parts.next()).ok_or_else(bad)?;
            match format {
                "iso8601" => Ok(iso8601(ms)),
                "rfc1123" => Ok(rfc1123(ms)),
                _ => Err(bad()),
            }
        }
        "localDatetime" => {
            let format = parts.next().ok_or_else(bad)?;
            let ms = ctx.now_ms + offset_ms(parts.next(), parts.next()).ok_or_else(bad)?;
            let off = ctx.local_offset_min;
            match format {
                "iso8601" => Ok(local_iso8601(ms, off)),
                "rfc1123" => Ok(local_rfc1123(ms, off)),
                _ => Err(bad()),
            }
        }
        "aadToken" => {
            let args: Vec<&str> = parts.collect();
            crate::aad::token(&args).map_err(|e| match e {
                crate::aad::AadError::CliMissing => VarError::AzureCliMissing,
                crate::aad::AadError::BadArgument => bad(),
                crate::aad::AadError::Failed => VarError::AzureTokenFailed,
            })
        }
        "processEnv" => {
            let name = parts.next().ok_or_else(bad)?;
            std::env::var(name).map_err(|_| VarError::BadReference(full.to_string()))
        }
        "dotenv" => {
            let name = parts.next().ok_or_else(bad)?;
            let dir = ctx.base_dir.ok_or_else(bad)?;
            let text = std::fs::read_to_string(dir.join(".env"))
                .map_err(|_| VarError::BadReference(full.to_string()))?;
            dotenv_get(&text, name).ok_or_else(|| VarError::BadReference(full.to_string()))
        }
        _ => Err(VarError::Unknown(full.to_string())),
    }
}

fn random_u64() -> u64 {
    let mut buf = [0_u8; 8];
    // A failed OS RNG degrades to a fixed value rather than aborting a send.
    let _ = getrandom::getrandom(&mut buf);
    u64::from_le_bytes(buf)
}

/// `<n> <unit>` as milliseconds; no arguments means zero.
fn offset_ms(n: Option<&str>, unit: Option<&str>) -> Option<i64> {
    let (Some(n), Some(unit)) = (n, unit) else {
        return if n.is_none() { Some(0) } else { None };
    };
    let n: i64 = n.parse().ok()?;
    let per = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 604_800_000,
        _ => return None,
    };
    n.checked_mul(per)
}

/// `KEY=value` lookup in `.env` text; strips quotes and `export `.
fn dotenv_get(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim().strip_prefix("export ").unwrap_or(l.trim());
        let (k, v) = l.split_once('=')?;
        (k.trim() == name).then(|| v.trim().trim_matches(['"', '\'']).to_string())
    })
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `+hh:mm` (or `-hh:mm`) for an offset in minutes.
fn offset_colon(off: i32) -> String {
    let (sign, a) = (if off < 0 { '-' } else { '+' }, off.abs());
    format!("{sign}{:02}:{:02}", a / 60, a % 60)
}

fn local_iso8601(ms: i64, off: i32) -> String {
    let shifted = iso8601(ms + i64::from(off) * 60_000);
    // Swap the trailing `Z` for the real offset.
    format!("{}{}", shifted.trim_end_matches('Z'), offset_colon(off))
}

fn local_rfc1123(ms: i64, off: i32) -> String {
    let shifted = rfc1123(ms + i64::from(off) * 60_000);
    let zone = offset_colon(off).replace(':', "");
    format!("{}{zone}", shifted.trim_end_matches("GMT"))
}

fn iso8601(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

fn rfc1123(ms: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{}, {d:02} {} {y:04} {:02}:{:02}:{:02} GMT",
        DAYS[usize::try_from(days.rem_euclid(7)).unwrap_or(0)],
        MONTHS[usize::try_from(m - 1).unwrap_or(0)],
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60
    )
}

/// Replace a body that is a single `< path` line (with the space) with the file's contents.
///
/// The path must stay inside `base` (relative, no `..`): a `.http` file can
/// come from anywhere, and without this a crafted one could send a private
/// local file such as an SSH key to its own URL.
///
/// # Errors
/// A message when the path escapes `base` or the file cannot be read.
pub fn load_body_file(req: &mut Request, base: &Path) -> Result<(), String> {
    let body = req.body.trim();
    let Some(rel) = body.strip_prefix("< ").filter(|_| !body.contains('\n')) else {
        return Ok(());
    };
    let rel = rel.trim();
    let path = Path::new(rel);
    if rel.is_empty()
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!(
            "body file must be inside the .http file's folder: {rel}"
        ));
    }
    let full: PathBuf = base.join(path);
    req.body = std::fs::read_to_string(&full).map_err(|e| format!("{}: {e}", full.display()))?;
    Ok(())
}

/// Rewrite an `Authorization: Basic user:pass` (or `user pass`) header to the
/// base64 form. A value that is already base64 is left alone.
pub fn encode_basic_auth(req: &mut Request) {
    use base64::Engine as _;
    for (name, value) in &mut req.headers {
        if !name.eq_ignore_ascii_case("authorization") {
            continue;
        }
        let Some(cred) = value.strip_prefix("Basic ").map(str::trim) else {
            continue;
        };
        let joined = if cred.contains(':') {
            cred.to_string()
        } else if let Some((u, p)) = cred.split_once(' ') {
            format!("{u}:{}", p.trim())
        } else {
            continue;
        };
        *value = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(joined)
        );
    }
}

/// `req` as a `curl` command line.
#[must_use]
pub fn to_curl(req: &Request) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let mut out = format!("curl -X {} {}", req.method, q(&req.url));
    for (name, value) in &req.headers {
        out.push_str(" \\\n  -H ");
        out.push_str(&q(&format!("{name}: {value}")));
    }
    if !req.body.is_empty() {
        out.push_str(" \\\n  --data-raw ");
        out.push_str(&q(&req.body));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(vars: &'a [(String, String)], named: &'a HashMap<String, Response>) -> Context<'a> {
        Context {
            vars,
            named,
            base_dir: None,
            now_ms: 1_700_000_000_000,
            local_offset_min: 60,
        }
    }

    fn resp(body: &str) -> Response {
        Response {
            status: 200,
            reason: "OK".into(),
            version: "HTTP/1.1".into(),
            headers: vec![("X-Token".into(), "abc".into())],
            body: body.into(),
        }
    }

    #[test]
    fn splits_on_hashes_and_finds_the_block_at_a_line() {
        let text =
            "GET https://a/1\n###\n# note\nPOST https://a/2\n\nbody\n### x\n\n###\nGET https://a/3";
        let all = blocks(text);
        assert_eq!(all.len(), 3);
        assert_eq!(block_at(text, 0), Some(all[0]));
        assert_eq!(block_at(text, 3), Some(all[1]));
        // On the separator after block 2 the earlier block is chosen.
        assert_eq!(block_at(text, 6), Some(all[1]));
        assert_eq!(block_at(text, 9), Some(all[2]));
        assert!(block_text(text, all[1]).starts_with("# note"));
    }

    #[test]
    fn variable_only_prelude_is_not_a_block() {
        let text = "@host = https://e.com\n\n###\nGET {{host}}/x";
        assert_eq!(blocks(text).len(), 1);
        assert_eq!(
            file_variables(text),
            vec![("host".to_string(), "https://e.com".to_string())]
        );
        // A cursor on the prelude picks the first request.
        assert_eq!(block_at(text, 0), blocks(text).first().copied());
    }

    #[test]
    fn names_a_request() {
        assert_eq!(
            request_name("# @name login\nPOST https://e/"),
            Some("login".into())
        );
        assert_eq!(
            request_name("// @name = x\nGET https://e/"),
            Some("x".into())
        );
        assert_eq!(request_name("GET https://e/"), None);
    }

    #[test]
    fn expands_file_variables_recursively_and_rejects_cycles() {
        let vars = vec![
            ("host".to_string(), "https://e.com".to_string()),
            ("url".to_string(), "{{host}}/v1".to_string()),
            ("a".to_string(), "{{b}}".to_string()),
            ("b".to_string(), "{{a}}".to_string()),
        ];
        let named = HashMap::new();
        let c = ctx(&vars, &named);
        assert_eq!(
            expand("GET {{ url }}/x", &c).unwrap(),
            "GET https://e.com/v1/x"
        );
        assert!(matches!(expand("{{a}}", &c), Err(VarError::TooDeep(_))));
        assert_eq!(
            expand("{{nope}}", &c),
            Err(VarError::Unknown("nope".into()))
        );
        assert_eq!(expand("{{oops", &c), Err(VarError::Unterminated));
    }

    #[test]
    fn chains_named_responses() {
        let vars = vec![];
        let mut named = HashMap::new();
        named.insert(
            "login".to_string(),
            resp(r#"{"token":"t0k","data":{"items":[{"id":7}]}}"#),
        );
        let c = ctx(&vars, &named);
        assert_eq!(
            expand("{{login.response.body.$.token}}", &c).unwrap(),
            "t0k"
        );
        assert_eq!(
            expand("{{login.response.body.$.data.items[0].id}}", &c).unwrap(),
            "7"
        );
        assert_eq!(
            expand("{{login.response.headers.x-token}}", &c).unwrap(),
            "abc"
        );
        assert!(
            expand("{{login.response.body.*}}", &c)
                .unwrap()
                .contains("t0k")
        );
        assert_eq!(
            expand("{{other.response.body.$.x}}", &c),
            Err(VarError::NotSent("other".into()))
        );
        assert!(matches!(
            expand("{{login.response.body.$.missing}}", &c),
            Err(VarError::BadReference(_))
        ));
    }

    #[test]
    fn system_variables_have_the_right_shape() {
        let vars = vec![];
        let named = HashMap::new();
        let c = ctx(&vars, &named);
        assert_eq!(expand("{{$guid}}", &c).unwrap().len(), 36);
        assert_eq!(expand("{{$timestamp}}", &c).unwrap(), "1700000000");
        assert_eq!(expand("{{$timestamp -1 h}}", &c).unwrap(), "1699996400");
        assert_eq!(
            expand("{{$datetime iso8601}}", &c).unwrap(),
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(
            expand("{{$datetime rfc1123}}", &c).unwrap(),
            "Tue, 14 Nov 2023 22:13:20 GMT"
        );
        let n: i64 = expand("{{$randomInt 5 8}}", &c).unwrap().parse().unwrap();
        assert!((5..8).contains(&n));
        assert!(matches!(
            expand("{{$randomInt 8 5}}", &c),
            Err(VarError::BadArguments(_))
        ));
        assert!(matches!(
            expand("{{$bogus}}", &c),
            Err(VarError::Unknown(_))
        ));
        assert_ne!(expand("{{$processEnv PATH}}", &c).unwrap(), "");
    }

    #[test]
    fn local_datetime_applies_the_zone_offset() {
        let vars = vec![];
        let named = HashMap::new();
        let c = ctx(&vars, &named);
        assert_eq!(
            expand("{{$localDatetime iso8601}}", &c).unwrap(),
            "2023-11-14T23:13:20.000+01:00"
        );
        assert_eq!(
            expand("{{$localDatetime rfc1123}}", &c).unwrap(),
            "Tue, 14 Nov 2023 23:13:20 +0100"
        );
        assert!(matches!(
            expand("{{$localDatetime bogus}}", &c),
            Err(VarError::BadArguments(_))
        ));
        assert_eq!(offset_colon(-330), "-05:30");
    }

    #[test]
    fn aad_token_errors_map_without_leaking_detail() {
        let vars = vec![];
        let named = HashMap::new();
        let c = ctx(&vars, &named);
        assert!(matches!(
            expand("{{$aadToken --evil}}", &c),
            Err(VarError::BadArguments(_))
        ));
    }

    #[test]
    fn chains_xml_responses_with_xpath() {
        let vars = vec![];
        let mut named = HashMap::new();
        named.insert(
            "soap".to_string(),
            resp("<env><body><id n=\"9\">42</id></body></env>"),
        );
        let c = ctx(&vars, &named);
        assert_eq!(expand("{{soap.response.body.//id}}", &c).unwrap(), "42");
        assert_eq!(expand("{{soap.response.body.//id/@n}}", &c).unwrap(), "9");
        assert!(matches!(
            expand("{{soap.response.body.//nope}}", &c),
            Err(VarError::BadReference(_))
        ));
    }

    #[test]
    fn json_path_handles_roots_arrays_and_misses() {
        assert_eq!(json_path("[1,2,3]", "$[1]"), Some("2".into()));
        assert_eq!(
            json_path(r#"{"a":{"b":true}}"#, "$.a.b"),
            Some("true".into())
        );
        assert_eq!(json_path(r#"{"a":1}"#, "$.b"), None);
        assert_eq!(json_path("not json", "$.a"), None);
        assert_eq!(json_path("{}", "a"), None);
    }

    #[test]
    fn civil_dates_are_right_on_the_boundaries() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(rfc1123(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(iso8601(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn basic_auth_is_encoded_once() {
        let mut r = Request {
            method: "GET".into(),
            url: "https://e/".into(),
            headers: vec![("Authorization".into(), "Basic user:pass".into())],
            body: String::new(),
        };
        encode_basic_auth(&mut r);
        assert_eq!(r.headers[0].1, "Basic dXNlcjpwYXNz");
        encode_basic_auth(&mut r);
        assert_eq!(r.headers[0].1, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn body_files_stay_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("vix-rest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.json"), "{\"a\":1}").unwrap();
        let mk = |body: &str| Request {
            method: "POST".into(),
            url: "https://e/".into(),
            headers: vec![],
            body: body.into(),
        };
        let mut ok = mk("< ./b.json");
        load_body_file(&mut ok, &dir).unwrap();
        assert_eq!(ok.body, "{\"a\":1}");
        assert!(load_body_file(&mut mk("< ../secret"), &dir).is_err());
        assert!(load_body_file(&mut mk("< /etc/passwd"), &dir).is_err());
        let mut plain = mk("hello <b>");
        load_body_file(&mut plain, &dir).unwrap();
        assert_eq!(plain.body, "hello <b>");
        let mut xml = mk("<a/>");
        load_body_file(&mut xml, &dir).unwrap();
        assert_eq!(xml.body, "<a/>");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn curl_quotes_single_quotes() {
        let r = Request {
            method: "POST".into(),
            url: "https://e/x".into(),
            headers: vec![("A".into(), "b".into())],
            body: "it's".into(),
        };
        assert_eq!(
            to_curl(&r),
            "curl -X POST 'https://e/x' \\\n  -H 'A: b' \\\n  --data-raw 'it'\\''s'"
        );
    }
}
