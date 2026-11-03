//! A small library of reusable text snippets, plus the picker's selection state.
//!
//! Vix's Tools → Snippets… picker lists each snippet by name; choosing one
//! inserts its body at the cursor. The set is curated and language-agnostic; the
//! host owns insertion.
//!
//! The crate has two halves:
//!
//! - the **catalogue** ([`SNIPPETS`]): static, compiled-in snippets, and
//! - the **parser** ([`parse`], [`parse_lsp`], [`parse_with`]): a small
//!   recursive-descent implementation of the LSP/VS Code snippet grammar that
//!   turns a body into literal text plus ordered tabstops ([`Parsed`]). The same
//!   parser serves user snippets (`vix-snippets`) and LSP completions (T613).
//!
//! Parsing is total: every input yields a `Parsed`, malformed constructs degrade
//! to literal text, and nesting depth is capped (`MAX_DEPTH` levels) so
//! untrusted LSP text cannot overflow the stack. All offsets are **char**
//! offsets, matching the editor buffer. Spec: `crates/vix-snippet-tool/spec/index.md`.
//!
//! # Examples
//! ```
//! let p = vix_snippet_tool::parse("fn ${1:name}() {\n    $0\n}");
//! assert_eq!(p.text, "fn name() {\n    \n}");
//! // Navigation order: $1 first, the final stop $0 last.
//! assert_eq!(p.stops.iter().map(|s| s.num).collect::<Vec<_>>(), vec![1, 0]);
//! ```

/// One named, insertable snippet (a compiled-in entry of [`SNIPPETS`]).
pub struct Snippet {
    /// Display name shown in the picker.
    pub name: &'static str,
    /// Text inserted at the cursor when chosen.
    pub body: &'static str,
}

/// The bundled snippets, in display order. Bodies may contain **tabstops** —
/// `$1`, `$2`, …, `$0` (final), and `${1:placeholder}` — which [`parse`] turns
/// into navigable fields (Tab jumps between them).
pub static SNIPPETS: &[Snippet] = &[
    Snippet {
        name: "Bash shebang",
        body: "#!/usr/bin/env bash\nset -euo pipefail\n\n$0",
    },
    Snippet {
        name: "HTML5 boilerplate",
        body: "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>${1:Title}</title>\n</head>\n<body>\n  $0\n</body>\n</html>\n",
    },
    Snippet {
        name: "MIT license header",
        body: "SPDX-License-Identifier: MIT\nCopyright (c) ${1:2026} ${2:Your Name}\n$0",
    },
    Snippet {
        name: "TODO comment",
        body: "TODO: $0",
    },
    Snippet {
        name: "FIXME comment",
        body: "FIXME: $0",
    },
    Snippet {
        name: "Markdown link",
        body: "[${1:text}](${2:https://example.com})$0",
    },
    Snippet {
        name: "Markdown table",
        body: "| ${1:Column A} | ${2:Column B} |\n| -------- | -------- |\n| $3 | $4 |\n$0",
    },
    Snippet {
        name: "Rust function",
        body: "fn ${1:name}(${2}) -> ${3:()} {\n    $0\n}\n",
    },
    Snippet {
        name: "Lorem ipsum",
        body: "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n",
    },
];

/// One tabstop in a parsed snippet: its number (`0` is the final stop) and the
/// char range `[start, end)` of its placeholder text within the parsed body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop {
    /// Tabstop number (`$1` → 1, `$0` → 0).
    pub num: u32,
    /// Start char offset of the placeholder in the parsed text.
    pub start: usize,
    /// End char offset (equals `start` for an empty tabstop).
    pub end: usize,
}

/// A snippet body with its tabstops resolved: the literal `text` to insert and
/// the ordered `stops` (ascending by number, with `$0` last).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Literal text to insert (tabstop markers removed, placeholders kept).
    pub text: String,
    /// Navigation-ordered tabstops, offsets relative to `text`.
    pub stops: Vec<Stop>,
}

