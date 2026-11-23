//! The query beautifier (Alt+Shift+F in the query editor).
//!
//! Rewrites one SQL statement into the house style promised by `spec/index.md`:
//! keywords `UPPERCASE`, each major clause (`SELECT`, `FROM`, `WHERE`,
//! `GROUP BY`, `ORDER BY`, joins, …) starting a new line, and `AND` / `OR`
//! conditions continued on their own lines with 4-space indentation. String
//! literals and comments pass through untouched; clause breaks apply only at
//! parenthesis depth zero so inline subqueries stay inline.
//!
//! The formatter is token-based: [`beautify`] scans the statement into a few
//! coarse tokens, then re-emits them, synthesizing whitespace. It is not a SQL
//! parser, so unusual syntax is laid out by the same simple rules rather than
//! rejected. The workbench applies it to the statement under the cursor only
//! (`Browser::format_at_cursor`). Owning spec: `crates/vix-db/spec/index.md`
//! ("Query Formatting").

use super::highlight::is_keyword;

/// Tokens the beautifier re-arranges. Whitespace is discarded during the scan
/// and re-synthesized on output.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FTok {
    /// A bare word (identifier or keyword).
    Word(String),
    /// A quoted string literal, quotes included.
    Str(String),
    /// A comment, delimiters included; line comments force a break after.
    Comment(String, bool),
    /// A single punctuation character.
    Punct(char),
}

/// Keywords that start a new line at parenthesis depth zero.
const CLAUSE_STARTERS: &[&str] = &[
    "select",
    "from",
    "where",
    "group",
    "order",
    "having",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
    "values",
    "set",
    "returning",
    "join",
    "left",
    "right",
    "inner",
    "full",
    "cross",
];

/// Keywords that continue a clause on an indented line of their own.
const CONTINUATIONS: &[&str] = &["and", "or"];

/// The char at `i`, or NUL past the end: the end-of-text sentinel (the scanners
/// stop at the end before reading it).
fn ch(chars: &[char], i: usize) -> char {
    chars.get(i).copied().unwrap_or('\0')
}

