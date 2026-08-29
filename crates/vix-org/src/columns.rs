//! Org [Column View](https://orgmode.org/manual/Column-View.html) (`C-c C-x C-c`):
//! a spreadsheet-like overlay on the outline, driven by a `#+COLUMNS`/`:COLUMNS:`
//! format spec, arbitrary entry properties, Org's built-in *special properties*,
//! and recursive per-column summaries rolled up from direct children.
//!
//! This module is the pure engine: parsing the format spec, resolving which spec
//! applies at a given line (nearest enclosing `:COLUMNS:` wins over the file-level
//! `#+COLUMNS:` wins over a hardcoded default), building the display table,
//! recomputing+rewriting summary values into property drawers, single-cell edits,
//! and the `#+BEGIN: columnview` dynamic-block capture. It has no notion of a live
//! editor or cursor beyond the 0-based line numbers callers already use elsewhere
//! in this crate.
//!
//! All functions are pure so they can be unit-tested without a live editor.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

/// Special property names resolved from the entry itself rather than looked up
/// in its `:PROPERTIES:` drawer. Matched case-insensitively.
const SPECIAL_PROPERTIES: &[&str] = &[
    "ITEM",
    "TODO",
    "PRIORITY",
    "TAGS",
    "ALLTAGS",
    "DEADLINE",
    "SCHEDULED",
    "CLOSED",
    "CATEGORY",
    "FILE",
    "TIMESTAMP",
    "TIMESTAMP_IA",
    "CLOCKSUM",
    "CLOCKSUM_T",
    "BLOCKED",
];

/// TODO-like keywords this crate recognizes when scanning a headline's text for
/// column view (beyond the crate-wide literal `TODO`/`DONE` pair other helpers
/// hardcode): a fixed, conservative word list rather than a general "any
/// uppercase word" heuristic, so an all-caps title word (an acronym like `URL`)
/// is never mistaken for a keyword.
const KEYWORDS: &[&str] = &[
    "TODO",
    "DONE",
    "NEXT",
    "WAITING",
    "SOMEDAY",
    "CANCELLED",
    "CANCELED",
    "HOLD",
    "STARTED",
    "INPROGRESS",
];

/// One `%[WIDTH]PROPERTY[(TITLE)]{SUMMARY}` token inside a `#+COLUMNS`/`:COLUMNS:`
/// spec.
static COL_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"%(\d+)?([A-Za-z_][A-Za-z0-9_]*)(?:\(([^)]*)\))?(?:\{([^}]*)\})?")
        .expect("column token regex")
});

/// One parsed column definition from a `COLUMNS` spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// The property name to display (a special property like `ITEM`, or an
    /// arbitrary entry property).
    pub property: String,
    /// The column header, if given; defaults to `property` when absent.
    pub title: Option<String>,
    /// The column's fixed display width, if given; auto-width from content
    /// when absent.
    pub width: Option<u16>,
    /// The raw summary-type token (e.g. `"+"`, `"X/"`, `":mean"`, `"est+"`),
    /// if this column rolls up a recursive summary for parent headings.
    pub summary: Option<String>,
}

/// A full parsed `COLUMNS` spec: ordered column definitions plus any `*_ALL`
/// allowed-value lists found alongside it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColumnsSpec {
    /// The columns, in display order.
    pub columns: Vec<ColumnDef>,
    /// `PROPERTY -> allowed values` from `:PROPERTY_ALL:` lines, for cycling.
    pub allowed_values: BTreeMap<String, Vec<String>>,
}

/// Parse one `%...` format line (the value of a `#+COLUMNS:` keyword or a
/// `:COLUMNS:` property) into its column definitions. Returns `None` if the
/// line has no `%PROPERTY` tokens at all. The `*_ALL` allowed-value lists are
/// not part of this line; see [`parse_all_values`].
#[must_use]
pub fn parse_columns_spec(line: &str) -> Option<ColumnsSpec> {
    let mut rest = line.trim();
    for prefix in ["#+COLUMNS:", ":COLUMNS:"] {
        if rest.len() >= prefix.len() && rest[..prefix.len()].eq_ignore_ascii_case(prefix) {
            rest = rest[prefix.len()..].trim();
            break;
        }
    }
    let mut columns = Vec::new();
    for cap in COL_TOKEN.captures_iter(rest) {
        let width = cap.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
        let property = cap[2].to_string();
        let title = cap
            .get(3)
            .map(|m| m.as_str().to_string())
            .filter(|s| !s.is_empty());
        let summary = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .filter(|s| !s.is_empty());
        columns.push(ColumnDef {
            property,
            title,
            width,
            summary,
        });
    }
    if columns.is_empty() {
        None
    } else {
        Some(ColumnsSpec {
            columns,
            allowed_values: BTreeMap::new(),
        })
    }
}

/// Tokenize a value list: a double-quoted run (which may contain spaces)
/// counts as one token; otherwise tokens are whitespace-separated.
fn tokenize_quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut tok = String::new();
            for c2 in chars.by_ref() {
                if c2 == '"' {
                    break;
                }
                tok.push(c2);
            }
            out.push(tok);
        } else {
            let mut tok = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_whitespace() {
                    break;
                }
                tok.push(c2);
                chars.next();
            }
            out.push(tok);
        }
    }
    out
}

/// Scan `drawer_lines` for `:PROPERTY_ALL:` lines (Org's allowed-value lists),
/// tokenizing each value list (quoted-with-spaces tokens count as one value;
/// see [`tokenize_quoted`]).
#[must_use]
pub fn parse_all_values(drawer_lines: &[&str]) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    for line in drawer_lines {
        let t = line.trim();
        let Some(rest) = t.strip_prefix(':') else {
            continue;
        };
        let Some((name, value)) = rest.split_once(':') else {
            continue;
        };
        let Some(prop) = name.trim().strip_suffix("_ALL") else {
            continue;
        };
        if prop.is_empty() {
            continue;
        }
        map.insert(prop.to_string(), tokenize_quoted(value.trim()));
    }
    map
}

/// The hardcoded fallback spec when no `COLUMNS` definition applies anywhere:
/// `%ITEM %TODO %PRIORITY %TAGS`.
fn default_columns_spec() -> ColumnsSpec {
    let plain = |property: &str| ColumnDef {
        property: property.to_string(),
        title: None,
        width: None,
        summary: None,
    };
    ColumnsSpec {
        columns: vec![
            plain("ITEM"),
            plain("TODO"),
            plain("PRIORITY"),
            plain("TAGS"),
        ],
        allowed_values: BTreeMap::new(),
    }
}

/// Resolve the effective [`ColumnsSpec`] for column view at `line`: the nearest
/// enclosing headline's own `:COLUMNS:` property wins; else the file-level
/// `#+COLUMNS:` keyword (or an equivalent `COLUMNS` property in a drawer before
/// the first headline); else the hardcoded default.
#[must_use]
pub fn resolve_columns_spec(text: &str, line: usize) -> ColumnsSpec {
    let lines: Vec<&str> = text.split('\n').collect();
    ancestor_columns_spec(&lines, line)
        .or_else(|| file_level_columns_spec(&lines))
        .unwrap_or_else(default_columns_spec)
}

/// Search upward from the headline governing `line` through its ancestors for
/// the nearest `:COLUMNS:` entry property.
fn ancestor_columns_spec(lines: &[&str], line: usize) -> Option<ColumnsSpec> {
    let mut cur = crate::governing(lines, line);
    while let Some(h) = cur {
        if let Some(val) = property_value_opt(lines, h, "COLUMNS")
            && let Some(mut spec) = parse_columns_spec(&val)
        {
            if let Some((start, end)) = own_property_drawer(lines, h) {
                spec.allowed_values = parse_all_values(&lines[start + 1..end]);
            }
            return Some(spec);
        }
        cur = parent_headline(lines, h);
    }
    None
}

