//! The built-in agenda views (the dated agenda, the global TODO list, tag
//! matching, search, stuck projects) and time tracking (clock in/out, the
//! per-headline time report). Extracted from `lib.rs` (T516). `days_from_civil`
//! stays `pub(crate)`: the "Dates & scheduling" section (`properties_and_dates.rs`,
//! T516 slice 3) needs it to shift a timestamp's date. `clock_start`/
//! `clock_minutes`/`hhmm` stay `pub(crate)` too: `columns.rs` needs them to
//! render a column-view CLOCK summary.

use std::fmt::Write as _;

use super::{headline_level, headline_todo};

// ----- Agenda & time tracking -----------------------------------------------

/// Extract the `YYYY-MM-DD` date from the first `<…>`/`[…]` timestamp in `s`.
fn first_date(s: &str) -> Option<String> {
    let start = s.find(['<', '['])?;
    let date: String = s[start + 1..].chars().take(10).collect();
    let b = date.as_bytes();
    if date.len() == 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit) {
        Some(date)
    } else {
        None
    }
}

/// One entry in a compiled agenda, carrying enough provenance to act on it (the
/// source `file` display name and 0-based headline `line`) so an interactive
/// agenda can toggle the underlying task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgendaItem {
    /// `Some(YYYY-MM-DD)` for a dated (`DEADLINE`/`SCHEDULED`) entry, `None` for
    /// an unscheduled `TODO`.
    pub date: Option<String>,
    /// `DEADLINE`, `SCHEDULED`, or `TODO`.
    pub kind: String,
    /// The headline text (after the leading stars), e.g. `TODO Ship it`.
    pub headline: String,
    /// The source document's display name (as supplied to [`agenda_items`]).
    pub file: String,
    /// 0-based line of the headline within its source document.
    pub line: usize,
}

/// Compile the agenda **items** from `(filename, content)` Org documents:
/// `DEADLINE:` / `SCHEDULED:` planning lines (dated) and `TODO` headlines that
/// carry no date (unscheduled). Items are returned in document order; each
/// records the source line of its headline so a caller can act on it. Pure.
#[must_use]
pub fn agenda_items(files: &[(String, String)]) -> Vec<AgendaItem> {
    let mut items: Vec<AgendaItem> = Vec::new();
    for (name, content) in files {
        let mut current = String::new();
        let mut current_line = 0;
        for (idx, line) in content.lines().enumerate() {
            if let Some(level) = headline_level(line) {
                current = line[level..].trim().to_string();
                current_line = idx;
                if current.split_whitespace().next() == Some("TODO") {
                    items.push(AgendaItem {
                        date: None,
                        kind: "TODO".to_string(),
                        headline: current.clone(),
                        file: name.clone(),
                        line: current_line,
                    });
                }
                continue;
            }
            let trimmed = line.trim();
            for kind in ["DEADLINE", "SCHEDULED"] {
                if let Some(rest) = trimmed.strip_prefix(&format!("{kind}:"))
                    && let Some(date) = first_date(rest)
                {
                    items.push(AgendaItem {
                        date: Some(date),
                        kind: kind.to_string(),
                        headline: current.clone(),
                        file: name.clone(),
                        line: current_line,
                    });
                }
            }
        }
    }
    items
}

