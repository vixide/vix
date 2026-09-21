//! Exporters: Org text to Markdown, a standalone HTML document, a standalone
//! LaTeX document, or an iCalendar (RFC 5545) calendar of `SCHEDULED:`/
//! `DEADLINE:` entries. Extracted from `lib.rs` (T516) -- the last of its
//! 15 sections, closing out the split. `LINK`/`BARE_LINK` stay `pub(crate)`:
//! `text_refs.rs`'s Hyperlinks functions need them too. `safe_href` stays
//! `pub(crate)`: `lib.rs`'s own test module asserts on it directly.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use super::{agenda_items, headline_level};

// ----- Export ---------------------------------------------------------------

pub(crate) static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\[([^\]]+)\]\]").expect("link regex"));
pub(crate) static BARE_LINK: LazyLock<Regex> =
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
pub(crate) fn safe_href(url: &str) -> String {
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
