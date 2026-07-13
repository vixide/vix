//! Basic Org-mode operations: headline structure editing and lightweight export.
//!
//! This is a pragmatic subset of Org (<https://orgmode.org/>), not a complete
//! implementation. The structural helpers operate on the whole buffer text plus
//! a 0-based cursor line and return the rewritten text (and, where the cursor
//! should follow a moved subtree, its new line). The exporters turn Org markup
//! into Markdown or a small standalone HTML document.
//!
//! All functions are pure so they can be unit-tested without a live editor.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

/// The TODO keywords Org cycles through (besides the empty state).
const TODO: &str = "TODO";
const DONE: &str = "DONE";

/// The number of leading `*` of a headline (followed by a space), or `None` for
/// a non-headline line.
#[must_use]
pub fn headline_level(line: &str) -> Option<usize> {
    let stars = line.len() - line.trim_start_matches('*').len();
    if stars > 0 && line[stars..].starts_with(' ') {
        Some(stars)
    } else {
        None
    }
}

/// The `[start, end)` line range of the subtree rooted at `line` — the headline
/// plus every following line until the next headline of the same or higher level
/// (a smaller or equal star count). `None` if `line` is not a headline.
#[must_use]
pub fn subtree_range(lines: &[&str], line: usize) -> Option<(usize, usize)> {
    let level = headline_level(lines.get(line)?)?;
    let mut end = line + 1;
    while end < lines.len() {
        if headline_level(lines[end]).is_some_and(|l| l <= level) {
            break;
        }
        end += 1;
    }
    Some((line, end))
}

/// Promote (shallower, fewer stars) every headline in the subtree at `line`.
/// No-op returning `None` if not on a headline or any headline is already level 1.
#[must_use]
pub fn promote(text: &str, line: usize) -> Option<String> {
    reindent_subtree(text, line, false)
}

/// Demote (deeper, more stars) every headline in the subtree at `line`.
#[must_use]
pub fn demote(text: &str, line: usize) -> Option<String> {
    reindent_subtree(text, line, true)
}

/// Shared promote/demote: add or remove one leading `*` on each headline in the
/// subtree. Promoting a level-1 headline is refused (returns `None`).
fn reindent_subtree(text: &str, line: usize, deeper: bool) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (start, end) = subtree_range(&lines, line)?;
    if !deeper
        && lines[start..end]
            .iter()
            .any(|l| headline_level(l) == Some(1))
    {
        return None;
    }
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    for l in out.iter_mut().take(end).skip(start) {
        if headline_level(l).is_some() {
            if deeper {
                l.insert(0, '*');
            } else {
                l.remove(0);
            }
        }
    }
    Some(out.join("\n"))
}

/// Cycle the TODO state of the headline at `line`: none → `TODO` → `DONE` → none.
/// `None` if `line` is not a headline.
#[must_use]
pub fn cycle_todo(text: &str, line: usize) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let target = lines.get(line)?;
    let stars = headline_level(target)?;
    let (prefix, rest) = target.split_at(stars + 1); // include the space
    let new_rest = if let Some(after) = rest.strip_prefix(&format!("{TODO} ")) {
        format!("{DONE} {after}")
    } else if rest == TODO {
        DONE.to_string()
    } else if let Some(after) = rest.strip_prefix(&format!("{DONE} ")) {
        after.to_string()
    } else if rest == DONE {
        String::new()
    } else {
        format!("{TODO} {rest}")
    };
    lines[line] = format!("{prefix}{new_rest}");
    Some(lines.join("\n"))
}

static CHECKBOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*(?:[-+*]|\d+[.)])\s+)\[([ xX-])\]").expect("checkbox regex")
});

/// Toggle a list checkbox on the line at `line`: `[ ]` ⇄ `[x]` (treating `[-]`
/// and `[X]` as checked). `None` if the line has no checkbox.
#[must_use]
pub fn toggle_checkbox(text: &str, line: usize) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let target = lines.get(line)?;
    let caps = CHECKBOX.captures(target)?;
    let mark = caps.get(2)?.as_str();
    let new_mark = if mark == " " { "x" } else { " " };
    let lead_end = caps.get(1)?.end();
    let rest = &target[lead_end + 3..]; // skip "[m]"
    lines[line] = format!("{}[{new_mark}]{rest}", &target[..lead_end]);
    Some(lines.join("\n"))
}

