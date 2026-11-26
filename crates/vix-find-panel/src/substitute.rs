//! Ex `:s` / `:%s`: a bounded, **literal** substitute (T841).
//!
//! [`parse`] reads one Ex command line and says whether it is a substitute
//! and, if so, either the [`Substitute`] it asks for or why it is refused.
//! Everything the grammar does not support is refused here, before the host
//! touches the buffer. [`plan`] finds the character ranges a substitute
//! replaces and [`apply`] rewrites the text in one pass, so the host can
//! make the whole command one undo step.
//!
//! Grammar (see `spec/find-and-replace/index.md`, "Ex substitute"):
//!
//! ```text
//! [%]s{delim}{pattern}[{delim}{replacement}[{delim}[flags]]]
//! ```
//!
//! The name may be any prefix of `substitute` from `s`. The pattern is matched literally and case-sensitively. Characters Vim's
//! default (`magic`) syntax would treat specially are refused unless escaped.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::fmt;

/// Which lines a substitute covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    /// No range: the cursor's line.
    CurrentLine,
    /// `%`: every line of the buffer.
    WholeBuffer,
}

/// A parsed, supported `:s` command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Substitute {
    /// The lines to search.
    pub range: Range,
    /// The literal text to find (escapes already resolved; never empty).
    pub pattern: String,
    /// The literal text to insert (escapes and `&` already resolved).
    pub replacement: String,
    /// `g`: every occurrence on a line, not only the first.
    pub global: bool,
    /// `c`: ask before each replacement.
    pub confirm: bool,
}

/// Why a substitute command is refused. Nothing is edited for any of these.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubstituteError {
    /// A range other than none or `%` (the range text as typed).
    Range(String),
    /// A delimiter Vim does not allow (a letter, digit, blank, `\`, `"` or `|`).
    Delimiter(char),
    /// `:s` with no pattern (Vim would reuse the last search).
    EmptyPattern,
    /// Pattern syntax this literal `:s` does not support (the token).
    Pattern(String),
    /// Replacement syntax this literal `:s` does not support (the token).
    Replacement(String),
    /// A flag other than `g` or `c` (or a count) after the last delimiter.
    Flag(char),
}

impl fmt::Display for SubstituteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range(r) => write!(f, "unsupported range {r:?}"),
            Self::Delimiter(c) => write!(f, "unsupported delimiter {c:?}"),
            Self::EmptyPattern => f.write_str("empty pattern"),
            Self::Pattern(t) => write!(f, "unsupported pattern syntax {t:?}"),
            Self::Replacement(t) => write!(f, "unsupported replacement syntax {t:?}"),
            Self::Flag(c) => write!(f, "unsupported flag {c:?}"),
        }
    }
}

impl std::error::Error for SubstituteError {}

/// Characters that may make up a Vim line range (`1,5`, `.`, `$`, `'a,'b`, `.+2`).
fn is_range_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '.' | ',' | ';' | '$' | '+' | '-' | '\'' | '<' | '>')
}

/// What follows a substitute command name (a prefix of `substitute`) at the
/// start of `rest`, or `None` when `rest` does not start with one followed by a
/// non-letter or the end.
fn strip_name(rest: &str) -> Option<&str> {
    let name_len = rest
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map_or(rest.len(), |(i, _)| i);
    let name = rest.get(..name_len)?;
    if name.is_empty() || !"substitute".starts_with(name) {
        return None;
    }
    rest.get(name_len..)
}

/// Parse `cmd` (the text after `:`, already trimmed). `None` when it is not a
/// substitute command at all; otherwise the command or why it is refused.
///
/// # Examples
///
/// ```
/// use vix_find_panel::substitute::{parse, Range, SubstituteError};
///
/// let sub = parse(r"%s/a\/b/c/g").unwrap().unwrap();
/// assert_eq!((sub.range, sub.pattern.as_str(), sub.global), (Range::WholeBuffer, "a/b", true));
/// assert_eq!(parse("s/a.b/c/"), Some(Err(SubstituteError::Pattern(".".into()))));
/// assert_eq!(parse("set"), None);
/// ```
#[must_use]
pub fn parse(cmd: &str) -> Option<Result<Substitute, SubstituteError>> {
    let mut range_len = cmd.len();
    let mut after_quote = false;
    for (i, c) in cmd.char_indices() {
        // A letter right after `'` names a mark and is part of the range.
        let in_range = c == '%' || is_range_char(c) || (after_quote && c.is_ascii_alphabetic());
        if !in_range {
            range_len = i;
            break;
        }
        after_quote = c == '\'';
    }
    let range_text = cmd.get(..range_len)?;
    let rest = strip_name(cmd.get(range_len..)?)?;
    let range = match range_text {
        "" => Range::CurrentLine,
        "%" => Range::WholeBuffer,
        // `:3` alone (go to line) has no command name and returned above.
        other => return Some(Err(SubstituteError::Range(other.to_string()))),
    };
    let mut chars = rest.chars();
    let Some(delim) = chars.next() else {
        return Some(Err(SubstituteError::EmptyPattern));
    };
    if delim.is_ascii_alphanumeric() || delim.is_whitespace() || matches!(delim, '\\' | '"' | '|') {
        return Some(Err(SubstituteError::Delimiter(delim)));
    }
    Some(parse_body(range, delim, chars))
}

