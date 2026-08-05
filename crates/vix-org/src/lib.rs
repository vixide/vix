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

/// The name of a `:NAME:` line — a colon, a run of non-colon, non-whitespace
/// characters, a closing colon, and nothing else after trimming — or `None`.
/// Matches a drawer header (`:PROPERTIES:`, `:LOGBOOK:`) or the `:END:`
/// terminator, but not a property line like `:foo: 123` (which does not end
/// with a colon).
fn drawer_name(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix(':')?.strip_suffix(':')?;
    if inner.is_empty() || inner.contains([':', ' ', '\t']) {
        None
    } else {
        Some(inner)
    }
}

/// Whether `line` opens an Org drawer: a `:NAME:` header line that is not the
/// `:END:` terminator (nor a property line, which carries a value after the
/// second colon). `:PROPERTIES:` and `:LOGBOOK:` are headers; `:END:` and
/// `:foo: 123` are not.
#[must_use]
pub fn is_drawer_header(line: &str) -> bool {
    drawer_name(line).is_some_and(|n| !n.eq_ignore_ascii_case("END"))
}

/// The inclusive `[header, end]` line range of the drawer opened at `line` —
/// the `:NAME:` header through its matching `:END:` line. `None` if `line` is
/// not a drawer header, or no `:END:` closes it before the next headline or the
/// end of the buffer. The header line stays visible when folded; the body lines
/// (`header+1 ..= end`) are what a drawer fold hides.
#[must_use]
pub fn drawer_range(lines: &[&str], line: usize) -> Option<(usize, usize)> {
    if !is_drawer_header(lines.get(line)?) {
        return None;
    }
    let mut end = line + 1;
    while end < lines.len() {
        if headline_level(lines[end]).is_some() {
            return None; // a headline closes the section before any :END:
        }
        if lines[end].trim().eq_ignore_ascii_case(":END:") {
            return Some((line, end));
        }
        end += 1;
    }
    None
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

/// Rewrite `headline` so its TODO keyword is `keyword` (e.g. `DONE`), replacing
/// any existing `TODO`/`DONE` keyword or inserting one before the title text.
/// Returns the line unchanged if it is not a headline.
fn set_headline_keyword(headline: &str, keyword: &str) -> String {
    let Some(stars) = headline_level(headline) else {
        return headline.to_string();
    };
    let (prefix, rest) = headline.split_at(stars + 1); // include the space
    let body = rest
        .strip_prefix(&format!("{TODO} "))
        .or_else(|| rest.strip_prefix(&format!("{DONE} ")))
        .unwrap_or(if rest == TODO || rest == DONE {
            ""
        } else {
            rest
        });
    if body.is_empty() {
        format!("{prefix}{keyword}")
    } else {
        format!("{prefix}{keyword} {body}")
    }
}

// ----- Priority ---------------------------------------------------------

/// Split a headline's post-stars text `rest` into its TODO/DONE keyword
/// (including the trailing space, or `""` if there is none) and the
/// remaining body.
fn split_keyword(rest: &str) -> (&str, &str) {
    if let Some(body) = rest.strip_prefix(&format!("{TODO} ")) {
        (&rest[..=TODO.len()], body)
    } else if rest == TODO {
        (rest, "")
    } else if let Some(body) = rest.strip_prefix(&format!("{DONE} ")) {
        (&rest[..=DONE.len()], body)
    } else if rest == DONE {
        (rest, "")
    } else {
        ("", rest)
    }
}

/// Strip a leading priority cookie `[#X]` (and the single space after it, if
/// any) from `body`, returning the cookie's character and the remaining text.
fn strip_priority(body: &str) -> Option<(char, &str)> {
    let mut chars = body.strip_prefix("[#")?.chars();
    let p = chars.next()?;
    let after = chars.as_str().strip_prefix(']')?;
    Some((p, after.strip_prefix(' ').unwrap_or(after)))
}

/// The priority cookie (`[#X]`) on `headline`, read right after its TODO/DONE
/// keyword (or right after the stars, if there is no keyword). `None` if
/// `headline` isn't a headline or carries no cookie.
#[must_use]
pub fn priority(headline: &str) -> Option<char> {
    let stars = headline_level(headline)?;
    let (_, rest) = headline.split_at(stars + 1);
    let (_, body) = split_keyword(rest);
    strip_priority(body).map(|(p, _)| p)
}

/// Set, replace, or remove the priority cookie on the headline at `line`.
/// `priority = None` removes an existing cookie (a no-op if there wasn't
/// one). Returns `None` if `line` is not a headline.
#[must_use]
pub fn set_priority(text: &str, line: usize, priority: Option<char>) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let headline = lines.get(line)?;
    let stars = headline_level(headline)?;
    let (prefix, rest) = headline.split_at(stars + 1);
    let (keyword, body) = split_keyword(rest);
    let bare = strip_priority(body).map_or(body, |(_, after)| after);
    let new_body = match priority {
        Some(p) if bare.is_empty() => format!("[#{p}]"),
        Some(p) => format!("[#{p}] {bare}"),
        None => bare.to_string(),
    };
    lines[line] = format!("{prefix}{keyword}{new_body}");
    Some(lines.join("\n"))
}

/// The valid priority characters from `highest` to `lowest` inclusive, in
/// that order (so index 0 is always `highest`).
fn priority_range(highest: char, lowest: char) -> Vec<char> {
    let (lo, hi) = if highest <= lowest {
        (highest as u32, lowest as u32)
    } else {
        (lowest as u32, highest as u32)
    };
    let mut chars: Vec<char> = (lo..=hi).filter_map(char::from_u32).collect();
    if highest > lowest {
        chars.reverse();
    }
    chars
}

/// Move the headline at `line`'s priority one step toward `highest` if
/// `up`, else toward `lowest`; clamped at that bound (no wraparound). Sets
/// `default` if there is no cookie yet. `None` if `line` is not a headline.
fn step_priority(
    text: &str,
    line: usize,
    highest: char,
    lowest: char,
    default: char,
    up: bool,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let headline = *lines.get(line)?;
    headline_level(headline)?;
    let order = priority_range(highest, lowest);
    let next = match priority(headline).and_then(|c| order.iter().position(|&x| x == c)) {
        None => default,
        Some(idx) => {
            let new_idx = if up {
                idx.saturating_sub(1)
            } else {
                (idx + 1).min(order.len().saturating_sub(1))
            };
            order.get(new_idx).copied().unwrap_or(default)
        }
    };
    set_priority(text, line, Some(next))
}

/// Move the headline at `line`'s priority one step toward `highest` (Shift+Up
/// equivalent). Sets `default` if there is no cookie yet; clamped at
/// `highest` (no wraparound). `None` if `line` is not a headline.
#[must_use]
pub fn priority_up(
    text: &str,
    line: usize,
    highest: char,
    lowest: char,
    default: char,
) -> Option<String> {
    step_priority(text, line, highest, lowest, default, true)
}