// ----- Statistics cookies & checkbox propagation ----------------------------

/// A statistics cookie: `[/]`/`[n/m]` (fraction) or `[%]`/`[n%]` (percent).
static COOKIE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\d*/\d*\]|\[\d*%\]").expect("cookie regex"));

/// The indentation (leading-whitespace width) of a line.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Rewrite the first statistics cookie in `line` for `done`/`total`, preserving
/// its kind (`[n/m]` vs `[pct%]`). Percent truncates toward zero (Org's form);
/// `total == 0` yields `0%` or `0/0`. Lines without a cookie are returned as-is.
fn set_cookie(line: &str, done: usize, total: usize) -> String {
    let Some(m) = COOKIE.find(line) else {
        return line.to_string();
    };
    let replacement = if m.as_str().contains('%') {
        let pct = (done * 100).checked_div(total).unwrap_or(0);
        format!("[{pct}%]")
    } else {
        format!("[{done}/{total}]")
    };
    format!("{}{replacement}{}", &line[..m.start()], &line[m.end()..])
}

/// Replace a checkbox's mark character on a line that already has one.
fn set_checkbox_mark(line: &str, mark: char) -> String {
    let Some(caps) = CHECKBOX.captures(line) else {
        return line.to_string();
    };
    let lead_end = caps.get(1).map_or(0, |g| g.end());
    format!("{}[{mark}]{}", &line[..lead_end], &line[lead_end + 3..])
}

/// The TODO state of a headline: `Some(true)` if it carries the DONE keyword,
/// `Some(false)` for TODO, `None` if it has no TODO keyword (or isn't a headline).
fn headline_todo(line: &str) -> Option<bool> {
    let stars = headline_level(line)?;
    let kw = line[stars..].split_whitespace().next()?;
    if kw == DONE {
        Some(true)
    } else if kw == TODO {
        Some(false)
    } else {
        None
    }
}

/// Parse a `:COOKIE_DATA:` property from a headline's drawer lines into
/// `(count_todo, recursive)`. `None` when the property is absent (caller infers).
fn cookie_data(drawer: &[String]) -> Option<(bool, bool)> {
    for line in drawer {
        let t = line.trim();
        if t.eq_ignore_ascii_case(":END:") {
            break;
        }
        if let Some(rest) = t.get(..13)
            && rest.eq_ignore_ascii_case(":COOKIE_DATA:")
        {
            let value = t[13..].to_ascii_lowercase();
            let recursive = value.contains("recursive");
            if value.contains("todo") {
                return Some((true, recursive));
            }
            if value.contains("checkbox") {
                return Some((false, recursive));
            }
            return Some((false, recursive));
        }
    }
    None
}

/// Recompute every checkbox parent state and every statistics cookie in `text`,
/// matching Org's behavior:
///
/// * A checkbox list item with sub-items is set from its **direct** children —
///   all checked → `[X]`, none → `[ ]`, otherwise → `[-]`.
/// * A `[/]`/`[%]` cookie in a list item counts that item's direct child
///   checkboxes.
/// * A cookie in a headline counts either child checkboxes or child TODO
///   headlines. The `:COOKIE_DATA:` property (`checkbox`/`todo`, plus
///   `recursive`) resolves the ambiguity; absent it, a body with top-level
///   checkboxes counts checkboxes, otherwise direct child TODO headlines.
///
/// Pure: returns the rewritten buffer (line count unchanged).
#[must_use]
pub fn update_statistics(text: &str) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    update_checkboxes(&mut lines);
    update_headline_cookies(&mut lines);
    lines.join("\n")
}

/// One checkbox list item: its line, indent, and current mark.
struct Checkbox {
    line: usize,
    indent: usize,
    mark: char,
}

