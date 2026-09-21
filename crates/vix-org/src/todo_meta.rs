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

use super::{DONE, TODO, headline_level, set_headline_keyword, subtree_range};

// ----- Priority ---------------------------------------------------------

/// Split a headline's post-stars text `rest` into its TODO/DONE keyword
/// (including the trailing space, or `""` if there is none) and the
/// remaining body.
pub(crate) fn split_keyword(rest: &str) -> (&str, &str) {
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
pub(crate) fn headline_todo(line: &str) -> Option<bool> {
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
