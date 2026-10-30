//! Kernelspecs: `kernels/<name>/kernel.json` under Jupyter's data
//! directories.
//!
//! [`data_dirs`] is the search path as a pure function of a [`PathEnv`]
//! snapshot, in the order `jupyter_core` uses: `$JUPYTER_PATH`, the user
//! data directory, each environment prefix's `share/jupyter`, then the
//! system directories. [`KernelSpec::parse`] reads one `kernel.json`, and
//! [`KernelSpec::argv_for`] substitutes `{connection_file}` and
//! `{resource_dir}` into its `argv`. Reading the directories is the
//! transport crate's job.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

/// Largest `kernel.json` [`KernelSpec::parse`] accepts.
pub const MAX_KERNEL_JSON_BYTES: usize = 64 * 1024;

/// Most `argv` entries a kernelspec may have.
pub const MAX_ARGV: usize = 256;

/// How a kernel wants to be interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterruptMode {
    /// Send the process SIGINT (the default).
    #[default]
    Signal,
    /// Send an `interrupt_request` on the control channel.
    Message,
}

/// One kernelspec.
#[derive(Debug, Clone, PartialEq)]
pub struct KernelSpec {
    /// The directory name, such as `python3`.
    pub name: String,
    /// The directory holding `kernel.json` (the `{resource_dir}`).
    pub dir: PathBuf,
    /// The command line, with `{connection_file}` to substitute.
    pub argv: Vec<String>,
    /// Human-readable name.
    pub display_name: String,
    /// The kernel's language, such as `python`.
    pub language: String,
    /// How to interrupt it.
    pub interrupt_mode: InterruptMode,
    /// Extra environment variables for the kernel process.
    pub env: BTreeMap<String, String>,
    /// Free-form metadata.
    pub metadata: Value,
}

/// Why a `kernel.json` was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecError {
    /// Larger than [`MAX_KERNEL_JSON_BYTES`].
    TooLarge,
    /// Not valid `kernel.json`.
    Malformed,
    /// The directory name is not a valid kernel name.
    BadName,
    /// `argv` is empty, too long, or has a NUL byte.
    BadArgv,
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "kernel.json too large",
            Self::Malformed => "malformed kernel.json",
            Self::BadName => "invalid kernel name",
            Self::BadArgv => "invalid kernel argv",
        })
    }
}

impl std::error::Error for SpecError {}

#[derive(Deserialize)]
struct RawSpec {
    argv: Vec<String>,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    interrupt_mode: InterruptMode,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    metadata: Value,
}

/// Whether `name` is a valid kernel name: ASCII letters, digits, `.`, `_`
/// and `-` (as `jupyter_client` requires).
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

impl KernelSpec {
    /// Parse the `kernel.json` text of kernel `name` found in `dir`.
    ///
    /// # Errors
    ///
    /// A [`SpecError`] when the file is too large, malformed, or names an
    /// unusable command.
    pub fn parse(name: &str, dir: &Path, json: &str) -> Result<Self, SpecError> {
        if json.len() > MAX_KERNEL_JSON_BYTES {
            return Err(SpecError::TooLarge);
        }
        if !is_valid_name(name) {
            return Err(SpecError::BadName);
        }
        let raw: RawSpec = serde_json::from_str(json).map_err(|_| SpecError::Malformed)?;
        if raw.argv.is_empty()
            || raw.argv.len() > MAX_ARGV
            || raw.argv[0].is_empty()
            || raw.argv.iter().any(|a| a.contains('\0'))
        {
            return Err(SpecError::BadArgv);
        }
        Ok(Self {
            name: name.to_owned(),
            dir: dir.to_owned(),
            display_name: if raw.display_name.is_empty() {
                name.to_owned()
            } else {
                raw.display_name
            },
            argv: raw.argv,
            language: raw.language,
            interrupt_mode: raw.interrupt_mode,
            env: raw.env,
            metadata: raw.metadata,
        })
    }

    /// `argv` with `{connection_file}` and `{resource_dir}` substituted.
    /// Other `{…}` placeholders are left as they are.
    #[must_use]
    pub fn argv_for(&self, connection_file: &Path) -> Vec<String> {
        let file = connection_file.to_string_lossy();
        let dir = self.dir.to_string_lossy();
        self.argv
            .iter()
            .map(|arg| {
                arg.replace("{connection_file}", &file)
                    .replace("{resource_dir}", &dir)
            })
            .collect()
    }

    /// Whether this kernel runs `language`. Case-insensitive, and an
    /// `ob-jupyter` style `jupyter-python` matches `python`.
    #[must_use]
    pub fn runs(&self, language: &str) -> bool {
        normalize_language(language) == normalize_language(&self.language)
    }
}

/// `language` lowercased, without an `ob-jupyter` `jupyter-` prefix.
#[must_use]
pub fn normalize_language(language: &str) -> String {
    let lower = language.trim().to_ascii_lowercase();
    match lower.strip_prefix("jupyter-") {
        Some(rest) => rest.to_owned(),
        None => lower,
    }
}