/// Look for a file-wide `COLUMNS` definition: a `#+COLUMNS:` keyword line, or a
/// `:COLUMNS:` property in a drawer, either one before the first headline.
fn file_level_columns_spec(lines: &[&str]) -> Option<ColumnsSpec> {
    let first_headline = lines
        .iter()
        .position(|l| crate::headline_level(l).is_some())
        .unwrap_or(lines.len());
    let top = &lines[..first_headline];
    for line in top {
        let t = line.trim();
        if t.len() >= 10
            && t[..10].eq_ignore_ascii_case("#+COLUMNS:")
            && let Some(mut spec) = parse_columns_spec(t)
        {
            spec.allowed_values = parse_all_values(top);
            return Some(spec);
        }
    }
    let mut i = 0;
    while i < top.len() {
        if top[i].trim().eq_ignore_ascii_case(":PROPERTIES:")
            && let Some((_, end)) = crate::drawer_range(top, i)
        {
            for l in &top[i + 1..end] {
                let t = l.trim();
                let Some(rest) = t.strip_prefix(':') else {
                    continue;
                };
                let Some((k, v)) = rest.split_once(':') else {
                    continue;
                };
                if k.trim().eq_ignore_ascii_case("COLUMNS")
                    && let Some(mut spec) = parse_columns_spec(v.trim())
                {
                    spec.allowed_values = parse_all_values(top);
                    return Some(spec);
                }
            }
        }
        i += 1;
    }
    None
}

// ----- Entry-level lookups shared by the special properties --------------

/// The `[header, end]` line range of the `:PROPERTIES:` drawer immediately
/// owned by headline `h` (after any planning line), or `None` if it has none.
fn own_property_drawer(lines: &[&str], h: usize) -> Option<(usize, usize)> {
    let mut at = h + 1;
    if at < lines.len() && crate::is_planning(lines[at]) {
        at += 1;
    }
    if at < lines.len() && lines[at].trim().eq_ignore_ascii_case(":PROPERTIES:") {
        crate::drawer_range(lines, at)
    } else {
        None
    }
}

