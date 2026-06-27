//! A small library of reusable text snippets, plus the picker's selection state.
//!
//! Vix's Tools → Snippets… picker lists each snippet by name; choosing one
//! inserts its body at the cursor. The set is curated and language-agnostic; the
//! host owns insertion.

#![warn(clippy::pedantic)]

/// One named, insertable snippet.
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
    Snippet { name: "Bash shebang", body: "#!/usr/bin/env bash\nset -euo pipefail\n\n$0" },
    Snippet {
        name: "HTML5 boilerplate",
        body: "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>${1:Title}</title>\n</head>\n<body>\n  $0\n</body>\n</html>\n",
    },
    Snippet {
        name: "MIT license header",
        body: "SPDX-License-Identifier: MIT\nCopyright (c) ${1:2026} ${2:Your Name}\n$0",
    },
    Snippet { name: "TODO comment", body: "TODO: $0" },
    Snippet { name: "FIXME comment", body: "FIXME: $0" },
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

/// Parse a snippet body, extracting `$N` / `${N}` / `${N:placeholder}` tabstops.
/// `\$` is a literal dollar sign. Stops are returned in navigation order
/// (`$1`, `$2`, …, then `$0`).
#[must_use]
pub fn parse(body: &str) -> Parsed {
    let chars: Vec<char> = body.chars().collect();
    let mut text = String::new();
    let mut len = 0usize; // char count pushed to `text`
    let mut stops: Vec<Stop> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1) == Some(&'$') {
            text.push('$');
            len += 1;
            i += 2;
            continue;
        }
        if c == '$' {
            // Bare `$N`.
            if chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                let (num, j) = read_number(&chars, i + 1);
                stops.push(Stop { num, start: len, end: len });
                i = j;
                continue;
            }
            // `${N}` or `${N:placeholder}`.
            if chars.get(i + 1) == Some(&'{') && chars.get(i + 2).is_some_and(char::is_ascii_digit) {
                let (num, mut j) = read_number(&chars, i + 2);
                let start = len;
                if chars.get(j) == Some(&':') {
                    j += 1;
                    while j < chars.len() && chars[j] != '}' {
                        text.push(chars[j]);
                        len += 1;
                        j += 1;
                    }
                }
                if chars.get(j) == Some(&'}') {
                    j += 1;
                    stops.push(Stop { num, start, end: len });
                    i = j;
                    continue;
                }
            }
        }
        text.push(c);
        len += 1;
        i += 1;
    }
    stops.sort_by_key(|s| if s.num == 0 { u32::MAX } else { s.num });
    Parsed { text, stops }
}

/// Read a run of ASCII digits starting at `from`, returning `(value, next_index)`.
fn read_number(chars: &[char], from: usize) -> (u32, usize) {
    let mut j = from;
    let mut num = 0u32;
    while j < chars.len() && chars[j].is_ascii_digit() {
        num = num.saturating_mul(10).saturating_add(u32::from(chars[j] as u8 - b'0'));
        j += 1;
    }
    (num, j)
}

/// Selection state for the Snippets picker.
#[derive(Default)]
pub struct Picker {
    /// Highlighted row.
    pub selected: usize,
}

impl Picker {
    /// A picker with the first snippet highlighted.
    #[must_use]
    pub fn new() -> Self {
        Picker { selected: 0 }
    }

    /// Number of snippets.
    #[must_use]
    pub fn len(&self) -> usize {
        SNIPPETS.len()
    }

    /// Whether there are no snippets (there always are, but clippy asks).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        SNIPPETS.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < SNIPPETS.len() {
            self.selected += 1;
        }
    }

    /// Select a row directly (e.g. from a click); returns whether it was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < SNIPPETS.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// The highlighted snippet's body, ready to insert.
    #[must_use]
    pub fn selected_body(&self) -> &'static str {
        SNIPPETS.get(self.selected).map_or("", |s| s.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_navigates_and_yields_body() {
        let mut p = Picker::new();
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_body(), SNIPPETS[0].body);
        p.down();
        assert_eq!(p.selected, 1);
        p.up();
        p.up();
        assert_eq!(p.selected, 0, "saturates at the top");
    }

    #[test]
    fn down_saturates_at_the_end() {
        let mut p = Picker::new();
        for _ in 0..100 {
            p.down();
        }
        assert_eq!(p.selected, SNIPPETS.len() - 1);
        assert_eq!(p.selected_body(), SNIPPETS[SNIPPETS.len() - 1].body);
    }

    #[test]
    fn select_index_bounds() {
        let mut p = Picker::new();
        assert!(p.select_index(2));
        assert_eq!(p.selected, 2);
        assert!(!p.select_index(9999));
    }

    #[test]
    fn parse_extracts_ordered_tabstops() {
        let p = parse("fn ${1:name}($2) {\n    $0\n}");
        assert_eq!(p.text, "fn name() {\n    \n}");
        // Navigation order: $1, $2, then $0 last.
        assert_eq!(p.stops.iter().map(|s| s.num).collect::<Vec<_>>(), vec![1, 2, 0]);
        // $1 placeholder "name" spans chars 3..7.
        assert_eq!((p.stops[0].start, p.stops[0].end), (3, 7));
        // $2 is empty (start == end).
        assert_eq!(p.stops[1].start, p.stops[1].end);
    }

    #[test]
    fn parse_handles_escaped_dollar_and_plain_text() {
        let p = parse("cost is \\$5");
        assert_eq!(p.text, "cost is $5");
        assert!(p.stops.is_empty());
    }

    #[test]
    fn parse_plain_snippet_has_no_stops() {
        let p = parse("plain text\n");
        assert_eq!(p.text, "plain text\n");
        assert!(p.stops.is_empty());
    }
}
