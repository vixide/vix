//! Tags (`:tag1:tag2:`), the `:PROPERTIES:` drawer, archiving a subtree,
//! and date/timestamp handling (parsing, shifting the date under the
//! cursor, and `SCHEDULED:`/`DEADLINE:` planning lines). Extracted from
//! `lib.rs` (T516). `TAGS` and `is_planning` stay `pub(crate)`:
//! `columns.rs` needs both (to render a column-view table cell and to
//! skip a headline's planning line when placing a dblock), and
//! `text_refs::column_view` needs `TAGS` too. `line_of_char` stays
//! `pub(crate)` for `text_refs`'s Footnotes functions.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use super::{drawer_range, governing, headline_level, relevel, subtree_range};
use crate::agenda::days_from_civil;

// ----- Tags & properties -----------------------------------------------------

/// The trailing `:tag1:tag2:` group on a headline, with leading whitespace.
pub(crate) static TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]+(:(?:[A-Za-z0-9_@#%]+:)+)[ \t]*$").expect("tags regex"));

/// The tags of the headline governing `line`, colon-joined without the outer
/// colons (e.g. `"work:urgent"`, `""` when untagged). `None` off any subtree.
#[must_use]
pub fn get_tags(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    Some(
        TAGS.captures(lines[h])
            .map_or_else(String::new, |c| c[1].trim_matches(':').replace("::", ":")),
    )
}

/// Set the tags of the headline governing `line` (Org `C-c C-q`). `tags` may be
/// colon-, comma-, or space-separated; empty input removes all tags.
#[must_use]
pub fn set_tags(text: &str, line: usize, tags: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let head = TAGS.replace(lines[h], "").trim_end().to_string();
    let list: Vec<&str> = tags
        .split([':', ',', ' ', '\t'])
        .filter(|s| !s.is_empty())
        .collect();
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    out[h] = if list.is_empty() {
        head
    } else {
        format!("{head} :{}:", list.join(":"))
    };
    Some(out.join("\n"))
}

/// Add `tag` to the governing headline, or remove it when already present
/// (case-sensitive), like Org `C-c C-x a` for the ARCHIVE tag.
#[must_use]
pub fn toggle_tag(text: &str, line: usize, tag: &str) -> Option<String> {
    let cur = get_tags(text, line)?;
    let mut list: Vec<&str> = cur.split(':').filter(|s| !s.is_empty()).collect();
    if let Some(i) = list.iter().position(|t| *t == tag) {
        list.remove(i);
    } else {
        list.push(tag);
    }
    set_tags(text, line, &list.join(":"))
}

/// Whether `line` is an Org planning line (`SCHEDULED:` / `DEADLINE:` /
/// `CLOSED:` after the headline).
pub(crate) fn is_planning(line: &str) -> bool {
    let t = line.trim_start();
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"]
        .iter()
        .any(|k| t.starts_with(k))
}

/// Set property `name` to `value` in the `:PROPERTIES:` drawer of the headline
/// governing `line` (Org `C-c C-x p`), creating the drawer (after any planning
/// line) when missing and replacing the property when present.
#[must_use]
pub fn set_property(text: &str, line: usize, name: &str, value: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let mut at = h + 1;
    if at < lines.len() && is_planning(lines[at]) {
        at += 1;
    }
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    let entry = format!(":{name}: {value}");
    let needle = format!(":{}:", name.to_ascii_lowercase());
    if at < lines.len() && lines[at].trim().eq_ignore_ascii_case(":PROPERTIES:") {
        let (_, end) = drawer_range(&lines, at)?;
        if let Some(i) = (at + 1..end).find(|&i| {
            lines[i]
                .trim_start()
                .to_ascii_lowercase()
                .starts_with(&needle)
        }) {
            out[i] = entry;
        } else {
            out.insert(end, entry);
        }
    } else {
        out.splice(
            at..at,
            [":PROPERTIES:".to_string(), entry, ":END:".to_string()],
        );
    }
    Some(out.join("\n"))
}

// ----- Archive ---------------------------------------------------------------

/// Cut the subtree governing `line` for archiving: returns the remaining text
/// and the extracted subtree, promoted to level 1 with an `:ARCHIVE_TIME:`
/// property stamped `time` (Org `C-c C-x C-s`). `None` off any subtree.
#[must_use]
pub fn archive_subtree(text: &str, line: usize, time: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let (ss, se) = subtree_range(&lines, h)?;
    let level = headline_level(lines[ss])?;
    let delta = 1 - i64::try_from(level).ok()?;
    let block: Vec<String> = lines[ss..se].iter().map(|l| relevel(l, delta)).collect();
    let block = set_property(&block.join("\n"), 0, "ARCHIVE_TIME", time)?;
    let rest: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !(ss..se).contains(i))
        .map(|(_, s)| *s)
        .collect();
    Some((rest.join("\n"), block))
}

// ----- Dates & scheduling ----------------------------------------------------

/// Parse `YYYY-MM-DD` into `(year, month, day)`.
fn parse_ymd(s: &str) -> Option<(i64, i64, i64)> {
    let bytes = s.as_bytes();
    if s.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = s[..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
}

/// Civil date for days since 1970-01-01 (inverse of [`days_from_civil`]).
///
/// Delegates to `vix-civil-date` (Run H, T520) — this crate,
/// `vix-file-information-panel`, and `vix-git` used to each hand-roll an
/// independent copy of the same algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    vix_civil_date::civil_from_days(z)
}

/// Three-letter weekday for days since 1970-01-01 (a Thursday).
fn weekday_name(days: i64) -> &'static str {
    ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"]
        [usize::try_from(days.rem_euclid(7)).expect("rem_euclid(7) fits usize")]
}

/// An Org timestamp for a `YYYY-MM-DD` date with the weekday computed:
/// `<2026-08-05 Wed>` (active) or `[2026-08-05 Wed]` (inactive). `None` on a
/// malformed date.
#[must_use]
pub fn timestamp_for(date: &str, active: bool) -> Option<String> {
    let (y, m, d) = parse_ymd(date)?;
    let dow = weekday_name(days_from_civil(y, m, d));
    Some(if active {
        format!("<{date} {dow}>")
    } else {
        format!("[{date} {dow}]")
    })
}

/// A `YYYY-MM-DD` date with an optional weekday, as found inside timestamps.
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}( [A-Za-z]{2,3})?").expect("date regex"));

