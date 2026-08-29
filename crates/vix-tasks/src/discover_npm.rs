//! Discovery of `npm`/`yarn`/`pnpm`/`bun` `package.json` scripts.
//!
//! [`PackageManager`](crate::project_type::PackageManager) and
//! [`detect_package_manager`](crate::project_type::detect_package_manager)
//! live in [`crate::project_type`] rather than here, since directory-level
//! project-type detection needs them too; this module just uses them.

use crate::project_type::PackageManager;
use crate::task::{NamedTask, TaskSource};

/// Parse the `scripts` object of a `package.json`, prefixing each task name
/// with `manager`'s tool prefix (`npm:`, `pnpm:`, `yarn:`, or `bun:`) and
/// its run command with the matching binary (`npm run NAME`, `pnpm NAME`,
/// `yarn NAME`, `bun run NAME`). Malformed JSON, a missing `scripts` key, or
/// a non-object `scripts` value all return an empty vector rather than
/// erroring — a source that would signal an error is silently skipped, not
/// surfaced as a failure. Any non-string script value is skipped.
#[must_use]
pub fn discover_npm_scripts(package_json: &str, manager: PackageManager) -> Vec<NamedTask> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(package_json) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let (prefix, run) = match manager {
        PackageManager::Npm => ("npm", "npm run"),
        PackageManager::Pnpm => ("pnpm", "pnpm"),
        PackageManager::Yarn => ("yarn", "yarn"),
        PackageManager::Bun => ("bun", "bun run"),
    };
    scripts
        .iter()
        .filter_map(|(name, cmd)| {
            // The script body itself (`cmd`) is not part of the resulting
            // command line — running `npm run NAME` re-reads it from
            // `package.json` — but a malformed (non-string) value still
            // disqualifies the entry, matching real `package.json` schemas.
            cmd.as_str()?;
            Some(NamedTask {
                name: format!("{prefix}:{name}"),
                command: format!("{run} {name}"),
                source: TaskSource::Discovered,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_npm_scripts_with_prefix() {
        let json = r#"{"scripts": {"build": "tsc", "test": "jest"}}"#;
        let tasks = discover_npm_scripts(json, PackageManager::Npm);
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|t| t.name == "npm:build" && t.command == "npm run build")
        );
        assert!(
            tasks
                .iter()
                .any(|t| t.name == "npm:test" && t.command == "npm run test")
        );
    }

    #[test]
    fn pnpm_yarn_bun_use_their_own_binary() {
        let json = r#"{"scripts": {"build": "tsc"}}"#;
        assert_eq!(
            discover_npm_scripts(json, PackageManager::Pnpm)[0].command,
            "pnpm build"
        );
        assert_eq!(
            discover_npm_scripts(json, PackageManager::Yarn)[0].command,
            "yarn build"
        );
        assert_eq!(
            discover_npm_scripts(json, PackageManager::Bun)[0].command,
            "bun run build"
        );
    }

    #[test]
    fn malformed_or_missing_scripts_is_empty_not_panicking() {
        assert!(discover_npm_scripts("not json", PackageManager::Npm).is_empty());
        assert!(discover_npm_scripts("{}", PackageManager::Npm).is_empty());
        assert!(discover_npm_scripts(r#"{"scripts": "oops"}"#, PackageManager::Npm).is_empty());
        assert!(discover_npm_scripts(r#"{"scripts": {"x": 1}}"#, PackageManager::Npm).is_empty());
    }
}
