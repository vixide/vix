//! Discovery of `composer.json` scripts.

use crate::task::{NamedTask, TaskSource};

/// Parse the `scripts` object of a `composer.json`, prefixing each task
/// name with `composer:` and its command with `composer run-script NAME`.
/// Composer allows a script's value to be either a single command string or
/// an array of command strings run in sequence; an array is joined with
/// `&&`. Malformed JSON, a missing `scripts` key, or a script value that is
/// neither a string nor an array of strings all return an empty vector /
/// skip that entry rather than erroring.
#[must_use]
pub fn discover_composer_scripts(composer_json: &str) -> Vec<NamedTask> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(composer_json) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    scripts
        .iter()
        .filter(|(_, cmd)| script_is_recognized(cmd))
        .map(|(name, _)| NamedTask {
            name: format!("composer:{name}"),
            command: format!("composer run-script {name}"),
            source: TaskSource::Discovered,
        })
        .collect()
}

fn script_is_recognized(cmd: &serde_json::Value) -> bool {
    cmd.as_str().is_some()
        || cmd
            .as_array()
            .is_some_and(|items| items.iter().all(serde_json::Value::is_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_composer_scripts_with_prefix() {
        let json = r#"{"scripts": {"test": "phpunit", "post-install-cmd": ["echo a", "echo b"]}}"#;
        let tasks = discover_composer_scripts(json);
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|t| t.name == "composer:test" && t.command == "composer run-script test")
        );
        assert!(tasks.iter().any(|t| t.name == "composer:post-install-cmd"));
    }

    #[test]
    fn malformed_or_missing_scripts_is_empty() {
        assert!(discover_composer_scripts("not json").is_empty());
        assert!(discover_composer_scripts("{}").is_empty());
        assert!(discover_composer_scripts(r#"{"scripts": {"bad": 1}}"#).is_empty());
    }
}