/// Parse the pattern, replacement and flags after the delimiter.
fn parse_body(
    range: Range,
    delim: char,
    mut chars: std::str::Chars<'_>,
) -> Result<Substitute, SubstituteError> {
    let (pattern, closed) = parse_pattern(delim, &mut chars)?;
    if pattern.is_empty() {
        return Err(SubstituteError::EmptyPattern);
    }
    let (replacement, closed) = if closed {
        parse_replacement(delim, &pattern, &mut chars)?
    } else {
        (String::new(), false)
    };
    let mut global = false;
    let mut confirm = false;
    if closed {
        for c in chars.as_str().trim_end().chars() {
            match c {
                // Vim toggles on a repeated `g` (`:s/a/b/gg` is first-only).
                'g' => global = !global,
                'c' => confirm = true,
                other => return Err(SubstituteError::Flag(other)),
            }
        }
    }
    Ok(Substitute {
        range,
        pattern,
        replacement,
        global,
        confirm,
    })
}

/// The literal pattern up to the next unescaped `delim`, and whether that
/// delimiter was found (else the line ended).
fn parse_pattern(
    delim: char,
    chars: &mut std::str::Chars<'_>,
) -> Result<(String, bool), SubstituteError> {
    let mut out = String::new();
    let mut first = true;
    while let Some(c) = chars.next() {
        if c == delim {
            return Ok((out, true));
        }
        match c {
            '\\' => match chars.next() {
                Some(e) if e == delim => out.push(e),
                Some(e @ ('\\' | '.' | '*' | '[' | ']' | '~' | '^' | '$' | '/')) => out.push(e),
                Some(e) => return Err(SubstituteError::Pattern(format!("\\{e}"))),
                None => return Err(SubstituteError::Pattern("\\".into())),
            },
            '.' | '*' | '[' | '~' => return Err(SubstituteError::Pattern(c.to_string())),
            // `^` anchors only at the start of a Vim pattern; elsewhere it is literal.
            '^' if first => return Err(SubstituteError::Pattern(c.to_string())),
            // `$` anchors only at the end of the pattern.
            '$' => {
                let rest = chars.as_str();
                if rest.is_empty() || rest.starts_with(delim) {
                    return Err(SubstituteError::Pattern(c.to_string()));
                }
                out.push(c);
            }
            other => out.push(other),
        }
        first = false;
    }
    Ok((out, false))
}

/// The literal replacement up to the next unescaped `delim`, with `&` as the
/// matched text (`pattern`), and whether that delimiter was found.
fn parse_replacement(
    delim: char,
    pattern: &str,
    chars: &mut std::str::Chars<'_>,
) -> Result<(String, bool), SubstituteError> {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == delim {
            return Ok((out, true));
        }
        match c {
            '\\' => match chars.next() {
                Some(e) if e == delim => out.push(e),
                Some(e @ ('\\' | '&' | '~' | '/')) => out.push(e),
                // Vim: `\r` splits the line; `\t` is a tab.
                Some('r') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(e) => return Err(SubstituteError::Replacement(format!("\\{e}"))),
                None => return Err(SubstituteError::Replacement("\\".into())),
            },
            '&' => out.push_str(pattern),
            '~' => return Err(SubstituteError::Replacement(c.to_string())),
            other => out.push(other),
        }
    }
    Ok((out, false))
}

