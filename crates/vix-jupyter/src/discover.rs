//! Find installed kernelspecs.
//!
//! [`path_env`] snapshots the process environment into a
//! [`PathEnv`]; [`kernelspecs`] reads
//! `kernels/*/kernel.json` under each of its directories. A name found in an
//! earlier directory hides the same name later on, as in Jupyter. Bad or
//! oversized `kernel.json` files are skipped, and the scan is bounded.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use vix_jupyter_core::kernelspec::{self, MAX_KERNEL_JSON_BYTES, Os};
use vix_jupyter_core::{KernelSpec, PathEnv};

/// Most kernel directories read per data directory.
pub const MAX_KERNELS_PER_DIR: usize = 256;

/// Read an environment variable as a path, treating an unset or empty value as absent (an empty `JUPYTER_DATA_DIR=` must not mean "the current directory").
fn var_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The environment prefixes whose `share/jupyter` to search: an active
/// virtualenv or conda environment, then the prefix of the first `python3`
/// or `python` on `PATH` (`<prefix>/bin/python3`, or `<prefix>\python.exe`
/// on Windows).
#[must_use]
pub fn python_prefixes() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["VIRTUAL_ENV", "CONDA_PREFIX"]
        .iter()
        .filter_map(|v| var_path(v))
        .collect();
    if let Some(path) = env::var_os("PATH") {
        'search: for dir in env::split_paths(&path) {
            for exe in ["python3", "python", "python3.exe", "python.exe"] {
                if dir.join(exe).is_file() {
                    let prefix = if cfg!(windows) {
                        Some(dir)
                    } else {
                        dir.parent().map(Path::to_path_buf)
                    };
                    out.extend(prefix);
                    break 'search;
                }
            }
        }
    }
    out.dedup();
    out
}

/// A snapshot of this process's environment for the kernelspec search.
#[must_use]
pub fn path_env() -> PathEnv {
    PathEnv {
        os: Os::current(),
        jupyter_path: env::var("JUPYTER_PATH").ok(),
        jupyter_data_dir: var_path("JUPYTER_DATA_DIR"),
        home: var_path(if cfg!(windows) { "USERPROFILE" } else { "HOME" }),
        xdg_data_home: var_path("XDG_DATA_HOME"),
        appdata: var_path("APPDATA"),
        programdata: var_path("PROGRAMDATA"),
        prefixes: python_prefixes(),
    }
}

/// Read a kernel.json for at most `MAX_KERNEL_JSON_BYTES + 1` bytes, so an oversized or hostile file costs bounded memory. The extra byte lets the parser see that the cap was exceeded and reject it, instead of silently truncating.
fn read_capped(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut text = String::new();
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "RCA-EXC-2: MAX_KERNEL_JSON_BYTES is a small constant, so one more than it fits u64"
    )]
    let cap = u64::try_from(MAX_KERNEL_JSON_BYTES).unwrap_or(u64::MAX) + 1;
    file.take(cap).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The kernelspecs under `kernel_dirs` (each a `…/kernels` directory), best
/// directory first, sorted by name within it, without duplicate names.
#[must_use]
pub fn kernelspecs_in(kernel_dirs: &[PathBuf]) -> Vec<KernelSpec> {
    let mut found: Vec<KernelSpec> = Vec::new();
    for dir in kernel_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut here: Vec<KernelSpec> = entries
            .filter_map(Result::ok)
            .take(MAX_KERNELS_PER_DIR)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let spec_dir = entry.path();
                let text = read_capped(&spec_dir.join("kernel.json"))?;
                KernelSpec::parse(&name, &spec_dir, &text).ok()
            })
            .filter(|spec| !found.iter().any(|f| f.name == spec.name))
            .collect();
        here.sort_by(|a, b| a.name.cmp(&b.name));
        found.extend(here);
    }
    found
}

