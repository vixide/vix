//! Priority cookies (`[#A]`/`[#B]`/`[#C]`, cycling and clearing) and
//! statistics cookies (`[n/m]`/`[n%]`) with checkbox-state propagation up a
//! list, plus the two subtree-reordering commands that don't fit any other
//! section (`move_subtree_up`/`move_subtree_down`). Extracted from `lib.rs`
//! (T516). `split_keyword`/`strip_priority` stay `pub(crate)`: the Column
//! view section still needs them to render a headline's TODO/priority
//! prefix. `headline_todo` stays `pub(crate)` too, for the "Other built-in
//! agenda views" section's stuck-project/next-action checks.

use std::sync::LazyLock;

use regex::Regex;

use super::{
    DONE, TODO, add_or, headline_level, line_at, next, set_headline_keyword, sub_or, subtree_range,
};

/// Line `i` of an owned-line buffer, or the empty line past the end (callers
/// iterate indices that exist, so the empty line is never observed).
fn owned_at(lines: &[String], i: usize) -> &str {
    lines.get(i).map_or("", String::as_str)
}

// ----- Priority ---------------------------------------------------------

/// Split a headline's post-stars text `rest` into its TODO/DONE keyword
/// (including the trailing space, or `""` if there is none) and the
/// remaining body.
pub(crate) fn split_keyword(rest: &str) -> (&str, &str) {
    if let Some(body) = rest.strip_prefix(&format!("{TODO} ")) {
        (rest.get(..=TODO.len()).unwrap_or_default(), body)
    } else if rest == TODO {
        (rest, "")
    } else if let Some(body) = rest.strip_prefix(&format!("{DONE} ")) {
        (rest.get(..=DONE.len()).unwrap_or_default(), body)
    } else if rest == DONE {
        (rest, "")
    } else {
        ("", rest)
    }
}

/// Strip a leading priority cookie `[#X]` (and the single space after it, if
/// any) from `body`, returning the cookie's character and the remaining text.
pub(crate) fn strip_priority(body: &str) -> Option<(char, &str)> {
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
    let (_, rest) = headline.split_at_checked(next(stars))?;
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
    let (prefix, rest) = headline.split_at_checked(next(stars))?;
    let (keyword, body) = split_keyword(rest);
    let bare = strip_priority(body).map_or(body, |(_, after)| after);
    let new_body = match priority {
        Some(p) if bare.is_empty() => format!("[#{p}]"),
        Some(p) => format!("[#{p}] {bare}"),
        None => bare.to_string(),
    };
    // A bare `TODO`/`DONE` has no trailing space of its own to separate it from
    // a cookie that is now the whole body.
    let gap = if !keyword.is_empty() && !keyword.ends_with(' ') && !new_body.is_empty() {
        " "
    } else {
        ""
    };
    let rewritten = format!("{prefix}{keyword}{gap}{new_body}");
    *lines.get_mut(line)? = rewritten;
    Some(lines.join("\n"))
}

/// The valid priority characters from `highest` to `lowest` inclusive, in
/// that order (so index 0 is always `highest`).
fn priority_range(highest: char, lowest: char) -> Vec<char> {
    let (lo, hi) = if highest <= lowest {
        (u32::from(highest), u32::from(lowest))
    } else {
        (u32::from(lowest), u32::from(highest))
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
                sub_or(idx, 1, 0)
            } else {
                next(idx).min(sub_or(order.len(), 1, 0))
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
    let marked = set_headline_keyword(lines.get(line)?, DONE);
    *lines.get_mut(line)? = marked;

    // The CLOSED planning line sits immediately under the headline; refresh an
    // existing one rather than stacking duplicates.
    let closed = format!("CLOSED: [{now}]");
    let closed_idx = next(line);
    if lines
        .get(closed_idx)
        .is_some_and(|l| l.trim_start().starts_with("CLOSED:"))
    {
        *lines.get_mut(closed_idx)? = closed;
    } else {
        lines.insert(closed_idx, closed);
    }

    if !note.is_empty() {
        let mut entry: Vec<String> = vec![format!("- Note taken on [{now}] \\\\")];
        entry.extend(note.split('\n').map(|l| format!("  {l}")));
        // The subtree body runs until the next headline of the same/higher level.
        let mut end = next(closed_idx);
        while end < lines.len() && headline_level(owned_at(&lines, end)).is_none_or(|l| l > level) {
            end = next(end);
        }
        if let Some(lb) = (next(closed_idx)..end)
            .find(|&i| owned_at(&lines, i).trim().eq_ignore_ascii_case(":LOGBOOK:"))
        {
            for (k, e) in entry.into_iter().enumerate() {
                lines.insert(add_or(next(lb), k, usize::MAX).min(lines.len()), e);
            }
        } else {
            let mut drawer = vec![":LOGBOOK:".to_string()];
            drawer.extend(entry);
            drawer.push(":END:".to_string());
            for (k, d) in drawer.into_iter().enumerate() {
                lines.insert(add_or(next(closed_idx), k, usize::MAX).min(lines.len()), d);
            }
        }
    }
    Some(lines.join("\n"))
}

#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
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
    let rest = target.get(add_or(lead_end, 3, usize::MAX)..)?; // skip "[m]"
    let toggled = format!(
        "{}[{new_mark}]{rest}",
        target.get(..lead_end).unwrap_or_default()
    );
    *lines.get_mut(line)? = toggled;
    Some(lines.join("\n"))
}

