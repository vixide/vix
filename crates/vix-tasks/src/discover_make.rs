//! Discovery of plain named Make targets from a `Makefile`.

use regex::Regex;
use std::sync::LazyLock;

use crate::task::{NamedTask, TaskSource};

// A target line: `name:` (optionally followed by prerequisites) at column
// 0. The `regex` crate has no look-around, so this does not itself exclude
// a `VAR := value` assignment; [`is_variable_assignment`] checks the
// character right after the match for that.
static TARGET_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_.-]+)\s*:").unwrap());

/// `true` when the character immediately after a [`TARGET_DEF`] match is
/// `=`, meaning the line was actually a `VAR := value` / `VAR ?= value`
/// variable assignment, not a target definition.
fn is_variable_assignment(line: &str, match_end: usize) -> bool {
    line.as_bytes().get(match_end) == Some(&b'=')
}

/// Find plain named Make targets in a `Makefile`: a line matching
/// `^([A-Za-z0-9_.-]+):` at column 0 (recipe body lines are always
/// indented, so this alone excludes them), skipping targets starting with
/// `.` (special targets like `.PHONY`, `.SUFFIXES`) and targets containing
/// `%` (pattern rules like `%.o: %.c`). Command is `make NAME`, prefixed
/// `make:`.
///
/// Only plain named Make targets are offered — pattern rules, file targets,
/// and dot-targets aren't things you'd typically run by hand. This function
/// cannot perfectly distinguish a genuine phony target
/// (`test:`) from a file target that merely lacks an extension
/// (`output:`) by static analysis alone — the chosen heuristic accepts that
/// ambiguity and does **not** try to exclude targets by "looks like it has
/// a file extension"; it only excludes the two structural cases above
/// (leading `.`, containing `%`). A target line listing more than one name
/// before the colon (`foo bar: deps`) is not recognized at all, since the
/// name character class does not include whitespace — documented, not
/// handled.
#[must_use]
pub fn discover_make_targets(makefile: &str) -> Vec<NamedTask> {
    makefile
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let caps = TARGET_DEF.captures(line)?;
            let whole = caps.get(0)?;
            if is_variable_assignment(line, whole.end()) {
                return None;
            }
            Some(caps[1].to_string())
        })
        .filter(|name| !name.starts_with('.') && !name.contains('%'))
        .map(|name| NamedTask {
            name: format!("make:{name}"),
            command: format!("make {name}"),
            source: TaskSource::Discovered,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_plain_targets() {
        let makefile = "build:\n\tcargo build\n\ntest: build\n\tcargo test\n";
        let tasks = discover_make_targets(makefile);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].name, "make:build");
        assert_eq!(tasks[0].command, "make build");
        assert_eq!(tasks[1].name, "make:test");
    }

    #[test]
    fn skips_dot_targets_and_pattern_rules() {
        let makefile = ".PHONY: build\n%.o: %.c\n\tcc -c $<\nbuild:\n\tcc\n";
        let tasks = discover_make_targets(makefile);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "make:build");
    }

    #[test]
    fn skips_variable_assignments() {
        let makefile = "CC := gcc\nCFLAGS = -Wall\nbuild:\n\t$(CC) $(CFLAGS)\n";
        let tasks = discover_make_targets(makefile);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "make:build");
    }

    #[test]
    fn empty_makefile_is_empty() {
        assert!(discover_make_targets("").is_empty());
    }
}