/// The `[start, end)` character ranges `sub` replaces in `text`, ascending and
/// non-overlapping: the first occurrence on each covered line, or all of them
/// with `g`. `cursor_line` (0-based) is the line for [`Range::CurrentLine`].
///
/// # Examples
///
/// ```
/// use vix_find_panel::substitute::{parse, plan};
///
/// let sub = parse("%s/ab/x/").unwrap().unwrap();
/// assert_eq!(plan("ab ab\nab\n", &sub, 0), [(0, 2), (6, 8)]);
/// ```
#[must_use]
pub fn plan(text: &str, sub: &Substitute, cursor_line: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let pat_chars = sub.pattern.chars().count();
    let mut line_start = 0usize;
    for (n, line) in text.split('\n').enumerate() {
        if sub.range == Range::WholeBuffer || n == cursor_line {
            for (byte, _) in line.match_indices(sub.pattern.as_str()) {
                let Some(start) = line
                    .get(..byte)
                    .and_then(|p| line_start.checked_add(p.chars().count()))
                else {
                    break;
                };
                let Some(end) = start.checked_add(pat_chars) else {
                    break;
                };
                out.push((start, end));
                if !sub.global {
                    break;
                }
            }
        }
        // The next line starts past this one and its newline.
        let Some(next) = line_start
            .checked_add(line.chars().count())
            .and_then(|s| s.checked_add(1))
        else {
            break;
        };
        line_start = next;
    }
    out
}

