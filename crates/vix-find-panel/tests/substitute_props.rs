//! Property tests for the literal Ex substitute (T831, over T841's
//! `parse` / `plan` / `apply`): escaping any text into a command parses
//! back to that text, `plan` returns ascending, non-overlapping in-bounds
//! ranges that each hold the pattern, and `apply(plan)` equals a naive
//! per-line `str::replace`.

use proptest::prelude::*;
use vix_find_panel::substitute::{Range, Substitute, apply, parse, plan};

/// `p` written as a `/`-delimited pattern: every character the magic
/// syntax treats specially gets a backslash.
fn escape_pattern(p: &str) -> String {
    let mut out = String::new();
    for c in p.chars() {
        if matches!(c, '\\' | '.' | '*' | '[' | ']' | '~' | '^' | '$' | '/') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `r` written as a `/`-delimited replacement (`\r` for a line break).
fn escape_replacement(r: &str) -> String {
    let mut out = String::new();
    for c in r.chars() {
        match c {
            '\n' => out.push_str("\\r"),
            '\\' | '&' | '~' | '/' => {
                out.push('\\');
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out
}

/// Text mixing a small alphabet (so patterns match often) with arbitrary
/// Unicode, line breaks included.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            4 => prop::sample::select(vec!['a', 'b', 'é', '\n', ' ']),
            1 => any::<char>(),
        ],
        0..60,
    )
    .prop_map(|cs| cs.into_iter().collect())
}

/// A non-empty pattern: usually from the small alphabet, sometimes any text.
fn pattern() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => "[abé ]{1,3}",
        1 => "[^\n]{1,4}",
    ]
}

/// Build and parse the command for these parts; a refusal fails the case.
fn command(whole: bool, pat: &str, rep: &str, global: bool) -> Result<Substitute, TestCaseError> {
    let cmd = format!(
        "{}s/{}/{}/{}",
        if whole { "%" } else { "" },
        escape_pattern(pat),
        escape_replacement(rep),
        if global { "g" } else { "" }
    );
    match parse(&cmd) {
        Some(Ok(sub)) => Ok(sub),
        other => Err(TestCaseError::fail(format!("{cmd:?} gave {other:?}"))),
    }
}

/// The reference: `str::replace` / `replacen` on each covered line.
fn naive(text: &str, sub: &Substitute, cursor_line: usize) -> String {
    text.split('\n')
        .enumerate()
        .map(|(n, line)| {
            if sub.range == Range::WholeBuffer || n == cursor_line {
                if sub.global {
                    line.replace(&sub.pattern, &sub.replacement)
                } else {
                    line.replacen(&sub.pattern, &sub.replacement, 1)
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn parse_never_panics(cmd in any::<String>()) {
        let _ = parse(&cmd);
    }

    #[test]
    fn parse_never_panics_on_substitute_like_text(cmd in "%?s(ub)?[/#,|a ][a-z/\\\\.*&~^$#,\\[\\] ]{0,20}") {
        let _ = parse(&cmd);
    }

    /// Escaping any pattern and replacement into a command parses back to
    /// exactly that pattern and replacement, with the range and flags given.
    #[test]
    fn escaped_parts_parse_back(
        pat in "[^\n]{1,8}",
        rep in any::<String>(),
        whole: bool,
        global: bool,
    ) {
        let sub = command(whole, &pat, &rep, global)?;
        prop_assert_eq!(&sub.pattern, &pat);
        prop_assert_eq!(&sub.replacement, &rep);
        prop_assert_eq!(sub.range == Range::WholeBuffer, whole);
        prop_assert_eq!(sub.global, global);
        prop_assert!(!sub.confirm);
    }

    /// `plan`'s ranges are ascending, non-overlapping, inside the text and
    /// each covers exactly the pattern; `apply` of them equals the naive
    /// per-line replace.
    #[test]
    fn apply_of_plan_equals_naive_replace(
        text in text(),
        pat in pattern(),
        rep in "(?s).{0,3}",
        whole: bool,
        global: bool,
        cursor_line in 0usize..8,
    ) {
        let sub = command(whole, &pat, &rep, global)?;
        let ranges = plan(&text, &sub, cursor_line);
        let chars: Vec<char> = text.chars().collect();
        let mut prev_end = 0;
        for &(s, e) in &ranges {
            prop_assert!(s >= prev_end, "ascending, non-overlapping: {:?}", ranges);
            prop_assert!(s < e && e <= chars.len(), "in bounds: {:?}", ranges);
            let hit: String = chars[s..e].iter().collect();
            prop_assert_eq!(&hit, &pat);
            prev_end = e;
        }
        prop_assert_eq!(apply(&text, &ranges, &sub.replacement), naive(&text, &sub, cursor_line));
    }
}
