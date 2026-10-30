//! Format with the user's own Prettier (T683).
//!
//! Vix never bundles Node or Prettier. [`resolve_binary`] finds the
//! project-local `node_modules/.bin/prettier` first and falls back to `PATH`;
//! `format` pipes the buffer through `prettier --stdin-filepath <path>` so
//! Prettier itself honors `.prettierrc*`, `.editorconfig` and
//! `.prettierignore` (an ignored file comes back unchanged).
//!
//! The process runs with a timeout and is killed when it overruns, so a hung
//! Prettier cannot wedge the editor.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "process")]
pub use crate::run::format;

/// How long a Prettier run may take before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Prettier-supported languages: `(language id, file extensions)`.
///
/// The language id is the key users list in `save.prettier_languages`.
pub const LANGUAGES: &[(&str, &[&str])] = &[
    ("javascript", &["js", "jsx", "mjs", "cjs"]),
    ("typescript", &["ts", "tsx", "mts", "cts"]),
    ("json", &["json", "jsonc", "json5"]),
    ("css", &["css", "scss", "less"]),
    ("html", &["html", "htm", "vue"]),
    ("markdown", &["md", "markdown", "mdx"]),
    ("yaml", &["yml", "yaml"]),
    ("graphql", &["graphql", "gql"]),
    ("handlebars", &["hbs", "handlebars"]),
];

/// Why a Prettier run failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum PrettierError {
    /// No `prettier` was found in a project `node_modules/.bin` or on `PATH`.
    NotFound,
    /// The process could not be started or talked to.
    Io(std::io::Error),
    /// Prettier ran longer than the timeout and was killed.
    TimedOut,
    /// Prettier exited non-zero (usually a syntax error); carries its stderr.
    Failed(String),
    /// Prettier's output was not valid UTF-8.
    BadOutput,
}

impl fmt::Display for PrettierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrettierError::NotFound => f.write_str("prettier not found"),
            PrettierError::Io(e) => write!(f, "prettier I/O error: {e}"),
            PrettierError::TimedOut => f.write_str("prettier timed out"),
            PrettierError::Failed(m) => write!(f, "prettier failed: {m}"),
            PrettierError::BadOutput => f.write_str("prettier output was not UTF-8"),
        }
    }
}

impl std::error::Error for PrettierError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PrettierError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// The Prettier language id for `path`'s extension, if Prettier supports it.
#[must_use]
pub fn language_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(_, exts)| exts.contains(&ext.as_str()))
        .map(|(id, _)| *id)
}

/// Whether `path` is a Prettier language listed in `enabled` (language ids).
#[must_use]
pub fn is_enabled_for(path: &Path, enabled: &[String]) -> bool {
    language_of(path).is_some_and(|lang| enabled.iter().any(|e| e.eq_ignore_ascii_case(lang)))
}

fn exe_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["prettier.cmd", "prettier.exe", "prettier"]
    } else {
        &["prettier"]
    }
}

/// Locate Prettier for `file`: when `allow_project_local`, the nearest
/// `node_modules/.bin/prettier` walking up from the file's directory; then
/// the first match on `path_var` (the value of `PATH`).
///
/// A project-local binary is project-controlled code, so callers pass
/// `allow_project_local = false` for an untrusted workspace.
#[must_use]
pub fn resolve_binary(
    file: &Path,
    path_var: Option<&std::ffi::OsStr>,
    allow_project_local: bool,
) -> Option<PathBuf> {
    let start = file.parent()?;
    // The project-local binary lives in the workspace, so it is looked up
    // through the authority (T710); `PATH` is this machine's.
    let auth = vix_authority::current();
    for dir in start.ancestors().filter(|_| allow_project_local) {
        let bin = dir.join("node_modules").join(".bin");
        if let Some(found) = exe_names()
            .iter()
            .map(|n| bin.join(n))
            .find(|p| auth.is_file(p))
        {
            return Some(found);
        }
    }
    let path_var = path_var?;
    std::env::split_paths(path_var)
        .flat_map(|d| exe_names().iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

#[cfg(all(test, unix, feature = "process"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, rel: &str, body: &str) -> std::io::Result<PathBuf> {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n"))?;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))?;
        Ok(p)
    }

    fn tmp(tag: &str) -> std::io::Result<PathBuf> {
        let d = std::env::temp_dir().join(format!("vix-prettier-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    #[test]
    fn language_detection() {
        assert_eq!(language_of(Path::new("a/b.TSX")), Some("typescript"));
        assert_eq!(language_of(Path::new("a.rs")), None);
        let on = vec!["typescript".to_owned()];
        assert!(is_enabled_for(Path::new("x.ts"), &on));
        assert!(!is_enabled_for(Path::new("x.css"), &on));
        assert!(!is_enabled_for(Path::new("x.ts"), &[]));
    }

    #[test]
    fn project_local_beats_path() {
        let d = tmp("resolve").unwrap();
        let local = script(&d, "node_modules/.bin/prettier", "cat").unwrap();
        let global = script(&d, "global/prettier", "cat").unwrap();
        let file = d.join("src/a.ts");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let path_var = std::env::join_paths([d.join("global")]).unwrap();
        assert_eq!(resolve_binary(&file, Some(&path_var), true), Some(local));
        let other = tmp("resolve2").unwrap();
        assert_eq!(
            resolve_binary(&other.join("a.ts"), Some(&path_var), true),
            Some(global.clone())
        );
        assert_eq!(resolve_binary(&other.join("a.ts"), None, true), None);
        // Untrusted workspace: the project-local binary is skipped.
        assert_eq!(resolve_binary(&file, Some(&path_var), false), Some(global));
    }

    #[test]
    fn formats_via_stdin_and_passes_filepath() {
        let d = tmp("fmt").unwrap();
        let bin = script(&d, "p", r#"echo "// $1 $2"; cat"#).unwrap();
        let file = d.join("a.ts");
        let out = format(&bin, &file, "let x=1\n", TIMEOUT).unwrap();
        assert_eq!(
            out,
            format!("// --stdin-filepath {}\nlet x=1\n", file.display())
        );
    }

    #[test]
    fn nonzero_exit_is_failed_with_stderr() {
        let d = tmp("fail").unwrap();
        let bin = script(&d, "p", "echo 'SyntaxError' >&2; exit 2").unwrap();
        match format(&bin, &d.join("a.ts"), "x", TIMEOUT) {
            Err(PrettierError::Failed(m)) => assert_eq!(m, "SyntaxError"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn hang_times_out() {
        let d = tmp("hang").unwrap();
        let bin = script(&d, "p", "sleep 30").unwrap();
        let r = format(&bin, &d.join("a.ts"), "x", Duration::from_millis(100));
        assert!(matches!(r, Err(PrettierError::TimedOut)), "{r:?}");
    }

    #[test]
    fn missing_binary_is_io_error() {
        let r = format(
            Path::new("/nonexistent/prettier"),
            Path::new("a.ts"),
            "x",
            TIMEOUT,
        );
        assert!(matches!(r, Err(PrettierError::Io(_))));
    }
}
