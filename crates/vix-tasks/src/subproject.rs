//! Monorepo subproject discovery: the nearest enclosing project-type marker
//! file for any given path.

use crate::project_type::PROJECT_TYPES;

/// One subproject: a project-relative directory containing at least one
/// registered [`ProjectType`](crate::project_type::ProjectType) marker
/// file. The project root itself is never a `Subproject` — only
/// directories *below* it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subproject {
    /// The subproject's root, relative to the overall project root, using
    /// `/` as the separator regardless of host OS (e.g.
    /// `"crates/vix-org"`). Never empty.
    pub relative_root: String,
}

fn split_dir_file(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// Scan a project's own file listing (already gitignore-aware — the host
/// supplies relative-path strings, e.g. from its own file index) for any
/// marker file belonging to a registered
/// [`ProjectType`](crate::project_type::ProjectType), at any directory below
/// the project root. Returns one [`Subproject`] per directory containing at
/// least one marker, deduplicated and sorted by `relative_root` for
/// deterministic output. A marker file directly at the project root (no `/`
/// in its path) does **not** produce a `Subproject` — the project root
/// itself is not a subproject of itself.
#[must_use]
pub fn find_subprojects(project_relative_files: &[&str]) -> Vec<Subproject> {
    let marker_names: Vec<&str> = PROJECT_TYPES
        .iter()
        .flat_map(|t| t.marker_files.iter().copied())
        .collect();

    let mut dirs: Vec<String> = Vec::new();
    for path in project_relative_files {
        let (dir, file) = split_dir_file(path);
        if dir.is_empty() || !marker_names.contains(&file) {
            continue;
        }
        if !dirs.iter().any(|d| d == dir) {
            dirs.push(dir.to_string());
        }
    }
    dirs.sort();
    dirs.into_iter()
        .map(|relative_root| Subproject { relative_root })
        .collect()
}

/// The nearest enclosing subproject for a given project-relative file path
/// (longest matching `relative_root` prefix), or `None` if no subproject
/// contains it — commands should then run at the project root, or the host
/// should report that no manifest was found: if there's no manifest between
/// the current file and the project root, those commands should say so
/// rather than silently building the whole repository.
#[must_use]
pub fn nearest_subproject<'a>(
    subprojects: &'a [Subproject],
    project_relative_file: &str,
) -> Option<&'a Subproject> {
    subprojects
        .iter()
        .filter(|s| {
            project_relative_file == s.relative_root
                || project_relative_file.starts_with(&format!("{}/", s.relative_root))
        })
        .max_by_key(|s| s.relative_root.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn finds_subprojects_from_marker_files() {
        let files = [
            "Cargo.toml",
            "crates/vix-org/Cargo.toml",
            "crates/vix-org/src/lib.rs",
            "crates/vix-tasks/Cargo.toml",
            "web/package.json",
        ];
        let subs = find_subprojects(&files);
        let roots: Vec<_> = subs.iter().map(|s| s.relative_root.as_str()).collect();
        assert_eq!(roots, ["crates/vix-org", "crates/vix-tasks", "web"]);
    }

    #[test]
    fn project_root_marker_is_not_a_subproject() {
        let files = ["Cargo.toml", "package.json"];
        assert!(find_subprojects(&files).is_empty());
    }

    #[test]
    fn deduplicates_multiple_markers_in_same_dir() {
        let files = ["web/package.json", "web/yarn.lock"];
        let subs = find_subprojects(&files);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].relative_root, "web");
    }

    #[test]
    fn nested_monorepo_picks_longest_prefix() {
        let subs = vec![
            Subproject {
                relative_root: "packages".into(),
            },
            Subproject {
                relative_root: "packages/core".into(),
            },
        ];
        let nearest = nearest_subproject(&subs, "packages/core/src/index.ts");
        assert_eq!(nearest.unwrap().relative_root, "packages/core");
    }

    #[test]
    fn no_enclosing_subproject_is_none() {
        let subs = vec![Subproject {
            relative_root: "crates/vix-org".into(),
        }];
        assert!(nearest_subproject(&subs, "src/main.rs").is_none());
    }

    #[test]
    fn file_exactly_at_subproject_root_matches() {
        let subs = vec![Subproject {
            relative_root: "crates/vix-org".into(),
        }];
        let nearest = nearest_subproject(&subs, "crates/vix-org/Cargo.toml");
        assert!(nearest.is_some());
    }

    proptest! {
        #[test]
        fn find_subprojects_never_panics(paths in proptest::collection::vec("[a-zA-Z0-9/._-]{0,20}", 0..10)) {
            let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
            let _ = find_subprojects(&refs);
        }
    }
}