/// The value of property `key` (case-insensitive) in headline `h`'s own
/// drawer, only if present and non-empty.
fn property_value_opt(lines: &[&str], h: usize, key: &str) -> Option<String> {
    let (start, end) = own_property_drawer(lines, h)?;
    for line in &lines[start + 1..end] {
        let t = line.trim();
        let Some(rest) = t.strip_prefix(':') else {
            continue;
        };
        if let Some((k, v)) = rest.split_once(':')
            && k.trim().eq_ignore_ascii_case(key)
        {
            let val = v.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// The value of property `key` in headline `h`'s own drawer (not inherited),
/// or `""` if absent — the general (non-special) `PROPERTY` lookup rule.
fn property_value(lines: &[&str], h: usize, key: &str) -> String {
    property_value_opt(lines, h, key).unwrap_or_default()
}

/// The nearest headline above `h` with a smaller level (its outline parent),
/// or `None` at top level.
fn parent_headline(lines: &[&str], h: usize) -> Option<usize> {
    let level = crate::headline_level(lines[h])?;
    (0..h)
        .rev()
        .find(|&i| crate::headline_level(lines[i]).is_some_and(|l| l < level))
}

/// The direct children of headline `h`: headlines immediately inside its
/// subtree, skipping over each child's own (deeper) descendants.
fn direct_children(lines: &[&str], h: usize) -> Vec<usize> {
    let Some((_, end)) = crate::subtree_range(lines, h) else {
        return Vec::new();
    };
    let mut kids = Vec::new();
    let mut i = h + 1;
    while i < end {
        if crate::headline_level(lines[i]).is_some() {
            kids.push(i);
            i = crate::subtree_range(lines, i).map_or(i + 1, |(_, e)| e);
        } else {
            i += 1;
        }
    }
    kids
}

/// Split a headline's post-stars text into its recognized TODO-like keyword
/// (see [`KEYWORDS`]) and the remaining body. Unlike `crate::split_keyword`
/// (hardcoded to literal `TODO`/`DONE`), this also recognizes the common
/// extra states column view needs to display and edit.
fn split_keyword_generic(rest: &str) -> (&str, &str) {
    let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let word = &rest[..word_end];
    if KEYWORDS.contains(&word) {
        (word, rest[word_end..].trim_start())
    } else {
        ("", rest)
    }
}

/// A headline's `(keyword, priority-cookie-display, title)`, with the trailing
/// tags group already stripped.
fn parse_headline_parts(line: &str) -> (String, String, String) {
    let Some(level) = crate::headline_level(line) else {
        return (String::new(), String::new(), String::new());
    };
    let bare = crate::TAGS.replace(line, "");
    let rest = bare[level..].trim();
    let (kw, body) = split_keyword_generic(rest);
    let (prio, title) = match crate::strip_priority(body) {
        Some((p, after)) => (format!("[#{p}]"), after),
        None => (String::new(), body),
    };
    (kw.to_string(), prio, title.trim().to_string())
}

fn item_text(lines: &[&str], h: usize) -> String {
    parse_headline_parts(lines[h]).2
}

fn item_keyword(line: &str) -> String {
    parse_headline_parts(line).0
}

fn item_priority(lines: &[&str], h: usize) -> String {
    parse_headline_parts(lines[h]).1
}

/// A headline's own trailing `:tag:tag:` group, colon-wrapped (`""` if none).
fn item_tags(lines: &[&str], h: usize) -> String {
    match crate::TAGS.captures(lines[h]) {
        Some(c) => {
            let inner = c[1].trim_matches(':').replace("::", ":");
            if inner.is_empty() {
                String::new()
            } else {
                format!(":{inner}:")
            }
        }
        None => String::new(),
    }
}

/// A headline's own tags plus every ancestor's tags, deduplicated in
/// nearest-first order, colon-wrapped.
fn item_alltags(lines: &[&str], h: usize) -> String {
    let mut tags: Vec<String> = Vec::new();
    let mut cur = Some(h);
    while let Some(c) = cur {
        let own = item_tags(lines, c);
        for t in own.trim_matches(':').split(':').filter(|s| !s.is_empty()) {
            if !tags.iter().any(|x| x == t) {
                tags.push(t.to_string());
            }
        }
        cur = parent_headline(lines, c);
    }
    if tags.is_empty() {
        String::new()
    } else {
        format!(":{}:", tags.join(":"))
    }
}

/// The raw `<...>`/`[...]` timestamp text following `KEYWORD:` on headline
/// `h`'s planning line(s), or `""` if absent.
fn planning_value(lines: &[&str], h: usize, keyword: &str) -> String {
    let mut i = h + 1;
    let mut combined = String::new();
    while i < lines.len() && crate::is_planning(lines[i]) {
        combined.push_str(lines[i]);
        combined.push(' ');
        i += 1;
    }
    let needle = format!("{keyword}:");
    let Some(pos) = combined.find(&needle) else {
        return String::new();
    };
    let after = &combined[pos + needle.len()..].trim_start();
    let Some(open) = after.find(['<', '[']) else {
        return String::new();
    };
    let close_char = if after.as_bytes()[open] == b'<' {
        '>'
    } else {
        ']'
    };
    let Some(close) = after[open..].find(close_char) else {
        return String::new();
    };
    after[open..=open + close].to_string()
}

/// `#+CATEGORY:` value from the filename, stripping any directory and
/// extension; `""` when no filename was given.
fn category_from_filename(file_name: Option<&str>) -> String {
    file_name.map_or_else(String::new, |f| {
        let base = f.rsplit('/').next().unwrap_or(f);
        base.rsplit_once('.').map_or(base, |(n, _)| n).to_string()
    })
}

/// `CATEGORY`: inherited from the entry's own drawer, then its ancestors, then
/// a file-level `#+CATEGORY:` keyword, then the filename without extension.
fn category_value(lines: &[&str], h: usize, file_name: Option<&str>) -> String {
    let mut cur = Some(h);
    while let Some(c) = cur {
        if let Some(v) = property_value_opt(lines, c, "CATEGORY") {
            return v;
        }
        cur = parent_headline(lines, c);
    }
    for line in lines {
        let t = line.trim();
        if t.len() >= 11 && t[..11].eq_ignore_ascii_case("#+CATEGORY:") {
            let v = t[11..].trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    category_from_filename(file_name)
}

/// The line one past the end of headline `h`'s own *entry* text (its body up
/// to the next headline of any level — unlike a subtree, this excludes
/// children).
fn entry_end(lines: &[&str], h: usize) -> usize {
    let mut i = h + 1;
    while i < lines.len() && crate::headline_level(lines[i]).is_none() {
        i += 1;
    }
    i
}

/// `TIMESTAMP`/`TIMESTAMP_IA`: the first active (`<...>`) or inactive
/// (`[...]`) plain timestamp anywhere in the entry's own body text, skipping
/// its property drawer and any `CLOCK:` lines.
fn entry_timestamp(lines: &[&str], h: usize, active: bool) -> String {
    let end = entry_end(lines, h);
    let drawer = own_property_drawer(lines, h);
    let (open, close) = if active { ('<', '>') } else { ('[', ']') };
    for (i, line) in lines.iter().enumerate().take(end).skip(h + 1) {
        if let Some((ds, de)) = drawer
            && i >= ds
            && i <= de
        {
            continue;
        }
        if line.trim_start().starts_with("CLOCK:") {
            continue;
        }
        let Some(pos) = line.find(open) else {
            continue;
        };
        let rest = &line[pos..];
        let Some(cp) = rest.find(close) else {
            continue;
        };
        let cand = &rest[..=cp];
        if cand.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
            return cand.to_string();
        }
    }
    String::new()
}

/// `CLOCKSUM`/`CLOCKSUM_T`: the total of every `CLOCK: [start]--[end] =>
/// H:MM` interval anywhere in headline `h`'s whole subtree (not a per-level
/// rollup), optionally restricted to intervals starting on `today`.
fn clocksum_value(lines: &[&str], h: usize, today_filter: Option<(i32, u32, u32)>) -> String {
    let Some((_, end)) = crate::subtree_range(lines, h) else {
        return String::new();
    };
    let mut total = 0u32;
    for line in &lines[h..end] {
        if let Some(filter) = today_filter {
            let Some(start_ts) = crate::clock_start(line) else {
                continue;
            };
            let Some(date) = start_ts.get(0..10) else {
                continue;
            };
            let want = format!("{:04}-{:02}-{:02}", filter.0, filter.1, filter.2);
            if date != want {
                continue;
            }
        }
        if let Some(min) = crate::clock_minutes(line) {
            total += min;
        }
    }
    crate::hhmm(total)
}

/// `BLOCKED`: `"t"` if headline `h` has at least one direct child whose TODO
/// keyword is not the literal `DONE`, else `""`.
fn blocked_value(lines: &[&str], h: usize) -> String {
    let kids = direct_children(lines, h);
    if !kids.is_empty() && kids.iter().any(|&k| item_keyword(lines[k]) != crate::DONE) {
        "t".to_string()
    } else {
        String::new()
    }
}

/// Resolve one column's *own* (unrolled) value for headline `h`: a special
/// property computed from the entry itself, or a general property looked up
/// (case-insensitively, not inherited) in its drawer.
fn entry_value(
    lines: &[&str],
    h: usize,
    col: &ColumnDef,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> String {
    match col.property.to_ascii_uppercase().as_str() {
        "ITEM" => item_text(lines, h),
        "TODO" => item_keyword(lines[h]),
        "PRIORITY" => item_priority(lines, h),
        "TAGS" => item_tags(lines, h),
        "ALLTAGS" => item_alltags(lines, h),
        "DEADLINE" => planning_value(lines, h, "DEADLINE"),
        "SCHEDULED" => planning_value(lines, h, "SCHEDULED"),
        "CLOSED" => planning_value(lines, h, "CLOSED"),
        "CATEGORY" => category_value(lines, h, file_name),
        "FILE" => file_name.unwrap_or_default().to_string(),
        "TIMESTAMP" => entry_timestamp(lines, h, true),
        "TIMESTAMP_IA" => entry_timestamp(lines, h, false),
        "CLOCKSUM" => clocksum_value(lines, h, None),
        "CLOCKSUM_T" => clocksum_value(lines, h, Some(today)),
        "BLOCKED" => blocked_value(lines, h),
        _ => property_value(lines, h, &col.property),
    }
}

fn is_special_property(name: &str) -> bool {
    SPECIAL_PROPERTIES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(name))
}

/// The fixed TODO-like keyword list this crate recognizes (see [`KEYWORDS`]),
/// for callers (the interactive column-view overlay) that need a fallback
/// cycle for a `TODO` column with no explicit `TODO_ALL` allowed-value list —
/// matching real Org, where `TODO` always has *some* cycle even undeclared.
#[must_use]
pub fn todo_keywords() -> &'static [&'static str] {
    KEYWORDS
}

/// Resolve which ancestor headline (if any) owns the effective `COLUMNS`
/// spec at `line`: `Some(line)` of the ancestor headline whose own
/// `:COLUMNS:` property is in effect, or `None` when the effective spec came
/// from the file level (`#+COLUMNS:`/a pre-first-headline `COLUMNS`
/// property) or the hardcoded default — callers writing a `PROPERTY_ALL`
/// allowed-value list back should target that headline's drawer, or a
/// file-level drawer before the first headline in the `None` case.
#[must_use]
pub fn columns_spec_anchor(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut cur = crate::governing(&lines, line);
    while let Some(h) = cur {
        if property_value_opt(&lines, h, "COLUMNS").is_some() {
            return Some(h);
        }
        cur = parent_headline(&lines, h);
    }
    None
}

// ----- Summary types --------------------------------------------------------

fn numeric_values(values: &[&str]) -> Vec<f64> {
    values
        .iter()
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .collect()
}

fn format_plain_number(n: f64) -> String {
    format!("{n}")
}

/// A pragmatic subset of printf float formatting: only the `%.<N>f` shape
/// used by Org's `+;FORMAT` summary type. Any other format string falls back
/// to the default `Display` rendering.
fn format_printf_float(fmt: &str, value: f64) -> String {
    if let Some(rest) = fmt.strip_prefix("%.")
        && let Some(prec_str) = rest.strip_suffix('f')
        && let Ok(prec) = prec_str.parse::<usize>()
    {
        return format!("{value:.prec$}");
    }
    format_plain_number(value)
}

fn sum_numeric(values: &[&str], fmt: Option<&str>) -> String {
    let sum: f64 = numeric_values(values).iter().sum();
    fmt.map_or_else(|| format_plain_number(sum), |f| format_printf_float(f, sum))
}

fn numeric_reduce(values: &[&str], f: impl Fn(f64, f64) -> f64) -> String {
    let nums = numeric_values(values);
    if nums.is_empty() {
        return String::new();
    }
    format_plain_number(nums.into_iter().reduce(f).unwrap_or(0.0))
}

fn numeric_mean(values: &[&str]) -> String {
    let nums = numeric_values(values);
    if nums.is_empty() {
        return String::new();
    }
    let len = u32::try_from(nums.len()).unwrap_or(u32::MAX);
    format_plain_number(nums.iter().sum::<f64>() / f64::from(len))
}

fn checkbox_counts(values: &[&str]) -> (usize, usize) {
    let total = values.len();
    let done = values.iter().filter(|v| v.trim() == "[X]").count();
    (done, total)
}

fn checkbox_all(values: &[&str]) -> String {
    if values.is_empty() {
        return String::new();
    }
    if values.iter().all(|v| v.trim() == "[X]") {
        "[X]".to_string()
    } else {
        "[ ]".to_string()
    }
}

fn checkbox_fraction(values: &[&str]) -> String {
    let (done, total) = checkbox_counts(values);
    format!("[{done}/{total}]")
}

fn checkbox_percent(values: &[&str]) -> String {
    let (done, total) = checkbox_counts(values);
    let pct = (done * 100).checked_div(total).unwrap_or(0);
    format!("[{pct}%]")
}

/// Parse a time-column value into minutes: `H:MM`, a bare integer (minutes),
/// or an Org-duration-style token run (`1h`, `1h 30min`, `2d`, ...).
fn parse_duration_minutes(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((h, m)) = s.split_once(':')
        && let (Ok(h), Ok(m)) = (h.trim().parse::<i64>(), m.trim().parse::<i64>())
    {
        return Some(h * 60 + m);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    parse_unit_duration(s, true)
}

/// Parse an age-column value into seconds: a bare number (seconds), or a
/// unit-token run (`3d 1h`, `90m`, ...).
#[allow(
    clippy::cast_possible_truncation,
    reason = "rounding a bare-seconds value to the nearest whole second is intentional"
)]
fn parse_age_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<f64>() {
        return Some(n.round() as i64);
    }
    parse_unit_duration(s, false)
}

/// Shared unit-token parser: reads `<number><unit>` pairs (whitespace
/// optional between them) and sums them. `minutes = true` returns whole
/// minutes (for time columns); `false` returns whole seconds (for age
/// columns, where a bare `m` unit means minutes, not months).
#[allow(
    clippy::cast_possible_truncation,
    reason = "rounding a summed duration to the nearest whole minute/second is intentional"
)]
fn parse_unit_duration(s: &str, minutes: bool) -> Option<i64> {
    let mut total_seconds = 0.0f64;
    let mut any = false;
    let mut num = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        if num.is_empty() {
            return None;
        }
        let mut unit = String::new();
        unit.push(c);
        while let Some(&nc) = chars.peek() {
            if nc.is_ascii_alphabetic() {
                unit.push(nc);
                chars.next();
            } else {
                break;
            }
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        let secs_per_unit = match unit.to_ascii_lowercase().as_str() {
            "d" | "day" | "days" => 86400.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            "w" | "week" | "weeks" => 604_800.0,
            _ => return None,
        };
        total_seconds += n * secs_per_unit;
        any = true;
    }
    if !num.trim().is_empty() || !any {
        return None;
    }
    if minutes {
        Some((total_seconds / 60.0).round() as i64)
    } else {
        Some(total_seconds.round() as i64)
    }
}

