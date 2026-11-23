//! Real-time SQL syntax highlighting for the query editor.
//!
//! A small hand-rolled tokenizer, not a grammar: each line is split into
//! spans tagged keyword / string / number / comment / plain, and [`style`]
//! maps each class to the active Vix theme's `syntax` colors — the same
//! ones the code editor uses — falling back to `spec/index.md`'s palette (keywords
//! cyan, strings green, numbers yellow, comments gray) for themes that leave
//! a token unset. Block comments can span lines, so the tokenizer threads a
//! "still inside `/* … */`" flag from line to line.
//!
//! Only the colours come from the theme; classification is purely lexical, so
//! it is fast enough to re-run on every visible line each frame. The keyword
//! table is shared with [`crate::complete`] and [`crate::format`]. Owning
//! spec: `crates/vix-db/spec/index.md` ("Syntax Highlighting").

use ratatui::style::{Color, Style};

/// SQL keywords recognized by the highlighter, the completer, and the
/// formatter. Lowercase; matching is ASCII case-insensitive.
pub const KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "as",
    "asc",
    "avg",
    "begin",
    "between",
    "by",
    "cascade",
    "case",
    "check",
    "column",
    "commit",
    "constraint",
    "count",
    "create",
    "cross",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "extension",
    "foreign",
    "from",
    "full",
    "function",
    "grant",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "max",
    "min",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "returning",
    "revoke",
    "right",
    "role",
    "rollback",
    "select",
    "set",
    "sum",
    "table",
    "then",
    "to",
    "trigger",
    "truncate",
    "union",
    "unique",
    "update",
    "usage",
    "user",
    "using",
    "values",
    "view",
    "when",
    "where",
    "with",
];

/// The char at `i`, or NUL past the end: the end-of-text sentinel (the scanners
/// stop at the end before reading it).
fn ch(chars: &[char], i: usize) -> char {
    chars.get(i).copied().unwrap_or('\0')
}

/// The chars between `from` and `to`; empty for a range that is not inside `chars`.
fn span(chars: &[char], from: usize, to: usize) -> &[char] {
    chars.get(from..to).unwrap_or(&[])
}

/// The chars from `from` on; empty past the end.
fn span_from(chars: &[char], from: usize) -> &[char] {
    chars.get(from..).unwrap_or(&[])
}

/// `from + step`; an index past the top of `usize` is past any text.
fn ahead(from: usize, step: usize) -> usize {
    add_or(from, step, usize::MAX)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// A highlight class for one span of text.
///
/// Each class maps to a theme token in [`style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    /// SQL keyword.
    Keyword,
    /// Quoted string literal.
    Str,
    /// Numeric literal.
    Num,
    /// `-- …` or `/* … */` comment.
    Comment,
    /// Everything else (identifiers, punctuation, whitespace).
    Plain,
}

/// Whether `word` is a SQL keyword (ASCII case-insensitive).
///
/// # Examples
///
/// ```
/// assert!(vix_db::highlight::is_keyword("SeLeCt"));
/// assert!(!vix_db::highlight::is_keyword("users"));
/// ```
#[must_use]
pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word.to_ascii_lowercase().as_str())
}

/// The render style for one highlight class: the active Vix theme's `syntax`
/// color for the matching token (`keyword` / `string` / `number` /
/// `comment`), or `spec/index.md`'s fallback palette when the theme leaves the
/// token unset.
#[must_use]
pub fn style(tok: Tok) -> Style {
    let themed = |token: &str, fallback: Color| {
        Style::default().fg(vix_theme::syntax_color(token).unwrap_or(fallback))
    };
    match tok {
        Tok::Keyword => themed("keyword", Color::Cyan),
        Tok::Str => themed("string", Color::Green),
        Tok::Num => themed("number", Color::Yellow),
        Tok::Comment => themed("comment", Color::DarkGray),
        Tok::Plain => vix_theme::base(),
    }
}

