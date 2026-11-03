//! Parsers for what the engines print with `--format json`.
//!
//! Docker prints **one JSON object per line** (NDJSON) for `ps`, `images`,
//! `volume ls` and `network ls`; Podman prints **one JSON array**; `inspect`
//! prints an array on every engine. [`records`] accepts both shapes. The typed
//! readers are deliberately tolerant: key names are matched case-insensitively
//! (Docker says `ID`, Podman `Id` or `id`), a name may be a string or an array,
//! and a field an engine does not report is left empty rather than an error.
//!
//! Tolerance is the point: engine output formats drift between versions, and
//! a panel that shows a blank column is better than one that shows nothing.
//! Only structurally invalid JSON is an [`OciError::Parse`]. Owning spec:
//! `crates/vix-oci/spec/index.md`.
//!
//! ```
//! use vix_oci::parse;
//! // Docker style: one object per line.
//! let docker = parse::volumes("{\"Name\":\"data\",\"Driver\":\"local\"}\n").unwrap();
//! // Podman style: one array.
//! let podman = parse::volumes("[{\"Name\":\"data\",\"Driver\":\"local\"}]").unwrap();
//! assert_eq!(docker, podman);
//! assert_eq!(docker[0].name, "data");
//! ```

use crate::OciError;
use serde_json::Value;

/// Split engine output into its JSON records, whichever shape it has.
///
/// # Errors
/// [`OciError::Parse`] when a line or the array is not valid JSON.
pub fn records(text: &str) -> Result<Vec<Value>, OciError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    // A leading `[` means Podman-style single array (or `inspect` output);
    // otherwise it is NDJSON, one record per non-blank line.
    if text.starts_with('[') {
        return match serde_json::from_str::<Value>(text) {
            Ok(Value::Array(items)) => Ok(items),
            Ok(_) => Err(OciError::Parse("expected a JSON array".into())),
            Err(e) => Err(OciError::Parse(e.to_string())),
        };
    }
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<Value>(l).map_err(|e| OciError::Parse(e.to_string())))
        .collect()
}

/// The first present key of `keys`, matched case-insensitively. A JSON `null`
/// counts as absent so a later alias can supply the value.
fn field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let obj = v.as_object()?;
    keys.iter().find_map(|k| {
        obj.iter()
            .find(|(name, val)| name.eq_ignore_ascii_case(k) && !val.is_null())
            .map(|(_, val)| val)
    })
}

/// A field as display text: strings as-is, arrays of strings joined by `, `,
/// numbers formatted, anything else empty.
fn text(v: &Value, keys: &[&str]) -> String {
    match field(v, keys) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

/// A creation time as text: strings as-is, a Unix-seconds number as an ISO date.
fn created(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| match field(v, &[k]) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Number(n)) => n.as_i64().map(iso_date),
            _ => None,
        })
        .unwrap_or_default()
}

/// `YYYY-MM-DD` for Unix seconds (proleptic Gregorian, UTC).
fn iso_date(secs: i64) -> String {
    // Howard Hinnant's `civil_from_days` algorithm: days since 1970-01-01 are
    // shifted to 0000-03-01 so leap days fall at the end of the "year", then
    // split into 400-year eras. Avoids a date-library dependency.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A byte count as `12.3MB` (decimal units, as Docker prints).
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    // Work in tenths of a unit to get one decimal place with integers only;
    // `>= 10_000` means "at least 1000.0 of this unit", so step up.
    let mut tenths = bytes.saturating_mul(10);
    let mut unit = 0;
    while tenths >= 10_000 && unit + 1 < UNITS.len() {
        tenths /= 1000;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{}.{}{}", tenths / 10, tenths % 10, UNITS[unit])
    }
}

/// A container, from `ps`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Container {
    /// The container id.
    pub id: String,
    /// Its name (several are joined by `, `).
    pub name: String,
    /// The image it was created from.
    pub image: String,
    /// `running`, `exited`, `paused`, `created`...
    pub state: String,
    /// The engine's status text (`Up 2 hours`); empty on Podman.
    pub status: String,
    /// Published ports as display text.
    pub ports: String,
    /// The host ports published, for Open in Browser.
    pub host_ports: Vec<u16>,
    /// When it was created.
    pub created: String,
}

