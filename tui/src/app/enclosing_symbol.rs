//! The declaration enclosing a line, for the sticky-scroll header and the
//! breadcrumb (T858). Both used to scan the whole buffer for declarations
//! after every edit (a full copy of the text plus a regex over every line: on
//! a 64 MiB file 0.4 s and 64 MiB per keystroke); now each scans up from its
//! line to the nearest declaration and remembers what it saw, keeping that
//! knowledge across an edit for the lines above it
//! ([`vix_editor_core::code::Code::changed_since`]).

use std::borrow::Cow;
use std::path::PathBuf;

use vix_editor_core::code::Code;

use crate::palette::{Symbol, symbol_in_line};

/// What one scan learned about a buffer: at `revision` of the tab at index
/// `tab` (with `path`), line `found` (0-based) holds a declaration (or,
/// with `found` `None`, no line from the top does) and no line after it up
/// to `upto` does.
pub(crate) struct SymbolScan {
    tab: usize,
    path: Option<PathBuf>,
    revision: u64,
    upto: usize,
    found: Option<(usize, Symbol)>,
}

/// The last declaration at or above 0-based line `at` of `code` (the tab at
/// index `tab`, with `path`): what `palette::symbols(&text)` would give as
/// its last symbol with a line number of `at + 1` or less, found by reading
/// lines up from `at` instead of the whole buffer. `memo` keeps what the
/// scan learned for the next call. Lines are the editor's own (the rope's).
pub(crate) fn enclosing_symbol(
    code: &Code,
    tab: usize,
    path: Option<&PathBuf>,
    at: usize,
    memo: &mut Option<SymbolScan>,
) -> Option<Symbol> {
    let at = code
        .len_lines()
        .checked_sub(1)
        .map_or(0, |last| at.min(last));
    // What the last scan knows that still holds: the lines above the first
    // one an edit since then touched are the same lines, at the same index.
    let known = memo.take().and_then(|m| {
        if m.tab != tab || m.path.as_ref() != path {
            return None;
        }
        let upto = if m.revision == code.revision() {
            m.upto
        } else {
            let changed = code.changed_since(m.revision)?;
            let first = code.char_to_line(changed.start.min(code.len_chars()));
            m.upto.min(first.checked_sub(1)?)
        };
        match &m.found {
            Some((line, _)) if *line > upto => None,
            _ => Some((upto, m.found)),
        }
    });
    let mut line = at;
    let (found, upto) = loop {
        if let Some((upto, found)) = &known
            && line <= *upto
            && found.as_ref().is_none_or(|(f, _)| line >= *f)
        {
            break (found.clone(), (*upto).max(at));
        }
        if let Some(symbol) = symbol_at(code, line) {
            break (Some((line, symbol)), at);
        }
        let Some(up) = line.checked_sub(1) else {
            break (None, at);
        };
        line = up;
    };
    let symbol = found.as_ref().map(|(_, s)| s.clone());
    *memo = Some(SymbolScan {
        tab,
        path: path.cloned(),
        revision: code.revision(),
        upto,
        found,
    });
    symbol
}

/// The declaration on 0-based line `line`, if it holds one.
fn symbol_at(code: &Code, line: usize) -> Option<Symbol> {
    let text: Cow<'_, str> = code.line(line).into();
    symbol_in_line(strip_line_break(&text), line.checked_add(1)?)
}