/// Render agenda `items` into an Org document (dated entries grouped by date,
/// then an *Unscheduled tasks* section) alongside a per-line map: `map[i]` is
/// the index into `items` of the entry shown on buffer line `i`, or `None` for
/// title/heading/blank lines. The text is identical to what [`agenda`] returns.
#[must_use]
pub fn render_agenda(items: &[AgendaItem]) -> (String, Vec<Option<usize>>) {
    let mut buf = String::new();
    let mut map: Vec<Option<usize>> = Vec::new();
    let push = |buf: &mut String, map: &mut Vec<Option<usize>>, line: &str, item| {
        buf.push_str(line);
        buf.push('\n');
        map.push(item);
    };
    push(&mut buf, &mut map, "#+title: Agenda", None);

    // Dated entries, sorted by (date, kind, headline, file) and grouped by date.
    let mut dated: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].date.is_some())
        .collect();
    dated.sort_by(|&a, &b| {
        let x = &items[a];
        let y = &items[b];
        (&x.date, &x.kind, &x.headline, &x.file).cmp(&(&y.date, &y.kind, &y.headline, &y.file))
    });
    let mut last: Option<&str> = None;
    for &i in &dated {
        let it = &items[i];
        let date = it.date.as_deref().unwrap_or_default();
        if last != Some(date) {
            push(&mut buf, &mut map, "", None);
            push(&mut buf, &mut map, &format!("* {date}"), None);
            last = Some(date);
        }
        push(
            &mut buf,
            &mut map,
            &format!("- {}: {} ({})", it.kind, it.headline, it.file),
            Some(i),
        );
    }

    // Unscheduled TODOs, in document order.
    let undated: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].date.is_none())
        .collect();
    if !undated.is_empty() {
        push(&mut buf, &mut map, "", None);
        push(&mut buf, &mut map, "* Unscheduled tasks", None);
        for &i in &undated {
            let it = &items[i];
            push(
                &mut buf,
                &mut map,
                &format!("- {} ({})", it.headline, it.file),
                Some(i),
            );
        }
    }
    (buf, map)
}

/// Compile an **agenda** from `(filename, content)` Org documents: `DEADLINE:` and
/// `SCHEDULED:` planning lines grouped by date, plus `TODO` headlines that have no
/// date. Returns an Org document (open it in a buffer). Pure and testable.
#[must_use]
pub fn agenda(files: &[(String, String)]) -> String {
    render_agenda(&agenda_items(files)).0
}

// ----- Other built-in agenda views ------------------------------------------

/// The headline text (after the leading stars) of a headline `line`.
fn headline_text(line: &str) -> &str {
    match headline_level(line) {
        Some(level) => line[level..].trim_start(),
        None => line,
    }
}

/// Make an [`AgendaItem`] for the (undated) list views from a headline.
fn list_item(headline: &str, file: &str, line: usize) -> AgendaItem {
    AgendaItem {
        date: None,
        kind: String::new(),
        headline: headline_text(headline).to_string(),
        file: file.to_string(),
        line,
    }
}

/// The **global TODO list** (Org agenda `t`): every headline across `files`
/// whose keyword is a not-DONE TODO. Pure.
#[must_use]
pub fn todo_list(files: &[(String, String)]) -> Vec<AgendaItem> {
    let mut out = Vec::new();
    for (name, content) in files {
        for (idx, line) in content.lines().enumerate() {
            if headline_todo(line) == Some(false) {
                out.push(list_item(line, name, idx));
            }
        }
    }
    out
}

/// The trailing `:a:b:c:` tags of a headline, lower-cased, or empty.
fn headline_tags(line: &str) -> Vec<String> {
    let trimmed = line.trim_end();
    let Some(sp) = trimmed.rfind(char::is_whitespace) else {
        return Vec::new();
    };
    let tail = &trimmed[sp + 1..];
    if tail.len() < 2 || !tail.starts_with(':') || !tail.ends_with(':') {
        return Vec::new();
    }
    let inner = &tail[1..tail.len() - 1];
    if inner.is_empty()
        || inner.split(':').any(|t| {
            t.is_empty()
                || !t
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%'))
        })
    {
        return Vec::new();
    }
    inner.split(':').map(str::to_ascii_lowercase).collect()
}

/// Parse a pragmatic tag-match `query` into `(required, excluded)` tag lists.
/// Tokens are separated by whitespace or `+`/`-`; a `-` marks the following tag
/// as excluded, `+` (or nothing) as required (e.g. `work+urgent-boss`). Tags are
/// lower-cased for case-insensitive matching.
fn parse_tag_query(query: &str) -> (Vec<String>, Vec<String>) {
    let mut required = Vec::new();
    let mut excluded = Vec::new();
    let mut sign = '+';
    let mut cur = String::new();
    let mut flush = |cur: &mut String, sign: char| {
        if !cur.is_empty() {
            let tag = std::mem::take(cur).to_ascii_lowercase();
            if sign == '-' {
                excluded.push(tag);
            } else {
                required.push(tag);
            }
        }
    };
    for ch in query.chars() {
        match ch {
            '+' | '-' => {
                flush(&mut cur, sign);
                sign = ch;
            }
            c if c.is_whitespace() => {
                flush(&mut cur, sign);
                sign = '+';
            }
            c => cur.push(c),
        }
    }
    flush(&mut cur, sign);
    (required, excluded)
}