/// The operating-system family, for the directory layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
    /// Linux and the other Unixes.
    Unix,
}

impl Os {
    /// The family this binary was built for.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// A snapshot of the environment the search path depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEnv {
    /// The OS family.
    pub os: Os,
    /// `$JUPYTER_PATH`.
    pub jupyter_path: Option<String>,
    /// `$JUPYTER_DATA_DIR`.
    pub jupyter_data_dir: Option<PathBuf>,
    /// The home directory.
    pub home: Option<PathBuf>,
    /// `$XDG_DATA_HOME` (Unix).
    pub xdg_data_home: Option<PathBuf>,
    /// `%APPDATA%` (Windows).
    pub appdata: Option<PathBuf>,
    /// `%PROGRAMDATA%` (Windows).
    pub programdata: Option<PathBuf>,
    /// Python environment prefixes (`sys.prefix`), best first.
    pub prefixes: Vec<PathBuf>,
}

impl PathEnv {
    /// An empty snapshot for `os`.
    #[must_use]
    pub fn empty(os: Os) -> Self {
        Self {
            os,
            jupyter_path: None,
            jupyter_data_dir: None,
            home: None,
            xdg_data_home: None,
            appdata: None,
            programdata: None,
            prefixes: Vec::new(),
        }
    }
}

fn non_empty(path: Option<&PathBuf>) -> Option<&PathBuf> {
    path.filter(|p| !p.as_os_str().is_empty())
}

/// The user data directory (`jupyter --data-dir`).
#[must_use]
pub fn user_data_dir(env: &PathEnv) -> Option<PathBuf> {
    if let Some(dir) = non_empty(env.jupyter_data_dir.as_ref()) {
        return Some(dir.clone());
    }
    match env.os {
        Os::MacOs => env.home.as_ref().map(|h| h.join("Library").join("Jupyter")),
        Os::Windows => non_empty(env.appdata.as_ref())
            .map(|a| a.join("jupyter"))
            .or_else(|| env.home.as_ref().map(|h| h.join(".jupyter").join("data"))),
        Os::Unix => non_empty(env.xdg_data_home.as_ref())
            .map(|x| x.join("jupyter"))
            .or_else(|| {
                env.home
                    .as_ref()
                    .map(|h| h.join(".local").join("share").join("jupyter"))
            }),
    }
}

/// Jupyter's data directories, best first, without duplicates.
#[must_use]
pub fn data_dirs(env: &PathEnv) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let separator = if env.os == Os::Windows { ';' } else { ':' };
    if let Some(list) = &env.jupyter_path {
        dirs.extend(
            list.split(separator)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        );
    }
    dirs.extend(user_data_dir(env));
    dirs.extend(env.prefixes.iter().map(|p| p.join("share").join("jupyter")));
    match env.os {
        Os::Windows => dirs.extend(non_empty(env.programdata.as_ref()).map(|p| p.join("jupyter"))),
        Os::MacOs | Os::Unix => {
            dirs.push(PathBuf::from("/usr/local/share/jupyter"));
            dirs.push(PathBuf::from("/usr/share/jupyter"));
        }
    }
    let mut seen = Vec::new();
    dirs.retain(|d| {
        if seen.contains(d) {
            false
        } else {
            seen.push(d.clone());
            true
        }
    });
    dirs
}

/// The `kernels` directory under each of [`data_dirs`].
#[must_use]
pub fn kernel_dirs(env: &PathEnv) -> Vec<PathBuf> {
    data_dirs(env)
        .into_iter()
        .map(|d| d.join("kernels"))
        .collect()
}

/// Virtual-environment folders, relative to a workspace root, whose
/// `share/jupyter/kernels` [`gate_project_dirs`] adds in a trusted
/// workspace.
pub const PROJECT_ENVS: [&str; 2] = [".venv", "venv"];

