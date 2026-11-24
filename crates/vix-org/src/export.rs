//! Exporters: Org text to Markdown, a standalone HTML document, a standalone
//! LaTeX document, or an iCalendar (RFC 5545) calendar of `SCHEDULED:`/
//! `DEADLINE:` entries. Extracted from `lib.rs` (T516) -- the last of its
//! 15 sections, closing out the split. `LINK`/`BARE_LINK` stay `pub(crate)`:
//! `text_refs.rs`'s Hyperlinks functions need them too. `safe_href` stays
//! `pub(crate)`: `lib.rs`'s own test module asserts on it directly.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use super::{add_or, agenda_items, headline_level, sub_or};

// ----- Export ---------------------------------------------------------------

#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// Matches a described Org link `[[target][description]]` (groups 1 and 2).
pub(crate) static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\[([^\]]+)\]\]").expect("link regex"));
#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// Matches an undescribed Org link `[[target]]` (group 1).
pub(crate) static BARE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").expect("bare link regex"));

/// Marks the start of a stashed fragment (see [`Stash`]); a private-use char, so it
/// cannot be confused with text.
const STASH_OPEN: char = '\u{E000}';
/// Marks the end of a stashed fragment.
const STASH_CLOSE: char = '\u{E001}';

/// Fragments set aside during inline conversion so a later emphasis pass cannot
/// see them: generated tags (their `/` is not an italic marker), link targets
/// and verbatim text. Each is replaced in the working text by a placeholder and
/// put back by [`Stash::restore`] once every pass has run.
#[derive(Default)]
struct Stash(Vec<String>);

impl Stash {
    /// Set `fragment` aside and return its placeholder.
    fn put(&mut self, fragment: impl Into<String>) -> String {
        let index = self.0.len();
        self.0.push(fragment.into());
        format!("{STASH_OPEN}{index}{STASH_CLOSE}")
    }

    /// `text` with every placeholder replaced by its fragment (fragments may
    /// themselves hold placeholders, so this recurses).
    fn restore(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c != STASH_OPEN {
                out.push(c);
                continue;
            }
            let digits: String = chars.by_ref().take_while(|&d| d != STASH_CLOSE).collect();
            if let Some(fragment) = digits.parse::<usize>().ok().and_then(|i| self.0.get(i)) {
                out.push_str(&self.restore(fragment));
            }
        }
        out
    }
}

/// `text` without the stash placeholder characters, so input that happens to
/// contain them cannot forge a placeholder.
fn without_stash_marks(text: &str) -> String {
    text.chars()
        .filter(|&c| c != STASH_OPEN && c != STASH_CLOSE)
        .collect()
}

/// Whether Org allows an emphasis marker to *start* right after `c`.
fn emphasis_may_follow(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '-' | '(' | '{' | '\'' | '"' | ';' | '\u{2}' | STASH_CLOSE
        )
}

/// Whether Org allows an emphasis marker to *end* right before `c`. (`&` begins
/// an HTML entity the exporter has already made of a quote, and `\u{3}` ends a
/// LaTeX link's text; neither is part of the word.)
fn emphasis_may_precede(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '-' | '.'
                | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | '\''
                | ')'
                | '}'
                | '"'
                | '['
                | '&'
                | '\u{3}'
                | STASH_OPEN
        )
}

/// Apply an inline-emphasis substitution for a single marker char, mapping
/// `<m>text<m>` to `open`…`close`, as Org does: the opening marker follows the
/// start of the text, whitespace or an opening punctuation mark and hugs the
/// text; the closing marker hugs the text and precedes whitespace, closing
/// punctuation or the end. A marker anywhere else (`and/or`, `a_b`, the
/// slashes of a URL) is plain text. The tags go into `stash`, so a later
/// marker's pass cannot read them as markers; with `literal` the text between
/// the markers is stashed too (Org's `=verbatim=` and `~code~`). A link target
/// between `\u{1}` and `\u{2}` (see [`inline_latex`]) is never scanned.
fn emph(
    input: &str,
    marker: char,
    (open, close): (&str, &str),
    literal: bool,
    stash: &mut Stash,
) -> String {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let at = |k: usize| chars.get(k).copied();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < n {
        let Some(c) = at(i) else { break };
        if c == '\u{1}' {
            // Copy a link target through its closing `\u{2}` untouched.
            let end = (i..n)
                .find(|&k| at(k) == Some('\u{2}'))
                .map_or(n, |k| add_or(k, 1, n));
            out.extend((i..end).filter_map(at));
            i = end;
            continue;
        }
        let opens = c == marker
            && (i == 0 || at(sub_or(i, 1, 0)).is_some_and(emphasis_may_follow))
            && at(add_or(i, 1, n)).is_some_and(|d| !d.is_whitespace() && d != marker);
        let close_at = opens
            .then(|| (add_or(i, 1, n)..n).find(|&k| at(k) == Some(marker)))
            .flatten()
            .filter(|&j| {
                at(sub_or(j, 1, 0)).is_some_and(|d| !d.is_whitespace())
                    && at(add_or(j, 1, n)).is_none_or(emphasis_may_precede)
            });
        if let Some(j) = close_at {
            let inner: String = (add_or(i, 1, n)..j).filter_map(at).collect();
            if literal {
                out.push_str(&stash.put(format!("{open}{inner}{close}")));
            } else {
                out.push_str(&stash.put(open));
                out.push_str(&inner);
                out.push_str(&stash.put(close));
            }
            i = add_or(j, 1, n);
        } else {
            out.push(c);
            i = add_or(i, 1, n);
        }
    }
    out
}