/// Propagate checkbox parent states and list-item cookies (first pass).
fn update_checkboxes(lines: &mut [String]) {
    let mut items: Vec<Checkbox> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if headline_level(l).is_some() {
            continue;
        }
        if let Some(c) = CHECKBOX.captures(l) {
            let mark = c
                .get(2)
                .and_then(|g| g.as_str().chars().next())
                .unwrap_or(' ');
            items.push(Checkbox {
                line: i,
                indent: indent_of(l),
                mark,
            });
        }
    }
    // Parent of each item = nearest preceding item with smaller indent, with the
    // nesting stack reset whenever a headline separates two items.
    let mut parent: Vec<Option<usize>> = vec![None; items.len()];
    let mut stack: Vec<usize> = Vec::new();
    for k in 0..items.len() {
        if k > 0
            && (items[k - 1].line + 1..items[k].line).any(|li| headline_level(&lines[li]).is_some())
        {
            stack.clear();
        }
        while stack
            .last()
            .is_some_and(|&top| items[top].indent >= items[k].indent)
        {
            stack.pop();
        }
        parent[k] = stack.last().copied();
        stack.push(k);
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    for (k, p) in parent.iter().enumerate() {
        if let Some(p) = *p {
            children[p].push(k);
        }
    }
    // Process deepest items first so a parent sees its children's final marks.
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(items[k].indent));
    for k in order {
        if children[k].is_empty() {
            continue;
        }
        let total = children[k].len();
        let done = children[k]
            .iter()
            .filter(|&&c| matches!(items[c].mark, 'x' | 'X'))
            .count();
        let any_partial = children[k].iter().any(|&c| items[c].mark == '-');
        let new_mark = if done == total {
            'X'
        } else if done == 0 && !any_partial {
            ' '
        } else {
            '-'
        };
        items[k].mark = new_mark;
        let li = items[k].line;
        lines[li] = set_checkbox_mark(&lines[li], new_mark);
        lines[li] = set_cookie(&lines[li], done, total);
    }
}

/// Update statistics cookies that live in headlines (second pass).
fn update_headline_cookies(lines: &mut [String]) {
    let levels: Vec<Option<usize>> = lines.iter().map(|l| headline_level(l)).collect();
    for h in 0..lines.len() {
        let Some(level) = levels[h] else { continue };
        if !COOKIE.is_match(&lines[h]) {
            continue;
        }
        // The subtree runs until the next headline of the same or higher level.
        let mut end = h + 1;
        while end < lines.len() && levels[end].is_none_or(|l| l > level) {
            end += 1;
        }
        let drawer: Vec<String> = lines[h + 1..end].to_vec();
        let body_end = (h + 1..end).find(|&j| levels[j].is_some()).unwrap_or(end);
        let has_checkboxes = (h + 1..body_end).any(|j| CHECKBOX.is_match(&lines[j]));
        let (count_todo, recursive) = cookie_data(&drawer).unwrap_or((!has_checkboxes, false));
        let (done, total) = if count_todo {
            let mut d = 0;
            let mut t = 0;
            for j in h + 1..end {
                let direct = levels[j] == Some(level + 1);
                let counted = if recursive {
                    levels[j].is_some()
                } else {
                    direct
                };
                if counted && let Some(is_done) = headline_todo(&lines[j]) {
                    t += 1;
                    if is_done {
                        d += 1;
                    }
                }
            }
            (d, t)
        } else {
            // Top-level checkboxes in the body (the shallowest indent).
            let cbs: Vec<(usize, char)> = (h + 1..body_end)
                .filter_map(|j| {
                    CHECKBOX
                        .captures(&lines[j])
                        .and_then(|c| c.get(2))
                        .map(|g| {
                            (
                                indent_of(&lines[j]),
                                g.as_str().chars().next().unwrap_or(' '),
                            )
                        })
                })
                .collect();
            cbs.iter()
                .map(|(i, _)| *i)
                .min()
                .map_or((0, 0), |min_indent| {
                    let top: Vec<char> = cbs
                        .iter()
                        .filter(|(i, _)| *i == min_indent)
                        .map(|(_, m)| *m)
                        .collect();
                    let d = top.iter().filter(|m| matches!(m, 'x' | 'X')).count();
                    (d, top.len())
                })
        };
        lines[h] = set_cookie(&lines[h], done, total);
    }
}