/// Move the headline at `line`'s priority one step toward `lowest`
/// (Shift+Down equivalent). Sets `default` if there is no cookie yet;
/// clamped at `lowest` (no wraparound). `None` if `line` is not a headline.
#[must_use]
pub fn priority_down(
    text: &str,
    line: usize,
    highest: char,
    lowest: char,
    default: char,
) -> Option<String> {
    step_priority(text, line, highest, lowest, default, false)
}

/// Mark the headline at `line` `DONE` and record its completion the way Emacs
/// Org's `org-todo` with logging does (`C-u C-c C-t`):
///
/// * force the keyword to `DONE`,
/// * insert (or refresh) a `CLOSED: [now]` planning line just under the
///   headline, and
/// * when `note` is non-empty, log it into the headline's `:LOGBOOK:` drawer as
///   `- Note taken on [now] \\` followed by the note body indented two spaces
///   (creating the drawer if the headline has none, else prepending as the
///   newest entry).
///
/// Returns the rewritten buffer, or `None` if `line` is not a headline.
#[must_use]
pub fn close_headline(text: &str, line: usize, now: &str, note: &str) -> Option<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let level = headline_level(lines.get(line)?)?;
    lines[line] = set_headline_keyword(&lines[line], DONE);

    // The CLOSED planning line sits immediately under the headline; refresh an
    // existing one rather than stacking duplicates.
    let closed = format!("CLOSED: [{now}]");
    let closed_idx = line + 1;
    if lines
        .get(closed_idx)
        .is_some_and(|l| l.trim_start().starts_with("CLOSED:"))
    {
        lines[closed_idx] = closed;
    } else {
        lines.insert(closed_idx, closed);
    }

    if !note.is_empty() {
        let mut entry: Vec<String> = vec![format!("- Note taken on [{now}] \\\\")];
        entry.extend(note.split('\n').map(|l| format!("  {l}")));
        // The subtree body runs until the next headline of the same/higher level.
        let mut end = closed_idx + 1;
        while end < lines.len() && headline_level(&lines[end]).is_none_or(|l| l > level) {
            end += 1;
        }
        if let Some(lb) =
            (closed_idx + 1..end).find(|&i| lines[i].trim().eq_ignore_ascii_case(":LOGBOOK:"))
        {
            for (k, e) in entry.into_iter().enumerate() {
                lines.insert(lb + 1 + k, e);
            }
        } else {
            let mut drawer = vec![":LOGBOOK:".to_string()];
            drawer.extend(entry);
            drawer.push(":END:".to_string());
            for (k, d) in drawer.into_iter().enumerate() {
                lines.insert(closed_idx + 1 + k, d);
            }
        }
    }
    Some(lines.join("\n"))
}

static CHECKBOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*(?:[-+*]|\d+[.)])\s+)\[([ xX-])\]").expect("checkbox regex")
});

/// Whether `line` carries a list checkbox (`- [ ]`, `1. [X]`, …) — the lines on
/// which Org's `C-c C-c` toggles the box.
#[must_use]
pub fn has_checkbox(line: &str) -> bool {
    CHECKBOX.is_match(line)
}

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

// ----- Headline location & structure editing --------------------------------

/// The line of the headline governing `line`: the nearest headline at or above
/// it. `None` when the cursor sits before the first headline.
fn governing(lines: &[&str], line: usize) -> Option<usize> {
    let last = lines.len().checked_sub(1)?;
    (0..=line.min(last))
        .rev()
        .find(|&i| headline_level(lines[i]).is_some())
}

/// Rewrite a headline's stars to shift its level by `delta` (clamped to ≥ 1).
/// Non-headline lines pass through unchanged.
fn relevel(line: &str, delta: i64) -> String {
    match headline_level(line) {
        Some(level) => {
            let new = usize::try_from((i64::try_from(level).unwrap_or(i64::MAX) + delta).max(1))
                .unwrap_or(1);
            format!("{} {}", "*".repeat(new), line[level..].trim_start())
        }
        None => line.to_string(),
    }
}

/// Insert a sibling headline after the cursor line (Org `M-RET`): same level as
/// the governing headline, or level 1 outside any subtree. Returns the new text
/// and the line of the inserted headline (its stars and trailing space, ready to
/// type the title).
#[must_use]
pub fn new_heading(text: &str, line: usize) -> (String, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let level = governing(&lines, line)
        .and_then(|h| headline_level(lines[h]))
        .unwrap_or(1);
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    let at = (line + 1).min(out.len());
    out.insert(at, format!("{} ", "*".repeat(level)));
    (out.join("\n"), at)
}

/// Org `C-c C-u`: the governing headline when the cursor is in a body, else the
/// parent headline (nearest above with a smaller level). `None` at top level.
#[must_use]
pub fn nav_parent(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    if h != line {
        return Some(h);
    }
    let level = headline_level(lines[h])?;
    (0..h)
        .rev()
        .find(|&i| headline_level(lines[i]).is_some_and(|l| l < level))
}

/// The next headline after `line` (any level), like Org `C-c C-n`.
#[must_use]
pub fn nav_next(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    (line + 1..lines.len()).find(|&i| headline_level(lines[i]).is_some())
}

/// The previous headline before `line` (any level), like Org `C-c C-p`.
#[must_use]
pub fn nav_prev(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    (0..line.min(lines.len()))
        .rev()
        .find(|&i| headline_level(lines[i]).is_some())
}

/// The next sibling headline (same level, within the same parent), like Org
/// `C-c C-f`. Stops at the parent boundary.
#[must_use]
pub fn nav_forward_same(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let level = headline_level(lines[h])?;
    for (i, l) in lines.iter().enumerate().skip(h + 1) {
        match headline_level(l) {
            Some(l) if l < level => return None,
            Some(l) if l == level => return Some(i),
            _ => {}
        }
    }
    None
}

/// The previous sibling headline (same level, within the same parent), like Org
/// `C-c C-b`. Stops at the parent boundary.
#[must_use]
pub fn nav_backward_same(text: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let h = governing(&lines, line)?;
    let level = headline_level(lines[h])?;
    for (i, l) in lines.iter().enumerate().take(h).rev() {
        match headline_level(l) {
            Some(l) if l < level => return None,
            Some(l) if l == level => return Some(i),
            _ => {}
        }
    }
    None
}

/// Every headline in `text` as `(line, level, title)` (title = text after the
/// stars, trimmed). Used by refile target matching and link following.
#[must_use]
pub fn headlines(text: &str) -> Vec<(usize, usize, String)> {
    text.split('\n')
        .enumerate()
        .filter_map(|(i, l)| headline_level(l).map(|lv| (i, lv, l[lv..].trim().to_string())))
        .collect()
}

/// The `[start, end)` line range of the subtree governing `line` (walking up to
/// the nearest headline first). `None` before the first headline.
#[must_use]
pub fn governing_subtree(text: &str, line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    subtree_range(&lines, governing(&lines, line)?)
}