/// The 0-based line index and starting char offset of the line containing char
/// offset `cursor`.
pub(crate) fn line_of_char(text: &str, cursor: usize) -> (usize, usize) {
    let mut start = 0;
    for (i, l) in text.split('\n').enumerate() {
        let len = l.chars().count();
        if cursor <= start + len {
            return (i, start);
        }
        start += len + 1;
    }
    (text.split('\n').count().saturating_sub(1), start)
}

/// Shift the date under the cursor by `delta_days`, rewriting its weekday (Org
/// `S-↑`/`S-↓` on a timestamp). Falls back to the line's only date when the
/// cursor is not on one; `None` when the line has no date or several.
#[must_use]
pub fn shift_timestamp_at(text: &str, cursor: usize, delta_days: i64) -> Option<(String, usize)> {
    let (line_idx, line_start) = line_of_char(text, cursor);
    let lines: Vec<&str> = text.split('\n').collect();
    let l = *lines.get(line_idx)?;
    let col = cursor - line_start.min(cursor);
    let matches: Vec<regex::Match> = DATE.find_iter(l).collect();
    let m = matches
        .iter()
        .find(|m| {
            let sc = l[..m.start()].chars().count();
            let ec = sc + l[m.start()..m.end()].chars().count();
            (sc..=ec).contains(&col)
        })
        .or_else(|| (matches.len() == 1).then(|| &matches[0]))?;
    let (y, mo, d) = parse_ymd(&m.as_str()[..10])?;
    let days = days_from_civil(y, mo, d) + delta_days;
    let (ny, nm, nd) = civil_from_days(days);
    let mut new_date = format!("{ny:04}-{nm:02}-{nd:02}");
    if m.as_str().len() > 10 {
        let _ = write!(new_date, " {}", weekday_name(days));
    }
    let new_line = format!("{}{}{}", &l[..m.start()], new_date, &l[m.end()..]);
    let mut out: Vec<&str> = lines.clone();
    out[line_idx] = &new_line;
    Some((out.join("\n"), cursor))
}

/// Set the `SCHEDULED:`/`DEADLINE:` (`keyword`) entry of the headline governing
/// `line` to `stamp` (a full `<…>` timestamp): replace it on an existing
/// planning line, append it there, or insert a new planning line after the
/// headline (Org `C-c C-s` / `C-c C-d`).
#[must_use]
pub fn plan(text: &str, line: usize, keyword: &str, stamp: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let pl = h + 1;
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    if pl < lines.len() && is_planning(lines[pl]) {
        let re = Regex::new(&format!(r"{keyword}:\s*[<\[][^>\]]*[>\]]")).ok()?;
        out[pl] = if re.is_match(lines[pl]) {
            re.replace(lines[pl], format!("{keyword}: {stamp}").as_str())
                .into_owned()
        } else {
            format!("{} {keyword}: {stamp}", lines[pl].trim_end())
        };
    } else {
        out.insert(pl, format!("{keyword}: {stamp}"));
    }
    Some(out.join("\n"))
}