/// Move the subtree at `line` down past its next sibling, returning the new text
/// and the subtree's new starting line. `None` if there is no following sibling.
#[must_use]
pub fn move_subtree_down(text: &str, line: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let level = headline_level(lines.get(line)?)?;
    let (start, end) = subtree_range(&lines, line)?;
    if end >= lines.len() || headline_level(lines[end]) != Some(level) {
        return None; // no sibling of the same level follows
    }
    let (_, sib_end) = subtree_range(&lines, end)?;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..start]);
    out.extend_from_slice(&lines[end..sib_end]); // sibling first
    out.extend_from_slice(&lines[start..end]); // then this subtree
    out.extend_from_slice(&lines[sib_end..]);
    let new_start = start + (sib_end - end);
    Some((out.join("\n"), new_start))
}

/// Move the subtree at `line` up past its previous sibling, returning the new
/// text and the subtree's new starting line. `None` if there is no prior sibling.
#[must_use]
pub fn move_subtree_up(text: &str, line: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let level = headline_level(lines.get(line)?)?;
    let (start, end) = subtree_range(&lines, line)?;
    // Find the previous sibling's start: scan back to a headline of the same
    // level, bailing if a higher-level (parent) headline appears first.
    let mut prev = None;
    for i in (0..start).rev() {
        if let Some(l) = headline_level(lines[i]) {
            if l < level {
                break;
            }
            if l == level {
                prev = Some(i);
                break;
            }
        }
    }
    let prev = prev?;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..prev]);
    out.extend_from_slice(&lines[start..end]); // this subtree first
    out.extend_from_slice(&lines[prev..start]); // then the previous sibling
    out.extend_from_slice(&lines[end..]);
    Some((out.join("\n"), prev))
}

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

/// Compile an **agenda** from `(filename, content)` Org documents: `DEADLINE:` and
/// `SCHEDULED:` planning lines grouped by date, plus `TODO` headlines that have no
/// date. Returns an Org document (open it in a buffer). Pure and testable.
#[must_use]
pub fn agenda(files: &[(String, String)]) -> String {
    let mut dated: Vec<(String, String, String, String)> = Vec::new(); // date, kind, headline, file
    let mut undated: Vec<(String, String)> = Vec::new(); // headline, file
    for (name, content) in files {
        let mut current = String::new();
        for line in content.lines() {
            if let Some(level) = headline_level(line) {
                current = line[level..].trim().to_string();
                if current.split_whitespace().next() == Some("TODO") {
                    undated.push((current.clone(), name.clone()));
                }
                continue;
            }
            let trimmed = line.trim();
            for kind in ["DEADLINE", "SCHEDULED"] {
                if let Some(rest) = trimmed.strip_prefix(&format!("{kind}:"))
                    && let Some(date) = first_date(rest)
                {
                    dated.push((date, kind.to_string(), current.clone(), name.clone()));
                }
            }
        }
    }
    dated.sort();
    let mut out = String::from("#+title: Agenda\n");
    let mut last = String::new();
    for (date, kind, headline, file) in &dated {
        if *date != last {
            let _ = writeln!(out, "\n* {date}");
            last.clone_from(date);
        }
        let _ = writeln!(out, "- {kind}: {headline} ({file})");
    }
    if !undated.is_empty() {
        out.push_str("\n* Unscheduled tasks\n");
        for (headline, file) in &undated {
            let _ = writeln!(out, "- {headline} ({file})");
        }
    }
    out
}

/// Minutes in a `CLOCK:` line's explicit `=> H:MM` total, if present.
fn clock_minutes(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix("CLOCK:")?;
    let after = &rest[rest.find("=>")? + 2..];
    let (h, m) = after.trim().split_once(':')?;
    Some(h.trim().parse::<u32>().ok()? * 60 + m.trim().parse::<u32>().ok()?)
}