/// Format minutes back as Org-duration style: `"1h 0min"` for whole hours,
/// `"Ymin"` under an hour. Chosen (rather than the terser `H:MM` clock
/// style) because that is the literal round-trip format Org's own manual
/// shows for a recursively-summarized `EFFORT` property.
fn format_duration_minutes(total: i64) -> String {
    let sign = if total < 0 { "-" } else { "" };
    let total = total.unsigned_abs();
    if total >= 60 {
        format!("{sign}{}h {}min", total / 60, total % 60)
    } else {
        format!("{sign}{total}min")
    }
}

enum TimeOp {
    Sum,
    Min,
    Max,
    Mean,
}

fn time_reduce(values: &[&str], op: &TimeOp) -> String {
    let mins: Vec<i64> = values
        .iter()
        .filter_map(|v| parse_duration_minutes(v))
        .collect();
    if mins.is_empty() {
        return String::new();
    }
    let result = match op {
        TimeOp::Sum => mins.iter().sum(),
        TimeOp::Min => *mins.iter().min().unwrap_or(&0),
        TimeOp::Max => *mins.iter().max().unwrap_or(&0),
        TimeOp::Mean => {
            let len = i64::try_from(mins.len()).unwrap_or(1).max(1);
            mins.iter().sum::<i64>() / len
        }
    };
    format_duration_minutes(result)
}

/// Format seconds as the largest whole unit that still leaves a non-zero
/// remainder in the next-smaller unit: days(+hours) > hours(+minutes) >
/// minutes > seconds. A pragmatic, documented choice — Org itself does not
/// prescribe an exact age-summary display format.
fn format_age_seconds(total: i64) -> String {
    let sign = if total < 0 { "-" } else { "" };
    let secs = total.unsigned_abs();
    let days = secs / 86400;
    let rem1 = secs % 86400;
    let hours = rem1 / 3600;
    let rem2 = rem1 % 3600;
    let minutes = rem2 / 60;
    let seconds = rem2 % 60;
    if days > 0 {
        if hours > 0 {
            format!("{sign}{days}d {hours}h")
        } else {
            format!("{sign}{days}d")
        }
    } else if hours > 0 {
        if minutes > 0 {
            format!("{sign}{hours}h {minutes}m")
        } else {
            format!("{sign}{hours}h")
        }
    } else if minutes > 0 {
        format!("{sign}{minutes}m")
    } else {
        format!("{sign}{seconds}s")
    }
}

enum AgeOp {
    Min,
    Max,
    Mean,
}

fn age_reduce(values: &[&str], op: &AgeOp) -> String {
    let secs: Vec<i64> = values.iter().filter_map(|v| parse_age_seconds(v)).collect();
    if secs.is_empty() {
        return String::new();
    }
    let result = match op {
        AgeOp::Min => *secs.iter().min().unwrap_or(&0),
        AgeOp::Max => *secs.iter().max().unwrap_or(&0),
        AgeOp::Mean => {
            let len = i64::try_from(secs.len()).unwrap_or(1).max(1);
            secs.iter().sum::<i64>() / len
        }
    };
    format_age_seconds(result)
}

/// `est+`: combine `LOW-HIGH` estimate ranges by summing each child's
/// approximate mean (`(low+high)/2`) and variance (`((high-low)/4)^2`,
/// treating the range as a pragmatic ~95%-interval approximation — *not*
/// Org's exact Calc statistics, which are out of scope here), then reporting
/// `mean - 2*sqrt(variance_sum)` .. `mean + 2*sqrt(variance_sum)`, rounded to
/// one decimal.
fn est_combine(values: &[&str]) -> String {
    let mut mean_sum = 0.0f64;
    let mut var_sum = 0.0f64;
    let mut any = false;
    for v in values {
        let Some((lo, hi)) = v.trim().split_once('-') else {
            continue;
        };
        let (Ok(lo), Ok(hi)) = (lo.trim().parse::<f64>(), hi.trim().parse::<f64>()) else {
            continue;
        };
        let mean = f64::midpoint(lo, hi);
        let variance = ((hi - lo) / 4.0).powi(2);
        mean_sum += mean;
        var_sum += variance;
        any = true;
    }
    if !any {
        return String::new();
    }
    let spread = 2.0 * var_sum.sqrt();
    format!("{:.1}-{:.1}", mean_sum - spread, mean_sum + spread)
}

fn apply_summary(token: &str, child_values: &[&str]) -> String {
    match token {
        "+" => sum_numeric(child_values, None),
        t if t.starts_with("+;") => sum_numeric(child_values, Some(&t[2..])),
        "$" => sum_numeric(child_values, Some("%.2f")),
        "min" => numeric_reduce(child_values, f64::min),
        "max" => numeric_reduce(child_values, f64::max),
        "mean" => numeric_mean(child_values),
        "X" => checkbox_all(child_values),
        "X/" => checkbox_fraction(child_values),
        "X%" => checkbox_percent(child_values),
        ":" => time_reduce(child_values, &TimeOp::Sum),
        ":min" => time_reduce(child_values, &TimeOp::Min),
        ":max" => time_reduce(child_values, &TimeOp::Max),
        ":mean" => time_reduce(child_values, &TimeOp::Mean),
        "@min" => age_reduce(child_values, &AgeOp::Min),
        "@max" => age_reduce(child_values, &AgeOp::Max),
        "@mean" => age_reduce(child_values, &AgeOp::Mean),
        "est+" => est_combine(child_values),
        _ => String::new(),
    }
}

// ----- Table building & write-back ------------------------------------------

/// One row of the column-view table: the source headline's line number (for
/// write-back targeting), its outline level, and the resolved display string
/// for each column in the active [`ColumnsSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRow {
    /// The 0-based source line of the headline this row displays.
    pub line: usize,
    /// The headline's outline level (star count).
    pub level: usize,
    /// One resolved display string per column of the active spec, in order.
    pub values: Vec<String>,
}

/// Immutable inputs shared by every recursive rollup step, bundled to keep
/// the recursive helper's argument count small.
struct RollupCtx<'a> {
    spec: &'a ColumnsSpec,
    children: &'a HashMap<usize, Vec<usize>>,
    own_values: &'a HashMap<(usize, usize), String>,
    write_back_idx: &'a HashMap<String, usize>,
}

/// Post-order (deepest first) recursive rollup: fill `display` for `h` and
/// every descendant, and record any write-back-eligible summary cells into
/// `pending`.
fn visit(
    h: usize,
    ctx: &RollupCtx<'_>,
    display: &mut HashMap<(usize, usize), String>,
    pending: &mut Vec<(usize, String, String)>,
) {
    let kids = ctx.children.get(&h).cloned().unwrap_or_default();
    for &k in &kids {
        visit(k, ctx, display, pending);
    }
    for (ci, col) in ctx.spec.columns.iter().enumerate() {
        let is_special = is_special_property(&col.property);
        let value = if !is_special && !kids.is_empty() {
            if let Some(summary) = &col.summary {
                let child_vals: Vec<&str> =
                    kids.iter().map(|k| display[&(*k, ci)].as_str()).collect();
                let rolled = apply_summary(summary, &child_vals);
                if ctx.write_back_idx.get(&col.property) == Some(&ci) {
                    pending.push((h, col.property.clone(), rolled.clone()));
                }
                rolled
            } else {
                ctx.own_values[&(h, ci)].clone()
            }
        } else {
            ctx.own_values[&(h, ci)].clone()
        };
        display.insert((h, ci), value);
    }
}