/// Which snippet grammar [`parse_with`] accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Dialect {
    /// Bundled and user snippet bodies: tabstops, (nested) placeholders,
    /// choices, and escapes. `$NAME` is literal text here, so a body like a
    /// shell `$HOME` survives unchanged.
    #[default]
    Tabstops,
    /// An LSP `InsertTextFormat.Snippet` body (T613): everything
    /// [`Dialect::Tabstops`] accepts, plus variables (`$NAME`, `${NAME}`,
    /// `${NAME:default}`, `${NAME/regex/format/}`), each resolved to its
    /// default text, or to nothing.
    Lsp,
}

/// Maximum placeholder nesting depth followed; anything deeper is literal text, so
/// a hostile body cannot exhaust the stack.
const MAX_DEPTH: usize = 32;

/// Parse a snippet body, extracting `$N` / `${N}` / `${N:placeholder}` tabstops.
/// `\$` is a literal dollar sign. Stops are returned in navigation order
/// (`$1`, `$2`, …, then `$0`). Shorthand for [`parse_with`] with
/// [`Dialect::Tabstops`].
#[must_use]
pub fn parse(body: &str) -> Parsed {
    parse_with(body, Dialect::Tabstops)
}

/// Parse an LSP snippet body (`insertTextFormat` 2) — [`parse_with`] with
/// [`Dialect::Lsp`].
#[must_use]
pub fn parse_lsp(body: &str) -> Parsed {
    parse_with(body, Dialect::Lsp)
}

/// Parse a snippet body in `dialect`, following the LSP/VS Code snippet
/// grammar:
///
/// - `$N`, `${N}` — an empty tabstop; `${N:placeholder}` — one pre-filled
///   with `placeholder`, which may itself hold tabstops (nesting);
/// - `${N|one,two|}` — a choice, inserted as its first option;
/// - `\$`, `\}`, `\\` — literal `$`, `}`, `\` (any other `\` is literal);
/// - a malformed construct (say an unclosed `${1:`) is literal text.
///
/// A number used twice (a mirror) is navigated once, at its first
/// occurrence in *closing* order (so for nested placeholders an inner stop wins
/// over a same-numbered outer one, as it closes first). Stops come back in
/// navigation order: `$1`, `$2`, …, then `$0`.
///
/// In [`Dialect::Lsp`] variables resolve to their default text or to nothing;
/// `${NAME/regex/format/}` transforms are skipped rather than evaluated.
#[must_use]
pub fn parse_with(body: &str, dialect: Dialect) -> Parsed {
    let chars: Vec<char> = body.chars().collect();
    let mut p = Parser {
        chars: &chars,
        dialect,
        text: String::new(),
        len: 0,
        stops: Vec::new(),
    };
    p.seq(0, 0);
    let mut stops: Vec<Stop> = Vec::new();
    for s in p.stops {
        if !stops.iter().any(|t| t.num == s.num) {
            stops.push(s);
        }
    }
    // Inner (nested) stops are recorded before their parent, but number
    // order decides navigation.
    stops.sort_by_key(|s| if s.num == 0 { u32::MAX } else { s.num });
    Parsed {
        text: p.text,
        stops,
    }
}

/// Recursive-descent state for [`parse_with`]. Works on a `Vec<char>` snapshot
/// of the body so all indices and recorded offsets are char-based.
struct Parser<'a> {
    /// The body being parsed.
    chars: &'a [char],
    /// Which grammar applies.
    dialect: Dialect,
    /// Literal output so far.
    text: String,
    /// `text`'s length in chars.
    len: usize,
    /// Stops in the order they closed.
    stops: Vec<Stop>,
}

