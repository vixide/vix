//! Project task running: named tasks loaded from `tasks.toml`, per-project-
//! type lifecycle commands (configure/compile/test/install/package/run),
//! tasks discovered from a project's own tooling (npm/yarn/pnpm/bun scripts,
//! Deno tasks, Composer scripts, justfile recipes, go-task Taskfiles, Rake
//! tasks, Make targets), subproject-scoped commands for monorepos, and
//! running the test under the cursor.
//!
//! A workspace can define reusable build/test/run commands in a `tasks.toml` at
//! its root (or in `.vix/tasks.toml`). Vix lists them in a chooser (Tools →
//! Tasks…) and runs the selected one through the same async pipeline as Run
//! Command, so output streams to the bottom dock and the completion posts to the
//! notification panel.
//!
//! ```toml
//! [[task]]
//! name = "build"
//! command = "cargo build"
//!
//! [[task]]
//! name = "test"
//! command = "cargo test"
//! ```
//!
//! Beyond the `tasks.toml` list above, this crate also provides the pieces
//! behind the project-wide Project menu: project-type detection, lifecycle
//! command resolution, task discovery from build-tool manifests, monorepo
//! subproject scoping, command history, and test-at-point. All of that is
//! pure logic: it takes file contents, directory listings, and cursor
//! positions as plain data and returns data, doing no filesystem or process
//! I/O itself, so it can be unit-tested without a live editor or project. The
//! host wires this into the app (menu, action ids, keybindings).
//!
//! ## The biggest documented deviation: test-at-point
//!
//! A syntax-tree-based test-at-point could cover many languages precisely via
//! per-language query grammars. Adding a full parser dependency to this crate
//! for that alone would be wildly disproportionate, so [`test_at_point`]
//! instead uses line-based regex heuristics: scan upward from the cursor line
//! for the nearest enclosing or preceding test-definition line. This is
//! simpler and occasionally wrong (e.g. it does not understand real block
//! nesting the way a parser would), but is cheap, dependency-light, and
//! correct for the common case of a cursor placed inside or just below a
//! single test body.
//!
//! ## Out of scope
//!
//! - **Per-directory configuration languages.** This crate's analog is the
//!   `project_override` parameter to [`lifecycle::effective_lifecycle`]: a
//!   plain [`lifecycle::LifecycleCommands`] the host builds however it likes
//!   (e.g. from a `.vix/project.toml`), not a configuration *language*.
//! - **Command history *scope*** (which history a given project root maps
//!   to across worktrees/checkouts of the "same" repository). [`history`]
//!   models only the push/dedup *policy*; scoping and persistence are a
//!   host/storage-layer concern.
//! - **Syntax-tree-based test-at-point** — see above.
//! - **Java, Erlang, F#, and other test-at-point languages** not listed in
//!   [`test_at_point`]'s doc comment — skipped for budget reasons; the
//!   per-language dispatch inside that module is structured so more
//!   languages can be added the same way.
//! - **Running commands.** This crate only resolves *what* command string to
//!   run; invoking a shell, streaming output, and reporting exit status are
//!   host concerns.
//! - **Caching command resolutions to disk**, prompting the user to edit a
//!   resolved command before running it, and any other UI-level behavior —
//!   this crate models the *data shapes* those features would read and write
//!   ([`lifecycle::effective_lifecycle`]'s `cached` parameter,
//!   [`history::push_history`]'s history list) but does not implement the UI
//!   or persistence around them.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;

use serde::Deserialize;

/// Recognized project types (cargo, npm, make, …), their marker files, and
/// detection from a directory listing.
pub mod project_type;

/// Lifecycle command resolution: merging project-type defaults with
/// per-project overrides and a resolved-command cache.
pub mod lifecycle;

/// Named tasks: the three-tier (user-configured / project-type / discovered)
/// model and its merge precedence.
pub mod task;

/// Discovery of `npm`/`yarn`/`pnpm`/`bun` `package.json` scripts.
pub mod discover_npm;

/// Discovery of `deno.json`/`deno.jsonc` tasks.
pub mod discover_deno;

/// Discovery of `composer.json` scripts.
pub mod discover_composer;

/// Discovery of `justfile` recipes.
pub mod discover_just;

/// Discovery of go-task `Taskfile.yml` tasks.
pub mod discover_taskfile;

/// Discovery of Rake tasks from a `Rakefile` and `.rake` files.
pub mod discover_rake;

/// Discovery of plain named Make targets from a `Makefile`.
pub mod discover_make;

/// Monorepo subproject discovery: the nearest enclosing project-type marker
/// file for any given path.
pub mod subproject;

/// Command run history: push-with-dedup policies.
pub mod history;

/// Test-at-point: locating the nearest enclosing/preceding test definition
/// and its run command, per language.
pub mod test_at_point;

/// One named task: a label and the shell command it runs.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Task {
    /// Display name shown in the chooser.
    pub name: String,
    /// Shell command line executed via the Run Command pipeline.
    pub command: String,
}

/// The `tasks.toml` schema: a list of `[[task]]` tables.
#[derive(Debug, Default, Deserialize)]
struct TaskFile {
    #[serde(default)]
    task: Vec<Task>,
}

/// Load the workspace tasks from `<root>/tasks.toml`, falling back to
/// `<root>/.vix/tasks.toml`. Returns an empty list when neither exists or the
/// file fails to parse. Tasks with an empty name or command are dropped.
#[must_use]
pub fn load(root: &Path) -> Vec<Task> {
    let candidates = [
        root.join("tasks.toml"),
        root.join(".vix").join("tasks.toml"),
    ];
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<TaskFile>(&text) else {
            continue;
        };
        let tasks: Vec<Task> = parsed
            .task
            .into_iter()
            .filter(|t| !t.name.trim().is_empty() && !t.command.trim().is_empty())
            .collect();
        if !tasks.is_empty() {
            return tasks;
        }
    }
    Vec::new()
}

/// Parse tasks from a TOML string (the `tasks.toml` body). Used by [`load`] and
/// directly in tests.
#[must_use]
pub fn parse(text: &str) -> Vec<Task> {
    toml::from_str::<TaskFile>(text)
        .map(|f| {
            f.task
                .into_iter()
                .filter(|t| !t.name.trim().is_empty() && !t.command.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tasks_and_drops_blank() {
        let toml = r#"
            [[task]]
            name = "build"
            command = "cargo build"

            [[task]]
            name = "test"
            command = "cargo test"

            [[task]]
            name = ""
            command = "ignored"
        "#;
        let tasks = parse(toml);
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[0],
            Task {
                name: "build".into(),
                command: "cargo build".into()
            }
        );
        assert_eq!(tasks[1].name, "test");
    }

    #[test]
    fn empty_or_bad_toml_is_empty() {
        assert!(parse("").is_empty());
        assert!(parse("not = valid = toml").is_empty());
    }
}