/// Build a column-view table over the headlines in `[start, end)`, applying
/// every summary rollup and writing computed values back into the source
/// text (only the first column definition for a property, if it carries a
/// summary type, triggers a write-back — see the module docs).
fn compute_table_in_scope(
    text: &str,
    scope: (usize, usize),
    spec: &ColumnsSpec,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> (Vec<ColumnRow>, String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let (start, end) = (scope.0, scope.1.min(lines.len()));
    let heads: Vec<usize> = (start..end)
        .filter(|&i| crate::headline_level(lines[i]).is_some())
        .collect();
    if heads.is_empty() || spec.columns.is_empty() {
        return (Vec::new(), text.to_string());
    }

    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for &h in &heads {
        children.insert(h, direct_children(&lines, h));
    }
    let child_set: HashSet<usize> = children.values().flatten().copied().collect();
    let roots: Vec<usize> = heads
        .iter()
        .copied()
        .filter(|h| !child_set.contains(h))
        .collect();

    let mut first_idx_for_prop: HashMap<String, usize> = HashMap::new();
    for (i, c) in spec.columns.iter().enumerate() {
        first_idx_for_prop.entry(c.property.clone()).or_insert(i);
    }
    let write_back_idx: HashMap<String, usize> = first_idx_for_prop
        .into_iter()
        .filter(|(_, idx)| spec.columns[*idx].summary.is_some())
        .collect();

    let mut own_values: HashMap<(usize, usize), String> = HashMap::new();
    for &h in &heads {
        for (ci, col) in spec.columns.iter().enumerate() {
            own_values.insert((h, ci), entry_value(&lines, h, col, today, file_name));
        }
    }

    let ctx = RollupCtx {
        spec,
        children: &children,
        own_values: &own_values,
        write_back_idx: &write_back_idx,
    };
    let mut display: HashMap<(usize, usize), String> = HashMap::new();
    let mut pending: Vec<(usize, String, String)> = Vec::new();
    for &r in &roots {
        visit(r, &ctx, &mut display, &mut pending);
    }

    pending.sort_by_key(|p| std::cmp::Reverse(p.0));
    let mut out_text = text.to_string();
    for (line_no, prop, val) in pending {
        if let Some(updated) = crate::set_property(&out_text, line_no, &prop, &val) {
            out_text = updated;
        }
    }

    let rows: Vec<ColumnRow> = heads
        .iter()
        .map(|&h| {
            let level = crate::headline_level(lines[h]).unwrap_or(1);
            let values: Vec<String> = (0..spec.columns.len())
                .map(|ci| display[&(h, ci)].clone())
                .collect();
            ColumnRow {
                line: h,
                level,
                values,
            }
        })
        .collect();
    (rows, out_text)
}

/// Build the full column-view table for the subtree/file rooted at `line`
/// (the subtree governing `line`, or the whole file when `line` sits before
/// any headline), given `today` for age/`CLOCKSUM_T` calculations and
/// `file_name` for the `FILE` special property. Returns the display rows
/// plus the buffer text, possibly rewritten with summary write-backs.
#[must_use]
pub fn build_column_table(
    text: &str,
    line: usize,
    spec: &ColumnsSpec,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> (Vec<ColumnRow>, String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let scope = match crate::governing(&lines, line) {
        Some(h) => crate::subtree_range(&lines, h).unwrap_or((0, lines.len())),
        None => (0, lines.len()),
    };
    compute_table_in_scope(text, scope, spec, today, file_name)
}

// ----- Single-cell edits ----------------------------------------------------

fn set_item_text(text: &str, line: usize, new_title: &str) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let original = lines.get(line)?.clone();
    let level = crate::headline_level(&original)?;
    let tags_suffix = crate::TAGS
        .captures(&original)
        .map_or_else(String::new, |c| format!(" {}", &c[1]));
    let bare = crate::TAGS.replace(&original, "").to_string();
    let rest = bare[level..].trim();
    let (kw, body) = split_keyword_generic(rest);
    let kw_prefix = if kw.is_empty() {
        String::new()
    } else {
        format!("{kw} ")
    };
    let priority_prefix = match crate::strip_priority(body) {
        Some((p, _)) => format!("[#{p}] "),
        None => String::new(),
    };
    let stars = "*".repeat(level);
    lines[line] = format!("{stars} {kw_prefix}{priority_prefix}{new_title}{tags_suffix}");
    Some(lines.join("\n"))
}

fn set_item_todo(text: &str, line: usize, new_keyword: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let original = *lines.get(line)?;
    let level = crate::headline_level(original)?;
    let (prefix, rest) = original.split_at(level + 1);
    let (_, body) = split_keyword_generic(rest);
    let new_keyword = new_keyword.trim();
    let new_rest = if new_keyword.is_empty() {
        body.to_string()
    } else if body.is_empty() {
        new_keyword.to_string()
    } else {
        format!("{new_keyword} {body}")
    };
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    out[line] = format!("{prefix}{new_rest}");
    Some(out.join("\n"))
}

fn set_item_priority(text: &str, line: usize, new_value: &str) -> Option<String> {
    let v = new_value.trim();
    if v.is_empty() {
        return crate::set_priority(text, line, None);
    }
    let ch = v
        .trim_start_matches("[#")
        .trim_end_matches(']')
        .chars()
        .next();
    crate::set_priority(text, line, ch)
}

fn write_single_field(
    text: &str,
    row_line: usize,
    property: &str,
    new_value: &str,
) -> Option<String> {
    match property.to_ascii_uppercase().as_str() {
        "ITEM" => set_item_text(text, row_line, new_value),
        "TODO" => set_item_todo(text, row_line, new_value),
        "PRIORITY" => set_item_priority(text, row_line, new_value),
        "TAGS" => crate::set_tags(text, row_line, new_value),
        name @ ("DEADLINE" | "SCHEDULED" | "CLOSED") => {
            crate::plan(text, row_line, name, new_value)
        }
        "CATEGORY" => crate::set_property(text, row_line, "CATEGORY", new_value),
        "ALLTAGS" | "FILE" | "TIMESTAMP" | "TIMESTAMP_IA" | "CLOCKSUM" | "CLOCKSUM_T"
        | "BLOCKED" => None,
        _ => crate::set_property(text, row_line, property, new_value),
    }
}

/// Apply a single in-place edit: the user changed column `col_index` of the
/// row at `row_line` to `new_value`. Writes the new value back at its source
/// location (the headline itself for `ITEM`/`TODO`/`PRIORITY`/`TAGS`, the
/// planning line for `DEADLINE`/`SCHEDULED`/`CLOSED`, or the property drawer
/// otherwise), then recomputes and rewrites every summary column across the
/// whole buffer so ancestor rollups stay in sync. Read-only special
/// properties (`ALLTAGS`, `FILE`, `TIMESTAMP`, `TIMESTAMP_IA`, `CLOCKSUM`,
/// `CLOCKSUM_T`, `BLOCKED`) cannot be edited and return `None`, as does an
/// out-of-range `col_index` or a `row_line` that is not a headline.
#[must_use]
pub fn apply_column_edit(
    text: &str,
    row_line: usize,
    spec: &ColumnsSpec,
    col_index: usize,
    new_value: &str,
) -> Option<String> {
    let col = spec.columns.get(col_index)?;
    let edited = write_single_field(text, row_line, &col.property, new_value)?;
    let end = edited.split('\n').count();
    let (_, rewritten) = compute_table_in_scope(&edited, (0, end), spec, (1970, 1, 1), None);
    Some(rewritten)
}

// ----- Dynamic-block capture -------------------------------------------------

/// Parameters parsed from a `#+BEGIN: columnview ...` line.
// Mirrors Org's own `:vlines`/`:skip-empty-rows`/`:indent`/`:link` dblock
// parameters, each an independent on/off switch — grouping them would only
// relocate the lint, not the shape of the syntax being parsed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DblockParams {
    /// The capture scope: `"local"` (the dblock's own subtree), `"global"`
    /// (the whole file), `"file:NAME"` (see the module docs — treated the
    /// same as `"global"`, same-buffer only), or an arbitrary label matched
    /// against an entry's `:ID:` property.
    pub id: String,
    /// The raw `:hlines` value (`"t"` or a count), if given.
    pub hlines: Option<String>,
    /// Whether `:vlines t` was given.
    pub vlines: bool,
    /// The `:maxlevel` depth limit, if given.
    pub maxlevel: Option<usize>,
    /// Whether `:skip-empty-rows t` was given.
    pub skip_empty_rows: bool,
    /// Tags to exclude from `:exclude-tags`.
    pub exclude_tags: Vec<String>,
    /// Whether `:indent t` was given (indent the `ITEM` cell by level).
    pub indent: bool,
    /// Whether `:link t` was given (wrap the `ITEM` cell as an internal link).
    pub link: bool,
    /// The `:format` override, if given — a `COLUMNS` spec string used
    /// instead of the one [`resolve_columns_spec`] would find.
    pub format: Option<String>,
}