impl Parser<'_> {
    /// The char at `i`, if any.
    fn at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    /// Append one literal char.
    fn push(&mut self, c: char) {
        self.text.push(c);
        self.len += 1;
    }

    /// Parse elements from `i` until the end, or — inside a placeholder
    /// (`depth > 0`) — until an unescaped `}`. Returns the index it stopped
    /// at.
    fn seq(&mut self, mut i: usize, depth: usize) -> usize {
        while let Some(c) = self.at(i) {
            if depth > 0 && c == '}' {
                return i;
            }
            i = match c {
                '\\' => {
                    if let Some(e @ ('$' | '}' | '\\')) = self.at(i + 1) {
                        self.push(e);
                        i + 2
                    } else {
                        self.push('\\');
                        i + 1
                    }
                }
                '$' => self.dollar(i, depth).unwrap_or_else(|| {
                    self.push('$');
                    i + 1
                }),
                _ => {
                    self.push(c);
                    i + 1
                }
            };
        }
        i
    }

    /// Parse a `$` construct at `i`, returning the index after it — or
    /// `None`, with any partial output rolled back, when it is malformed.
    fn dollar(&mut self, i: usize, depth: usize) -> Option<usize> {
        let saved = (self.text.len(), self.len, self.stops.len());
        let out = self.dollar_inner(i, depth);
        if out.is_none() {
            self.text.truncate(saved.0);
            self.len = saved.1;
            self.stops.truncate(saved.2);
        }
        out
    }

    /// [`Parser::dollar`] without the rollback.
    fn dollar_inner(&mut self, i: usize, depth: usize) -> Option<usize> {
        let lsp = self.dialect == Dialect::Lsp;
        // A trailing `$` (no next char) is malformed -> `None` -> literal `$`.
        let next = self.at(i + 1)?;
        if next.is_ascii_digit() {
            let (num, j) = self.number(i + 1);
            self.stop(num, self.len);
            return Some(j);
        }
        if lsp && is_ident_start(next) {
            return Some(self.ident_end(i + 1));
        }
        // Past MAX_DEPTH a `${` is treated as literal text rather than recursing.
        if next != '{' || depth >= MAX_DEPTH {
            return None;
        }
        let first = self.at(i + 2)?;
        if first.is_ascii_digit() {
            let (num, j) = self.number(i + 2);
            let start = self.len;
            let end = match self.at(j)? {
                '}' => j + 1,
                ':' => {
                    let k = self.seq(j + 1, depth + 1);
                    self.closed(k)?
                }
                '|' => self.choice(j + 1)?,
                _ => return None,
            };
            self.stop(num, start);
            return Some(end);
        }
        if !(lsp && is_ident_start(first)) {
            return None;
        }
        let j = self.ident_end(i + 2);
        match self.at(j)? {
            '}' => Some(j + 1),
            ':' => {
                let k = self.seq(j + 1, depth + 1);
                self.closed(k)
            }
            // A transform: nothing to transform, so it resolves to nothing.
            '/' => self.closed(self.skip_to_brace(j + 1)),
            _ => None,
        }
    }

    /// `Some(k + 1)` when `k` is a closing `}`.
    fn closed(&self, k: usize) -> Option<usize> {
        (self.at(k) == Some('}')).then_some(k + 1)
    }

    /// Record a stop numbered `num` spanning `start..` the current end.
    fn stop(&mut self, num: u32, start: usize) {
        self.stops.push(Stop {
            num,
            start,
            end: self.len,
        });
    }

    /// Parse a choice's options from `i` (just past the opening `|`), insert
    /// the first, and return the index after the closing `|}`.
    fn choice(&mut self, mut i: usize) -> Option<usize> {
        let mut first = true;
        loop {
            match self.at(i)? {
                '\\' if matches!(self.at(i + 1), Some('$' | '}' | '\\' | ',' | '|')) => {
                    if first {
                        let e = self.at(i + 1)?;
                        self.push(e);
                    }
                    i += 2;
                }
                ',' => {
                    first = false;
                    i += 1;
                }
                '|' => return self.closed(i + 1),
                c => {
                    if first {
                        self.push(c);
                    }
                    i += 1;
                }
            }
        }
    }

    /// The index of the next unescaped `}` from `i` (or the end).
    fn skip_to_brace(&self, mut i: usize) -> usize {
        while let Some(c) = self.at(i) {
            match c {
                '\\' => i += 2,
                '}' => return i,
                _ => i += 1,
            }
        }
        i
    }

    /// Read a run of ASCII digits from `from`: `(value, next_index)`.
    fn number(&self, from: usize) -> (u32, usize) {
        let mut j = from;
        let mut num = 0u32;
        while let Some(d) = self.at(j).and_then(|c| c.to_digit(10)) {
            num = num.saturating_mul(10).saturating_add(d);
            j += 1;
        }
        (num, j)
    }

    /// The index just past a variable name starting at `from`.
    fn ident_end(&self, from: usize) -> usize {
        let mut j = from;
        while self
            .at(j)
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            j += 1;
        }
        j
    }
}