/// Render `minutes` as `H:MM`.
fn hhmm(minutes: u32) -> String {
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
fn clock_start(line: &str) -> Option<String> {
    let inner = line
        .trim()
        .strip_prefix("CLOCK:")?
        .trim()
        .strip_prefix('[')?;
    Some(inner[..inner.find(']')?].to_string())
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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

// ----- Export ---------------------------------------------------------------

static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\[([^\]]+)\]\]").expect("link regex"));
static BARE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").expect("bare link regex"));

/// Apply an inline-emphasis substitution for a single marker char, mapping
/// `<m>text<m>` to `open`…`close`. Word-ish: the marker hugs the text.
fn emph(input: &str, marker: char, open: &str, close: &str) -> String {
    let m = regex::escape(&marker.to_string());
    let re = Regex::new(&format!(r"{m}([^{m}\s][^{m}]*?){m}")).expect("emph regex");
    re.replace_all(input, format!("{open}$1{close}"))
        .into_owned()
}

/// Convert Org inline markup (links and emphasis) to Markdown.
fn inline_md(s: &str) -> String {
    let s = LINK.replace_all(s, "[$2]($1)").into_owned();
    let s = BARE_LINK.replace_all(&s, "<$1>").into_owned();
    let s = emph(&s, '*', "**", "**");
    let s = emph(&s, '/', "*", "*");
    let s = emph(&s, '~', "`", "`");
    let s = emph(&s, '=', "`", "`");
    emph(&s, '+', "~~", "~~")
}

/// Convert Org text to Markdown (a pragmatic, line-oriented mapping).
#[must_use]
pub fn to_markdown(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end();
        if let Some(rest) = line
            .strip_prefix("#+title:")
            .or_else(|| line.strip_prefix("#+TITLE:"))
        {
            out.push(format!("# {}", rest.trim()));
        } else if let Some(rest) = line
            .strip_prefix("#+author:")
            .or_else(|| line.strip_prefix("#+AUTHOR:"))
        {
            out.push(format!("*{}*", rest.trim()));
        } else if line.starts_with("#+BEGIN_")
            || line.starts_with("#+END_")
            || line.starts_with("#+begin_")
            || line.starts_with("#+end_")
        {
            // Drop block delimiters; their inner lines pass through as-is.
        } else if let Some(level) = headline_level(line) {
            let rest = line[level..].trim_start();
            out.push(format!("{} {}", "#".repeat(level), inline_md(rest)));
        } else {
            out.push(inline_md(line));
        }
    }
    out.join("\n")
}

/// HTML-escape the five significant characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Return `url` if it uses a safe scheme for an exported hyperlink, otherwise
/// `"#"`. The exported HTML is a standalone document a user opens in a browser,
/// so an active `javascript:`/`data:`/`vbscript:` href would run attacker script
/// (stored XSS from a crafted `.org` file). Allow only http(s), mailto, file,
/// fragment/relative, and scheme-less relative links; neutralize everything else.
fn safe_href(url: &str) -> String {
    let lower = url.trim_start().to_ascii_lowercase();
    let ok = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("file:")
        || lower.starts_with('#')
        || lower.starts_with('/')
        || !lower.contains(':'); // scheme-less relative path
    if ok { url.to_string() } else { "#".to_string() }
}

/// Convert Org inline markup to HTML (escaping text first).
fn inline_html(s: &str) -> String {
    use regex::Captures;
    let s = escape_html(s);
    // Links: the regex ran on escaped text, so brackets are intact. The href is
    // scheme-checked so a `javascript:`/`data:` URL can't produce an active link.
    let s = LINK
        .replace_all(&s, |c: &Captures| {
            format!("<a href=\"{}\">{}</a>", safe_href(&c[1]), &c[2])
        })
        .into_owned();
    let s = BARE_LINK
        .replace_all(&s, |c: &Captures| {
            format!("<a href=\"{}\">{}</a>", safe_href(&c[1]), &c[1])
        })
        .into_owned();
    let s = emph(&s, '*', "<b>", "</b>");
    let s = emph(&s, '/', "<i>", "</i>");
    let s = emph(&s, '_', "<u>", "</u>");
    let s = emph(&s, '~', "<code>", "</code>");
    let s = emph(&s, '=', "<code>", "</code>");
    emph(&s, '+', "<del>", "</del>")
}