/// Sort the children of the subtree governing `line` alphabetically by headline
/// text (case-insensitive): direct children when inside a subtree, the top-level
/// trees when before the first headline. `None` with fewer than two children.
#[must_use]
pub fn sort_children(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (start, end, child_level) = match governing(&lines, line) {
        Some(h) => {
            let (s, e) = subtree_range(&lines, h)?;
            (s + 1, e, headline_level(lines[h])? + 1)
        }
        None => (0, lines.len(), 1),
    };
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut i = start;
    while i < end {
        if headline_level(lines[i]) == Some(child_level) {
            let (s, e) = subtree_range(&lines, i)?;
            let e = e.min(end);
            blocks.push((s, e));
            i = e;
        } else {
            i += 1;
        }
    }
    if blocks.len() < 2 {
        return None;
    }
    let first = blocks[0].0;
    let mut sorted = blocks.clone();
    sorted.sort_by_key(|&(s, _)| {
        let l = lines[s];
        let lv = headline_level(l).unwrap_or(0);
        l[lv..].trim().to_ascii_lowercase()
    });
    let mut out: Vec<&str> = lines[..first].to_vec();
    for &(s, e) in &sorted {
        out.extend_from_slice(&lines[s..e]);
    }
    out.extend_from_slice(&lines[end..]);
    Some(out.join("\n"))
}

/// Refile the subtree governing `line` to the end of the subtree at
/// `target_line` (releveled to one deeper than the target), like Org `C-c C-w`.
/// Returns the new text and the moved headline's line. `None` when either side
/// is not a headline or the target lies inside the source subtree.
#[must_use]
pub fn refile(text: &str, line: usize, target_line: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let src = governing(&lines, line)?;
    let (ss, se) = subtree_range(&lines, src)?;
    if (ss..se).contains(&target_line) {
        return None;
    }
    let target_level = headline_level(lines.get(target_line)?)?;
    let (_, te) = subtree_range(&lines, target_line)?;
    let src_level = headline_level(lines[ss])?;
    let delta = i64::try_from(target_level + 1).ok()? - i64::try_from(src_level).ok()?;
    let block: Vec<String> = lines[ss..se].iter().map(|l| relevel(l, delta)).collect();
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !(ss..se).contains(i))
        .map(|(_, s)| (*s).to_string())
        .collect();
    let insert_at = if te > ss { te - (se - ss) } else { te };
    for (k, l) in block.into_iter().enumerate() {
        out.insert(insert_at + k, l);
    }
    Some((out.join("\n"), insert_at))
}

/// Paste a cut/copied subtree after the subtree governing `line`, releveled to
/// match it as a sibling (level 1 outside any subtree), like Org `C-c C-x C-y`.
/// Returns the new text and the pasted headline's line. `None` when `clip` does
/// not start with a headline.
#[must_use]
pub fn paste_subtree(text: &str, line: usize, clip: &str) -> Option<(String, usize)> {
    let clip = clip.trim_end_matches('\n');
    let clip_lines: Vec<&str> = clip.split('\n').collect();
    let clip_level = headline_level(clip_lines.first()?)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let (at, target_level) = match governing(&lines, line) {
        Some(h) => {
            let (_, e) = subtree_range(&lines, h)?;
            (e, headline_level(lines[h])?)
        }
        None => ((line + 1).min(lines.len()), 1),
    };
    let delta = i64::try_from(target_level).ok()? - i64::try_from(clip_level).ok()?;
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    for (k, l) in clip_lines.iter().enumerate() {
        out.insert(at + k, relevel(l, delta));
    }
    Some((out.join("\n"), at))
}

// ----- Tags & properties -----------------------------------------------------

/// The trailing `:tag1:tag2:` group on a headline, with leading whitespace.
static TAGS: LazyLock<Regex> =
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
fn is_planning(line: &str) -> bool {
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
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (era * 400 + yoe + i64::from(m <= 2), m, d)
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
fn line_of_char(text: &str, cursor: usize) -> (usize, usize) {
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

// ----- Hyperlinks ------------------------------------------------------------

/// The `[[target][description]]` or `[[target]]` link under the cursor, as
/// `(target, Some(description))` / `(target, None)`. `None` off any link.
#[must_use]
pub fn link_at(text: &str, cursor: usize) -> Option<(String, Option<String>)> {
    let (line_idx, line_start) = line_of_char(text, cursor);
    let l = *text.split('\n').collect::<Vec<_>>().get(line_idx)?;
    let col = cursor - line_start.min(cursor);
    let contains = |l: &str, start: usize, end: usize| {
        let sc = l[..start].chars().count();
        let ec = sc + l[start..end].chars().count();
        (sc..=ec).contains(&col)
    };
    for c in LINK.captures_iter(l) {
        let m = c.get(0)?;
        if contains(l, m.start(), m.end()) {
            return Some((c[1].to_string(), Some(c[2].to_string())));
        }
    }
    for c in BARE_LINK.captures_iter(l) {
        let m = c.get(0)?;
        if contains(l, m.start(), m.end()) {
            return Some((c[1].to_string(), None));
        }
    }
    None
}

/// The char offset of the next (`forward`) or previous link start after/before
/// `cursor`, cycling not included. `None` when there is no link that way.
#[must_use]
pub fn link_pos(text: &str, cursor: usize, forward: bool) -> Option<usize> {
    let mut starts: Vec<usize> = LINK
        .find_iter(text)
        .chain(BARE_LINK.find_iter(text))
        .map(|m| text[..m.start()].chars().count())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    if forward {
        starts.into_iter().find(|&s| s > cursor)
    } else {
        starts.into_iter().rev().find(|&s| s < cursor)
    }
}

// ----- Sparse trees ----------------------------------------------------------

/// The fold ranges of a sparse tree: for every subtree containing no line
/// matching `pred`, the topmost such headline and the last line of its subtree
/// (inclusive — the headline stays visible as a fold marker, the body hides).
/// Subtrees with a match are descended into so non-matching children fold.
fn sparse_folds(text: &str, pred: impl Fn(&str) -> bool) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut folds = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if headline_level(lines[i]).is_some() {
            let Some((s, e)) = subtree_range(&lines, i) else {
                break;
            };
            if lines[s..e].iter().any(|l| pred(l)) {
                i += 1; // a match inside: descend, folding only its misses
            } else {
                if e - 1 > s {
                    folds.push((s, e - 1));
                }
                i = e;
            }
        } else {
            i += 1;
        }
    }
    folds
}

/// Whether a line is a headline whose keyword is `TODO`.
fn is_todo_headline(line: &str) -> bool {
    headline_level(line).is_some_and(|lv| line[lv..].split_whitespace().next() == Some("TODO"))
}

/// Sparse-tree folds showing only `TODO` headlines (Org `C-c / t`): every
/// subtree without a TODO folds down to its headline.
#[must_use]
pub fn todo_tree_folds(text: &str) -> Vec<(usize, usize)> {
    sparse_folds(text, is_todo_headline)
}

/// Sparse-tree folds showing only subtrees containing `query`
/// (case-insensitive; Org `C-c /`'s occur view).
#[must_use]
pub fn occur_folds(text: &str, query: &str) -> Vec<(usize, usize)> {
    let q = query.to_lowercase();
    sparse_folds(text, |l| l.to_lowercase().contains(&q))
}

// ----- Footnotes -------------------------------------------------------------

/// A `[fn:LABEL]` footnote token.
static FOOTNOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[fn:([A-Za-z0-9_-]+)\]").expect("footnote regex"));