/// **Match tags** (Org agenda `m`): headlines across `files` whose trailing
/// `:tags:` satisfy `query` — all required tags present and no excluded tag
/// present (a pragmatic subset of Org's match syntax: `+tag`, `-tag`, bare
/// `tag`, case-insensitive). An empty query matches every tagged headline. Pure.
#[must_use]
pub fn tags_match(files: &[(String, String)], query: &str) -> Vec<AgendaItem> {
    let (required, excluded) = parse_tag_query(query);
    let mut out = Vec::new();
    for (name, content) in files {
        for (idx, line) in content.lines().enumerate() {
            if headline_level(line).is_none() {
                continue;
            }
            let tags = headline_tags(line);
            if tags.is_empty() {
                continue;
            }
            let ok = required.iter().all(|r| tags.contains(r))
                && !excluded.iter().any(|e| tags.contains(e));
            if ok {
                out.push(list_item(line, name, idx));
            }
        }
    }
    out
}

/// **Search view** (Org agenda `s`): headlines whose entry body (the headline
/// through the line before the next headline) contains *all* whitespace-split
/// words of `query`, case-insensitively. An empty query matches nothing. Pure.
#[must_use]
pub fn search(files: &[(String, String)], query: &str) -> Vec<AgendaItem> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let mut out = Vec::new();
    if words.is_empty() {
        return out;
    }
    for (name, content) in files {
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if headline_level(line).is_none() {
                continue;
            }
            let mut end = idx + 1;
            while end < lines.len() && headline_level(lines[end]).is_none() {
                end += 1;
            }
            let body = lines[idx..end].join("\n").to_ascii_lowercase();
            if words.iter().all(|w| body.contains(w)) {
                out.push(list_item(line, name, idx));
            }
        }
    }
    out
}

/// **Stuck projects** (Org agenda `#`): a *project* is a not-DONE headline with
/// at least one child headline; it is *stuck* when none of its descendants is a
/// not-DONE TODO (no next action). Returns the stuck project headlines. Pure.
#[must_use]
pub fn stuck_projects(files: &[(String, String)]) -> Vec<AgendaItem> {
    let mut out = Vec::new();
    for (name, content) in files {
        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let Some(level) = headline_level(line) else {
                continue;
            };
            if headline_todo(line) == Some(true) {
                continue; // the project itself is already DONE
            }
            let mut end = idx + 1;
            while end < lines.len() && headline_level(lines[end]).is_none_or(|l| l > level) {
                end += 1;
            }
            let children = &lines[idx + 1..end];
            let has_child = children.iter().any(|l| headline_level(l).is_some());
            let has_next_action = children.iter().any(|l| headline_todo(l) == Some(false));
            if has_child && !has_next_action {
                out.push(list_item(line, name, idx));
            }
        }
    }
    out
}

/// Render a flat list `view` (TODO list / match / search / stuck projects) into
/// an Org document titled `title`, alongside the per-line map (`map[i]` is the
/// item index shown on buffer line `i`, or `None`). Mirrors [`render_agenda`]'s
/// contract so the same interactive machinery drives every view.
#[must_use]
pub fn render_list(title: &str, items: &[AgendaItem]) -> (String, Vec<Option<usize>>) {
    let mut buf = String::new();
    let mut map: Vec<Option<usize>> = Vec::new();
    let push = |buf: &mut String, map: &mut Vec<Option<usize>>, line: &str, item| {
        buf.push_str(line);
        buf.push('\n');
        map.push(item);
    };
    push(&mut buf, &mut map, &format!("#+title: {title}"), None);
    for (i, it) in items.iter().enumerate() {
        push(
            &mut buf,
            &mut map,
            &format!("- {} ({})", it.headline, it.file),
            Some(i),
        );
    }
    (buf, map)
}

