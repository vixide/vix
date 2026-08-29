//! Recognized project types and their detection from a directory listing.

/// A recognized project type: how to detect it and its default lifecycle
/// commands. `None` in any `*_cmd` field means this crate has no sensible
/// default for that slot — the host should fall back to a discovered/user
/// task, or omit the menu entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectType {
    /// Short identifier, e.g. `"cargo"`, `"npm"`, `"make"`.
    pub name: &'static str,
    /// Files whose presence identifies this type. For most types, *any one*
    /// of these files present in a directory is a match (see
    /// [`detect_project_types`]). The four JS package-manager types
    /// (`npm`/`yarn`/`pnpm`/`bun`) are the one documented exception — see
    /// that function's doc comment.
    pub marker_files: &'static [&'static str],
    /// Command that prepares the project to build (installs dependencies,
    /// runs a generator, etc.).
    pub configure_cmd: Option<&'static str>,
    /// Command that builds/compiles the project.
    pub compile_cmd: Option<&'static str>,
    /// Command that runs the project's test suite.
    pub test_cmd: Option<&'static str>,
    /// Command that installs the project (into the system, a venv, etc.).
    pub install_cmd: Option<&'static str>,
    /// Command that produces a distributable package/artifact.
    pub package_cmd: Option<&'static str>,
    /// Command that runs the project.
    pub run_cmd: Option<&'static str>,
}

/// The built-in registry of recognized project types — a pragmatic subset of
/// common build-tool ecosystems, not an exhaustive list (see the crate doc
/// comment). Order is significant: it is the tie-break order used by
/// [`crate::lifecycle::default_lifecycle`] and is preserved by
/// [`detect_project_types`].
pub const PROJECT_TYPES: &[ProjectType] = &[
    ProjectType {
        name: "cargo",
        marker_files: &["Cargo.toml"],
        configure_cmd: None,
        compile_cmd: Some("cargo build"),
        test_cmd: Some("cargo test"),
        install_cmd: Some("cargo install --path ."),
        package_cmd: Some("cargo build --release"),
        run_cmd: Some("cargo run"),
    },
    ProjectType {
        name: "npm",
        marker_files: &["package.json"],
        configure_cmd: Some("npm install"),
        compile_cmd: Some("npm run build"),
        test_cmd: Some("npm test"),
        install_cmd: Some("npm install"),
        package_cmd: Some("npm run build"),
        run_cmd: Some("npm start"),
    },
    ProjectType {
        name: "yarn",
        marker_files: &["package.json", "yarn.lock"],
        configure_cmd: Some("yarn install"),
        compile_cmd: Some("yarn build"),
        test_cmd: Some("yarn test"),
        install_cmd: Some("yarn install"),
        package_cmd: Some("yarn build"),
        run_cmd: Some("yarn start"),
    },
    ProjectType {
        name: "pnpm",
        marker_files: &["package.json", "pnpm-lock.yaml"],
        configure_cmd: Some("pnpm install"),
        compile_cmd: Some("pnpm build"),
        test_cmd: Some("pnpm test"),
        install_cmd: Some("pnpm install"),
        package_cmd: Some("pnpm build"),
        run_cmd: Some("pnpm start"),
    },
    ProjectType {
        name: "bun",
        marker_files: &["package.json", "bun.lockb", "bun.lock"],
        configure_cmd: Some("bun install"),
        compile_cmd: Some("bun run build"),
        test_cmd: Some("bun test"),
        install_cmd: Some("bun install"),
        package_cmd: Some("bun run build"),
        run_cmd: Some("bun start"),
    },
    ProjectType {
        name: "make",
        marker_files: &["Makefile", "makefile", "GNUmakefile"],
        configure_cmd: None,
        compile_cmd: Some("make"),
        test_cmd: Some("make test"),
        install_cmd: Some("make install"),
        package_cmd: None,
        run_cmd: None,
    },
    ProjectType {
        name: "just",
        marker_files: &["justfile", ".justfile", "Justfile"],
        configure_cmd: None,
        compile_cmd: Some("just build"),
        test_cmd: Some("just test"),
        install_cmd: Some("just install"),
        package_cmd: None,
        run_cmd: Some("just run"),
    },
    ProjectType {
        name: "python-poetry",
        marker_files: &["pyproject.toml"],
        configure_cmd: Some("poetry install"),
        compile_cmd: None,
        test_cmd: Some("poetry run pytest"),
        install_cmd: Some("poetry install"),
        package_cmd: Some("poetry build"),
        run_cmd: None,
    },
    ProjectType {
        name: "python",
        marker_files: &["setup.py", "requirements.txt"],
        configure_cmd: Some("pip install -r requirements.txt"),
        compile_cmd: None,
        test_cmd: Some("python -m pytest"),
        install_cmd: Some("pip install -r requirements.txt"),
        package_cmd: None,
        run_cmd: None,
    },
    ProjectType {
        name: "go",
        marker_files: &["go.mod"],
        configure_cmd: None,
        compile_cmd: Some("go build ./..."),
        test_cmd: Some("go test ./..."),
        install_cmd: Some("go install ./..."),
        package_cmd: None,
        run_cmd: Some("go run ."),
    },
    ProjectType {
        name: "deno",
        marker_files: &["deno.json", "deno.jsonc"],
        configure_cmd: None,
        compile_cmd: None,
        test_cmd: Some("deno test"),
        install_cmd: None,
        package_cmd: None,
        run_cmd: None,
    },
];

