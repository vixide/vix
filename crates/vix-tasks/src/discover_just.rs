//! Discovery of `justfile` recipes.

use regex::Regex;
use std::sync::LazyLock;

use crate::task::{NamedTask, TaskSource};

// A recipe header: `name`, optionally followed by parameters (no `=`
// default values — see the doc comment below), then a `:`. The `regex`
// crate has no look-around, so this does not itself exclude a `:=`
// variable assignment; [`is_variable_assignment`] checks the character
// right after the match for that. Anchored to the start of the line, so
// only column-0 (non-indented) lines match — recipe *body* lines are always
// indented.
static RECIPE_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z_][A-Za-z0-9_-]*)[^\n:=]*:").unwrap());

/// `true` when the character immediately after a [`RECIPE_HEADER`] match is
/// `=`, meaning the line was actually a `name := value` variable
/// assignment, not a recipe header.
fn is_variable_assignment(line: &str, match_end: usize) -> bool {
    line.as_bytes().get(match_end) == Some(&b'=')
}

/// Find `justfile` recipe definitions: a name at column 0, not a comment
/// (`#`) or a variable assignment (`name := value` / `name = value`),
/// followed by `:` and (ignored here) any dependencies. Command is
/// `just NAME`, prefixed `just:`.
///
/// Pragmatic and line-based, not a real justfile parser. Known gaps,
/// documented rather than handled:
/// - **Attributes** (`[private]`, `[group('x')]`) written on the line above
///   a recipe are not associated with it (harmless here — the recipe
///   itself is still found on its own line).
/// - **Parametrized recipes with a default value** (`build target="release":`)
///   are *not* recognized: the `=` inside the header falls inside this
///   parser's "not `:` or `=`" run, so the header fails to match before it
///   reaches the real trailing `:`. Parameters *without* a default
///   (`build target:`) are recognized fine, since nothing runs afoul of the
///   `=` exclusion.
/// - Recipes are recognized independent of any leading `@`-quiet prefix on
///   their *body* lines (the header line itself never starts with `@`).
#[must_use]
pub fn discover_just_recipes(justfile: &str) -> Vec<NamedTask> {
    justfile
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let caps = RECIPE_HEADER.captures(line)?;
            let whole = caps.get(0)?;
            if is_variable_assignment(line, whole.end()) {
                return None;
            }
            Some(caps[1].to_string())
        })
        .map(|name| NamedTask {
            name: format!("just:{name}"),
            command: format!("just {name}"),
            source: TaskSource::Discovered,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_simple_recipes() {
        let justfile = "build:\n    cargo build\n\ntest: build\n    cargo test\n";
        let tasks = discover_just_recipes(justfile);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].name, "just:build");
        assert_eq!(tasks[0].command, "just build");
        assert_eq!(tasks[1].name, "just:test");
    }

    #[test]
    fn skips_comments_variables_and_body_lines() {
        let justfile = "# a comment\nname := \"x\"\nother = 1\nbuild:\n    echo {{name}}\n";
        let tasks = discover_just_recipes(justfile);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "just:build");
    }

    #[test]
    fn recipe_with_parameter_without_default_is_recognized() {
        let justfile = "greet name:\n    echo {{name}}\n";
        let tasks = discover_just_recipes(justfile);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "just:greet");
    }

    #[test]
    fn empty_justfile_is_empty() {
        assert!(discover_just_recipes("").is_empty());
    }
}
