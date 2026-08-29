//! Discovery of go-task `Taskfile.yml` tasks.

use crate::task::{NamedTask, TaskSource};

/// Parse the top-level `tasks:` mapping of a go-task `Taskfile.yml`: each
/// key under it becomes a task name, command `task NAME`, prefixed `task:`.
/// Uses `serde_yaml` (already a workspace dependency) rather than a
/// hand-rolled scanner, so this handles any valid YAML shape for the
/// `tasks` mapping (flow or block style, quoted or unquoted keys). Malformed
/// YAML, a missing `tasks` key, or a non-mapping `tasks` value all return an
/// empty vector.
#[must_use]
pub fn discover_taskfile_tasks(taskfile_yml: &str) -> Vec<NamedTask> {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(taskfile_yml) else {
        return Vec::new();
    };
    let Some(tasks) = value.get("tasks").and_then(serde_yaml::Value::as_mapping) else {
        return Vec::new();
    };
    tasks
        .keys()
        .filter_map(|k| k.as_str())
        .map(|name| NamedTask {
            name: format!("task:{name}"),
            command: format!("task {name}"),
            source: TaskSource::Discovered,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_taskfile_tasks_with_prefix() {
        let yml = "version: '3'\ntasks:\n  build:\n    cmds:\n      - go build ./...\n  test:\n    cmds:\n      - go test ./...\n";
        let tasks = discover_taskfile_tasks(yml);
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|t| t.name == "task:build" && t.command == "task build")
        );
        assert!(tasks.iter().any(|t| t.name == "task:test"));
    }

    #[test]
    fn malformed_or_missing_tasks_is_empty() {
        assert!(discover_taskfile_tasks("tasks: [1, 2").is_empty());
        assert!(discover_taskfile_tasks("version: '3'\n").is_empty());
        assert!(discover_taskfile_tasks("tasks: not-a-mapping\n").is_empty());
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(discover_taskfile_tasks("").is_empty());
    }
}