/// Whether `c` can begin a snippet variable name (`[A-Za-z_]`).
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_ordered_tabstops() {
        let p = parse("fn ${1:name}($2) {\n    $0\n}");
        assert_eq!(p.text, "fn name() {\n    \n}");
        // Navigation order: $1, $2, then $0 last.
        assert_eq!(
            p.stops.iter().map(|s| s.num).collect::<Vec<_>>(),
            vec![1, 2, 0]
        );
        // $1 placeholder "name" spans chars 3..7.
        assert_eq!((p.stops[0].start, p.stops[0].end), (3, 7));
        // $2 is empty (start == end).
        assert_eq!(p.stops[1].start, p.stops[1].end);
    }

    #[test]
    fn parse_handles_escaped_dollar_and_plain_text() {
        let p = parse("cost is \\$5");
        assert_eq!(p.text, "cost is $5");
        assert_eq!(p.stops, [] as [Stop; 0]);
    }

    #[test]
    fn parse_plain_snippet_has_no_stops() {
        let p = parse("plain text\n");
        assert_eq!(p.text, "plain text\n");
        assert_eq!(p.stops, [] as [Stop; 0]);
    }

    fn spans(p: &Parsed) -> Vec<(u32, usize, usize)> {
        p.stops.iter().map(|s| (s.num, s.start, s.end)).collect()
    }

    #[test]
    fn nested_placeholders_keep_both_stops() {
        let p = parse("f(${1:a, ${2:b}})$0");
        assert_eq!(p.text, "f(a, b)");
        // $1 covers "a, b" (2..6), the nested $2 covers "b" (5..6).
        assert_eq!(spans(&p), [(1, 2, 6), (2, 5, 6), (0, 7, 7)]);
    }

    #[test]
    fn choices_insert_their_first_option() {
        let p = parse("${1|one,t\\,wo|} x");
        assert_eq!(p.text, "one x");
        assert_eq!(spans(&p), [(1, 0, 3)]);
    }

    #[test]
    fn escapes_and_malformed_constructs_are_literal() {
        assert_eq!(parse("a\\}b\\\\c\\n").text, "a}b\\c\\n");
        let p = parse("x ${1:open");
        assert_eq!(p.text, "x ${1:open");
        assert_eq!(p.stops, [] as [Stop; 0]);
        // The documented `${1:{}}` idiom: placeholder `{`, then a literal `}`.
        assert_eq!(parse("${1:{}}").text, "{}");
    }

    #[test]
    fn a_mirrored_number_is_navigated_once() {
        let p = parse("${1:x} = $1;$0");
        assert_eq!(p.text, "x = ;");
        assert_eq!(spans(&p), [(1, 0, 1), (0, 5, 5)]);
    }

    #[test]
    fn variables_are_literal_except_in_the_lsp_dialect() {
        assert_eq!(parse("echo $HOME ${HOME}").text, "echo $HOME ${HOME}");
        let p = parse_lsp("$TM_FILENAME|${TM_X:${1:dflt}}|${V/a/b/g}|${2}");
        assert_eq!(p.text, "|dflt||");
        assert_eq!(spans(&p), [(1, 1, 5), (2, 7, 7)]);
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let body = "${1:".repeat(10_000) + &"}".repeat(10_000);
        let p = parse(&body);
        assert!(p.text.len() < body.len(), "outer levels still parse");
    }
}