/// Convert Org inline markup (links and emphasis) to Markdown.
fn inline_md(s: &str) -> String {
    use regex::Captures;
    let mut stash = Stash::default();
    let s = without_stash_marks(s);
    // Links first, their targets stashed: a URL's slashes are not italics.
    let s = LINK
        .replace_all(&s, |c: &Captures| {
            format!(
                "{}{}{}",
                stash.put("["),
                &c[2],
                stash.put(format!("]({})", &c[1]))
            )
        })
        .into_owned();
    let s = BARE_LINK
        .replace_all(&s, |c: &Captures| stash.put(format!("<{}>", &c[1])))
        .into_owned();
    let s = emph(&s, '~', ("`", "`"), true, &mut stash);
    let s = emph(&s, '=', ("`", "`"), true, &mut stash);
    let s = emph(&s, '*', ("**", "**"), false, &mut stash);
    let s = emph(&s, '/', ("*", "*"), false, &mut stash);
    let s = emph(&s, '+', ("~~", "~~"), false, &mut stash);
    stash.restore(&s)
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
            let rest = line.get(level..).unwrap_or_default().trim_start();
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
    let mut stash = Stash::default();
    let s = escape_html(&without_stash_marks(s));
    // Links: the regex ran on escaped text, so brackets are intact. The href is
    // scheme-checked so a `javascript:`/`data:` URL can't produce an active
    // link, and stashed so the emphasis passes never read its slashes.
    let s = LINK
        .replace_all(&s, |c: &Captures| {
            format!(
                "{}{}{}",
                stash.put(format!("<a href=\"{}\">", safe_href(&c[1]))),
                &c[2],
                stash.put("</a>")
            )
        })
        .into_owned();
    let s = BARE_LINK
        .replace_all(&s, |c: &Captures| {
            stash.put(format!("<a href=\"{}\">{}</a>", safe_href(&c[1]), &c[1]))
        })
        .into_owned();
    let s = emph(&s, '~', ("<code>", "</code>"), true, &mut stash);
    let s = emph(&s, '=', ("<code>", "</code>"), true, &mut stash);
    let s = emph(&s, '*', ("<b>", "</b>"), false, &mut stash);
    let s = emph(&s, '/', ("<i>", "</i>"), false, &mut stash);
    let s = emph(&s, '_', ("<u>", "</u>"), false, &mut stash);
    let s = emph(&s, '+', ("<del>", "</del>"), false, &mut stash);
    stash.restore(&s)
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
                inline_html(line.get(level..).unwrap_or_default().trim_start())
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
#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// Matches the control-char-delimited link placeholders (`\x01target\x02desc\x03`)
/// inserted by `inline_latex` so link text survives LaTeX escaping.
static SENTINEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\u{1}([^\u{2}]*)\u{2}([^\u{3}]*)\u{3}").expect("sentinel"));

/// Convert Org inline markup to LaTeX (escaping text first). Emphasis markers
/// are matched on the escaped text, so `_`/`~` (escaped above) drop out as
/// markers; bold/italic/strike and links are what remain meaningful.
fn inline_latex(s: &str) -> String {
    use regex::Captures;
    let s = without_stash_marks(s);
    let s = LINK
        .replace_all(&s, |c: &Captures| {
            format!("\u{1}{}\u{2}{}\u{3}", &c[1], &c[2])
        })
        .into_owned();
    let mut stash = Stash::default();
    let s = escape_latex(&s);
    let s = emph(&s, '=', (r"\texttt{", "}"), true, &mut stash);
    let s = emph(&s, '*', (r"\textbf{", "}"), false, &mut stash);
    let s = emph(&s, '/', (r"\textit{", "}"), false, &mut stash);
    let s = emph(&s, '+', (r"\sout{", "}"), false, &mut stash);
    let s = stash.restore(&s);
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
                inline_latex(line.get(level..).unwrap_or_default().trim_start())
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