/// The char offset of the start of 0-based `line` in `text`.
fn line_start_char(text: &str, line: usize) -> usize {
    text.split('\n')
        .take(line)
        .map(|l| l.chars().count() + 1)
        .sum()
}

/// Org's footnote action (`C-c C-x f`), all three behaviors in one:
/// on a `[fn:x]` **reference**, jump to its definition (created under a
/// `* Footnotes` headline when missing); on a **definition** line (starting
/// with `[fn:x]`), jump back to the first reference; anywhere else, insert a
/// new numbered reference at the cursor and append its empty definition.
/// Returns the new text (unchanged for pure jumps) and cursor.
#[must_use]
pub fn footnote(text: &str, cursor: usize) -> (String, usize) {
    let (line_idx, line_start) = line_of_char(text, cursor);
    let lines: Vec<&str> = text.split('\n').collect();
    let line = lines.get(line_idx).copied().unwrap_or_default();
    let col = cursor - line_start.min(cursor);
    let on_definition = FOOTNOTE.find(line).is_some_and(|m| m.start() == 0);
    let label_at_cursor = FOOTNOTE.captures_iter(line).find_map(|c| {
        let m = c.get(0)?;
        let sc = line[..m.start()].chars().count();
        let ec = sc + line[m.start()..m.end()].chars().count();
        (sc..=ec).contains(&col).then(|| c[1].to_string())
    });
    match label_at_cursor {
        // On a definition: jump to the first reference elsewhere.
        Some(label) if on_definition && col <= label.chars().count() + 5 => {
            let needle = format!("[fn:{label}]");
            for (i, l) in lines.iter().enumerate() {
                if let Some(pos) = l.find(&needle)
                    && (i != line_idx || pos != 0)
                {
                    let sc = l[..pos].chars().count();
                    return (text.to_string(), line_start_char(text, i) + sc);
                }
            }
            (text.to_string(), cursor)
        }
        // On a reference: jump to (or create) the definition.
        Some(label) => {
            let needle = format!("[fn:{label}]");
            if let Some(i) = lines
                .iter()
                .enumerate()
                .find_map(|(i, l)| (i != line_idx && l.starts_with(&needle)).then_some(i))
            {
                return (text.to_string(), line_start_char(text, i));
            }
            let new = append_footnote_definition(text, &label);
            let pos = new.chars().count();
            (new, pos)
        }
        // Elsewhere: create the next numbered footnote.
        None => {
            let next = FOOTNOTE
                .captures_iter(text)
                .filter_map(|c| c[1].parse::<u64>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            let mut new: String = text.chars().take(cursor).collect();
            let tail: String = text.chars().skip(cursor).collect();
            let _ = write!(new, "[fn:{next}]");
            new.push_str(&tail);
            let new = append_footnote_definition(&new, &next.to_string());
            let pos = new.chars().count();
            (new, pos)
        }
    }
}

/// Append an empty `[fn:label] ` definition at the end of `text`, under a
/// `* Footnotes` headline (created when missing, matching Org's default
/// `org-footnote-section`). The cursor belongs at the end of the result.
fn append_footnote_definition(text: &str, label: &str) -> String {
    let mut out = text.trim_end_matches('\n').to_string();
    let has_section = out
        .split('\n')
        .any(|l| headline_level(l).is_some_and(|lv| l[lv..].trim() == "Footnotes"));
    if !has_section {
        out.push_str("\n\n* Footnotes");
    }
    let _ = write!(out, "\n\n[fn:{label}] ");
    out
}

/// The governing headline line of the `:ID: value` property line matching
/// `id` (the property line itself before any headline). `None` when `text`
/// carries no such property.
#[must_use]
pub fn id_location(text: &str, id: &str) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let hit = lines.iter().position(|l| {
        let t = l.trim();
        t.len() >= 4 && t[..4].eq_ignore_ascii_case(":id:") && t[4..].trim() == id
    })?;
    Some(governing(&lines, hit).unwrap_or(hit))
}

// ----- Source blocks ---------------------------------------------------------

/// Whether `line` opens a source block, returning its language (may be empty).
fn src_begin(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = if t.len() >= 11 && t[..11].eq_ignore_ascii_case("#+begin_src") {
        &t[11..]
    } else {
        return None;
    };
    Some(
        rest.split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    )
}

/// The `#+begin_src` block containing `line` (begin/end line inclusive, or the
/// cursor on either fence), as `(begin, end, language)`. `None` outside any
/// block or in an unterminated one.
#[must_use]
pub fn src_block_at(text: &str, line: usize) -> Option<(usize, usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let begin = (0..=line.min(lines.len().saturating_sub(1)))
        .rev()
        .find(|&i| src_begin(lines[i]).is_some())?;
    let lang = src_begin(lines[begin])?;
    let end = (begin + 1..lines.len()).find(|&i| {
        lines[i]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("#+end_src")
    })?;
    (line <= end).then_some((begin, end, lang))
}

/// Replace the body of the source block opening at `begin_line` with `body`
/// (trailing newline optional). `None` when `begin_line` no longer opens a
/// terminated source block — e.g. the buffer changed while a dedicated-buffer
/// edit was open.
#[must_use]
pub fn replace_src_body(text: &str, begin_line: usize, body: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    src_begin(lines.get(begin_line)?)?;
    let (begin, end, _) = src_block_at(text, begin_line)?;
    if begin != begin_line {
        return None;
    }
    let mut out: Vec<&str> = lines[..=begin].to_vec();
    let body = body.trim_end_matches('\n');
    if !body.is_empty() {
        out.extend(body.split('\n'));
    }
    out.extend_from_slice(&lines[end..]);
    Some(out.join("\n"))
}

// ----- Column view -----------------------------------------------------------

/// Render the buffer's headlines as an Org column-view table (Org
/// `C-c C-x C-c`, read-only flavor): `ITEM` (indented by level), `TODO`,
/// `PRIORITY`, and `TAGS` columns.
#[must_use]
pub fn column_view(text: &str) -> String {
    let mut out = String::from("| ITEM | TODO | PRIORITY | TAGS |\n|---|---|---|---|\n");
    for line in text.split('\n') {
        let Some(level) = headline_level(line) else {
            continue;
        };
        let bare = TAGS.replace(line, "");
        let tags = TAGS
            .captures(line)
            .map_or_else(String::new, |c| c[1].to_string());
        let (keyword, body) = split_keyword(bare[level..].trim());
        let (prio, title) = match strip_priority(body) {
            Some((p, after)) => (format!("[#{p}]"), after),
            None => (String::new(), body),
        };
        let _ = writeln!(
            out,
            "| {}{} | {} | {} | {} |",
            "  ".repeat(level - 1),
            title.replace('|', "\\vert{}"),
            keyword.trim(),
            prio,
            tags,
        );
    }
    out
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

/// Escape the LaTeX special characters in plain text.
fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\textbackslash{}"),
            '{' => out.push_str(r"\{"),
            '}' => out.push_str(r"\}"),
            '$' => out.push_str(r"\$"),
            '&' => out.push_str(r"\&"),
            '#' => out.push_str(r"\#"),
            '%' => out.push_str(r"\%"),
            '_' => out.push_str(r"\_"),
            '^' => out.push_str(r"\^{}"),
            '~' => out.push_str(r"\~{}"),
            _ => out.push(c),
        }
    }
    out
}

