//! Named tasks: the three-tier (user-configured / project-type / discovered)
//! model and its merge precedence.

/// Where a named task came from, for display and to establish merge
/// precedence in [`merge_tasks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSource {
    /// From the host's user-configured task list — this crate's own
    /// `tasks.toml`.
    UserConfigured,
    /// A default task contributed by a detected project type.
    ProjectType,
    /// Discovered by inspecting a build-tool manifest (`package.json`,
    /// `justfile`, a `Makefile`, …). Always name-prefixed with its tool
    /// (`npm:build`, `just:test`, …) by the `discover_*` function that
    /// produced it, so it structurally cannot collide with a user/
    /// project-type task name.
    Discovered,
}

/// One named task: a display name, the shell command it runs, and where it
/// came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedTask {
    /// Display name shown in a chooser.
    pub name: String,
    /// Shell command line to run.
    pub command: String,
    /// Which tier this task came from.
    pub source: TaskSource,
}

impl From<&crate::Task> for NamedTask {
    fn from(task: &crate::Task) -> Self {
        NamedTask {
            name: task.name.clone(),
            command: task.command.clone(),
            source: TaskSource::UserConfigured,
        }
    }
}

/// Convert a slice of user-configured `tasks.toml` tasks into [`NamedTask`]s,
/// all tagged [`TaskSource::UserConfigured`].
#[must_use]
pub fn from_vix_tasks(tasks: &[crate::Task]) -> Vec<NamedTask> {
    tasks.iter().map(NamedTask::from).collect()
}

/// Merge project-type tasks, user-configured tasks, and discovered tasks
/// into one effective list. On a name collision, `user_tasks` wins, then
/// `project_type_tasks`, then `discovered_tasks`: when several sources
/// define a task with the same name, the user-configured entry wins, then
/// the project type's. Order is first-appearance order across the three
/// tiers (lowest precedence first), so a task promoted by a later,
/// higher-precedence tier keeps its original position in the output.
#[must_use]
pub fn merge_tasks(
    project_type_tasks: &[NamedTask],
    user_tasks: &[NamedTask],
    discovered_tasks: &[NamedTask],
) -> Vec<NamedTask> {
    let mut merged: Vec<NamedTask> = Vec::new();
    for tier in [discovered_tasks, project_type_tasks, user_tasks] {
        for task in tier {
            if let Some(existing) = merged.iter_mut().find(|m| m.name == task.name) {
                *existing = task.clone();
            } else {
                merged.push(task.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn task(name: &str, source: TaskSource) -> NamedTask {
        NamedTask {
            name: name.into(),
            command: format!("cmd-for-{name}"),
            source,
        }
    }

    #[test]
    fn merge_with_no_collisions_keeps_all() {
        let pt = vec![task("build", TaskSource::ProjectType)];
        let user = vec![task("release", TaskSource::UserConfigured)];
        let disc = vec![task("npm:test", TaskSource::Discovered)];
        let merged = merge_tasks(&pt, &user, &disc);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn user_wins_over_project_type_and_discovered() {
        let pt = vec![task("build", TaskSource::ProjectType)];
        let user = vec![NamedTask {
            name: "build".into(),
            command: "user build".into(),
            source: TaskSource::UserConfigured,
        }];
        let disc = vec![task("build", TaskSource::Discovered)];
        let merged = merge_tasks(&pt, &user, &disc);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].command, "user build");
        assert_eq!(merged[0].source, TaskSource::UserConfigured);
    }

    #[test]
    fn project_type_wins_over_discovered_when_no_user_task() {
        let pt = vec![NamedTask {
            name: "build".into(),
            command: "type build".into(),
            source: TaskSource::ProjectType,
        }];
        let disc = vec![task("build", TaskSource::Discovered)];
        let merged = merge_tasks(&pt, &[], &disc);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].command, "type build");
    }

    #[test]
    fn from_vix_tasks_tags_user_configured() {
        let tasks = vec![crate::Task {
            name: "build".into(),
            command: "cargo build".into(),
        }];
        let named = from_vix_tasks(&tasks);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].source, TaskSource::UserConfigured);
        assert_eq!(named[0].command, "cargo build");
    }

    proptest! {
        #[test]
        fn merge_tasks_never_duplicates_a_name(
            names in proptest::collection::vec("[a-c]", 0..6)
        ) {
            let tasks: Vec<NamedTask> = names
                .iter()
                .map(|n| task(n, TaskSource::Discovered))
                .collect();
            let merged = merge_tasks(&[], &[], &tasks);
            let mut seen = std::collections::HashSet::new();
            for t in &merged {
                prop_assert!(seen.insert(t.name.clone()));
            }
        }
    }
}
