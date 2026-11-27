//! Property tests for the palette's `path:line[:col]` parser (T831):
//! [`parse_path_target`] never panics, always returns a path the input
//! starts with, and reads back a generated `path`, `path:line` or
//! `path:line:col`, including Windows drive-letter paths, whose own colon
//! is not the separator (T547).

use proptest::prelude::*;
use vix_palette::parse_path_target;

/// A path with no colon, Unix or Windows style.
const PATH: &str = "[A-Za-z0-9_. /\\\\-]{1,24}";

proptest! {
    /// Any input: the returned path is a prefix of it, and with no colon
    /// there is no target at all.
    #[test]
    fn total_and_path_is_a_prefix(input in "(?s).{0,40}") {
        let (path, target) = parse_path_target(&input);
        prop_assert!(input.starts_with(&path), "{input:?} -> {path:?}");
        if !input.contains(':') {
            prop_assert_eq!(path, input);
            prop_assert_eq!(target, None);
        }
    }

    /// `path`, `path:line` and `path:line:col` read back as written; a
    /// missing column is column 1.
    #[test]
    fn generated_targets_round_trip(
        drive in prop::option::of(("[A-Za-z]", prop::sample::select(vec!["\\", "/"]))),
        path in PATH,
        line in prop::option::of(0usize..1_000_000),
        col in prop::option::of(0usize..10_000),
    ) {
        let path = match drive {
            Some((letter, sep)) => format!("{letter}:{sep}{path}"),
            None => path,
        };
        let mut input = path.clone();
        if let Some(l) = line {
            input = format!("{input}:{l}");
            if let Some(c) = col {
                input = format!("{input}:{c}");
            }
        }
        let (got, target) = parse_path_target(&input);
        prop_assert_eq!(got, path);
        prop_assert_eq!(target, line.map(|l| (l, col.unwrap_or(1))));
    }

    /// A line that is not a number gives no target, and a column that is not
    /// one falls back to column 1.
    #[test]
    fn junk_numbers_are_not_targets(path in PATH, line in 1usize..1000, junk in "[a-z ]{1,6}") {
        prop_assert_eq!(parse_path_target(&format!("{path}:{junk}")), (path.clone(), None));
        prop_assert_eq!(
            parse_path_target(&format!("{path}:{line}:{junk}")),
            (path, Some((line, 1)))
        );
    }
}