/// `text` with every range in `ranges` (ascending, non-overlapping, non-empty
/// character ranges, as [`plan`] returns) replaced by `replacement`.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_find_panel::substitute::apply("aXbX", &[(1, 2), (3, 4)], "yy"), "ayybyy");
/// ```
#[must_use]
pub fn apply(text: &str, ranges: &[(usize, usize)], replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = ranges.iter().copied().peekable();
    for (i, c) in text.chars().enumerate() {
        // Drop ranges that end at or before this character.
        while pending.peek().is_some_and(|&(_, e)| e <= i) {
            pending.next();
        }
        match pending.peek().copied() {
            Some((s, _)) if i == s => out.push_str(replacement),
            Some((s, e)) if i > s && i < e => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(cmd: &str) -> Substitute {
        parse(cmd).unwrap().unwrap()
    }

    fn err(cmd: &str) -> SubstituteError {
        parse(cmd).unwrap().unwrap_err()
    }

    #[test]
    fn non_substitute_commands_are_not_claimed() {
        for cmd in [
            "", "w", "q!", "qa", "3", "e foo", "set", "Ex", "cn", "cope", "sx/a/b/",
        ] {
            assert_eq!(parse(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn names_ranges_and_flags() {
        let s = ok("s/a/b/");
        assert_eq!(
            (s.range, s.global, s.confirm),
            (Range::CurrentLine, false, false)
        );
        let s = ok("%substitute/a/b/gc");
        assert_eq!(
            (s.range, s.global, s.confirm),
            (Range::WholeBuffer, true, true)
        );
        assert!(ok("sub/a/b/cg").global);
        assert!(
            !ok("s/a/b/gg").global,
            "a repeated g toggles off, as in Vim"
        );
        assert!(ok("s/a/b/g  ").global, "trailing blanks are ignored");
        assert_eq!(ok("s/a/b").replacement, "b");
        assert_eq!(ok("s/a").replacement, "", "no replacement deletes");
        assert_eq!(ok("s/a/").replacement, "");
    }

    #[test]
    fn other_ranges_delimiters_and_flags_are_refused() {
        assert_eq!(err("1,5s/a/b/"), SubstituteError::Range("1,5".into()));
        assert_eq!(err(".s/a/b/"), SubstituteError::Range(".".into()));
        assert_eq!(err("'a,'bs/a/b/"), SubstituteError::Range("'a,'b".into()));
        assert_eq!(err("s"), SubstituteError::EmptyPattern);
        assert_eq!(err("s//b/"), SubstituteError::EmptyPattern);
        assert_eq!(err("s|a|b|"), SubstituteError::Delimiter('|'));
        assert_eq!(err("s\"a\"b\""), SubstituteError::Delimiter('"'));
        assert_eq!(err("s a b "), SubstituteError::Delimiter(' '));
        assert_eq!(err("s/a/b/i"), SubstituteError::Flag('i'));
        assert_eq!(err("s/a/b/g3"), SubstituteError::Flag('3'));
        assert_eq!(err("s/a/b/&"), SubstituteError::Flag('&'));
    }

    #[test]
    fn escaped_and_alternate_delimiters() {
        assert_eq!(ok(r"s/a\/b/c\/d/").pattern, "a/b");
        assert_eq!(ok(r"s/a\/b/c\/d/").replacement, "c/d");
        let s = ok("s#a/b#c/d#g");
        assert_eq!(
            (s.pattern.as_str(), s.replacement.as_str(), s.global),
            ("a/b", "c/d", true)
        );
        assert_eq!(ok(r"s#a\#b#x#").pattern, "a#b");
        assert_eq!(ok(r"s,a\,b,x,").pattern, "a,b");
    }

    #[test]
    fn magic_pattern_characters_are_refused_unless_escaped() {
        for (cmd, token) in [
            ("s/a.b/x/", "."),
            ("s/a*/x/", "*"),
            ("s/[ab]/x/", "["),
            ("s/~/x/", "~"),
            ("s/^a/x/", "^"),
            ("s/a$/x/", "$"),
            ("s/a$", "$"),
            (r"s/\(a\)/x/", r"\("),
            (r"s/\<a/x/", r"\<"),
            (r"s/a\|b/x/", r"\|"),
            (r"s/\v(a)/x/", r"\v"),
            (r"s/a\n/x/", r"\n"),
            (r"s/a\", r"\"),
        ] {
            assert_eq!(err(cmd), SubstituteError::Pattern(token.into()), "{cmd}");
        }
        assert_eq!(ok(r"s/a\.b\*\[\]\~\^\$/x/").pattern, "a.b*[]~^$");
        assert_eq!(
            ok("s/a^b$c]/x/").pattern,
            "a^b$c]",
            "^ and $ mid-pattern are literal"
        );
        assert_eq!(ok(r"s/a\\b/x/").pattern, r"a\b");
    }

    #[test]
    fn replacement_escapes_and_ampersand() {
        assert_eq!(ok("s/ab/[&]/").replacement, "[ab]");
        assert_eq!(ok(r"s/ab/\&/").replacement, "&");
        assert_eq!(ok(r"s/ab/x\ry\tz\\/").replacement, "x\ny\tz\\");
        assert_eq!(ok(r"s/ab/\~/").replacement, "~");
        assert_eq!(
            ok("s/ab/$1/").replacement,
            "$1",
            "no capture groups: literal"
        );
        assert_eq!(err("s/ab/~/"), SubstituteError::Replacement("~".into()));
        assert_eq!(err(r"s/ab/\1/"), SubstituteError::Replacement(r"\1".into()));
        assert_eq!(
            err(r"s/ab/\u&/"),
            SubstituteError::Replacement(r"\u".into())
        );
        assert_eq!(err(r"s/ab/x\"), SubstituteError::Replacement(r"\".into()));
    }

    #[test]
    fn plan_covers_the_cursor_line_or_every_line() {
        let text = "a a\nb a a\na\n";
        assert_eq!(plan(text, &ok("s/a/x/"), 1), [(6, 7)]);
        assert_eq!(plan(text, &ok("s/a/x/g"), 1), [(6, 7), (8, 9)]);
        assert_eq!(plan(text, &ok("%s/a/x/"), 0), [(0, 1), (6, 7), (10, 11)]);
        assert_eq!(plan(text, &ok("%s/a/x/g"), 0).len(), 5);
        assert_eq!(plan(text, &ok("s/zz/x/"), 0), Vec::<(usize, usize)>::new());
        assert_eq!(
            plan(text, &ok("s/a/x/"), 9),
            Vec::<(usize, usize)>::new(),
            "past the last line"
        );
        // Multi-byte text: character offsets, not bytes.
        assert_eq!(plan("é a\né", &ok("%s/é/e/g"), 0), [(0, 1), (4, 5)]);
        // Non-overlapping, like Vim: "aaa" holds one "aa".
        assert_eq!(plan("aaa", &ok("s/aa/x/g"), 0), [(0, 2)]);
    }

    #[test]
    fn apply_rewrites_each_range() {
        let text = "a a\nb a a\na\n";
        let sub = ok("%s/a/xy/g");
        assert_eq!(
            apply(text, &plan(text, &sub, 0), &sub.replacement),
            "xy xy\nb xy xy\nxy\n"
        );
        assert_eq!(apply("abc", &[], "z"), "abc");
        assert_eq!(apply("abc", &[(0, 3)], ""), "");
        assert_eq!(apply("é-é", &[(2, 3)], "e"), "é-e");
        assert_eq!(apply("abab", &[(0, 2), (2, 4)], "x"), "xx");
        assert_eq!(err("s/a/b/q").to_string(), "unsupported flag 'q'");
    }
}