/// The `kernels` directories to search for a workspace at `root`, given
/// the standard `dirs` (T734f): in a `trusted` workspace, `dirs` plus each
/// [`PROJECT_ENVS`] environment's `share/jupyter/kernels`; otherwise `dirs`
/// without any directory inside `root`, so a cloned repository cannot
/// supply the program a kernel runs. Paths are compared as given, so the
/// caller passes them canonicalized.
#[must_use]
pub fn gate_project_dirs(dirs: Vec<PathBuf>, root: &Path, trusted: bool) -> Vec<PathBuf> {
    if !trusted {
        return dirs.into_iter().filter(|d| !d.starts_with(root)).collect();
    }
    let mut out = dirs;
    for env in PROJECT_ENVS {
        let dir = root.join(env).join("share").join("jupyter").join("kernels");
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_dirs_need_trust() {
        let root = Path::new("/w");
        let dirs = vec![
            PathBuf::from("/home/u/.local/share/jupyter/kernels"),
            PathBuf::from("/w/.venv/share/jupyter/kernels"),
        ];
        assert_eq!(
            gate_project_dirs(dirs.clone(), root, false),
            [PathBuf::from("/home/u/.local/share/jupyter/kernels")]
        );
        assert_eq!(
            gate_project_dirs(dirs, root, true),
            [
                PathBuf::from("/home/u/.local/share/jupyter/kernels"),
                PathBuf::from("/w/.venv/share/jupyter/kernels"),
                PathBuf::from("/w/venv/share/jupyter/kernels"),
            ]
        );
        // `/wx` is not inside `/w`.
        assert_eq!(
            gate_project_dirs(vec![PathBuf::from("/wx/kernels")], root, false).len(),
            1
        );
    }

    const IPYKERNEL: &str = r#"{
     "argv": ["python", "-m", "ipykernel_launcher", "-f", "{connection_file}"],
     "display_name": "Python 3 (ipykernel)",
     "language": "python",
     "metadata": {"debugger": true}
    }"#;

    #[test]
    fn parses_ipykernel_and_substitutes() {
        let spec = KernelSpec::parse("python3", Path::new("/k/python3"), IPYKERNEL).unwrap();
        assert_eq!(spec.display_name, "Python 3 (ipykernel)");
        assert_eq!(spec.interrupt_mode, InterruptMode::Signal);
        assert_eq!(
            spec.argv_for(Path::new("/run/k.json")),
            vec!["python", "-m", "ipykernel_launcher", "-f", "/run/k.json"]
        );
        assert!(spec.runs("Python"));
        assert!(spec.runs("jupyter-python"));
        assert!(!spec.runs("r"));
    }

    #[test]
    fn rejects_bad_specs() {
        let dir = Path::new("/k");
        assert_eq!(
            KernelSpec::parse("../x", dir, IPYKERNEL),
            Err(SpecError::BadName)
        );
        assert_eq!(KernelSpec::parse("x", dir, "[]"), Err(SpecError::Malformed));
        assert_eq!(
            KernelSpec::parse("x", dir, r#"{"argv": []}"#),
            Err(SpecError::BadArgv)
        );
        assert_eq!(
            KernelSpec::parse("x", dir, "{\"argv\": [\"a\\u0000b\"]}"),
            Err(SpecError::BadArgv)
        );
        let big = format!("{{\"argv\": [\"{}\"]}}", "a".repeat(MAX_KERNEL_JSON_BYTES));
        assert_eq!(KernelSpec::parse("x", dir, &big), Err(SpecError::TooLarge));
    }

    #[test]
    fn message_interrupt_mode_and_env() {
        let spec = KernelSpec::parse(
            "evcxr",
            Path::new("/k"),
            r#"{"argv": ["evcxr_jupyter", "--control_file", "{connection_file}"], "language": "rust", "interrupt_mode": "message", "env": {"A": "1"}}"#,
        )
        .unwrap();
        assert_eq!(spec.interrupt_mode, InterruptMode::Message);
        assert_eq!(spec.display_name, "evcxr");
        assert_eq!(spec.env.get("A").map(String::as_str), Some("1"));
    }

    #[test]
    fn macos_search_path() {
        let mut env = PathEnv::empty(Os::MacOs);
        env.home = Some("/Users/a".into());
        env.jupyter_path = Some("/opt/j:/opt/k:".into());
        env.prefixes = vec!["/Users/a/.venv".into()];
        assert_eq!(
            kernel_dirs(&env),
            [
                "/opt/j/kernels",
                "/opt/k/kernels",
                "/Users/a/Library/Jupyter/kernels",
                "/Users/a/.venv/share/jupyter/kernels",
                "/usr/local/share/jupyter/kernels",
                "/usr/share/jupyter/kernels",
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn linux_xdg_and_data_dir_override() {
        let mut env = PathEnv::empty(Os::Unix);
        env.home = Some("/home/a".into());
        assert_eq!(
            user_data_dir(&env),
            Some(PathBuf::from("/home/a/.local/share/jupyter"))
        );
        env.xdg_data_home = Some("/xdg".into());
        assert_eq!(user_data_dir(&env), Some(PathBuf::from("/xdg/jupyter")));
        env.jupyter_data_dir = Some("/data".into());
        assert_eq!(user_data_dir(&env), Some(PathBuf::from("/data")));
    }

    #[test]
    fn windows_paths_and_dedup() {
        let mut env = PathEnv::empty(Os::Windows);
        env.appdata = Some(PathBuf::from("C:\\Users\\a\\AppData\\Roaming"));
        env.programdata = Some(PathBuf::from("C:\\ProgramData"));
        env.jupyter_path = Some("C:\\j;C:\\j".into());
        let dirs = data_dirs(&env);
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs[0], PathBuf::from("C:\\j"));
        assert_eq!(
            dirs[1],
            PathBuf::from("C:\\Users\\a\\AppData\\Roaming").join("jupyter")
        );
    }
}