/// Link sentinels used by [`inline_latex`]: links are captured before escaping
/// (the control chars survive the escape pass) and restored as `\href` after.
static SENTINEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\u{1}([^\u{2}]*)\u{2}([^\u{3}]*)\u{3}").expect("sentinel"));

/// Convert Org inline markup to LaTeX (escaping text first). Emphasis markers
/// are matched on the escaped text, so `_`/`~` (escaped above) drop out as
/// markers; bold/italic/strike and links are what remain meaningful.
fn inline_latex(s: &str) -> String {
    use regex::Captures;
    let s = LINK
        .replace_all(s, |c: &Captures| {
            format!("\u{1}{}\u{2}{}\u{3}", &c[1], &c[2])
        })
        .into_owned();
    let s = escape_latex(&s);
    let s = emph(&s, '*', r"\textbf{", "}");
    let s = emph(&s, '/', r"\textit{", "}");
    let s = emph(&s, '=', r"\texttt{", "}");
    let s = emph(&s, '+', r"\sout{", "}");
    SENTINEL
        .replace_all(&s, |c: &Captures| {
            format!(r"\href{{{}}}{{{}}}", c[1].replace(['{', '}'], ""), &c[2])
        })
        .into_owned()
}

/// Convert Org text to a small standalone LaTeX document (a pragmatic subset:
/// title/author, headlines to sections, bullet lists, verbatim blocks).
#[must_use]
pub fn to_latex(text: &str) -> String {
    let title = text.split('\n').find_map(|l| {
        l.strip_prefix("#+title:")
            .or_else(|| l.strip_prefix("#+TITLE:"))
            .map(str::trim)
    });
    let author = text.split('\n').find_map(|l| {
        l.strip_prefix("#+author:")
            .or_else(|| l.strip_prefix("#+AUTHOR:"))
            .map(str::trim)
    });
    let mut body: Vec<String> = Vec::new();
    let mut in_list = false;
    let mut in_verbatim = false;
    let close_list = |body: &mut Vec<String>, in_list: &mut bool| {
        if *in_list {
            body.push(r"\end{itemize}".to_string());
            *in_list = false;
        }
    };
    for raw in text.split('\n') {
        let line = raw.trim_end();
        let lower = line.trim_start().to_ascii_lowercase();
        if in_verbatim {
            if lower.starts_with("#+end_") {
                body.push(r"\end{verbatim}".to_string());
                in_verbatim = false;
            } else {
                body.push(raw.to_string()); // verbatim: no escaping
            }
        } else if lower.starts_with("#+begin_src") || lower.starts_with("#+begin_example") {
            close_list(&mut body, &mut in_list);
            body.push(r"\begin{verbatim}".to_string());
            in_verbatim = true;
        } else if line.starts_with("#+") {
            close_list(&mut body, &mut in_list); // other keywords/blocks dropped
        } else if let Some(level) = headline_level(line) {
            close_list(&mut body, &mut in_list);
            let cmd = match level {
                1 => r"\section",
                2 => r"\subsection",
                3 => r"\subsubsection",
                4 => r"\paragraph",
                _ => r"\subparagraph",
            };
            body.push(format!(
                "{cmd}{{{}}}",
                inline_latex(line[level..].trim_start())
            ));
        } else if let Some(item) = line
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| line.trim_start().strip_prefix("+ "))
        {
            if !in_list {
                body.push(r"\begin{itemize}".to_string());
                in_list = true;
            }
            body.push(format!(r"\item {}", inline_latex(item)));
        } else if line.trim().is_empty() {
            close_list(&mut body, &mut in_list);
            body.push(String::new());
        } else {
            body.push(inline_latex(line));
        }
    }
    if in_verbatim {
        body.push(r"\end{verbatim}".to_string());
    }
    close_list(&mut body, &mut in_list);
    let mut head = String::from(
        "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{hyperref}\n\\usepackage[normalem]{ulem}\n",
    );
    if let Some(t) = title {
        let _ = writeln!(head, "\\title{{{}}}", inline_latex(t));
    }
    if let Some(a) = author {
        let _ = writeln!(head, "\\author{{{}}}", inline_latex(a));
    }
    head.push_str("\\begin{document}\n");
    if title.is_some() {
        head.push_str("\\maketitle\n");
    }
    format!("{head}{}\n\\end{{document}}\n", body.join("\n"))
}

/// Escape an iCalendar text value (RFC 5545: backslash, semicolon, comma,
/// newline).
fn escape_ics(s: &str) -> String {
    s.replace('\\', r"\\")
        .replace(';', r"\;")
        .replace(',', r"\,")
        .replace('\n', r"\n")
}