/// The four JS package managers this crate tells apart. See
/// [`detect_package_manager`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    /// `npm` (the fallback when no other manager's lockfile is present).
    Npm,
    /// `pnpm`, identified by `pnpm-lock.yaml`.
    Pnpm,
    /// `yarn`, identified by `yarn.lock`.
    Yarn,
    /// `bun`, identified by `bun.lockb` or `bun.lock`.
    Bun,
}

/// Detect the package manager from lockfile presence in a directory listing.
/// Defaults to [`PackageManager::Npm`] when no manager-specific lockfile is
/// present (including when `package.json` itself is absent — the caller is
/// expected to have already checked for `package.json` before calling this).
#[must_use]
pub fn detect_package_manager(dir_entries: &[&str]) -> PackageManager {
    if dir_entries.contains(&"pnpm-lock.yaml") {
        PackageManager::Pnpm
    } else if dir_entries.contains(&"yarn.lock") {
        PackageManager::Yarn
    } else if dir_entries.contains(&"bun.lockb") || dir_entries.contains(&"bun.lock") {
        PackageManager::Bun
    } else {
        PackageManager::Npm
    }
}

fn project_type_by_name(name: &str) -> Option<&'static ProjectType> {
    PROJECT_TYPES.iter().find(|t| t.name == name)
}

const JS_FAMILY: [&str; 4] = ["npm", "yarn", "pnpm", "bun"];

/// Resolve the one JS project type (`npm`/`yarn`/`pnpm`/`bun`) that applies
/// to a directory, or `None` if it has no `package.json`.
fn resolve_js_project_type(dir_entries: &[&str]) -> Option<&'static ProjectType> {
    if !dir_entries.contains(&"package.json") {
        return None;
    }
    let name = match detect_package_manager(dir_entries) {
        PackageManager::Npm => "npm",
        PackageManager::Pnpm => "pnpm",
        PackageManager::Yarn => "yarn",
        PackageManager::Bun => "bun",
    };
    project_type_by_name(name)
}

/// Detect which registered project type(s) a directory matches, by
/// marker-file presence. `dir_entries` is the list of filenames present in
/// that one directory (host-supplied — this crate does no filesystem I/O).
/// Order matches [`PROJECT_TYPES`] (most-specific first where that matters,
/// e.g. a directory with both `Cargo.toml` and `package.json` reports both
/// `cargo` and the detected JS type — a polyglot directory is not resolved
/// to a single type).
///
/// Two documented special cases, both because filename presence alone is
/// ambiguous:
///
/// - **The JS package-manager family** (`npm`/`yarn`/`pnpm`/`bun`) all share
///   the `package.json` marker. Rather than report all four whenever any one
///   lockfile is present, this function reports **at most one** of them —
///   the manager [`detect_package_manager`] identifies from the lockfile,
///   defaulting to `npm` when `package.json` is present with no
///   manager-specific lockfile.
/// - **`python-poetry`** additionally requires inspecting `pyproject.toml`'s
///   *content* for a `[tool.poetry]` table (a `pyproject.toml` alone is also
///   used by Flit, Hatch, and plain PEP 621 projects). This function never
///   returns `python-poetry`, even when `pyproject.toml` is present — use
///   [`detect_project_types_with_content`] for that. A bare `pyproject.toml`
///   with no `setup.py`/`requirements.txt` is consequently not classified by
///   this function at all.
#[must_use]
pub fn detect_project_types(dir_entries: &[&str]) -> Vec<&'static ProjectType> {
    let mut js_reported = false;
    let mut out = Vec::new();
    for ty in PROJECT_TYPES {
        if JS_FAMILY.contains(&ty.name) {
            if js_reported {
                continue;
            }
            js_reported = true;
            if let Some(resolved) = resolve_js_project_type(dir_entries) {
                out.push(resolved);
            }
            continue;
        }
        if ty.name == "python-poetry" {
            continue;
        }
        if ty.marker_files.iter().any(|m| dir_entries.contains(m)) {
            out.push(ty);
        }
    }
    out
}