/// Split `line` into `(class, text)` spans. `in_block` says the line starts
/// inside a `/* … */` comment; the second return value says the *next* line
/// does. Adjacent same-class spans are merged.
///
/// Strings (single or double quoted) are not continued across lines: an
/// unterminated quote highlights to the end of its own line only. Numbers
/// include trailing alphanumerics and dots (`1.5e3`, `0xFF`).
///
/// # Examples
///
/// ```
/// use vix_db::highlight::{highlight_line, Tok};
/// let (spans, in_block) = highlight_line("select 1 -- hi", false);
/// assert_eq!(spans[0], (Tok::Keyword, "select".to_string()));
/// assert_eq!(spans.last().unwrap().0, Tok::Comment);
/// assert!(!in_block);
/// // An open block comment carries over to the next line.
/// assert!(highlight_line("/* open", false).1);
/// ```
#[must_use]
pub fn highlight_line(line: &str, in_block: bool) -> (Vec<(Tok, String)>, bool) {
    let mut spans: Vec<(Tok, String)> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut block = in_block;
    while i < chars.len() {
        if block {
            // Inside a block comment: consume to the closing `*/` or the end
            // of the line, whichever comes first (the flag carries over).
            let start = i;
            while i < chars.len() && !(ch(&chars, i) == '*' && chars.get(ahead(i, 1)) == Some(&'/'))
            {
                i = ahead(i, 1);
            }
            if i < chars.len() {
                i = ahead(i, 2);
                block = false;
            }
            push(
                &mut spans,
                Tok::Comment,
                span(&chars, start, i).iter().collect(),
            );
            continue;
        }
        let c = ch(&chars, i);
        if c == '-' && chars.get(ahead(i, 1)) == Some(&'-') {
            push(
                &mut spans,
                Tok::Comment,
                span_from(&chars, i).iter().collect(),
            );
            i = chars.len();
        } else if c == '/' && chars.get(ahead(i, 1)) == Some(&'*') {
            block = true;
            push(&mut spans, Tok::Comment, "/*".to_string());
            i = ahead(i, 2);
        } else if c == '\'' || c == '"' {
            let start = i;
            i = ahead(i, 1);
            while i < chars.len() && ch(&chars, i) != c {
                i = ahead(i, 1);
            }
            i = ahead(i, 1).min(chars.len());
            push(
                &mut spans,
                Tok::Str,
                span(&chars, start, i).iter().collect(),
            );
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (ch(&chars, i).is_ascii_alphanumeric() || ch(&chars, i) == '.')
            {
                i = ahead(i, 1);
            }
            push(
                &mut spans,
                Tok::Num,
                span(&chars, start, i).iter().collect(),
            );
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (ch(&chars, i).is_ascii_alphanumeric() || ch(&chars, i) == '_')
            {
                i = ahead(i, 1);
            }
            let word: String = span(&chars, start, i).iter().collect();
            let tok = if is_keyword(&word) {
                Tok::Keyword
            } else {
                Tok::Plain
            };
            push(&mut spans, tok, word);
        } else {
            push(&mut spans, Tok::Plain, c.to_string());
            i = ahead(i, 1);
        }
    }
    (spans, block)
}

/// Append `text` as a `tok` span, merging with a same-class predecessor.
fn push(spans: &mut Vec<(Tok, String)>, tok: Tok, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some((last, buf)) = spans.last_mut()
        && *last == tok
    {
        buf.push_str(&text);
        return;
    }
    spans.push((tok, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(line: &str) -> Vec<(Tok, String)> {
        highlight_line(line, false).0
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_classified() {
        let spans = classes("SELECT name, 42 FROM t -- done");
        assert_eq!(spans[0], (Tok::Keyword, "SELECT".to_string()));
        assert!(spans.contains(&(Tok::Num, "42".to_string())));
        assert!(
            spans
                .iter()
                .any(|(t, s)| *t == Tok::Comment && s.starts_with("--"))
        );
        let spans = classes("WHERE a = 'from'");
        assert!(
            spans.contains(&(Tok::Str, "'from'".to_string())),
            "keyword inside a string stays a string"
        );
    }

    #[test]
    fn block_comments_carry_across_lines() {
        let (spans, open) = highlight_line("SELECT /* start", false);
        assert!(open, "block comment continues");
        assert_eq!(spans.last().unwrap().0, Tok::Comment);
        let (spans, open) = highlight_line("still comment */ FROM t", true);
        assert!(!open);
        assert_eq!(spans[0].0, Tok::Comment);
        assert!(spans.iter().any(|(t, s)| *t == Tok::Keyword && s == "FROM"));
    }

    #[test]
    fn case_insensitive_keywords() {
        assert!(is_keyword("select") && is_keyword("SELECT") && is_keyword("SeLeCt"));
        assert!(!is_keyword("users"));
    }

    #[test]
    fn keyword_list_meets_the_spec_size() {
        assert!(
            KEYWORDS.len() >= 70,
            "spec promises 70+ keywords, have {}",
            KEYWORDS.len()
        );
    }

    #[test]
    fn styles_fall_back_to_the_documented_palette_without_a_theme() {
        crate::test_support::without_theme(|| {
            assert_eq!(style(Tok::Keyword).fg, Some(Color::Cyan));
            assert_eq!(style(Tok::Str).fg, Some(Color::Green));
            assert_eq!(style(Tok::Num).fg, Some(Color::Yellow));
            assert_eq!(style(Tok::Comment).fg, Some(Color::DarkGray));
            assert_eq!(style(Tok::Plain), vix_theme::base());
        });
    }

    #[test]
    fn a_theme_overrides_the_syntax_colors_it_sets() {
        let json = r#"{"name":"t","syntax":{"keyword":[1,2,3],"string":[4,5,6],"number":[7,8,9],"comment":[10,11,12]}}"#;
        crate::test_support::with_theme(json, || {
            assert_eq!(style(Tok::Keyword).fg, Some(Color::Rgb(1, 2, 3)));
            assert_eq!(style(Tok::Str).fg, Some(Color::Rgb(4, 5, 6)));
            assert_eq!(style(Tok::Num).fg, Some(Color::Rgb(7, 8, 9)));
            assert_eq!(style(Tok::Comment).fg, Some(Color::Rgb(10, 11, 12)));
        });
    }
}