/// The kernelspecs installed for this environment.
#[must_use]
pub fn kernelspecs(env: &PathEnv) -> Vec<KernelSpec> {
    kernelspecs_in(&kernelspec::kernel_dirs(env))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "vix-jupyter-discover-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn write_spec(kernels: &Path, name: &str, json: &str) {
        let dir = kernels.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("kernel.json"), json).unwrap();
    }

    #[test]
    fn first_directory_wins_and_bad_specs_are_skipped() {
        let root = scratch("order");
        let a = root.join("a/kernels");
        let b = root.join("b/kernels");
        write_spec(
            &a,
            "python3",
            r#"{"argv": ["a-python", "{connection_file}"], "language": "python"}"#,
        );
        write_spec(
            &b,
            "python3",
            r#"{"argv": ["b-python"], "language": "python"}"#,
        );
        write_spec(&b, "ir", r#"{"argv": ["R"], "language": "R"}"#);
        write_spec(&b, "broken", "{nope");
        write_spec(&b, "bad name", r#"{"argv": ["x"]}"#);
        let specs = kernelspecs_in(&[a, b, root.join("missing")]);
        let names: Vec<(&str, &str)> = specs
            .iter()
            .map(|s| (s.name.as_str(), s.argv[0].as_str()))
            .collect();
        assert_eq!(names, [("python3", "a-python"), ("ir", "R")]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn oversized_kernel_json_is_skipped() {
        let root = scratch("big");
        let k = root.join("kernels");
        let big = format!(
            r#"{{"argv": ["x"], "display_name": "{}"}}"#,
            "a".repeat(MAX_KERNEL_JSON_BYTES)
        );
        write_spec(&k, "big", &big);
        assert_eq!(kernelspecs_in(&[k]), Vec::new());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn var_path_treats_unset_and_empty_as_absent() {
        assert_eq!(var_path("VIX_JUPYTER_TEST_SURELY_UNSET_VARIABLE"), None);
        // PATH is set in every environment the tests run in.
        if env::var_os("PATH").is_some_and(|v| !v.is_empty()) {
            assert!(var_path("PATH").is_some());
        }
    }

    #[test]
    fn python_prefixes_lead_with_the_active_environments() {
        let prefixes = python_prefixes();
        let mut expected: Vec<PathBuf> = ["VIRTUAL_ENV", "CONDA_PREFIX"]
            .iter()
            .filter_map(|v| var_path(v))
            .collect();
        // Every active-environment prefix comes first, in that order; the
        // PATH python (if any) follows, and neighbours are never repeated.
        expected.dedup();
        assert!(prefixes.starts_with(&expected));
        assert!(prefixes.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn path_env_snapshots_the_process_environment() {
        let snapshot = path_env();
        assert_eq!(snapshot.os, Os::current());
        assert_eq!(snapshot.jupyter_path, env::var("JUPYTER_PATH").ok());
        assert_eq!(snapshot.jupyter_data_dir, var_path("JUPYTER_DATA_DIR"));
        assert_eq!(snapshot.xdg_data_home, var_path("XDG_DATA_HOME"));
        assert_eq!(snapshot.appdata, var_path("APPDATA"));
        assert_eq!(snapshot.programdata, var_path("PROGRAMDATA"));
        let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        assert_eq!(snapshot.home, var_path(home));
        assert_eq!(snapshot.prefixes, python_prefixes());
    }

    #[test]
    fn kernelspecs_follow_the_environment_directories() {
        let root = scratch("env");
        write_spec(
            &root.join("kernels"),
            "viaenv",
            r#"{"argv": ["e"], "language": "python"}"#,
        );
        // JUPYTER_PATH-style search path: its `kernels` subdirectory is read.
        let mut penv = PathEnv::empty(Os::Unix);
        penv.jupyter_path = Some(root.to_string_lossy().into_owned());
        let specs = kernelspecs(&penv);
        assert!(specs.iter().any(|s| s.name == "viaenv"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn directories_are_capped_and_unreadable_specs_skipped() {
        let root = scratch("cap");
        let k = root.join("kernels");
        // A directory entry with no kernel.json is not a kernel.
        fs::create_dir_all(k.join("empty")).unwrap();
        write_spec(&k, "good", r#"{"argv": ["g"]}"#);
        let specs = kernelspecs_in(&[k]);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "good");
        let _ = fs::remove_dir_all(&root);
    }
}