/// As [`detect_project_types`], but also given `contents`: `(marker
/// filename, file body)` pairs for any marker files the host has already
/// read. Currently used only to distinguish `python-poetry` from a generic
/// `pyproject.toml`: if a `pyproject.toml` entry's body contains
/// `[tool.poetry]`, `python-poetry` is appended to the result (after the
/// marker-only matches).
#[must_use]
pub fn detect_project_types_with_content(
    dir_entries: &[&str],
    contents: &[(&str, &str)],
) -> Vec<&'static ProjectType> {
    let mut out = detect_project_types(dir_entries);
    let is_poetry = contents
        .iter()
        .any(|(name, body)| *name == "pyproject.toml" && body.contains("[tool.poetry]"));
    if is_poetry && let Some(poetry) = project_type_by_name("python-poetry") {
        out.push(poetry);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn detects_cargo() {
        let types = detect_project_types(&["Cargo.toml", "src"]);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].name, "cargo");
    }

    #[test]
    fn detects_zero_types_for_empty_dir() {
        assert!(detect_project_types(&[]).is_empty());
        assert!(detect_project_types(&["README.md"]).is_empty());
    }

    #[test]
    fn detects_polyglot_dir_reporting_both() {
        let types = detect_project_types(&["Cargo.toml", "package.json"]);
        let names: Vec<_> = types.iter().map(|t| t.name).collect();
        assert!(names.contains(&"cargo"));
        assert!(names.contains(&"npm"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn js_family_resolves_to_exactly_one_manager() {
        let npm = detect_project_types(&["package.json"]);
        assert_eq!(npm.iter().map(|t| t.name).collect::<Vec<_>>(), ["npm"]);

        let pnpm = detect_project_types(&["package.json", "pnpm-lock.yaml"]);
        assert_eq!(pnpm.iter().map(|t| t.name).collect::<Vec<_>>(), ["pnpm"]);

        let yarn = detect_project_types(&["package.json", "yarn.lock"]);
        assert_eq!(yarn.iter().map(|t| t.name).collect::<Vec<_>>(), ["yarn"]);

        let bun = detect_project_types(&["package.json", "bun.lock"]);
        assert_eq!(bun.iter().map(|t| t.name).collect::<Vec<_>>(), ["bun"]);
    }

    #[test]
    fn make_matches_any_of_its_marker_names() {
        assert_eq!(
            detect_project_types(&["makefile"])[0].name,
            detect_project_types(&["Makefile"])[0].name
        );
        assert_eq!(detect_project_types(&["GNUmakefile"])[0].name, "make");
    }

    #[test]
    fn plain_pyproject_toml_alone_is_unclassified_without_content() {
        assert!(detect_project_types(&["pyproject.toml"]).is_empty());
    }

    #[test]
    fn generic_python_detected_by_setup_py_or_requirements() {
        assert_eq!(detect_project_types(&["setup.py"])[0].name, "python");
        assert_eq!(
            detect_project_types(&["requirements.txt"])[0].name,
            "python"
        );
    }

    #[test]
    fn detect_with_content_promotes_poetry() {
        let dir = ["pyproject.toml"];
        let contents = [("pyproject.toml", "[tool.poetry]\nname = \"x\"\n")];
        let types = detect_project_types_with_content(&dir, &contents);
        assert_eq!(
            types.iter().map(|t| t.name).collect::<Vec<_>>(),
            ["python-poetry"]
        );
    }

    #[test]
    fn detect_with_content_leaves_non_poetry_pyproject_unclassified() {
        let dir = ["pyproject.toml"];
        let contents = [("pyproject.toml", "[build-system]\nrequires = []\n")];
        let types = detect_project_types_with_content(&dir, &contents);
        assert!(types.is_empty());
    }

    #[test]
    fn go_and_deno_detected() {
        assert_eq!(detect_project_types(&["go.mod"])[0].name, "go");
        assert_eq!(detect_project_types(&["deno.jsonc"])[0].name, "deno");
    }

    #[test]
    fn detect_package_manager_priority() {
        assert_eq!(
            detect_package_manager(&["package.json"]),
            PackageManager::Npm
        );
        assert_eq!(
            detect_package_manager(&["package.json", "pnpm-lock.yaml"]),
            PackageManager::Pnpm
        );
        assert_eq!(
            detect_package_manager(&["package.json", "yarn.lock"]),
            PackageManager::Yarn
        );
        assert_eq!(
            detect_package_manager(&["package.json", "bun.lockb"]),
            PackageManager::Bun
        );
    }

    proptest! {
        #[test]
        fn detect_project_types_never_panics(entries in proptest::collection::vec("[a-zA-Z0-9._-]{0,12}", 0..8)) {
            let refs: Vec<&str> = entries.iter().map(String::as_str).collect();
            let _ = detect_project_types(&refs);
        }
    }
}
