//! Fuzz the Ex `:s` / `:%s` parser, planner and applier
//! (`vix-find-panel::substitute`).
//!
//! The command line is whatever the user typed after `:`. `parse` must be
//! total, and a command it accepts must plan ranges that `apply` can splice
//! safely: ascending, non-overlapping, in bounds, each covering exactly the
//! pattern, at most one per line without `g`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_find_panel::substitute::{Range, apply, parse, plan};

fuzz_target!(|data: &[u8]| {
    // Byte 0 is the cursor line; the rest is "command\nbuffer text".
    let Some((&cursor, rest)) = data.split_first() else {
        return;
    };
    let Ok(rest) = std::str::from_utf8(rest) else {
        return;
    };
    let (cmd, text) = rest.split_once('\n').unwrap_or((rest, ""));
    let Some(Ok(sub)) = parse(cmd) else {
        return;
    };
    assert!(!sub.pattern.is_empty(), "parse accepted an empty pattern");

    let cursor_line = usize::from(cursor % 8);
    let ranges = plan(text, &sub, cursor_line);
    let chars: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = sub.pattern.chars().collect();
    let mut prev_end = 0;
    let mut lines_hit = Vec::new();
    for &(s, e) in &ranges {
        assert!(
            s >= prev_end,
            "ranges overlap or are out of order: {ranges:?}"
        );
        assert!(s < e && e <= chars.len(), "range {s}..{e} out of bounds");
        assert_eq!(
            chars.get(s..e),
            Some(pattern.as_slice()),
            "range {s}..{e} is not the pattern"
        );
        let line = chars
            .get(..s)
            .map_or(0, |p| p.iter().filter(|&&c| c == '\n').count());
        if sub.range == Range::CurrentLine {
            assert_eq!(line, cursor_line, "a match outside the cursor's line");
        }
        if !sub.global {
            assert!(
                !lines_hit.contains(&line),
                "two matches on line {line} without g"
            );
        }
        lines_hit.push(line);
        prev_end = e;
    }

    // apply replaces exactly the planned characters.
    let out = apply(text, &ranges, &sub.replacement);
    let n = ranges.len();
    let expected = chars.len() - n * pattern.len() + n * sub.replacement.chars().count();
    assert_eq!(out.chars().count(), expected, "apply's length is off");
    if ranges.is_empty() {
        assert_eq!(out, text, "no ranges, but apply changed the text");
    }
});