// ----- Statistics cookies & checkbox propagation ----------------------------

/// A statistics cookie: `[/]`/`[n/m]` (fraction) or `[%]`/`[n%]` (percent).
#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
static COOKIE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\d*/\d*\]|\[\d*%\]").expect("cookie regex"));

/// The indentation (leading-whitespace width) of a line.
fn indent_of(line: &str) -> usize {
    sub_or(line.len(), line.trim_start().len(), 0)
}

/// Rewrite the first statistics cookie in `line` for `done`/`total`, preserving
/// its kind (`[n/m]` vs `[pct%]`). Percent truncates toward zero (Org's form);
/// `total == 0` yields `0%` or `0/0`. Lines without a cookie are returned as-is.
fn set_cookie(line: &str, done: usize, total: usize) -> String {
    let Some(m) = COOKIE.find(line) else {
        return line.to_string();
    };
    let replacement = if m.as_str().contains('%') {
        let pct = done
            .checked_mul(100)
            .and_then(|scaled| scaled.checked_div(total))
            .unwrap_or(0);
        format!("[{pct}%]")
    } else {
        format!("[{done}/{total}]")
    };
    format!(
        "{}{replacement}{}",
        line.get(..m.start()).unwrap_or_default(),
        line.get(m.end()..).unwrap_or_default()
    )
}

/// Replace a checkbox's mark character on a line that already has one.
fn set_checkbox_mark(line: &str, mark: char) -> String {
    let Some(caps) = CHECKBOX.captures(line) else {
        return line.to_string();
    };
    let lead_end = caps.get(1).map_or(0, |g| g.end());
    format!(
        "{}[{mark}]{}",
        line.get(..lead_end).unwrap_or_default(),
        line.get(add_or(lead_end, 3, usize::MAX)..)
            .unwrap_or_default()
    )
}

/// The TODO state of a headline: `Some(true)` if it carries the DONE keyword,
/// `Some(false)` for TODO, `None` if it has no TODO keyword (or isn't a headline).
pub(crate) fn headline_todo(line: &str) -> Option<bool> {
    let stars = headline_level(line)?;
    let kw = line
        .get(stars..)
        .unwrap_or_default()
        .split_whitespace()
        .next()?;
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
            let value = t.get(13..).unwrap_or_default().to_ascii_lowercase();
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
        let Some(item) = items.get(k) else {
            continue;
        };
        if let Some(prev) = k.checked_sub(1).and_then(|p| items.get(p))
            && (next(prev.line)..item.line).any(|li| headline_level(owned_at(lines, li)).is_some())
        {
            stack.clear();
        }
        while stack
            .last()
            .is_some_and(|&top| items.get(top).is_some_and(|t| t.indent >= item.indent))
        {
            stack.pop();
        }
        if let Some(slot) = parent.get_mut(k) {
            *slot = stack.last().copied();
        }
        stack.push(k);
    }
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    for (k, p) in parent.iter().enumerate() {
        if let Some(list) = p.and_then(|p| children.get_mut(p)) {
            list.push(k);
        }
    }
    // Process deepest items first so a parent sees its children's final marks.
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(items.get(k).map_or(0, |it| it.indent)));
    for k in order {
        let Some(kids) = children.get(k) else {
            continue;
        };
        if kids.is_empty() {
            continue;
        }
        let total = kids.len();
        let done = kids
            .iter()
            .filter(|&&c| items.get(c).is_some_and(|it| matches!(it.mark, 'x' | 'X')))
            .count();
        let any_partial = kids
            .iter()
            .any(|&c| items.get(c).is_some_and(|it| it.mark == '-'));
        let new_mark = if done == total {
            'X'
        } else if done == 0 && !any_partial {
            ' '
        } else {
            '-'
        };
        let Some(item) = items.get_mut(k) else {
            continue;
        };
        item.mark = new_mark;
        let Some(slot) = lines.get_mut(item.line) else {
            continue;
        };
        let marked = set_checkbox_mark(slot, new_mark);
        *slot = set_cookie(&marked, done, total);
    }
}