/// Convert the `SCHEDULED:`/`DEADLINE:` entries of one Org document into an
/// iCalendar (RFC 5545) all-day-event calendar. `now` is the `DTSTAMP` value
/// (UTC `YYYYMMDDTHHMMSSZ`); `name` seeds the event UIDs.
#[must_use]
pub fn to_ics(text: &str, name: &str, now: &str) -> String {
    let items = agenda_items(&[(name.to_string(), text.to_string())]);
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//vix//org//EN\r\n");
    for (i, it) in items.iter().enumerate() {
        let Some(date) = &it.date else { continue };
        let compact: String = date.chars().filter(char::is_ascii_digit).collect();
        let summary = if it.kind == "DEADLINE" {
            format!("DEADLINE: {}", it.headline)
        } else {
            it.headline.clone()
        };
        let _ = write!(
            out,
            "BEGIN:VEVENT\r\nUID:{i}-{}@vix\r\nDTSTAMP:{now}\r\nDTSTART;VALUE=DATE:{compact}\r\nSUMMARY:{}\r\nEND:VEVENT\r\n",
            escape_ics(name),
            escape_ics(&summary),
        );
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "#+title: T\n* One :old:\nbody\n** Child B\n** Child A\n* Two\nSCHEDULED: <2026-08-05 Wed>\n";

    #[test]
    fn new_heading_inserts_sibling_at_same_level() {
        let (text, line) = new_heading("* One\n** Sub\nbody", 2);
        assert_eq!(line, 3);
        assert_eq!(text.split('\n').nth(3), Some("** "));
        // Outside any subtree: level 1.
        let (text, line) = new_heading("plain", 0);
        assert_eq!(text.split('\n').nth(line), Some("* "));
    }

    #[test]
    fn navigation_moves_between_headlines() {
        assert_eq!(nav_parent(DOC, 2), Some(1)); // body -> its headline
        assert_eq!(nav_parent(DOC, 3), Some(1)); // child headline -> parent
        assert_eq!(nav_next(DOC, 1), Some(3));
        assert_eq!(nav_prev(DOC, 3), Some(1));
        assert_eq!(nav_forward_same(DOC, 3), Some(4));
        assert_eq!(nav_backward_same(DOC, 4), Some(3));
        assert_eq!(nav_forward_same(DOC, 4), None); // parent boundary
    }

    #[test]
    fn sort_children_orders_direct_children() {
        let sorted = sort_children(DOC, 1).expect("sortable");
        let lines: Vec<&str> = sorted.split('\n').collect();
        assert_eq!(lines[3], "** Child A");
        assert_eq!(lines[4], "** Child B");
        assert_eq!(lines[2], "body", "parent body stays before children");
    }

    #[test]
    fn refile_moves_subtree_under_target() {
        let (text, line) = refile(DOC, 3, 5).expect("refile");
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[line], "** Child B", "releveled under level-1 target");
        assert!(text.contains("* Two"));
        // Target inside the source subtree is refused.
        assert!(refile(DOC, 1, 3).is_none());
    }

    #[test]
    fn paste_subtree_relevels_to_sibling() {
        let (text, line) = paste_subtree(DOC, 3, "* Pasted\nbody\n").expect("paste");
        assert_eq!(text.split('\n').nth(line), Some("** Pasted"));
    }

    #[test]
    fn tags_get_set_and_toggle() {
        assert_eq!(get_tags(DOC, 2).as_deref(), Some("old"));
        let text = set_tags(DOC, 2, "work urgent").expect("set");
        assert!(text.contains("* One :work:urgent:"));
        let text = set_tags(&text, 2, "").expect("clear");
        assert!(text.contains("\n* One\n"));
        let text = toggle_tag(DOC, 1, "ARCHIVE").expect("toggle on");
        assert!(text.contains(":old:ARCHIVE:"));
        let text = toggle_tag(&text, 1, "ARCHIVE").expect("toggle off");
        assert!(text.contains("* One :old:"));
    }

    #[test]
    fn set_property_creates_and_updates_drawer() {
        let text = set_property("* H\nbody", 0, "ID", "42").expect("create");
        assert_eq!(text, "* H\n:PROPERTIES:\n:ID: 42\n:END:\nbody");
        let text = set_property(&text, 0, "id", "43").expect("update");
        assert!(text.contains(":id: 43") && !text.contains(":ID: 42"));
        // A planning line stays between headline and drawer.
        let text = set_property("* H\nSCHEDULED: <2026-01-01 Thu>", 0, "K", "v").expect("plan");
        assert_eq!(
            text,
            "* H\nSCHEDULED: <2026-01-01 Thu>\n:PROPERTIES:\n:K: v\n:END:"
        );
    }

    #[test]
    fn archive_subtree_extracts_and_stamps() {
        let (rest, block) = archive_subtree(DOC, 3, "2026-08-05 Wed 12:00").expect("archive");
        assert!(!rest.contains("Child B"));
        assert!(
            block.starts_with("* Child B"),
            "promoted to level 1: {block}"
        );
        assert!(block.contains(":ARCHIVE_TIME: 2026-08-05 Wed 12:00"));
    }

    #[test]
    fn timestamps_compute_weekdays_and_shift() {
        assert_eq!(
            timestamp_for("2026-08-05", true).as_deref(),
            Some("<2026-08-05 Wed>")
        );
        assert_eq!(
            timestamp_for("2026-08-05", false).as_deref(),
            Some("[2026-08-05 Wed]")
        );
        assert!(timestamp_for("2026-13-05", true).is_none());
        // Shift across a month boundary, weekday rewritten.
        let text = "SCHEDULED: <2026-08-31 Mon>";
        let (new, _) = shift_timestamp_at(text, 15, 1).expect("shift");
        assert_eq!(new, "SCHEDULED: <2026-09-01 Tue>");
        let (new, _) = shift_timestamp_at(text, 15, -1).expect("shift back");
        assert_eq!(new, "SCHEDULED: <2026-08-30 Sun>");
    }

    #[test]
    fn plan_sets_and_replaces_schedule() {
        let text = plan("* H\nbody", 0, "SCHEDULED", "<2026-08-05 Wed>").expect("insert");
        assert_eq!(text, "* H\nSCHEDULED: <2026-08-05 Wed>\nbody");
        let text = plan(&text, 0, "DEADLINE", "<2026-08-09 Sun>").expect("append");
        assert!(text.contains("SCHEDULED: <2026-08-05 Wed> DEADLINE: <2026-08-09 Sun>"));
        let text = plan(&text, 0, "DEADLINE", "<2026-08-10 Mon>").expect("replace");
        assert!(text.contains("DEADLINE: <2026-08-10 Mon>"));
        assert!(!text.contains("2026-08-09"));
    }

    #[test]
    fn links_are_found_and_iterated() {
        let text = "see [[https://x.test][site]] and [[*Target]]\n";
        assert_eq!(
            link_at(text, 6),
            Some(("https://x.test".to_string(), Some("site".to_string())))
        );
        assert_eq!(link_at(text, 35), Some(("*Target".to_string(), None)));
        assert_eq!(link_pos(text, 0, true), Some(4));
        assert_eq!(link_pos(text, 10, true), Some(33));
        assert_eq!(link_pos(text, 33, false), Some(4));
    }

    #[test]
    fn sparse_folds_hide_non_matching_subtrees() {
        let doc = "* TODO Ship\nbody\n* Notes\nplain\n** TODO Sub\nx\n* Done stuff\ny\n";
        let folds = todo_tree_folds(doc);
        // "Notes" contains a TODO child, so only its non-TODO parts fold; the
        // "Done stuff" subtree folds whole; "Ship" stays open.
        // (6, 8): the trailing blank line belongs to the last subtree.
        assert!(folds.contains(&(6, 8)), "Done stuff folded: {folds:?}");
        assert!(!folds.iter().any(|&(s, _)| s == 0), "TODO subtree open");
        assert!(
            !folds.iter().any(|&(s, _)| s == 2),
            "ancestor of match open"
        );
        let folds = occur_folds(doc, "plain");
        assert!(
            !folds.iter().any(|&(s, _)| s == 2),
            "occur keeps its subtree"
        );
        assert!(folds.iter().any(|&(s, _)| s == 0), "occur folds the miss");
    }

    #[test]
    fn footnote_creates_then_jumps_both_ways() {
        // Create: reference at the cursor, definition under * Footnotes.
        let (text, pos) = footnote("body here\n", 4);
        assert!(text.starts_with("body[fn:1] here"), "{text:?}");
        assert!(text.contains("* Footnotes\n\n[fn:1] "), "{text:?}");
        assert_eq!(pos, text.chars().count(), "cursor at the definition end");
        // Jump from the reference to the definition line.
        let ref_pos = text.find("[fn:1]").unwrap() + 1;
        let (same, def_pos) = footnote(&text, ref_pos);
        assert_eq!(same, text, "jump does not edit");
        let def_line_start = text.rfind("[fn:1] ").unwrap();
        assert_eq!(def_pos, text[..def_line_start].chars().count());
        // Jump from the definition back to the reference.
        let (same, back) = footnote(&text, def_pos + 1);
        assert_eq!(same, text);
        assert_eq!(back, text[..text.find("[fn:1]").unwrap()].chars().count());
    }

    #[test]
    fn id_location_finds_governing_headline() {
        let doc = "* One\n:PROPERTIES:\n:ID: abc-123\n:END:\n* Two\n";
        assert_eq!(id_location(doc, "abc-123"), Some(0));
        assert_eq!(id_location(doc, "missing"), None);
    }

    #[test]
    fn src_block_roundtrip() {
        let doc = "* H\n#+begin_src rust\nlet x = 1;\n#+end_src\ntail\n";
        assert_eq!(
            src_block_at(doc, 2),
            Some((1, 3, "rust".to_string())),
            "cursor in the body"
        );
        assert_eq!(src_block_at(doc, 1).map(|b| b.0), Some(1), "on the fence");
        assert_eq!(src_block_at(doc, 4), None, "after the block");
        let new = replace_src_body(doc, 1, "a\nb\n").expect("replace");
        assert_eq!(new, "* H\n#+begin_src rust\na\nb\n#+end_src\ntail\n");
        let new = replace_src_body(doc, 1, "").expect("empty body");
        assert_eq!(new, "* H\n#+begin_src rust\n#+end_src\ntail\n");
        assert!(replace_src_body(doc, 0, "x").is_none(), "not a fence line");
    }

    #[test]
    fn column_view_tabulates_headlines() {
        let doc = "* TODO [#1] Ship it :work:\nbody\n** Sub task\n";
        let table = column_view(doc);
        assert!(table.starts_with("| ITEM | TODO | PRIORITY | TAGS |"));
        assert!(
            table.contains("| Ship it | TODO | [#1] | :work: |"),
            "{table:?}"
        );
        assert!(table.contains("|   Sub task |  |  |  |"), "{table:?}");
    }

    #[test]
    fn latex_export_covers_structure_and_escaping() {
        let tex = to_latex(
            "#+title: T&T\n* A_B\n- item 50%\n#+begin_src rust\nlet x = a_b;\n#+end_src\n*bold* text\n",
        );
        assert!(tex.contains(r"\title{T\&T}"));
        assert!(tex.contains(r"\section{A\_B}"));
        assert!(tex.contains(r"\item item 50\%"));
        assert!(tex.contains("\\begin{verbatim}\nlet x = a_b;\n\\end{verbatim}"));
        assert!(tex.contains(r"\textbf{bold} text"));
        assert!(tex.ends_with("\\end{document}\n"));
    }

    #[test]
    fn ics_export_emits_dated_events_only() {
        let ics = to_ics(DOC, "plan.org", "20260805T120000Z");
        assert!(ics.contains("DTSTART;VALUE=DATE:20260805"));
        assert!(ics.contains("SUMMARY:Two"));
        assert!(!ics.contains("Child A"), "undated headlines are skipped");
        assert!(ics.starts_with("BEGIN:VCALENDAR"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
    }

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
            assert!(
                !lower.contains("href=\"javascript"),
                "leaked scheme: {html}"
            );
            assert!(!lower.contains("href=\"data:"), "leaked data: {html}");
            assert!(
                !lower.contains("href=\"vbscript"),
                "leaked vbscript: {html}"
            );
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
    fn detects_drawer_headers_and_ranges() {
        assert!(is_drawer_header(":properties:"));
        assert!(is_drawer_header(":PROPERTIES:"));
        assert!(is_drawer_header("  :logbook:  ")); // leading/trailing space ok
        assert!(!is_drawer_header(":end:")); // the terminator is not a header
        assert!(!is_drawer_header(":foo: 123")); // a property line, not a header
        assert!(!is_drawer_header("* Name")); // a headline
        assert!(!is_drawer_header("plain"));
        assert!(!is_drawer_header("::"));

        let text = "* Name\n:properties:\n:foo: 123\n:end:\nbody";
        let lines: Vec<&str> = text.split('\n').collect();
        // The drawer header at line 1 spans through its :end: at line 3.
        assert_eq!(drawer_range(&lines, 1), Some((1, 3)));
        // A property line inside the drawer is not itself a foldable header.
        assert_eq!(drawer_range(&lines, 2), None);
        // The headline is not a drawer.
        assert_eq!(drawer_range(&lines, 0), None);

        // A drawer with no matching :END: before EOF does not fold.
        let dangling: Vec<&str> = "* Name\n:properties:\n:foo: 123".split('\n').collect();
        assert_eq!(drawer_range(&dangling, 1), None);

        // A headline appearing before :END: closes the section: no fold.
        let interrupted: Vec<&str> = "* Name\n:properties:\n* Other\n:end:".split('\n').collect();
        assert_eq!(drawer_range(&interrupted, 1), None);
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
    fn priority_reads_the_cookie_after_keyword_or_stars() {
        assert_eq!(priority("* TODO [#A] Task"), Some('A'));
        assert_eq!(priority("* [#0] Task"), Some('0'));
        assert_eq!(priority("* TODO Task"), None);
        assert_eq!(priority("* [#A]"), Some('A'));
        assert_eq!(priority("not a headline"), None);
    }

    #[test]
    fn set_priority_inserts_replaces_and_removes() {
        assert_eq!(
            set_priority("* TODO Task", 0, Some('A')).unwrap(),
            "* TODO [#A] Task"
        );
        assert_eq!(
            set_priority("* TODO [#A] Task", 0, Some('B')).unwrap(),
            "* TODO [#B] Task"
        );
        assert_eq!(
            set_priority("* TODO [#A] Task", 0, None).unwrap(),
            "* TODO Task"
        );
        // No keyword and no body: the cookie is the whole headline text.
        assert_eq!(set_priority("* ", 0, Some('0')).unwrap(), "* [#0]");
        assert_eq!(set_priority("not a headline", 0, Some('A')), None);
    }

    #[test]
    fn priority_up_and_down_step_and_clamp_numeric_range() {
        // 0 = highest, 9 = lowest (this repo's preferred numeric scheme).
        let (highest, lowest, default) = ('0', '9', '0');
        let t = "* TODO Task";
        let t = priority_up(t, 0, highest, lowest, default).unwrap();
        assert_eq!(t, "* TODO [#0] Task", "no cookie yet -> the default");
        let t = priority_down(&t, 0, highest, lowest, default).unwrap();
        assert_eq!(t, "* TODO [#1] Task", "down moves toward lowest");
        let t = priority_up(&t, 0, highest, lowest, default).unwrap();
        assert_eq!(t, "* TODO [#0] Task", "up moves back toward highest");
        // Already at highest: up clamps, no wraparound.
        let t = priority_up(&t, 0, highest, lowest, default).unwrap();
        assert_eq!(t, "* TODO [#0] Task");
        // Walk down to the lowest bound and confirm it clamps too.
        let mut t = t;
        for _ in 0..12 {
            t = priority_down(&t, 0, highest, lowest, default).unwrap();
        }
        assert_eq!(t, "* TODO [#9] Task");
    }

    #[test]
    fn priority_up_and_down_on_letter_scheme_where_highest_sorts_first() {
        // Classic Emacs default: A = highest, C = lowest.
        let t = priority_up("* Task", 0, 'A', 'C', 'B').unwrap();
        assert_eq!(t, "* [#B] Task", "no cookie yet -> the default");
        let t = priority_up(&t, 0, 'A', 'C', 'B').unwrap();
        assert_eq!(t, "* [#A] Task");
        let t = priority_down(&t, 0, 'A', 'C', 'B').unwrap();
        assert_eq!(t, "* [#B] Task");
    }

    #[test]
    fn close_headline_marks_done_with_closed_and_logbook_note() {
        let now = "2024-08-23 Fri 11:30";
        let t = "* TODO Ship it\nsome body";
        let out = close_headline(t, 0, now, "Reviewed and shipped").unwrap();
        assert!(out.contains("* DONE Ship it"), "keyword set to DONE: {out}");
        assert!(
            out.contains("CLOSED: [2024-08-23 Fri 11:30]"),
            "closed stamp added: {out}"
        );
        assert!(out.contains(":LOGBOOK:"), "logbook drawer created: {out}");
        assert!(
            out.contains("- Note taken on [2024-08-23 Fri 11:30] \\\\"),
            "note entry uses org continuation marker: {out}"
        );
        assert!(
            out.contains("  Reviewed and shipped"),
            "note body indented: {out}"
        );
        assert!(out.contains(":END:"), "drawer closed: {out}");
        // The CLOSED line sits directly under the headline, above the drawer.
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines[0], "* DONE Ship it");
        assert_eq!(lines[1], "CLOSED: [2024-08-23 Fri 11:30]");
        assert_eq!(lines[2], ":LOGBOOK:");

        // An empty note marks done + CLOSED but writes no LOGBOOK drawer.
        let bare = close_headline("* TODO A", 0, now, "").unwrap();
        assert_eq!(bare, "* DONE A\nCLOSED: [2024-08-23 Fri 11:30]");

        // A second close refreshes the existing CLOSED line rather than stacking.
        let again = close_headline(&bare, 0, "2024-08-24 Sat 09:00", "").unwrap();
        assert_eq!(again, "* DONE A\nCLOSED: [2024-08-24 Sat 09:00]");

        // A pre-existing LOGBOOK gets the new note prepended as the newest entry.
        let with_lb = "* TODO B\n:LOGBOOK:\n- older entry\n:END:";
        let out = close_headline(with_lb, 0, now, "newer").unwrap();
        let li: Vec<&str> = out.split('\n').collect();
        let lb = li.iter().position(|l| *l == ":LOGBOOK:").unwrap();
        assert_eq!(li[lb + 1], "- Note taken on [2024-08-23 Fri 11:30] \\\\");
        assert_eq!(li[lb + 2], "  newer");
        assert!(
            li[lb + 3].contains("older entry"),
            "older entry kept: {out}"
        );

        // Not a headline → None.
        assert!(close_headline("plain text", 0, now, "x").is_none());
    }

    #[test]
    fn agenda_items_record_source_lines_and_render_maps() {
        let files = vec![(
            "work.org".to_string(),
            "* TODO Ship it\nDEADLINE: <2024-08-23 Fri>\n* Notes\n* TODO Loose end\n".to_string(),
        )];
        let items = agenda_items(&files);
        // Two TODO headlines (lines 0 and 3) and one DEADLINE (attached to line 0).
        let todo0 = items
            .iter()
            .find(|i| i.headline == "TODO Ship it" && i.date.is_none())
            .unwrap();
        assert_eq!(todo0.line, 0);
        let deadline = items.iter().find(|i| i.kind == "DEADLINE").unwrap();
        assert_eq!(deadline.line, 0, "deadline attributed to its headline line");
        let loose = items
            .iter()
            .find(|i| i.headline == "TODO Loose end")
            .unwrap();
        assert_eq!(loose.line, 3);

        // The render's line map points each entry line back to its item index.
        let (text, map) = render_agenda(&items);
        for (buf_line, entry) in map.iter().enumerate() {
            if let Some(idx) = entry {
                let line = text.split('\n').nth(buf_line).unwrap();
                assert!(line.starts_with("- "), "mapped line is an entry: {line:?}");
                assert!(line.contains(&items[*idx].headline));
            }
        }
    }

    #[test]
    fn todo_list_collects_not_done_headlines() {
        let files = vec![(
            "a.org".to_string(),
            "* TODO one\n* DONE two\n* three\n** TODO nested\n".to_string(),
        )];
        let items = todo_list(&files);
        let heads: Vec<&str> = items.iter().map(|i| i.headline.as_str()).collect();
        assert_eq!(heads, vec!["TODO one", "TODO nested"]);
        assert_eq!(items[1].line, 3, "source line recorded");
    }

    #[test]
    fn tags_match_honours_required_and_excluded_tags() {
        let files = vec![(
            "a.org".to_string(),
            "* Alfa :work:urgent:\n* Bravo :work:\n* Charlie :home:\n* Delta\n".to_string(),
        )];
        // `work-urgent` = require work, exclude urgent.
        let items = tags_match(&files, "work-urgent");
        let heads: Vec<&str> = items.iter().map(|i| i.headline.as_str()).collect();
        assert_eq!(heads, vec!["Bravo :work:"]);
        // Bare tag requires it; case-insensitive.
        assert_eq!(tags_match(&files, "HOME").len(), 1);
        // A headline with no tags never matches.
        assert!(
            tags_match(&files, "work")
                .iter()
                .all(|i| i.headline != "Delta")
        );
    }

    #[test]
    fn search_matches_entries_containing_all_words() {
        let files = vec![(
            "a.org".to_string(),
            "* Meeting\nnotes about budget\n* Other\nplan lunch\n".to_string(),
        )];
        let hit = search(&files, "budget");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].headline, "Meeting");
        // All words must appear in the entry body.
        assert!(search(&files, "budget lunch").is_empty());
        assert!(search(&files, "").is_empty(), "empty query matches nothing");
    }

    #[test]
    fn stuck_projects_finds_projects_without_a_next_action() {
        let files = vec![(
            "a.org".to_string(),
            // Alfa: has a TODO child -> not stuck.
            // Bravo: children all DONE/plain -> stuck.
            // Charlie: leaf (no children) -> not a project.
            "* Alfa\n** TODO do it\n* Bravo\n** DONE gone\n** note\n* Charlie\n".to_string(),
        )];
        let items = stuck_projects(&files);
        let heads: Vec<&str> = items.iter().map(|i| i.headline.as_str()).collect();
        assert_eq!(heads, vec!["Bravo"]);
    }

    #[test]
    fn render_list_titles_and_maps_entries() {
        let files = vec![("a.org".to_string(), "* TODO one\n* TODO two\n".to_string())];
        let items = todo_list(&files);
        let (text, map) = render_list("TODO List", &items);
        assert!(text.starts_with("#+title: TODO List\n"));
        assert!(text.contains("- TODO one (a.org)"));
        // The title line maps to nothing; entry lines map to their item.
        assert_eq!(map[0], None);
        assert_eq!(map[1], Some(0));
        assert_eq!(map[2], Some(1));
    }

    #[test]
    fn has_checkbox_detects_list_boxes() {
        assert!(has_checkbox("- [ ] task"));
        assert!(has_checkbox("  1. [X] done"));
        assert!(!has_checkbox("- plain item"));
        assert!(!has_checkbox("* TODO headline"));
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