impl Container {
    /// Whether the container is running (or paused), so Stop and Shell apply.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state.eq_ignore_ascii_case("running") || self.is_paused()
    }

    /// Whether the container is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.eq_ignore_ascii_case("paused")
    }

    /// The shortened id engines display (12 characters).
    ///
    /// Container ids are hexadecimal, so slicing by byte is safe for real
    /// engine output; a non-ASCII id from a hostile source could in theory
    /// split a character, which would panic.
    #[must_use]
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(12)]
    }
}

/// Docker's `0.0.0.0:8080->80/tcp, :::8080->80/tcp` as host port numbers,
/// deduplicated (IPv4 and IPv6 bindings repeat the same port).
fn host_ports_from_text(s: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for part in s.split(',') {
        if let Some((host, _)) = part.split_once("->")
            && let Some(port) = host.rsplit(':').next().and_then(|p| p.trim().parse().ok())
            && !out.contains(&port)
        {
            out.push(port);
        }
    }
    out
}

/// Parse `ps --format json` output.
///
/// # Errors
/// [`OciError::Parse`] for malformed JSON.
pub fn containers(out: &str) -> Result<Vec<Container>, OciError> {
    Ok(records(out)?
        .iter()
        .map(|r| {
            let (ports, host_ports) = match field(r, &["Ports"]) {
                Some(Value::String(s)) => (s.clone(), host_ports_from_text(s)),
                Some(Value::Array(items)) => {
                    // Podman: [{"host_port":8080,"container_port":80,"protocol":"tcp"}]
                    // Entries missing either port (an unpublished port) are
                    // skipped.
                    let mut shown = Vec::new();
                    let mut hosts = Vec::new();
                    for p in items {
                        let host = field(p, &["host_port"]).and_then(Value::as_u64);
                        let ctr = field(p, &["container_port"]).and_then(Value::as_u64);
                        let proto = text(p, &["protocol"]);
                        if let (Some(h), Some(c)) = (host, ctr) {
                            shown.push(format!("{h}->{c}/{proto}"));
                            if let Ok(h) = u16::try_from(h)
                                && !hosts.contains(&h)
                            {
                                hosts.push(h);
                            }
                        }
                    }
                    (shown.join(", "), hosts)
                }
                _ => (String::new(), Vec::new()),
            };
            Container {
                id: text(r, &["ID", "Id"]),
                name: text(r, &["Names", "Name"]),
                image: text(r, &["Image"]),
                state: text(r, &["State"]),
                status: text(r, &["Status"]),
                ports,
                host_ports,
                created: created(r, &["CreatedAt", "Created"]),
            }
        })
        .collect())
}

/// An image, from `images`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Image {
    /// The image id.
    pub id: String,
    /// The repository, or `<none>`.
    pub repository: String,
    /// The tag, or `<none>`.
    pub tag: String,
    /// Size as display text.
    pub size: String,
    /// When it was created.
    pub created: String,
}

impl Image {
    /// What to pass to the engine to name this image: `repository:tag` when it
    /// has both, else the id.
    #[must_use]
    pub fn reference(&self) -> String {
        if self.repository.is_empty()
            || self.repository == "<none>"
            || self.tag.is_empty()
            || self.tag == "<none>"
        {
            self.id.clone()
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }
}

/// Split `registry/name:tag` at the tag colon (the last `:` after the last `/`).
/// The `/` rule keeps a registry port (`localhost:5000/app`) from being read
/// as a tag; a reference with no tag gets `latest`.
fn split_reference(r: &str) -> (String, String) {
    let slash = r.rfind('/').map_or(0, |i| i + 1);
    match r[slash..].rfind(':') {
        Some(i) => (r[..slash + i].to_string(), r[slash + i + 1..].to_string()),
        None => (r.to_string(), "latest".to_string()),
    }
}

/// Parse `images --format json` output.
///
/// # Errors
/// [`OciError::Parse`] for malformed JSON.
pub fn images(out: &str) -> Result<Vec<Image>, OciError> {
    Ok(records(out)?
        .iter()
        .map(|r| {
            let (repository, tag) = {
                let repo = text(r, &["Repository"]);
                // Docker reports Repository/Tag columns; Podman reports full
                // references instead.
                if repo.is_empty() {
                    // Podman: Names / RepoTags hold full references.
                    let first = field(r, &["Names", "RepoTags"])
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(Value::as_str);
                    first.map_or_else(
                        || ("<none>".to_string(), "<none>".to_string()),
                        split_reference,
                    )
                } else {
                    (repo, text(r, &["Tag"]))
                }
            };
            let size = match field(r, &["Size"]) {
                Some(Value::Number(n)) => n.as_u64().map(human_size).unwrap_or_default(),
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let created_text = {
                let since = text(r, &["CreatedSince"]);
                if since.is_empty() {
                    created(r, &["CreatedAt", "Created"])
                } else {
                    since
                }
            };
            Image {
                id: text(r, &["ID", "Id"]),
                repository,
                tag,
                size,
                created: created_text,
            }
        })
        .collect())
}

/// A volume, from `volume ls`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Volume {
    /// The volume name.
    pub name: String,
    /// Its driver (`local`).
    pub driver: String,
    /// Where it lives on the host, when the engine says.
    pub mountpoint: String,
}

/// Parse `volume ls --format json` output.
///
/// # Errors
/// [`OciError::Parse`] for malformed JSON.
pub fn volumes(out: &str) -> Result<Vec<Volume>, OciError> {
    Ok(records(out)?
        .iter()
        .map(|r| Volume {
            name: text(r, &["Name"]),
            driver: text(r, &["Driver"]),
            mountpoint: text(r, &["Mountpoint"]),
        })
        .collect())
}

/// A network, from `network ls`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Network {
    /// The network id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its driver (`bridge`).
    pub driver: String,
    /// Its scope (`local`); empty on Podman.
    pub scope: String,
}