/// Update statistics cookies that live in headlines (second pass).
fn update_headline_cookies(lines: &mut [String]) {
    let levels: Vec<Option<usize>> = lines.iter().map(|l| headline_level(l)).collect();
    let level_at = |j: usize| levels.get(j).copied().flatten();
    for h in 0..lines.len() {
        let Some(level) = level_at(h) else { continue };
        if !COOKIE.is_match(owned_at(lines, h)) {
            continue;
        }
        // The subtree runs until the next headline of the same or higher level.
        let mut end = next(h);
        while end < lines.len() && level_at(end).is_none_or(|l| l > level) {
            end = next(end);
        }
        let drawer: Vec<String> = lines.get(next(h)..end).unwrap_or(&[]).to_vec();
        let body_end = (next(h)..end)
            .find(|&j| level_at(j).is_some())
            .unwrap_or(end);
        let has_checkboxes = (next(h)..body_end).any(|j| CHECKBOX.is_match(owned_at(lines, j)));
        let (count_todo, recursive) = cookie_data(&drawer).unwrap_or((!has_checkboxes, false));
        let (done, total) = if count_todo {
            let mut d = 0usize;
            let mut t = 0usize;
            for j in next(h)..end {
                let direct = level_at(j) == Some(next(level));
                let counted = if recursive {
                    level_at(j).is_some()
                } else {
                    direct
                };
                if counted && let Some(is_done) = headline_todo(owned_at(lines, j)) {
                    t = next(t);
                    if is_done {
                        d = next(d);
                    }
                }
            }
            (d, t)
        } else {
            // Top-level checkboxes in the body (the shallowest indent).
            let cbs: Vec<(usize, char)> = (next(h)..body_end)
                .filter_map(|j| {
                    CHECKBOX
                        .captures(owned_at(lines, j))
                        .and_then(|c| c.get(2))
                        .map(|g| {
                            (
                                indent_of(owned_at(lines, j)),
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
        let rewritten = set_cookie(owned_at(lines, h), done, total);
        if let Some(slot) = lines.get_mut(h) {
            *slot = rewritten;
        }
    }
}

/// Move the subtree at `line` down past its next sibling, returning the new text
/// and the subtree's new starting line. `None` if there is no following sibling.
#[must_use]
pub fn move_subtree_down(text: &str, line: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let level = headline_level(lines.get(line)?)?;
    let (start, end) = subtree_range(&lines, line)?;
    if end >= lines.len() || headline_level(line_at(&lines, end)) != Some(level) {
        return None; // no sibling of the same level follows
    }
    let (_, sib_end) = subtree_range(&lines, end)?;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(lines.get(..start)?);
    out.extend_from_slice(lines.get(end..sib_end)?); // sibling first
    out.extend_from_slice(lines.get(start..end)?); // then this subtree
    out.extend_from_slice(lines.get(sib_end..)?);
    let new_start = add_or(start, sub_or(sib_end, end, 0), usize::MAX);
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
        if let Some(l) = headline_level(line_at(&lines, i)) {
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
    out.extend_from_slice(lines.get(..prev)?);
    out.extend_from_slice(lines.get(start..end)?); // this subtree first
    out.extend_from_slice(lines.get(prev..start)?); // then the previous sibling
    out.extend_from_slice(lines.get(end..)?);
    Some((out.join("\n"), prev))
}