/// Minutes in a `CLOCK:` line's explicit `=> H:MM` total, if present.
pub(crate) fn clock_minutes(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix("CLOCK:")?;
    let after = &rest[rest.find("=>")? + 2..];
    let (h, m) = after.trim().split_once(':')?;
    Some(h.trim().parse::<u32>().ok()? * 60 + m.trim().parse::<u32>().ok()?)
}

/// Render `minutes` as `H:MM`.
pub(crate) fn hhmm(minutes: u32) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// Build a **time-tracking report** from `content`: sum each headline's `CLOCK:`
/// durations (the `=> H:MM` totals Org writes) into a table with a grand total.
/// Pure and testable.
#[must_use]
pub fn time_report(content: &str) -> String {
    let mut current = String::from("(top level)");
    let mut totals: Vec<(String, u32)> = Vec::new();
    for line in content.lines() {
        if let Some(level) = headline_level(line) {
            current = line[level..].trim().to_string();
            continue;
        }
        if let Some(min) = clock_minutes(line) {
            if let Some(entry) = totals.iter_mut().find(|(h, _)| *h == current) {
                entry.1 += min;
            } else {
                totals.push((current.clone(), min));
            }
        }
    }
    let mut out = String::from("| Headline | Time |\n|----------|------|\n");
    let mut grand = 0;
    for (headline, min) in &totals {
        let _ = writeln!(out, "| {headline} | {} |", hhmm(*min));
        grand += min;
    }
    let _ = writeln!(out, "| *Total* | {} |", hhmm(grand));
    out
}

// ----- Clocking -------------------------------------------------------------

/// An Org clock-in line for the timestamp `now` (e.g. `2024-08-23 Fri 10:00`).
#[must_use]
pub fn clock_in(now: &str) -> String {
    format!("CLOCK: [{now}]")
}

/// Whether `line` is an *open* clock entry (`CLOCK: [..]` with no end yet).
fn is_open_clock(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("CLOCK:") && t.contains('[') && t.ends_with(']') && !t.contains("--")
}

/// The start timestamp inside a `CLOCK: [start]` line.
pub(crate) fn clock_start(line: &str) -> Option<String> {
    let inner = line
        .trim()
        .strip_prefix("CLOCK:")?
        .trim()
        .strip_prefix('[')?;
    Some(inner[..inner.find(']')?].to_string())
}

/// Days since 1970-01-01 for a civil date. Delegates to `vix-civil-date`
/// (Run H, T520) — see [`civil_from_days`]'s doc comment.
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    vix_civil_date::days_from_civil(y, m, d)
}

/// Total minutes for an Org timestamp `YYYY-MM-DD … HH:MM` (date + trailing time).
fn timestamp_minutes(ts: &str) -> Option<i64> {
    let date = ts.get(0..10)?;
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;
    let (h, mi) = ts.rsplit(' ').next()?.split_once(':')?;
    let h: i64 = h.trim().parse().ok()?;
    let mi: i64 = mi.trim().parse().ok()?;
    Some(days_from_civil(y, m, d) * 1440 + h * 60 + mi)
}

/// Close the most recent open `CLOCK:` entry in `text` with end timestamp `now`,
/// appending the `=> H:MM` duration. Returns the rewritten text, or `None` if
/// there is no open clock entry.
#[must_use]
pub fn clock_out(text: &str, now: &str) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let idx = lines.iter().rposition(|l| is_open_clock(l))?;
    let start = clock_start(&lines[idx])?;
    let minutes = match (timestamp_minutes(now), timestamp_minutes(&start)) {
        (Some(n), Some(s)) => u32::try_from((n - s).max(0)).unwrap_or(0),
        _ => 0,
    };
    let lead: String = lines[idx]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    lines[idx] = format!("{lead}CLOCK: [{start}]--[{now}] =>  {}", hhmm(minutes));
    Some(lines.join("\n"))
}