/// Convert Org text to a small standalone HTML document (a pragmatic subset:
/// headlines, paragraphs, and bullet lists).
#[must_use]
pub fn to_html(text: &str) -> String {
    let mut body: Vec<String> = Vec::new();
    let mut in_list = false;
    let mut title = "Org";
    let close_list = |body: &mut Vec<String>, in_list: &mut bool| {
        if *in_list {
            body.push("</ul>".to_string());
            *in_list = false;
        }
    };
    for raw in text.split('\n') {
        let line = raw.trim_end();
        if let Some(rest) = line
            .strip_prefix("#+title:")
            .or_else(|| line.strip_prefix("#+TITLE:"))
        {
            title = rest.trim();
            close_list(&mut body, &mut in_list);
            body.push(format!("<h1>{}</h1>", inline_html(rest.trim())));
        } else if line.starts_with("#+") {
            close_list(&mut body, &mut in_list); // ignore other keywords/blocks
        } else if let Some(level) = headline_level(line) {
            close_list(&mut body, &mut in_list);
            let tag = level.min(6);
            body.push(format!(
                "<h{tag}>{}</h{tag}>",
                inline_html(line[level..].trim_start())
            ));
        } else if let Some(item) = line
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| line.trim_start().strip_prefix("+ "))
        {
            if !in_list {
                body.push("<ul>".to_string());
                in_list = true;
            }
            body.push(format!("<li>{}</li>", inline_html(item)));
        } else if line.trim().is_empty() {
            close_list(&mut body, &mut in_list);
        } else {
            close_list(&mut body, &mut in_list);
            body.push(format!("<p>{}</p>", inline_html(line)));
        }
    }
    close_list(&mut body, &mut in_list);
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n{}\n</body>\n</html>\n",
        escape_html(title),
        body.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_export_neutralizes_dangerous_link_schemes() {
        let danger = [
            "[[javascript:alert(1)][x]]",
            "[[JavaScript:alert(document.cookie)][x]]",
            "[[  javascript:alert(1)][x]]",
            "[[data:text/html,<script>1</script>][x]]",
            "[[vbscript:msgbox][x]]",
            "[[javascript:alert(1)]]", // bare link form
        ];
        for org in danger {
            let html = to_html(org);
            let lower = html.to_ascii_lowercase();
            assert!(!lower.contains("href=\"javascript"), "leaked scheme: {html}");
            assert!(!lower.contains("href=\"data:"), "leaked data: {html}");
            assert!(!lower.contains("href=\"vbscript"), "leaked vbscript: {html}");
        }
        // Safe links still render with their href intact (mailto:/fragment
        // links carry no `/`, so they're unaffected by the emphasis pass).
        assert!(to_html("[[mailto:a@b.test][mail]]").contains("href=\"mailto:a@b.test\""));
        assert!(to_html("[[#section][jump]]").contains("href=\"#section\""));
        // An http(s) scheme is recognized as safe by the guard itself.
        assert_eq!(safe_href("https://x.test"), "https://x.test");
        assert_eq!(safe_href("javascript:alert(1)"), "#");
    }

    proptest::proptest! {
        // For ANY org input, the exported HTML never contains an active
        // `javascript:`/`data:`/`vbscript:` href, and never panics.
        #[test]
        fn to_html_never_emits_active_script_hrefs(s in ".*") {
            let html = to_html(&s).to_ascii_lowercase();
            proptest::prop_assert!(!html.contains("href=\"javascript"), "{html}");
            proptest::prop_assert!(!html.contains("href=\"data:"), "{html}");
            proptest::prop_assert!(!html.contains("href=\"vbscript"), "{html}");
        }

        // The scheme guard maps every dangerous scheme to `#` and never panics.
        #[test]
        fn safe_href_neutralizes_non_allowlisted_schemes(scheme in "[a-zA-Z]{2,12}", rest in ".*") {
            let url = format!("{scheme}:{rest}");
            let out = safe_href(&url);
            let lower = scheme.to_ascii_lowercase();
            let allowed = matches!(lower.as_str(), "http" | "https" | "mailto" | "file");
            if !allowed {
                proptest::prop_assert_eq!(out, "#".to_string(), "unallowed scheme leaked: {}", url);
            }
        }
    }

    #[test]
    fn detects_headline_levels() {
        assert_eq!(headline_level("* A"), Some(1));
        assert_eq!(headline_level("*** C"), Some(3));
        assert_eq!(headline_level("*bold*"), None);
        assert_eq!(headline_level("not a headline"), None);
    }

    #[test]
    fn promote_and_demote_the_whole_subtree() {
        let text = "* A\n** B\nbody\n* C";
        let demoted = demote(text, 0).unwrap();
        assert_eq!(demoted, "** A\n*** B\nbody\n* C");
        // Promote refuses when a level-1 headline is in the subtree.
        assert_eq!(promote(text, 0), None);
        // But a level-2 subtree promotes fine.
        assert_eq!(
            promote("* A\n** B\nbody\n* C", 1).unwrap(),
            "* A\n* B\nbody\n* C"
        );
    }

    #[test]
    fn cycles_todo_keyword() {
        let t = "* Task";
        let t = cycle_todo(t, 0).unwrap();
        assert_eq!(t, "* TODO Task");
        let t = cycle_todo(&t, 0).unwrap();
        assert_eq!(t, "* DONE Task");
        let t = cycle_todo(&t, 0).unwrap();
        assert_eq!(t, "* Task");
    }

    #[test]
    fn toggles_checkboxes() {
        assert_eq!(toggle_checkbox("- [ ] a", 0).unwrap(), "- [x] a");
        assert_eq!(toggle_checkbox("- [x] a", 0).unwrap(), "- [ ] a");
        assert_eq!(toggle_checkbox("- [-] a", 0).unwrap(), "- [ ] a");
        assert_eq!(toggle_checkbox("plain", 0), None);
    }

    #[test]
    fn propagates_parent_checkbox_state() {
        // None checked → parent empty.
        let none = "- [ ] call people\n  - [ ] Peter\n  - [ ] Sarah";
        assert_eq!(
            update_statistics(none),
            "- [ ] call people\n  - [ ] Peter\n  - [ ] Sarah"
        );
        // Some checked → parent partial.
        let some = "- [ ] call people\n  - [X] Peter\n  - [ ] Sarah";
        assert_eq!(
            update_statistics(some),
            "- [-] call people\n  - [X] Peter\n  - [ ] Sarah"
        );
        // All checked → parent checked.
        let all = "- [ ] call people\n  - [X] Peter\n  - [X] Sarah";
        assert_eq!(
            update_statistics(all),
            "- [X] call people\n  - [X] Peter\n  - [X] Sarah"
        );
    }

    #[test]
    fn updates_list_item_fraction_cookie() {
        let t = "- [ ] tasks [/]\n  - [X] a\n  - [ ] b\n  - [X] c";
        let out = update_statistics(t);
        assert!(out.starts_with("- [-] tasks [2/3]"), "{out}");
    }

    #[test]
    fn updates_headline_cookies_for_todo_children() {
        // The manual's example: percent on the parent, fraction on the child.
        let t = "* Organize Party [%]\n** TODO Call people [/]\n*** TODO Peter\n*** DONE Sarah\n** TODO Buy food\n** DONE Talk to neighbor";
        let out = update_statistics(t);
        assert!(out.contains("* Organize Party [33%]"), "{out}");
        assert!(out.contains("** TODO Call people [1/2]"), "{out}");
    }

    #[test]
    fn cookie_data_todo_recursive_counts_whole_subtree() {
        let t = "* Parent [/]\n:PROPERTIES:\n:COOKIE_DATA: todo recursive\n:END:\n** TODO a\n*** DONE b\n** DONE c";
        let out = update_statistics(t);
        // Three TODO entries in the subtree (a, b, c); two are DONE.
        assert!(out.contains("* Parent [2/3]"), "{out}");
    }

    #[test]
    fn moves_subtrees_among_siblings() {
        let text = "* A\nbody a\n* B\nbody b";
        let (down, line) = move_subtree_down(text, 0).unwrap();
        assert_eq!(down, "* B\nbody b\n* A\nbody a");
        assert_eq!(line, 2);
        let (up, line) = move_subtree_up(&down, 2).unwrap();
        assert_eq!(up, text);
        assert_eq!(line, 0);
        // No sibling below the last subtree.
        assert!(move_subtree_down(text, 2).is_none());
    }

    #[test]
    fn exports_markdown() {
        let org = "#+title: Hi\n* Head\n/italic/ and *bold* and [[u][d]]";
        let md = to_markdown(org);
        assert!(md.contains("# Hi"));
        assert!(md.contains("# Head"));
        assert!(md.contains("*italic*"));
        assert!(md.contains("**bold**"));
        assert!(md.contains("[d](u)"));
    }

    #[test]
    fn agenda_groups_by_date_and_lists_undated_todos() {
        let files = vec![
            (
                "work.org".to_string(),
                "* TODO Ship it\nDEADLINE: <2024-08-23 Fri>\n* TODO Loose end\n".to_string(),
            ),
            (
                "home.org".to_string(),
                "* Meeting\nSCHEDULED: <2024-08-20 Tue>\n".to_string(),
            ),
        ];
        let a = agenda(&files);
        assert!(a.contains("* 2024-08-20"));
        assert!(a.contains("- SCHEDULED: Meeting (home.org)"));
        assert!(a.contains("* 2024-08-23"));
        assert!(a.contains("- DEADLINE: TODO Ship it (work.org)"));
        assert!(a.contains("* Unscheduled tasks"));
        assert!(a.contains("- TODO Loose end (work.org)"));
        // Dates are sorted ascending: 08-20 before 08-23.
        assert!(a.find("2024-08-20").unwrap() < a.find("2024-08-23").unwrap());
    }

    #[test]
    fn clock_in_and_out_record_a_duration() {
        let now_in = "2024-08-23 Fri 10:00";
        assert_eq!(clock_in(now_in), "CLOCK: [2024-08-23 Fri 10:00]");
        let text = format!("* Task\n  {}\n", clock_in(now_in));
        let out = clock_out(&text, "2024-08-23 Fri 11:30").unwrap();
        assert!(out.contains("CLOCK: [2024-08-23 Fri 10:00]--[2024-08-23 Fri 11:30] =>  1:30"));
        // Indentation of the original clock line is preserved.
        assert!(out.contains("\n  CLOCK:"));
        // No open clock → None.
        assert!(clock_out(&out, "2024-08-23 Fri 12:00").is_none());
    }

    #[test]
    fn clock_out_spans_midnight() {
        let text = "CLOCK: [2024-08-23 Fri 23:30]";
        let out = clock_out(text, "2024-08-24 Sat 00:15").unwrap();
        assert!(out.ends_with("=>  0:45"), "{out}");
    }

    #[test]
    fn time_report_sums_clock_durations_per_headline() {
        let org = "* Task A\nCLOCK: [..]--[..] =>  1:30\nCLOCK: [..]--[..] =>  0:45\n* Task B\nCLOCK: [..]--[..] => 2:00\n";
        let r = time_report(org);
        assert!(r.contains("| Task A | 2:15 |"));
        assert!(r.contains("| Task B | 2:00 |"));
        assert!(r.contains("| *Total* | 4:15 |"));
    }

    #[test]
    fn exports_html() {
        let org = "#+title: Hi\n* Head\n- one\n- two\npara";
        let html = to_html(org);
        assert!(html.contains("<title>Hi</title>"));
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<h1>Head</h1>") || html.contains("<h1>Head</h1>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<p>para</p>"));
    }
}