/// The chars between `from` and `to`; empty for a range that is not inside `chars`.
fn span(chars: &[char], from: usize, to: usize) -> &[char] {
    chars.get(from..to).unwrap_or(&[])
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

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// Reformat one SQL statement (no trailing semicolon required or added).
///
/// Keywords from [`crate::highlight::KEYWORDS`] are uppercased; other words
/// keep their case. The output has no trailing whitespace.
///
/// A quirk to know: the string scanner treats a doubled quote (`'it''s'`) as
/// two adjacent literals, so the formatter may insert a space between them.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     vix_db::format::beautify("select a from t where x=1 and y=2"),
///     "SELECT a\nFROM t\nWHERE x = 1\n    AND y = 2"
/// );
/// ```
#[must_use]
pub fn beautify(sql: &str) -> String {
    let tokens = scan(sql);
    let mut out = String::new();
    let mut line_len = 0usize;
    let mut depth = 0usize;
    let mut first = true;
    // `line_len` is the width of the current output line; zero means we are at
    // a line start, where no separating space is needed and a break would only
    // produce a blank line.
    // A starter directly after a starter ("LEFT JOIN") must not break again.
    let mut prev_starter = false;
    for tok in tokens {
        let starter_word = matches!(&tok, FTok::Word(w) if CLAUSE_STARTERS.contains(&w.to_ascii_lowercase().as_str()));
        let (text, break_before, indent) = match &tok {
            FTok::Word(w) => {
                let lower = w.to_ascii_lowercase();
                let text = if is_keyword(&lower) {
                    w.to_ascii_uppercase()
                } else {
                    w.clone()
                };
                if depth == 0 && starter_word && !prev_starter {
                    (text, true, 0)
                } else if depth == 0 && CONTINUATIONS.contains(&lower.as_str()) {
                    (text, true, 4)
                } else {
                    (text, false, 0)
                }
            }
            FTok::Str(s) => (s.clone(), false, 0),
            FTok::Comment(c, _) => (c.clone(), false, 0),
            FTok::Punct(c) => {
                match c {
                    '(' => depth = ahead(depth, 1),
                    ')' => depth = sub_or(depth, 1, 0),
                    _ => {}
                }
                (c.to_string(), false, 0)
            }
        };
        if break_before && !first && line_len > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            line_len = indent;
        } else if needs_space(&out, &text) {
            out.push(' ');
            line_len = ahead(line_len, 1);
        }
        out.push_str(&text);
        line_len = ahead(line_len, text.chars().count());
        first = false;
        prev_starter = starter_word;
        if let FTok::Comment(_, line_comment) = &tok
            && *line_comment
        {
            out.push('\n');
            line_len = 0;
        }
    }
    while out.ends_with('\n') || out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Whether a space belongs between the end of `out` and `next`: never at the
/// start, after a newline, `(` or `.`, nor before `,` `)` `;` `.`.
fn needs_space(out: &str, next: &str) -> bool {
    let Some(prev) = out.chars().last() else {
        return false;
    };
    let Some(head) = next.chars().next() else {
        return false;
    };
    if prev == '\n' || prev == '(' || prev == '.' {
        return false;
    }
    !matches!(head, ',' | ')' | ';' | '.')
}

/// Scan `sql` into tokens, preserving strings and comments verbatim.
/// Unterminated strings and block comments run to the end of the input.
fn scan(sql: &str) -> Vec<FTok> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = ch(&chars, i);
        if c.is_whitespace() {
            i = ahead(i, 1);
        } else if c == '-' && chars.get(ahead(i, 1)) == Some(&'-') {
            let start = i;
            while i < chars.len() && ch(&chars, i) != '\n' {
                i = ahead(i, 1);
            }
            out.push(FTok::Comment(span(&chars, start, i).iter().collect(), true));
        } else if c == '/' && chars.get(ahead(i, 1)) == Some(&'*') {
            let start = i;
            i = ahead(i, 2);
            while i < chars.len() && !(ch(&chars, i) == '*' && chars.get(ahead(i, 1)) == Some(&'/'))
            {
                i = ahead(i, 1);
            }
            i = ahead(i, 2).min(chars.len());
            out.push(FTok::Comment(
                span(&chars, start, i).iter().collect(),
                false,
            ));
        } else if c == '\'' || c == '"' {
            let start = i;
            i = ahead(i, 1);
            while i < chars.len() && ch(&chars, i) != c {
                i = ahead(i, 1);
            }
            i = ahead(i, 1).min(chars.len());
            out.push(FTok::Str(span(&chars, start, i).iter().collect()));
        } else if c.is_ascii_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (ch(&chars, i).is_ascii_alphanumeric() || ch(&chars, i) == '_')
            {
                i = ahead(i, 1);
            }
            out.push(FTok::Word(span(&chars, start, i).iter().collect()));
        } else {
            out.push(FTok::Punct(c));
            i = ahead(i, 1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clauses_break_onto_their_own_lines_uppercased() {
        let got = beautify("select id, name from users where age > 21 order by name");
        assert_eq!(
            got,
            "SELECT id, name\nFROM users\nWHERE age > 21\nORDER BY name"
        );
    }

    #[test]
    fn and_or_continue_with_four_space_indent() {
        let got = beautify("select * from t where a=1 and b=2 or c=3");
        assert!(got.contains("\n    AND b = 2"));
        assert!(got.contains("\n    OR c = 3"));
    }

    #[test]
    fn subqueries_stay_inline() {
        let got = beautify("select * from t where id in (select id from u where x=1)");
        assert!(
            got.contains("(SELECT id FROM u WHERE x = 1)"),
            "no breaks inside parens: {got}"
        );
    }

    #[test]
    fn strings_and_comments_pass_through() {
        let got = beautify("select 'from a to b' /* keep from */ from t");
        assert!(got.contains("'from a to b'"), "string untouched: {got}");
        assert!(got.contains("/* keep from */"));
        assert!(got.contains("\nFROM t"));
    }

    #[test]
    fn joins_break_before_the_join_phrase() {
        let got = beautify("select * from a left join b on a.id=b.id");
        assert!(got.contains("\nLEFT JOIN b ON a.id = b.id"), "{got}");
    }
}
