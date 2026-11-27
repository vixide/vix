//! Property tests for the problem matchers' location parsing (T831): a
//! location printed in each built-in format, with any file name, line and
//! column and with or without ANSI colors, is read back by [`locate`] and,
//! for the one-line formats, by [`scan`] with the matching built-in.
//! Neither panics on arbitrary text.

use proptest::prelude::*;
use vix_tasks::matcher::{Builtin, MatcherSpec, locate, scan};

/// A file name as compilers print it: no blanks or colons.
const FILE: &str = "[a-z_][a-z0-9_/.-]{0,16}";

/// One output line in a built-in format, and the file, line and column
/// `locate` should read from it.
fn printed(fmt: usize, file: &str, line: usize, col: usize) -> (String, Option<usize>, Builtin) {
    match fmt {
        0 => (
            format!("  --> {file}.rs:{line}:{col}"),
            Some(col),
            Builtin::Rustc,
        ),
        1 => (
            format!("{file}.c:{line}:{col}: error: expected ';'"),
            Some(col),
            Builtin::Gcc,
        ),
        2 => (
            format!("{file}.c:{line}: warning: unused"),
            None,
            Builtin::Gcc,
        ),
        3 => (
            format!("thread 'main' panicked at {file}.rs:{line}:{col}:"),
            Some(col),
            Builtin::Rustc,
        ),
        4 => (
            format!("{file}.ts({line},{col}): error TS2322: bad"),
            Some(col),
            Builtin::Tsc,
        ),
        5 => (
            format!("{file}.ts:{line}:{col} - error TS2322: bad"),
            Some(col),
            Builtin::Tsc,
        ),
        6 => (
            format!("{file}.go:{line}:{col}: undefined: x"),
            Some(col),
            Builtin::Go,
        ),
        _ => (
            format!("  File \"{file}.py\", line {line}, in f"),
            None,
            Builtin::Python,
        ),
    }
}

/// The extension format `fmt` adds to the file name.
fn ext(fmt: usize) -> &'static str {
    match fmt {
        0 | 3 => ".rs",
        1 | 2 => ".c",
        4 | 5 => ".ts",
        6 => ".go",
        _ => ".py",
    }
}

proptest! {
    /// Arbitrary text never panics either parser.
    #[test]
    fn total_on_any_text(text in "(?s).{0,120}") {
        let _ = locate(&text);
        let _ = scan(MatcherSpec::Auto, &text);
    }

    /// A printed location reads back exactly, colored or not.
    #[test]
    fn printed_locations_round_trip(
        fmt in 0usize..8,
        file in FILE,
        line in 1usize..1_000_000,
        col in 1usize..10_000,
        color in any::<bool>(),
    ) {
        let (text, col, _) = printed(fmt, &file, line, col);
        let text = if color { format!("\x1b[1m\x1b[31m{text}\x1b[0m") } else { text };
        let want = (format!("{file}{}", ext(fmt)), line, col);
        prop_assert_eq!(locate(&text), Some(want));
    }

    /// The one-line formats, one per line, are found by `scan` with their
    /// own built-in, in order and with the same location.
    #[test]
    fn scan_finds_every_one_line_problem(
        rows in prop::collection::vec((prop::sample::select(vec![1usize, 2, 4, 5, 6]), FILE, 1usize..100_000, 1usize..1000), 1..6),
    ) {
        for builtin in [Builtin::Gcc, Builtin::Tsc, Builtin::Go] {
            let mine: Vec<_> = rows
                .iter()
                .map(|(fmt, file, line, col)| (printed(*fmt, file, *line, *col), *fmt, file, *line))
                .filter(|((_, _, b), _, _, _)| *b == builtin)
                .collect();
            let output: String = rows
                .iter()
                .map(|(fmt, file, line, col)| printed(*fmt, file, *line, *col).0 + "\n")
                .collect();
            let found = scan(MatcherSpec::Builtin(builtin), &output);
            let got: Vec<_> = found.iter().map(|p| (p.file.clone(), p.line, p.column)).collect();
            let want: Vec<_> = mine
                .iter()
                .map(|((_, col, _), fmt, file, line)| (format!("{file}{}", ext(*fmt)), *line, *col))
                .collect();
            prop_assert_eq!(got, want, "{}", output);
        }
    }
}
