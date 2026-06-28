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
    if !deeper && lines[start..end].iter().any(|l| headline_level(l) == Some(1)) {
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

static CHECKBOX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*(?:[-+*]|\d+[.)])\s+)\[([ xX-])\]").expect("checkbox regex"));

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
    re.replace_all(input, format!("{open}$1{close}")).into_owned()
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
        if let Some(rest) = line.strip_prefix("#+title:").or_else(|| line.strip_prefix("#+TITLE:")) {
            out.push(format!("# {}", rest.trim()));
        } else if let Some(rest) = line.strip_prefix("#+author:").or_else(|| line.strip_prefix("#+AUTHOR:")) {
            out.push(format!("*{}*", rest.trim()));
        } else if line.starts_with("#+BEGIN_") || line.starts_with("#+END_")
            || line.starts_with("#+begin_") || line.starts_with("#+end_")
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

/// Convert Org inline markup to HTML (escaping text first).
fn inline_html(s: &str) -> String {
    let s = escape_html(s);
    // Links: the regex ran on escaped text, so brackets are intact.
    let s = LINK.replace_all(&s, "<a href=\"$1\">$2</a>").into_owned();
    let s = BARE_LINK.replace_all(&s, "<a href=\"$1\">$1</a>").into_owned();
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
        if let Some(rest) = line.strip_prefix("#+title:").or_else(|| line.strip_prefix("#+TITLE:")) {
            title = rest.trim();
            close_list(&mut body, &mut in_list);
            body.push(format!("<h1>{}</h1>", inline_html(rest.trim())));
        } else if line.starts_with("#+") {
            close_list(&mut body, &mut in_list); // ignore other keywords/blocks
        } else if let Some(level) = headline_level(line) {
            close_list(&mut body, &mut in_list);
            let tag = level.min(6);
            body.push(format!("<h{tag}>{}</h{tag}>", inline_html(line[level..].trim_start())));
        } else if let Some(item) = line.trim_start().strip_prefix("- ").or_else(|| line.trim_start().strip_prefix("+ ")) {
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
        assert_eq!(promote("* A\n** B\nbody\n* C", 1).unwrap(), "* A\n* B\nbody\n* C");
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
            ("home.org".to_string(), "* Meeting\nSCHEDULED: <2024-08-20 Tue>\n".to_string()),
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
