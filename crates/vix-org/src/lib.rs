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

mod columns;
pub use columns::{
    ColumnDef, ColumnRow, ColumnsSpec, DblockFlags, DblockParams, apply_column_edit,
    build_column_table, columns_spec_anchor, parse_all_values, parse_columns_spec,
    parse_dblock_params, render_columnview_dblock, resolve_columns_spec, todo_keywords,
    update_all_columnview_dblocks, update_columnview_dblock,
};

mod headline_nav;
use headline_nav::{governing, relevel};
pub use headline_nav::{
    governing_subtree, headlines, nav_backward_same, nav_forward_same, nav_next, nav_parent,
    nav_prev, new_heading, paste_subtree, refile, sort_children,
};

mod todo_meta;
pub use todo_meta::{
    close_headline, has_checkbox, move_subtree_down, move_subtree_up, priority, priority_down,
    priority_up, set_priority, toggle_checkbox, update_statistics,
};
use todo_meta::{headline_todo, split_keyword, strip_priority};

mod properties_and_dates;
use properties_and_dates::{TAGS, line_of_char};
pub use properties_and_dates::{
    archive_subtree, get_tags, plan, set_property, set_tags, shift_timestamp_at, timestamp_for,
    toggle_tag,
};

mod text_refs;
pub use text_refs::{
    column_view, footnote, id_location, link_at, link_pos, occur_folds, replace_src_body,
    src_block_at, todo_tree_folds,
};

mod agenda;
pub use agenda::{
    AgendaItem, agenda, agenda_items, clock_in, clock_out, render_agenda, render_list, search,
    stuck_projects, tags_match, time_report, todo_list,
};

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