/// Parse a `#+BEGIN: columnview :key value ...` line into its parameters.
/// `None` if the line does not name the `columnview` dynamic block. `:id`
/// defaults to `"local"` when omitted.
#[must_use]
pub fn parse_dblock_params(begin_line: &str) -> Option<DblockParams> {
    let trimmed = begin_line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let idx = lower.find("columnview")?;
    let rest = &trimmed[idx + "columnview".len()..];
    let tokens = tokenize_quoted(rest);

    let mut id = String::new();
    let mut hlines = None;
    let mut vlines = false;
    let mut maxlevel = None;
    let mut skip_empty_rows = false;
    let mut exclude_tags = Vec::new();
    let mut indent = false;
    let mut link = false;
    let mut format = None;

    let mut i = 0;
    while i < tokens.len() {
        let key = tokens[i].to_ascii_lowercase();
        let val = tokens.get(i + 1).cloned();
        match key.as_str() {
            ":id" => id = val.unwrap_or_default(),
            ":hlines" => hlines = val,
            ":vlines" => vlines = val.as_deref() == Some("t"),
            ":maxlevel" => maxlevel = val.as_deref().and_then(|s| s.parse().ok()),
            ":skip-empty-rows" => skip_empty_rows = val.as_deref() == Some("t"),
            ":exclude-tags" => {
                exclude_tags = val
                    .as_deref()
                    .map(|s| s.split_whitespace().map(str::to_string).collect())
                    .unwrap_or_default();
            }
            ":indent" => indent = val.as_deref() == Some("t"),
            ":link" => link = val.as_deref() == Some("t"),
            ":format" => format = val,
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    if id.is_empty() {
        id = "local".to_string();
    }
    Some(DblockParams {
        id,
        hlines,
        vlines,
        maxlevel,
        skip_empty_rows,
        exclude_tags,
        indent,
        link,
        format,
    })
}

/// Serialize `params` back into a `#+BEGIN: columnview ...` line.
fn render_begin_line(params: &DblockParams) -> String {
    let mut out = format!("#+BEGIN: columnview :id {}", params.id);
    if let Some(h) = &params.hlines {
        let _ = write!(out, " :hlines {h}");
    }
    if params.vlines {
        out.push_str(" :vlines t");
    }
    if let Some(m) = params.maxlevel {
        let _ = write!(out, " :maxlevel {m}");
    }
    if params.skip_empty_rows {
        out.push_str(" :skip-empty-rows t");
    }
    if !params.exclude_tags.is_empty() {
        let _ = write!(out, " :exclude-tags \"{}\"", params.exclude_tags.join(" "));
    }
    if params.indent {
        out.push_str(" :indent t");
    }
    if params.link {
        out.push_str(" :link t");
    }
    if let Some(f) = &params.format {
        let _ = write!(out, " :format \"{f}\"");
    }
    out
}

/// Resolve the `[start, end)` headline scope a dblock's `:id` selects.
fn dblock_scope(lines: &[&str], at_line: usize, id: &str) -> (usize, usize) {
    if id == "local" {
        if let Some(h) = crate::governing(lines, at_line) {
            return crate::subtree_range(lines, h).unwrap_or((0, lines.len()));
        }
        return (0, lines.len());
    }
    if id == "global" || id.starts_with("file:") {
        return (0, lines.len());
    }
    // An arbitrary label: the subtree whose entry has `:ID: LABEL`.
    for (i, line) in lines.iter().enumerate() {
        if crate::headline_level(line).is_some()
            && property_value_opt(lines, i, "ID").as_deref() == Some(id)
        {
            return crate::subtree_range(lines, i).unwrap_or((i, i + 1));
        }
    }
    (0, 0)
}

/// Render the `#+BEGIN: columnview ... \n <table> \n#+END:` block content for
/// the dblock at `at_line`, used both for fresh insertion and for updating an
/// existing block in place.
#[must_use]
pub fn render_columnview_dblock(
    text: &str,
    at_line: usize,
    params: &DblockParams,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let spec = params
        .format
        .as_deref()
        .and_then(parse_columns_spec)
        .unwrap_or_else(|| resolve_columns_spec(text, at_line));
    let scope = dblock_scope(&lines, at_line, &params.id);
    let root_level = (scope.0..scope.1.min(lines.len()))
        .find_map(|i| crate::headline_level(lines[i]))
        .unwrap_or(1);
    let (rows, _) = compute_table_in_scope(text, scope, &spec, today, file_name);

    let item_idx = spec
        .columns
        .iter()
        .position(|c| c.property.eq_ignore_ascii_case("ITEM"));

    let mut body = String::new();
    let header: Vec<String> = spec
        .columns
        .iter()
        .map(|c| c.title.clone().unwrap_or_else(|| c.property.clone()))
        .collect();
    let _ = writeln!(body, "| {} |", header.join(" | "));
    let _ = writeln!(body, "|{}|", "---|".repeat(header.len()));
    for row in &rows {
        if params.skip_empty_rows
            && row
                .values
                .iter()
                .enumerate()
                .all(|(i, v)| v.is_empty() || Some(i) == item_idx)
        {
            continue;
        }
        if let Some(max) = params.maxlevel
            && row.level > root_level + max - 1
        {
            continue;
        }
        if !params.exclude_tags.is_empty() {
            let tags = item_tags(&lines, row.line);
            if params
                .exclude_tags
                .iter()
                .any(|t| tags.contains(&format!(":{t}:")))
            {
                continue;
            }
        }
        if params.hlines.is_some() && row.level == root_level && !body.ends_with("---|\n") {
            let _ = writeln!(body, "|{}|", "---|".repeat(header.len()));
        }
        let mut cells: Vec<String> = row.values.clone();
        if let Some(ii) = item_idx {
            if params.indent {
                cells[ii] = format!(
                    "{}{}",
                    "  ".repeat(row.level.saturating_sub(root_level)),
                    cells[ii]
                );
            }
            if params.link {
                cells[ii] = format!("[[*{}]]", cells[ii]);
            }
        }
        let cells: Vec<String> = cells.iter().map(|c| c.replace('|', "\\vert{}")).collect();
        let _ = writeln!(body, "| {} |", cells.join(" | "));
    }
    format!("{}\n{}#+END:", render_begin_line(params), body)
}

/// Whether `line` opens a `columnview` dynamic block.
fn is_columnview_begin(line: &str) -> bool {
    let t = line.trim().to_ascii_lowercase();
    t.starts_with("#+begin:") && t.contains("columnview")
}

/// Find the matching `#+END:` line for the `#+BEGIN:` at `begin_line`,
/// stopping (returning `None`) if a headline is reached first.
fn dblock_end(lines: &[&str], begin_line: usize) -> Option<usize> {
    let mut i = begin_line + 1;
    while i < lines.len() {
        if crate::headline_level(lines[i]).is_some() {
            return None;
        }
        if lines[i].trim().eq_ignore_ascii_case("#+END:") {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find and update the `columnview` dblock whose `#+BEGIN:` line is at
/// `begin_line`. `None` if there is no `#+BEGIN: columnview` there, or no
/// matching `#+END:` below it.
#[must_use]
pub fn update_columnview_dblock(
    text: &str,
    begin_line: usize,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    if !is_columnview_begin(lines.get(begin_line)?) {
        return None;
    }
    let params = parse_dblock_params(lines[begin_line])?;
    let end_line = dblock_end(&lines, begin_line)?;
    let rendered = render_columnview_dblock(text, begin_line, &params, today, file_name);
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..begin_line]);
    let rendered_lines: Vec<&str> = rendered.split('\n').collect();
    out.extend_from_slice(&rendered_lines);
    out.extend_from_slice(&lines[end_line + 1..]);
    Some(out.join("\n"))
}

/// Update every `columnview` dblock found anywhere in the buffer.
#[must_use]
pub fn update_all_columnview_dblocks(
    text: &str,
    today: (i32, u32, u32),
    file_name: Option<&str>,
) -> String {
    let mut out = text.to_string();
    let mut search_from = 0usize;
    loop {
        let lines: Vec<&str> = out.split('\n').collect();
        let found = lines
            .iter()
            .enumerate()
            .skip(search_from)
            .find(|(_, l)| is_columnview_begin(l));
        let Some((idx, _)) = found else { break };
        match update_columnview_dblock(&out, idx, today, file_name) {
            Some(updated) => {
                out = updated;
                search_from = idx + 1;
            }
            None => search_from = idx + 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_columns_spec_matches_the_org_manual_example() {
        let line =
            ":COLUMNS:  %25ITEM %9Approved(Approved?){X} %Owner %11Status %10Time_Estimate{:}";
        let spec = parse_columns_spec(line).expect("parses");
        assert_eq!(spec.columns.len(), 5);
        assert_eq!(
            spec.columns[0],
            ColumnDef {
                property: "ITEM".into(),
                title: None,
                width: Some(25),
                summary: None
            }
        );
        assert_eq!(
            spec.columns[1],
            ColumnDef {
                property: "Approved".into(),
                title: Some("Approved?".into()),
                width: Some(9),
                summary: Some("X".into()),
            }
        );
        assert_eq!(
            spec.columns[2],
            ColumnDef {
                property: "Owner".into(),
                title: None,
                width: None,
                summary: None
            }
        );
        assert_eq!(
            spec.columns[3],
            ColumnDef {
                property: "Status".into(),
                title: None,
                width: Some(11),
                summary: None
            }
        );
        assert_eq!(
            spec.columns[4],
            ColumnDef {
                property: "Time_Estimate".into(),
                title: None,
                width: Some(10),
                summary: Some(":".into()),
            }
        );
    }

    #[test]
    fn parse_all_values_tokenizes_quoted_and_bare_lists() {
        let drawer = [
            ":Owner_ALL:    Tammy Mark Karl Lisa Don",
            r#":Status_ALL:   "In progress" "Not started yet" "Finished" """#,
            r#":Approved_ALL: "[ ]" "[X]""#,
        ];
        let map = parse_all_values(&drawer);
        assert_eq!(
            map.get("Owner").unwrap(),
            &vec!["Tammy", "Mark", "Karl", "Lisa", "Don"]
        );
        assert_eq!(
            map.get("Status").unwrap(),
            &vec!["In progress", "Not started yet", "Finished", ""]
        );
        assert_eq!(map.get("Approved").unwrap(), &vec!["[ ]", "[X]"]);
    }

    #[test]
    fn resolve_columns_spec_prefers_nearest_ancestor_then_file_then_default() {
        let doc = "#+COLUMNS: %ITEM %TODO\n\
* A\n\
:PROPERTIES:\n\
:COLUMNS: %ITEM %PRIORITY\n\
:END:\n\
** B\nbody\n\
* C\nbody\n";
        let lines: Vec<&str> = doc.split('\n').collect();
        let b_line = lines.iter().position(|l| *l == "** B").unwrap();
        let spec = resolve_columns_spec(doc, b_line);
        assert_eq!(spec.columns.len(), 2);
        assert_eq!(
            spec.columns[1].property, "PRIORITY",
            "nearest ancestor wins"
        );

        let c_line = lines.iter().position(|l| *l == "* C").unwrap();
        let spec = resolve_columns_spec(doc, c_line);
        assert_eq!(spec.columns[1].property, "TODO", "falls back to file level");

        let default = resolve_columns_spec("* Bare\n", 0);
        let names: Vec<&str> = default
            .columns
            .iter()
            .map(|c| c.property.as_str())
            .collect();
        assert_eq!(
            names,
            ["ITEM", "TODO", "PRIORITY", "TAGS"],
            "hardcoded default"
        );
    }

    fn spec_of(cols: &[(&str, Option<&str>)]) -> ColumnsSpec {
        ColumnsSpec {
            columns: cols
                .iter()
                .map(|(p, s)| ColumnDef {
                    property: (*p).to_string(),
                    title: None,
                    width: None,
                    summary: s.map(str::to_string),
                })
                .collect(),
            allowed_values: BTreeMap::new(),
        }
    }

    #[test]
    fn build_column_table_reads_special_properties() {
        let doc = "* TODO [#A] Write report :work:urgent:\nSCHEDULED: <2026-08-20 Thu>\n";
        let spec = spec_of(&[
            ("ITEM", None),
            ("TODO", None),
            ("PRIORITY", None),
            ("TAGS", None),
        ]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), Some("report.org"));
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].values,
            ["Write report", "TODO", "[#A]", ":work:urgent:"]
        );

        let spec2 = spec_of(&[("SCHEDULED", None), ("CATEGORY", None), ("FILE", None)]);
        let (rows2, _) = build_column_table(doc, 0, &spec2, (2026, 8, 12), Some("report.org"));
        assert_eq!(rows2[0].values[0], "<2026-08-20 Thu>");
        assert_eq!(
            rows2[0].values[1], "report",
            "CATEGORY falls back to the filename"
        );
        assert_eq!(rows2[0].values[2], "report.org");
    }

    #[test]
    fn build_column_table_general_property_is_not_inherited() {
        let doc = "* Parent\n:PROPERTIES:\n:OWNER: Tammy\n:END:\n** Child\nbody\n";
        let spec = spec_of(&[("ITEM", None), ("OWNER", None)]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(rows[0].values, ["Parent", "Tammy"]);
        assert_eq!(rows[1].values, ["Child", ""], "OWNER is not inherited");
    }

    #[test]
    fn summary_plus_sums_numbers() {
        assert_eq!(apply_summary("+", &["1", "2.5", "3"]), "6.5");
        assert_eq!(apply_summary("+;%.1f", &["1", "2", "3"]), "6.0");
        assert_eq!(apply_summary("$", &["1", "2"]), "3.00");
    }

    #[test]
    fn summary_min_max_mean() {
        assert_eq!(apply_summary("min", &["3", "1", "2"]), "1");
        assert_eq!(apply_summary("max", &["3", "1", "2"]), "3");
        assert_eq!(apply_summary("mean", &["1", "2", "3"]), "2");
    }

    #[test]
    fn summary_checkbox_variants() {
        assert_eq!(apply_summary("X", &["[X]", "[X]"]), "[X]");
        assert_eq!(apply_summary("X", &["[X]", "[ ]"]), "[ ]");
        assert_eq!(apply_summary("X/", &["[X]", "[ ]", "junk"]), "[1/3]");
        assert_eq!(apply_summary("X%", &["[X]", "[X]", "[ ]", "[ ]"]), "[50%]");
    }

    #[test]
    fn summary_time_family() {
        assert_eq!(apply_summary(":", &["1:00", "0:30"]), "1h 30min");
        assert_eq!(apply_summary(":mean", &["1h", "1h", "1h"]), "1h 0min");
        assert_eq!(apply_summary(":min", &["2h", "30min"]), "30min");
        assert_eq!(apply_summary(":max", &["2h", "30min"]), "2h 0min");
    }

    #[test]
    fn summary_age_family() {
        assert_eq!(apply_summary("@min", &["3d 1h", "1h"]), "1h");
        assert_eq!(apply_summary("@max", &["3d 1h", "1h"]), "3d 1h");
    }

    #[test]
    fn summary_est_plus_combines_ranges_within_a_sane_band() {
        let ten = ["0.5-2"; 10];
        let combined = apply_summary("est+", &ten);
        let (lo, hi) = combined.split_once('-').expect("range");
        let lo: f64 = lo.parse().expect("low");
        let hi: f64 = hi.parse().expect("high");
        assert!((8.0..=12.0).contains(&lo), "low {lo} out of band");
        assert!((13.0..=17.0).contains(&hi), "high {hi} out of band");
    }

    #[test]
    fn summary_unsupported_token_is_blank() {
        assert_eq!(apply_summary("bogus", &["1", "2"]), "");
    }

    #[test]
    fn nested_effort_mean_rolls_up_and_writes_back() {
        let doc = "\
* Top level
** Intermediate 1
:PROPERTIES:
:EFFORT: unused
:END:
*** Leaf 1
:PROPERTIES:
:EFFORT: 1h
:END:
*** Leaf 2
:PROPERTIES:
:EFFORT: 1h
:END:
*** Leaf 3
:PROPERTIES:
:EFFORT: 1h
:END:
** Intermediate 2
:PROPERTIES:
:EFFORT: 5h
:END:
";
        let spec = spec_of(&[("ITEM", None), ("EFFORT", Some(":mean"))]);
        let (rows, rewritten) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);

        let by_item: HashMap<&str, &str> = rows
            .iter()
            .map(|r| (r.values[0].as_str(), r.values[1].as_str()))
            .collect();
        assert_eq!(by_item["Top level"], "3h 0min");
        assert_eq!(by_item["Intermediate 1"], "1h 0min");
        assert_eq!(
            by_item["Intermediate 2"], "5h",
            "no children: kept as its own literal value"
        );
        assert_eq!(by_item["Leaf 1"], "1h", "leaf keeps its own literal value");

        let intermediate_1_line = rewritten
            .lines()
            .position(|l| l.trim() == "** Intermediate 1")
            .unwrap();
        assert_eq!(
            rewritten
                .lines()
                .nth(intermediate_1_line + 2)
                .unwrap()
                .trim(),
            ":EFFORT: 1h 0min"
        );
    }

    #[test]
    fn only_first_duplicate_column_definition_writes_back() {
        let doc = "\
* Top
** Child
:PROPERTIES:
:EFFORT: 1h
:END:
";
        // First definition has no summary type: nothing should be written,
        // even though the second (duplicate) definition does have one.
        let spec = spec_of(&[("ITEM", None), ("EFFORT", None), ("EFFORT", Some(":mean"))]);
        let (_, rewritten) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(
            rewritten, doc,
            "no write-back when the first definition has no summary"
        );
    }

    #[test]
    fn apply_column_edit_round_trips_tags_todo_and_property_then_recomputes_ancestor() {
        let doc = "\
* Parent
:PROPERTIES:
:EFFORT: unused
:END:
** Child
:PROPERTIES:
:EFFORT: 2h
:END:
";
        let spec = spec_of(&[("ITEM", None), ("EFFORT", Some(":mean"))]);
        let edited = apply_column_edit(doc, 4, &spec, 1, "4h").expect("edit applies");
        assert!(edited.contains(":EFFORT: 4h"), "child's own value updated");
        let parent_line = edited.lines().position(|l| l.trim() == "* Parent").unwrap();
        assert_eq!(
            edited.lines().nth(parent_line + 2).unwrap().trim(),
            ":EFFORT: 4h 0min",
            "ancestor rollup recomputed"
        );

        let tags_spec = spec_of(&[("ITEM", None), ("TAGS", None)]);
        let edited =
            apply_column_edit("* Plain\n", 0, &tags_spec, 1, "work urgent").expect("tags edit");
        assert_eq!(edited.trim_end(), "* Plain :work:urgent:");

        let todo_spec = spec_of(&[("ITEM", None), ("TODO", None)]);
        let edited = apply_column_edit("* Task\n", 0, &todo_spec, 1, "WAITING").expect("todo edit");
        assert_eq!(edited.trim_end(), "* WAITING Task");
    }

    #[test]
    fn parse_dblock_params_reads_the_manual_syntax() {
        let line = r#"#+BEGIN: columnview :id global :hlines t :maxlevel 2 :skip-empty-rows t :indent t :format "%ITEM %TODO""#;
        let params = parse_dblock_params(line).expect("parses");
        assert_eq!(params.id, "global");
        assert_eq!(params.hlines.as_deref(), Some("t"));
        assert_eq!(params.maxlevel, Some(2));
        assert!(params.skip_empty_rows);
        assert!(params.indent);
        assert_eq!(params.format.as_deref(), Some("%ITEM %TODO"));

        let bare = parse_dblock_params("#+BEGIN: columnview").expect("parses with no params");
        assert_eq!(bare.id, "local");
    }

    #[test]
    fn render_and_update_columnview_dblock_round_trip() {
        let doc = "\
* Project
** TODO Task One
** DONE Task Two
#+BEGIN: columnview :id local
#+END:
";
        let begin_line = doc.lines().position(|l| l.starts_with("#+BEGIN:")).unwrap();
        let updated = update_columnview_dblock(doc, begin_line, (2026, 8, 12), Some("proj.org"))
            .expect("dblock updates");
        assert!(updated.contains("Task One"));
        assert!(updated.contains("Task Two"));
        assert!(updated.contains("#+END:"));

        // Not a columnview dblock at this line.
        assert_eq!(update_columnview_dblock(doc, 0, (2026, 8, 12), None), None);
    }

    #[test]
    fn update_all_columnview_dblocks_updates_every_block() {
        let doc = "\
* A
** TODO One
#+BEGIN: columnview :id local
#+END:
* B
** TODO Two
#+BEGIN: columnview :id local
#+END:
";
        let updated = update_all_columnview_dblocks(doc, (2026, 8, 12), None);
        let count = updated.lines().filter(|l| is_columnview_begin(l)).count();
        assert_eq!(count, 2);
        assert!(updated.contains("One"));
        assert!(updated.contains("Two"));
    }

    #[test]
    fn clocksum_scans_whole_subtree_not_per_level() {
        let doc = "\
* Top
CLOCK: [2026-08-10 Mon 09:00]--[2026-08-10 Mon 10:00] =>  1:00
** Child
CLOCK: [2026-08-11 Tue 09:00]--[2026-08-11 Tue 09:30] =>  0:30
";
        let spec = spec_of(&[("ITEM", None), ("CLOCKSUM", None)]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(
            rows[0].values[1], "1:30",
            "top's CLOCKSUM includes the child's clock"
        );
        assert_eq!(rows[1].values[1], "0:30");
    }

    #[test]
    fn clocksum_t_filters_to_todays_clocks() {
        let doc = "\
* Top
CLOCK: [2026-08-10 Mon 09:00]--[2026-08-10 Mon 10:00] =>  1:00
CLOCK: [2026-08-12 Wed 09:00]--[2026-08-12 Wed 09:15] =>  0:15
";
        let spec = spec_of(&[("ITEM", None), ("CLOCKSUM_T", None)]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(rows[0].values[1], "0:15");
    }

    #[test]
    fn blocked_reflects_unfinished_direct_children() {
        let doc = "* Parent\n** TODO Child A\n** DONE Child B\n";
        let spec = spec_of(&[("ITEM", None), ("BLOCKED", None)]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(rows[0].values[1], "t");

        let doc2 = "* Parent\n** DONE Child A\n";
        let (rows2, _) = build_column_table(doc2, 0, &spec, (2026, 8, 12), None);
        assert_eq!(rows2[0].values[1], "");
    }

    #[test]
    fn todo_keywords_exposes_the_fixed_list() {
        assert!(todo_keywords().contains(&"TODO"));
        assert!(todo_keywords().contains(&"WAITING"));
    }

    #[test]
    fn columns_spec_anchor_finds_nearest_ancestor_or_none() {
        let doc = "#+COLUMNS: %ITEM %TODO\n\
* A\n\
:PROPERTIES:\n\
:COLUMNS: %ITEM %PRIORITY\n\
:END:\n\
** B\nbody\n\
* C\nbody\n";
        let lines: Vec<&str> = doc.split('\n').collect();
        let a_line = lines.iter().position(|l| *l == "* A").unwrap();
        let b_line = lines.iter().position(|l| *l == "** B").unwrap();
        let c_line = lines.iter().position(|l| *l == "* C").unwrap();
        assert_eq!(columns_spec_anchor(doc, b_line), Some(a_line));
        assert_eq!(
            columns_spec_anchor(doc, c_line),
            None,
            "file-level spec has no owning headline"
        );
    }

    #[test]
    fn alltags_includes_ancestor_tags() {
        let doc = "* Top :outer:\n** Child :inner:\nbody\n";
        let spec = spec_of(&[("ITEM", None), ("TAGS", None), ("ALLTAGS", None)]);
        let (rows, _) = build_column_table(doc, 0, &spec, (2026, 8, 12), None);
        assert_eq!(rows[1].values[1], ":inner:");
        assert_eq!(rows[1].values[2], ":inner:outer:");
    }
}