/// Parse `network ls --format json` output.
///
/// # Errors
/// [`OciError::Parse`] for malformed JSON.
pub fn networks(out: &str) -> Result<Vec<Network>, OciError> {
    Ok(records(out)?
        .iter()
        .map(|r| Network {
            id: text(r, &["ID", "Id"]),
            name: text(r, &["Name"]),
            driver: text(r, &["Driver"]),
            scope: text(r, &["Scope"]),
        })
        .collect())
}

/// A Podman pod, from `pod ps`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pod {
    /// The pod id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its status (`Running`).
    pub status: String,
    /// How many containers it holds.
    pub containers: usize,
}

/// Parse `pod ps --format json` output.
///
/// # Errors
/// [`OciError::Parse`] for malformed JSON.
pub fn pods(out: &str) -> Result<Vec<Pod>, OciError> {
    Ok(records(out)?
        .iter()
        .map(|r| Pod {
            id: text(r, &["Id", "ID"]),
            name: text(r, &["Name"]),
            status: text(r, &["Status"]),
            containers: field(r, &["Containers"]).map_or(0, |c| match c {
                Value::Array(a) => a.len(),
                Value::Number(n) => n.as_u64().map_or(0, |n| usize::try_from(n).unwrap_or(0)),
                _ => 0,
            }),
        })
        .collect())
}

/// Non-empty trimmed lines, for `compose config --services`.
#[must_use]
pub fn lines(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Pretty-print `inspect` output for a read-only JSON tab. Unparseable output
/// is returned unchanged.
#[must_use]
pub fn pretty(out: &str) -> String {
    serde_json::from_str::<Value>(out)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| out.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_record_shapes_parse() {
        assert_eq!(records("").unwrap().len(), 0);
        assert_eq!(records("{\"a\":1}\n\n{\"a\":2}\n").unwrap().len(), 2);
        assert_eq!(records("[{\"a\":1},{\"a\":2}]").unwrap().len(), 2);
        assert!(matches!(records("{oops"), Err(OciError::Parse(_))));
        assert!(matches!(records("[1,"), Err(OciError::Parse(_))));
    }

    #[test]
    fn dates_and_sizes() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_700_000_000), "2023-11-14");
        assert_eq!(human_size(512), "512B");
        assert_eq!(human_size(12_300_000), "12.3MB");
    }

    #[test]
    fn references_split_at_the_tag_colon() {
        assert_eq!(
            split_reference("localhost:5000/app"),
            ("localhost:5000/app".into(), "latest".into())
        );
        assert_eq!(
            split_reference("docker.io/library/alpine:3.19"),
            ("docker.io/library/alpine".into(), "3.19".into())
        );
    }

    #[test]
    fn host_ports_from_docker_text() {
        assert_eq!(
            host_ports_from_text("0.0.0.0:8080->80/tcp, :::8080->80/tcp, 443/tcp"),
            vec![8080]
        );
    }
}
