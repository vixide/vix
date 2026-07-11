//! Project task runner: named shell commands loaded from `tasks.toml`.
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

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;

use serde::Deserialize;

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