/// `text` without the one line break it ends with, if any (the rope's line
/// breaks: `\r\n` or one of `\n`, `\r`, VT, FF, NEL, LS, PS).
fn strip_line_break(text: &str) -> &str {
    if let Some(t) = text.strip_suffix("\r\n") {
        return t;
    }
    text.strip_suffix([
        '\n', '\r', '\u{0B}', '\u{0C}', '\u{85}', '\u{2028}', '\u{2029}',
    ])
    .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]
    use super::*;

    /// The old answer: the whole buffer's symbols, the last at or above.
    fn oracle(text: &str, at: usize) -> Option<String> {
        crate::palette::symbols(text)
            .into_iter()
            .rev()
            .find(|s| s.line <= at + 1)
            .map(|s| format!("{}:{}:{}", s.line, s.name, s.text))
    }

    fn show(s: Option<Symbol>) -> Option<String> {
        s.map(|s| format!("{}:{}:{}", s.line, s.name, s.text))
    }

    /// Every line of a few buffers, asked in an order that reuses and
    /// extends what the memo knows, matches the whole-buffer scan.
    #[test]
    fn matches_the_whole_buffer_scan_on_every_line() {
        let texts = [
            String::new(),
            "no declarations\nat all\n".to_string(),
            "fn a() {\n  x\n}\nstruct B;\n\n  impl B {\n    fn c() {}\n  }\n".to_string(),
            "a\r\nfn b()\r\n  c\r\n#define D 1\r\ne".to_string(),
            format!("{}fn mid() {{}}\n{}", "x\n".repeat(50), "y\n".repeat(50)),
        ];
        for text in &texts {
            let code = Code::new(text, "", None).unwrap();
            let lines = code.len_lines();
            let mut memo = None;
            let order: Vec<usize> = (0..lines)
                .chain((0..lines).rev())
                .chain([lines / 2, 0, lines - 1])
                .collect();
            for at in order {
                let got = show(enclosing_symbol(&code, 0, None, at, &mut memo));
                assert_eq!(got, oracle(text, at), "{text:?} at {at}");
            }
        }
    }

    /// After edits above, below and on the remembered lines, the answer
    /// is still the whole-buffer scan's.
    #[test]
    fn stays_right_across_edits() {
        let text = format!(
            "fn top() {{}}\n{}fn low() {{}}\n{}",
            "x\n".repeat(30),
            "y\n".repeat(30)
        );
        let mut code = Code::new(&text, "", None).unwrap();
        let mut memo = None;
        let end = code.len_chars() - 2;
        // (offset, text inserted, characters removed first)
        let edits: [(usize, &str, usize); 7] = [
            (40, "z", 0),
            (5, "fn new() {}\n", 0),
            (0, "", 3),
            (end, "\nfn end() {}\n", 0),
            (20, "struct S;\n", 0),
            (21, "", 4),
            (30, "", 40),
        ];
        for (i, (at, insert, remove)) in edits.into_iter().enumerate() {
            for line in [10, 35, 60, 31, 0, 61, 11] {
                let now = code.get_content();
                let got = show(enclosing_symbol(&code, 0, None, line, &mut memo));
                let last = code.len_lines() - 1;
                assert_eq!(got, oracle(&now, line.min(last)), "edit {i}, line {line}");
            }
            let at = at.min(code.len_chars());
            if remove > 0 {
                code.remove(at, (at + remove).min(code.len_chars()));
            }
            code.insert(at, insert);
        }
    }

    /// Another tab, or a memo too old to bring forward, starts over.
    #[test]
    fn another_tab_or_lost_history_starts_over() {
        let a = Code::new("fn a() {}\nx\n", "", None).unwrap();
        let b = Code::new("fn b() {}\nx\n", "", None).unwrap();
        let mut memo = None;
        let name = |s: Option<Symbol>| s.map(|s| s.name);
        assert_eq!(
            name(enclosing_symbol(&a, 0, None, 1, &mut memo)).as_deref(),
            Some("a")
        );
        assert_eq!(
            name(enclosing_symbol(&b, 1, None, 1, &mut memo)).as_deref(),
            Some("b")
        );
        let mut c = Code::new("fn c() {}\nx\n", "", None).unwrap();
        assert_eq!(
            name(enclosing_symbol(&c, 2, None, 1, &mut memo)).as_deref(),
            Some("c")
        );
        for _ in 0..300 {
            c.insert(0, "y");
            c.remove(0, 1);
        }
        c.remove(0, 3);
        assert!(enclosing_symbol(&c, 2, None, 1, &mut memo).is_none());
    }
}
