//! Finding `.rhai` scripts on disk (§ Script discovery).
//!
//! This module only walks directories and resolves which file wins per file
//! stem — it never reads or compiles a script (that's [`crate::Runtime`]).
//! Callers supply directories directly (the global scripts directory, the
//! project scripts directory); resolving *which paths those are* — e.g.
//! `Settings::scripts_dir()`, `<App::root>/.vix/scripts` — is host wiring
//! (tasks.md T103), not this crate's job.

#![warn(clippy::pedantic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One script found on disk: its file stem (the script's identity — `id`s
/// and bindings it registers are namespaced by this) and the path to the
/// copy that wins after project-vs-global shadowing (see [`discover`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredScript {
    /// The script's identity: its file name without the `.rhai` extension.
    pub stem: String,
    /// Path to the file to actually load.
    pub path: PathBuf,
}

/// List `.rhai` scripts under `global_dir` and `project_dir` (each optional,
/// each read non-recursively — § Script discovery). A project script
/// shadows a global one that shares the same file stem entirely: the global
/// file is not loaded at all, matching how a project's `.editorconfig` or
/// `.vix/project.toml` overrides its global counterpart elsewhere in Vix.
/// A missing or unreadable directory is not an error — it just contributes
/// no scripts. Returned in stem order, for a deterministic load order.
#[must_use]
pub fn discover(global_dir: Option<&Path>, project_dir: Option<&Path>) -> Vec<DiscoveredScript> {
    let mut by_stem: BTreeMap<String, PathBuf> = BTreeMap::new();
    if let Some(dir) = global_dir {
        collect_into(dir, &mut by_stem);
    }
    // Applied second: a `BTreeMap::insert` on the same key overwrites the
    // global entry, so a project script always wins a stem collision.
    if let Some(dir) = project_dir {
        collect_into(dir, &mut by_stem);
    }
    by_stem
        .into_iter()
        .map(|(stem, path)| DiscoveredScript { stem, path })
        .collect()
}

/// Add every `*.rhai` file directly inside `dir` (not its subdirectories) to
/// `by_stem`, keyed by file stem.
fn collect_into(dir: &Path, by_stem: &mut BTreeMap<String, PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // missing/unreadable directory: nothing to contribute
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rhai") {
            continue;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue; // skip a directory named e.g. "foo.rhai/"
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        by_stem.insert(stem.to_string(), path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory under the OS temp dir, unique per test, cleaned
    /// up on drop — the pattern already used by `vix-editorconfig`'s
    /// filesystem tests.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(name: &str) -> Self {
            // Parallel tests in this same process share a PID, so mix in a
            // monotonic counter too — the PID alone (fine for a crate with
            // just one such test, e.g. `vix-editorconfig`) would collide here.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("vix-script-{name}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, file_name: &str, contents: &str) {
            std::fs::write(self.0.join(file_name), contents).unwrap();
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn empty_when_both_dirs_absent() {
        assert_eq!(discover(None, None), Vec::new());
    }

    #[test]
    fn missing_directory_contributes_nothing() {
        let missing = std::env::temp_dir().join("vix-script-does-not-exist-at-all");
        assert_eq!(discover(Some(&missing), None), Vec::new());
    }

    #[test]
    fn lists_rhai_files_only_non_recursively() {
        let dir = ScratchDir::new("global");
        dir.write("uppercase.rhai", "// script");
        dir.write("readme.txt", "not a script");
        std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.rhai"), "// nested").unwrap();

        let found = discover(Some(dir.path()), None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stem, "uppercase");
        assert_eq!(found[0].path, dir.path().join("uppercase.rhai"));
    }

    #[test]
    fn project_shadows_global_by_stem() {
        let global = ScratchDir::new("shadow-global");
        let project = ScratchDir::new("shadow-project");
        global.write("rename.rhai", "// global version");
        global.write("only_global.rhai", "// global only");
        project.write("rename.rhai", "// project version");

        let found = discover(Some(global.path()), Some(project.path()));
        let stems: Vec<&str> = found.iter().map(|s| s.stem.as_str()).collect();
        assert_eq!(stems, vec!["only_global", "rename"]);
        let rename = found.iter().find(|s| s.stem == "rename").unwrap();
        assert_eq!(rename.path, project.path().join("rename.rhai"));
    }
}
