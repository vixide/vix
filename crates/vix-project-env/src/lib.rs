//! Project environment activation (T667).
//!
//! A project's `.venv`, direnv `.envrc` or mise config changes which
//! interpreter and packages a language server, task or terminal should see.
//! [`resolve`] works out that environment once per workspace; [`set_active`]
//! publishes it, and [`apply_to`] / [`active_vars`] are called where `vix-lsp`,
//! the task runner and `vix-terminal` build their commands.
//!
//! **Trust.** `direnv` and `mise` execute project-controlled code (an
//! `.envrc` is a shell script), so [`resolve`] runs them only when the
//! caller passes `trusted == true`. The `.venv` source only reads files and
//! always runs. An untrusted workspace that has a code-running source reports
//! it in [`Resolved::blocked`] instead of running it.
//!
//! Starting `direnv`/`mise` needs the `process` Cargo feature (enabled by
//! Vix's `tasks` capability); without it they are never started.
//!
//! **Order and precedence.** [`resolve_with`] tries, in order: a user-defined
//! [`Detector`] from `.vix/env.toml`, then direnv, then mise, then the `.venv`
//! (see [`Source`]). The first that yields variables wins; the `.venv` is the
//! fallback and is also what remains when a trusted tool prints nothing.
//!
//! **Process-global state.** The active environment lives in one `static`
//! behind a mutex ([`set_active`]), because the consumers (language-server
//! launch, task runner, terminal) are in different crates and have no shared
//! context to carry it. All pure functions here take explicit arguments and
//! are what the tests exercise. App side: `tui/src/app/project_env.rs`.
//! Owning spec: `crates/vix-project-env/spec/index.md`.
//!
//! ```
//! use std::path::Path;
//! use std::time::Duration;
//! use vix_project_env::resolve_with;
//!
//! // An empty directory has no environment; nothing is run.
//! let never = |_: &str, _: &[&str], _: &Path, _: Duration, _: &[(String, String)]| -> Option<String> { unreachable!() };
//! let dir = std::env::temp_dir().join("vix-project-env-doctest");
//! std::fs::create_dir_all(&dir).unwrap();
//! let resolved = resolve_with(&dir, false, None, &never, None);
//! assert!(resolved.env.is_none() && resolved.blocked.is_none());
//! ```

#[cfg(feature = "process")]
mod run;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Where an environment came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source {
    /// A Python virtual environment (`.venv/` or `venv/`); reads files only.
    Venv,
    /// `direnv export json`; runs the project's `.envrc`.
    Direnv,
    /// `mise env --json`; runs mise (which may run project-defined hooks).
    Mise,
    /// A user-defined detector from `.vix/env.toml`; runs its own command.
    Custom,
}

impl Source {
    /// A short stable label for the status bar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Source::Venv => "venv",
            Source::Direnv => "direnv",
            Source::Mise => "mise",
            Source::Custom => "custom",
        }
    }

    /// Whether resolving this source runs project-controlled code, so
    /// needs a trusted workspace.
    #[must_use]
    pub fn runs_project_code(self) -> bool {
        !matches!(self, Source::Venv)
    }
}

/// A resolved environment: variable overrides for spawned processes.
///
/// It is a *delta* applied on top of the inherited environment, not a full
/// environment: `Some` sets a variable and `None` removes it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectEnv {
    /// Where it came from.
    pub source: Source,
    /// The `name` of the user-defined detector that produced it (only for
    /// [`Source::Custom`]).
    pub name: Option<String>,
    /// Variables to set (`Some`) or remove (`None`), in a stable order
    /// (`BTreeMap`, so status text and tests are deterministic).
    pub vars: BTreeMap<String, Option<String>>,
    /// The extra variables the detector's own command was started with (its
    /// `env` table; empty for built-in sources). They apply to that command
    /// only, never to spawned processes, and are kept so the UI can show
    /// what ran.
    pub command_env: BTreeMap<String, String>,
}

/// The result of [`resolve`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolved {
    /// The environment to activate, if any source produced one.
    pub env: Option<ProjectEnv>,
    /// A code-running source that exists but was NOT run because the
    /// workspace is untrusted.
    pub blocked: Option<Source>,
}

impl ProjectEnv {
    /// A new environment with no detector name.
    #[must_use]
    pub fn new(source: Source, vars: BTreeMap<String, Option<String>>) -> ProjectEnv {
        ProjectEnv {
            source,
            name: None,
            vars,
            command_env: BTreeMap::new(),
        }
    }

    /// The status-bar label: the user detector's own name, else the source's.
    #[must_use]
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or_else(|| self.source.label())
    }
}

/// Runs an external program in a directory, killed after the given time, and
/// returns its stdout. A seam so tests can prove nothing is executed for an
/// untrusted workspace.
///
/// The last argument is the extra environment (name, value) to give the
/// command on top of the inherited one; empty for `direnv`/`mise`.
pub type Runner<'a> =
    &'a dyn Fn(&str, &[&str], &Path, Duration, &[(String, String)]) -> Option<String>;

/// How long a tool gets before it is killed unless a detector says otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest per-detector `timeout_secs` accepted.
pub const MAX_TIMEOUT_SECS: u64 = 60;

/// The most detectors one `.vix/env.toml` may define.
pub const MAX_DETECTORS: usize = 16;

/// A user-defined environment detector (`[[detector]]` in `.vix/env.toml`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Detector {
    /// A label for the detector (at most 64 characters).
    pub name: String,
    /// The detector applies when any of these paths (relative to the
    /// workspace root) exists.
    pub detect: Vec<String>,
    /// Program and arguments, run directly (no shell) in the root; prints the
    /// same JSON object as `direnv export json`.
    pub command: Vec<String>,
    /// Seconds the command may run before it is killed (1 to
    /// [`MAX_TIMEOUT_SECS`]); [`DEFAULT_TIMEOUT`] when absent.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Extra environment variables for the command only (plain strings, no
    /// shell expansion). Names must pass [`env_name_allowed`].
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// The most `env` entries one detector may declare.
pub const MAX_DETECTOR_ENV: usize = 32;

/// The longest `env` name or value accepted, in bytes.
pub const MAX_ENV_LEN: usize = 4096;

/// Whether `name` may be set for a detector's command: a plain identifier
/// (`[A-Za-z_][A-Za-z0-9_]*`, at most 128 bytes) that is not a dynamic-loader
/// injection variable (`LD_*`, `DYLD_*`, any case). `PATH` is allowed on
/// purpose (it changes how the command itself is found); a trusted workspace
/// could run any program anyway.
#[must_use]
pub fn env_name_allowed(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let upper = name.to_ascii_uppercase();
    first_ok
        && name.len() <= 128
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !upper.starts_with("LD_")
        && !upper.starts_with("DYLD_")
}

/// The `.vix/env.toml` file shape: zero or more `[[detector]]` tables.
#[derive(serde::Deserialize)]
struct DetectorFile {
    #[serde(default)]
    detector: Vec<Detector>,
}

impl Detector {
    /// Whether the detector is well-formed and safe to evaluate.
    ///
    /// `detect` paths must be relative and free of `..`, so a checked-in
    /// config cannot probe files outside the project; NUL bytes are refused
    /// because they cannot be passed to a process or path API.
    fn valid(&self) -> bool {
        let rel_ok = |p: &String| {
            let path = Path::new(p);
            // Plain names only: on Windows `/etc/passwd` and `C:x` count as
            // relative, yet `root.join` would leave the project (T772).
            !p.is_empty()
                && !p.contains('\0')
                && path.components().all(|c| {
                    matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
        };
        !self.name.is_empty()
            && self.name.chars().count() <= 64
            && !self.detect.is_empty()
            && self.detect.iter().all(rel_ok)
            && !self.command.is_empty()
            && self.command.iter().all(|a| !a.contains('\0'))
            && !self.command[0].is_empty()
            && self
                .timeout_secs
                .is_none_or(|t| (1..=MAX_TIMEOUT_SECS).contains(&t))
            && self.env.len() <= MAX_DETECTOR_ENV
            && self
                .env
                .iter()
                .all(|(k, v)| env_name_allowed(k) && v.len() <= MAX_ENV_LEN && !v.contains('\0'))
    }

    /// The extra environment as (name, value) pairs, in name order.
    fn command_env(&self) -> Vec<(String, String)> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// How long the command may run.
    fn timeout(&self) -> Duration {
        self.timeout_secs
            .map_or(DEFAULT_TIMEOUT, Duration::from_secs)
    }

    /// Whether any of the detector's files exists under `root` (`exists`,
    /// not `is_file`, so a directory marker also counts).
    fn applies(&self, root: &Path) -> bool {
        self.detect.iter().any(|f| root.join(f).exists())
    }
}

/// Parse the text of a `.vix/env.toml`. Any problem (bad TOML, too many
/// detectors, an invalid detector) rejects the whole file: `None`.
#[must_use]
pub fn parse_detectors(text: &str) -> Option<Vec<Detector>> {
    // All-or-nothing on purpose: a half-applied file could silently run a
    // different command than its author intended.
    let file: DetectorFile = toml::from_str(text).ok()?;
    if file.detector.len() > MAX_DETECTORS || !file.detector.iter().all(Detector::valid) {
        return None;
    }
    Some(file.detector)
}

/// The workspace's valid user-defined detectors (empty when there is no
/// `.vix/env.toml`, or it is invalid).
#[must_use]
pub fn load_detectors(root: &Path) -> Vec<Detector> {
    std::fs::read_to_string(root.join(".vix").join("env.toml"))
        .ok()
        .and_then(|t| parse_detectors(&t))
        .unwrap_or_default()
}

/// The valid detectors of a user-level detector file (empty when it is absent
/// or invalid). Same format as `.vix/env.toml`.
#[must_use]
pub fn load_user_detectors(user_file: Option<&Path>) -> Vec<Detector> {
    user_file
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| parse_detectors(&t))
        .unwrap_or_default()
}

/// The project's detectors followed by the user-level ones: the project wins.
fn all_detectors(root: &Path, user_file: Option<&Path>) -> Vec<Detector> {
    let mut all = load_detectors(root);
    all.extend(load_user_detectors(user_file));
    all
}

/// Whether the workspace has any config that makes a code-running source
/// applicable (so deserves a trust prompt): an `.envrc`, a mise config, or a
/// valid project or user-level detector whose files exist.
#[must_use]
pub fn needs_trust(root: &Path, user_file: Option<&Path>) -> bool {
    // `.venv` is deliberately absent: it only reads files, so it never
    // warrants a prompt.
    detect_code_source(root).is_some()
        || all_detectors(root, user_file)
            .iter()
            .any(|d| d.applies(root))
}

/// Which code-running source a workspace has config for, by file presence.
#[must_use]
pub fn detect_code_source(root: &Path) -> Option<Source> {
    if root.join(".envrc").is_file() {
        Some(Source::Direnv)
    } else if [".mise.toml", "mise.toml", ".tool-versions"]
        .iter()
        .any(|f| root.join(f).is_file())
    {
        Some(Source::Mise)
    } else {
        None
    }
}

/// The virtual-environment directory under `root`, if one exists.
#[must_use]
pub fn find_venv(root: &Path) -> Option<PathBuf> {
    [".venv", "venv"]
        .iter()
        .map(|d| root.join(d))
        .find(|d| d.join("pyvenv.cfg").is_file())
}

/// The environment `.venv` activation would produce: `VIRTUAL_ENV` set and
/// its `bin` (or `Scripts`) directory prefixed onto `PATH`. Pure file reads.
#[must_use]
pub fn venv_env(venv: &Path, current_path: Option<&OsString>) -> ProjectEnv {
    // Windows venvs use `Scripts`; prefer it only when `bin` is absent so a
    // POSIX venv on a shared filesystem is not misread.
    let bin = if venv.join("Scripts").is_dir() && !venv.join("bin").is_dir() {
        venv.join("Scripts")
    } else {
        venv.join("bin")
    };
    // The venv's bin directory goes first so its `python` shadows the system
    // one, exactly as `activate` does.
    let mut dirs = vec![bin];
    if let Some(p) = current_path {
        dirs.extend(std::env::split_paths(p));
    }
    let mut vars = BTreeMap::new();
    vars.insert(
        "VIRTUAL_ENV".to_string(),
        Some(venv.to_string_lossy().into_owned()),
    );
    // `join_paths` fails only for a path containing the separator; then we
    // leave PATH alone rather than set a broken one.
    if let Ok(joined) = std::env::join_paths(dirs) {
        vars.insert(
            "PATH".to_string(),
            Some(joined.to_string_lossy().into_owned()),
        );
    }
    // Like `activate`, drop PYTHONHOME so the venv interpreter is not confused.
    vars.insert("PYTHONHOME".to_string(), None);
    ProjectEnv::new(Source::Venv, vars)
}

/// Parse `direnv export json` / `mise env --json` output: an object of
/// string values, where `null` (direnv) means "unset". Anything else is
/// rejected as a whole.
#[must_use]
pub fn parse_env_json(text: &str) -> Option<BTreeMap<String, Option<String>>> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let object = value.as_object()?;
    let mut out = BTreeMap::new();
    for (key, val) in object {
        // Names the OS cannot represent in an environment block would make
        // `Command::env` panic; reject the whole document instead.
        if key.is_empty() || key.contains('=') || key.contains('\0') {
            return None;
        }
        match val {
            serde_json::Value::String(s) if !s.contains('\0') => {
                out.insert(key.clone(), Some(s.clone()));
            }
            serde_json::Value::Null => {
                out.insert(key.clone(), None);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Resolve the workspace's environment with the real `direnv`/`mise`.
///
/// `trusted` must be the workspace-trust decision; see the module docs.
/// `user_file` is the optional user-level detector file. This may start
/// external programs (each limited by its timeout) and so should not be
/// called on the UI thread.
#[must_use]
pub fn resolve(root: &Path, trusted: bool, user_file: Option<&Path>) -> Resolved {
    resolve_with(
        root,
        trusted,
        user_file,
        &real_runner,
        std::env::var_os("PATH").as_ref(),
    )
}

/// [`resolve`] with an injectable [`Runner`] and base `PATH`.
#[must_use]
pub fn resolve_with(
    root: &Path,
    trusted: bool,
    user_file: Option<&Path>,
    runner: Runner<'_>,
    current_path: Option<&OsString>,
) -> Resolved {
    let mut out = Resolved::default();
    // 1. User detectors override everything built in. Only the first
    //    applicable one is tried; if it yields nothing we fall through.
    if let Some(d) = all_detectors(root, user_file)
        .into_iter()
        .find(|d| d.applies(root))
    {
        if trusted {
            let args: Vec<&str> = d.command[1..].iter().map(String::as_str).collect();
            let extra = d.command_env();
            if let Some(vars) = runner(&d.command[0], &args, root, d.timeout(), &extra)
                .and_then(|t| parse_env_json(&t))
                .filter(|v| !v.is_empty())
            {
                let mut env = ProjectEnv::new(Source::Custom, vars);
                env.name = Some(d.name.clone());
                env.command_env = d.env.clone();
                out.env = Some(env);
                return out;
            }
        } else {
            out.blocked = Some(Source::Custom);
        }
    }
    // 2. direnv / mise. When a custom detector was already blocked, keep
    //    reporting that one (`blocked` holds a single source).
    if let Some(source) = detect_code_source(root) {
        if trusted {
            let text = match source {
                Source::Direnv => runner("direnv", &["export", "json"], root, DEFAULT_TIMEOUT, &[]),
                _ => runner("mise", &["env", "--json"], root, DEFAULT_TIMEOUT, &[]),
            };
            // direnv prints nothing when already loaded/empty; treat as no env.
            if let Some(vars) = text
                .and_then(|t| parse_env_json(&t))
                .filter(|v| !v.is_empty())
            {
                out.env = Some(ProjectEnv::new(source, vars));
                return out;
            }
        } else if out.blocked.is_none() {
            out.blocked = Some(source);
        }
    }
    // 3. The .venv fallback: pure file reads, so it runs even when untrusted
    //    (and then coexists with `blocked`).
    if let Some(venv) = find_venv(root) {
        out.env = Some(venv_env(&venv, current_path));
    }
    out
}

/// The environment currently applied to spawned processes. A poisoned lock
/// is ignored rather than propagated: losing the overlay is better than
/// failing to launch a language server.
static ACTIVE: Mutex<Option<ProjectEnv>> = Mutex::new(None);

/// Publish `env` (or clear with `None`) as the active environment.
pub fn set_active(env: Option<ProjectEnv>) {
    if let Ok(mut slot) = ACTIVE.lock() {
        *slot = env;
    }
}

/// The active environment, if any.
#[must_use]
pub fn active() -> Option<ProjectEnv> {
    ACTIVE.lock().ok().and_then(|slot| slot.clone())
}

/// The active variable overrides (`None` value = remove), empty when none.
#[must_use]
pub fn active_vars() -> Vec<(String, Option<String>)> {
    active()
        .map(|e| e.vars.into_iter().collect())
        .unwrap_or_default()
}

/// Call `set(name, Some(value))` / `set(name, None)` (remove) for each
/// variable of the active environment. Callers wire this to their own
/// command builder (`Command::env`/`env_remove`, `CommandBuilder`, …).
pub fn apply_to(mut set: impl FnMut(&str, Option<&str>)) {
    for (k, v) in active_vars() {
        set(&k, v.as_deref());
    }
}

/// The runner [`resolve`] uses: the real tool, or nothing without `process`.
#[cfg(feature = "process")]
fn real_runner(
    program: &str,
    args: &[&str],
    dir: &Path,
    timeout: Duration,
    env: &[(String, String)],
) -> Option<String> {
    run::run_tool(program, args, dir, timeout, env)
}

/// Without the `process` feature no external tool is ever started.
#[cfg(not(feature = "process"))]
fn real_runner(
    _program: &str,
    _args: &[&str],
    _dir: &Path,
    _timeout: Duration,
    _env: &[(String, String)],
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    /// A fresh empty directory under the system temp dir, removed on drop.
    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let p = std::env::temp_dir()
                .join(format!("vix-project-env-{}-{tag}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_venv(root: &Path) {
        fs::create_dir_all(root.join(".venv/bin")).unwrap();
        fs::write(root.join(".venv/pyvenv.cfg"), "home = /usr/bin\n").unwrap();
    }

    #[test]
    fn untrusted_workspace_never_runs_direnv_or_mise() {
        for file in [".envrc", ".mise.toml", ".tool-versions"] {
            let dir = Dir::new("untrusted");
            fs::write(dir.path().join(file), "x").unwrap();
            let calls = Cell::new(0);
            let runner = |_: &str, _: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
                calls.set(calls.get() + 1);
                Some(r#"{"EVIL":"1"}"#.to_string())
            };
            let r = resolve_with(dir.path(), false, None, &runner, None);
            assert_eq!(calls.get(), 0, "{file}: nothing may run");
            assert!(r.env.is_none());
            assert!(r.blocked.is_some());
        }
    }

    #[test]
    fn trusted_workspace_runs_direnv_and_uses_its_json() {
        let dir = Dir::new("direnv");
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        let runner = |p: &str, a: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
            assert_eq!((p, a), ("direnv", &["export", "json"][..]));
            Some(r#"{"FOO":"bar","GONE":null}"#.to_string())
        };
        let r = resolve_with(dir.path(), true, None, &runner, None);
        let env = r.env.unwrap();
        assert_eq!(env.source, Source::Direnv);
        assert_eq!(env.vars["FOO"].as_deref(), Some("bar"));
        assert_eq!(env.vars["GONE"], None);
        assert!(r.blocked.is_none());
    }

    #[test]
    fn mise_is_used_when_there_is_no_envrc() {
        let dir = Dir::new("mise");
        fs::write(dir.path().join("mise.toml"), "").unwrap();
        let runner = |p: &str, _: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
            assert_eq!(p, "mise");
            Some(r#"{"A":"1"}"#.to_string())
        };
        let r = resolve_with(dir.path(), true, None, &runner, None);
        assert_eq!(r.env.unwrap().source, Source::Mise);
    }

    #[test]
    fn venv_works_untrusted_and_without_running_anything() {
        let dir = Dir::new("venv");
        make_venv(dir.path());
        let runner = |_: &str,
                      _: &[&str],
                      _: &Path,
                      _: Duration,
                      _: &[(String, String)]|
         -> Option<String> { panic!("must not run anything") };
        let base = std::env::join_paths(["/usr/bin"]).unwrap();
        let r = resolve_with(dir.path(), false, None, &runner, Some(&base));
        let env = r.env.unwrap();
        assert_eq!(env.source, Source::Venv);
        let venv = dir.path().join(".venv");
        assert_eq!(
            env.vars["VIRTUAL_ENV"].as_deref(),
            Some(venv.to_string_lossy().as_ref())
        );
        let path = env.vars["PATH"].clone().unwrap();
        let first = std::env::split_paths(&path).next().unwrap();
        assert_eq!(first, venv.join("bin"));
        assert!(path.ends_with("/usr/bin"));
        assert_eq!(env.vars["PYTHONHOME"], None);
    }

    #[test]
    fn untrusted_envrc_still_falls_back_to_venv_and_reports_blocked() {
        let dir = Dir::new("fallback");
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        make_venv(dir.path());
        let r = resolve_with(dir.path(), false, None, &|_, _, _, _, _| None, None);
        assert_eq!(r.blocked, Some(Source::Direnv));
        assert_eq!(r.env.unwrap().source, Source::Venv);
    }

    #[test]
    fn failed_tool_falls_back_and_empty_dir_is_empty() {
        let dir = Dir::new("empty");
        assert_eq!(
            resolve_with(dir.path(), true, None, &|_, _, _, _, _| None, None),
            Resolved::default()
        );
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        let r = resolve_with(dir.path(), true, None, &|_, _, _, _, _| None, None);
        assert!(r.env.is_none() && r.blocked.is_none());
    }

    fn write_detectors(root: &Path, text: &str) {
        fs::create_dir_all(root.join(".vix")).unwrap();
        fs::write(root.join(".vix/env.toml"), text).unwrap();
    }

    const NIX: &str = "[[detector]]\nname = \"nix\"\ndetect = [\"flake.nix\"]\ncommand = [\"nix-env-json\", \"--all\"]\n";

    #[test]
    fn user_detector_runs_when_trusted_and_its_files_exist() {
        let dir = Dir::new("custom");
        write_detectors(dir.path(), NIX);
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        let runner = |p: &str, a: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
            assert_eq!((p, a), ("nix-env-json", &["--all"][..]));
            Some(r#"{"NIXY":"1"}"#.to_string())
        };
        let env = resolve_with(dir.path(), true, None, &runner, None)
            .env
            .unwrap();
        assert_eq!(env.source, Source::Custom, "custom wins over direnv");
        assert_eq!(env.vars["NIXY"].as_deref(), Some("1"));
    }

    #[test]
    fn user_detector_never_runs_untrusted_or_without_its_files() {
        let dir = Dir::new("custom-untrusted");
        write_detectors(dir.path(), NIX);
        let runner = |_: &str,
                      _: &[&str],
                      _: &Path,
                      _: Duration,
                      _: &[(String, String)]|
         -> Option<String> { panic!("must not run anything") };
        // No flake.nix: not applicable, even trusted.
        assert_eq!(
            resolve_with(dir.path(), true, None, &runner, None),
            Resolved::default()
        );
        assert!(!needs_trust(dir.path(), None));
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        assert!(needs_trust(dir.path(), None));
        let r = resolve_with(dir.path(), false, None, &runner, None);
        assert!(r.env.is_none());
        assert_eq!(r.blocked, Some(Source::Custom));
    }

    #[test]
    fn failed_user_detector_falls_through_to_the_built_in_sources() {
        let dir = Dir::new("custom-fail");
        write_detectors(dir.path(), NIX);
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        make_venv(dir.path());
        let r = resolve_with(dir.path(), true, None, &|_, _, _, _, _| None, None);
        assert_eq!(r.env.unwrap().source, Source::Venv);
    }

    #[test]
    fn invalid_detector_files_are_rejected_whole() {
        assert_eq!(parse_detectors(NIX).unwrap().len(), 1);
        assert_eq!(parse_detectors("").unwrap().len(), 0);
        for bad in [
            "not = [toml",
            "[[detector]]\nname=\"\"\ndetect=[\"a\"]\ncommand=[\"x\"]",
            "[[detector]]\nname=\"n\"\ndetect=[]\ncommand=[\"x\"]",
            "[[detector]]\nname=\"n\"\ndetect=[\"a\"]\ncommand=[]",
            "[[detector]]\nname=\"n\"\ndetect=[\"/etc/passwd\"]\ncommand=[\"x\"]",
            "[[detector]]\nname=\"n\"\ndetect=[\"../x\"]\ncommand=[\"x\"]",
            "[[detector]]\nname=\"n\"\ndetect=[\"a\"]\ncommand=[\"x\\u0000\"]",
        ] {
            assert!(parse_detectors(bad).is_none(), "{bad}");
        }
        // Windows: `\\x` and `C:x` count as relative there, yet leave the root.
        #[cfg(windows)]
        for bad in [r"\Windows", "C:x", r"C:\x"] {
            let toml = format!("[[detector]]\nname=\"n\"\ndetect=['{bad}']\ncommand=[\"x\"]");
            assert!(parse_detectors(&toml).is_none(), "{bad}");
        }
        let many = NIX.repeat(MAX_DETECTORS + 1);
        assert!(parse_detectors(&many).is_none());
    }

    #[test]
    fn custom_env_carries_the_detector_name_as_its_label() {
        let dir = Dir::new("custom-name");
        write_detectors(dir.path(), NIX);
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        let runner = |_: &str, _: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
            Some(r#"{"A":"1"}"#.to_string())
        };
        let env = resolve_with(dir.path(), true, None, &runner, None)
            .env
            .unwrap();
        assert_eq!((env.source, env.label()), (Source::Custom, "nix"));
        assert_eq!(
            ProjectEnv::new(Source::Venv, BTreeMap::new()).label(),
            "venv"
        );
    }

    #[test]
    fn per_detector_timeout_is_passed_to_the_runner_and_validated() {
        let dir = Dir::new("timeout");
        write_detectors(dir.path(), &format!("{NIX}timeout_secs = 30\n"));
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        let seen = Cell::new(Duration::ZERO);
        let runner = |_: &str, _: &[&str], _: &Path, t: Duration, _: &[(String, String)]| {
            seen.set(t);
            Some(r#"{"A":"1"}"#.to_string())
        };
        assert!(
            resolve_with(dir.path(), true, None, &runner, None)
                .env
                .is_some()
        );
        assert_eq!(seen.get(), Duration::from_secs(30));
        write_detectors(dir.path(), NIX);
        let _ = resolve_with(dir.path(), true, None, &runner, None);
        assert_eq!(seen.get(), DEFAULT_TIMEOUT);
        for bad in ["0", "61"] {
            let text = format!("{NIX}timeout_secs = {bad}\n");
            assert!(parse_detectors(&text).is_none(), "{bad}");
        }
    }

    #[test]
    fn user_level_detector_file_applies_after_the_project_one() {
        let dir = Dir::new("user-file");
        let user = dir.path().join("user-env.toml");
        fs::write(
            &user,
            "[[detector]]\nname=\"mine\"\ndetect=[\"flake.nix\"]\ncommand=[\"mine-env\"]\n",
        )
        .unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("flake.nix"), "").unwrap();
        let runner = |p: &str, _: &[&str], _: &Path, _: Duration, _: &[(String, String)]| {
            Some(format!(r#"{{"FROM":"{p}"}}"#))
        };
        assert!(needs_trust(&root, Some(&user)));
        assert!(!needs_trust(&root, None));
        let env = resolve_with(&root, true, Some(&user), &runner, None)
            .env
            .unwrap();
        assert_eq!(env.label(), "mine");
        // Untrusted: never runs, reported as blocked.
        let blocked = resolve_with(
            &root,
            false,
            Some(&user),
            &|_, _, _, _, _| panic!("no"),
            None,
        );
        assert_eq!(blocked.blocked, Some(Source::Custom));
        // A project detector beats the user-level one.
        write_detectors(&root, NIX);
        let env = resolve_with(&root, true, Some(&user), &runner, None)
            .env
            .unwrap();
        assert_eq!(env.label(), "nix");
        // A missing or invalid user file is ignored.
        assert_eq!(
            load_user_detectors(Some(&dir.path().join("absent.toml"))),
            Vec::new()
        );
        fs::write(&user, "not = [toml").unwrap();
        assert_eq!(load_user_detectors(Some(&user)), Vec::new());
    }

    #[test]
    fn detector_env_reaches_only_the_detector_command() {
        let dir = Dir::new("detector-env");
        write_detectors(
            dir.path(),
            &format!("{NIX}[detector.env]\nZ_LAST = \"$HOME/x\"\nA_FIRST = \"1\"\n"),
        );
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        let runner = |_: &str, _: &[&str], _: &Path, _: Duration, e: &[(String, String)]| {
            seen.borrow_mut().extend(e.iter().cloned());
            Some(r#"{"OUT":"1"}"#.to_string())
        };
        let env = resolve_with(dir.path(), true, None, &runner, None)
            .env
            .unwrap();
        // Name order, and the value is a plain string (no expansion).
        assert_eq!(
            *seen.borrow(),
            vec![
                ("A_FIRST".to_string(), "1".to_string()),
                ("Z_LAST".to_string(), "$HOME/x".to_string())
            ]
        );
        // Not leaked into the variables applied to spawned processes.
        assert_eq!(env.vars.keys().collect::<Vec<_>>(), ["OUT"]);
        assert_eq!(env.command_env.len(), 2);
        // Untrusted: nothing runs, so nothing is passed anywhere.
        let blocked = resolve_with(dir.path(), false, None, &|_, _, _, _, _| panic!("no"), None);
        assert_eq!(blocked.blocked, Some(Source::Custom));
    }

    #[test]
    fn detector_env_names_and_values_are_validated() {
        for ok in ["FOO", "_x1", "PATH", "ldx"] {
            assert!(env_name_allowed(ok), "{ok}");
        }
        for bad in [
            "",
            "1A",
            "A-B",
            "A=B",
            "A B",
            "LD_PRELOAD",
            "ld_library_path",
            "DYLD_INSERT_LIBRARIES",
            "dyld_x",
        ] {
            assert!(!env_name_allowed(bad), "{bad}");
        }
        let with = |env: &str| format!("{NIX}[detector.env]\n{env}\n");
        assert!(parse_detectors(&with("FOO = \"bar\"")).is_some());
        assert!(parse_detectors(&with("PATH = \"/opt/bin\"")).is_some());
        for bad in [
            "LD_PRELOAD = \"x\"",
            "DYLD_INSERT_LIBRARIES = \"x\"",
            "\"A=B\" = \"x\"",
            "FOO = \"a\\u0000b\"",
            "FOO = 1",
        ] {
            assert!(parse_detectors(&with(bad)).is_none(), "{bad}");
        }
        let big = format!("FOO = \"{}\"", "x".repeat(MAX_ENV_LEN + 1));
        assert!(parse_detectors(&with(&big)).is_none());
        let mut many = String::new();
        for i in 0..=MAX_DETECTOR_ENV {
            std::fmt::Write::write_fmt(&mut many, format_args!("V{i} = \"1\"\n")).unwrap();
        }
        assert!(parse_detectors(&with(&many)).is_none());
    }

    #[test]
    fn parse_rejects_malformed_and_hostile_json() {
        assert!(parse_env_json("not json").is_none());
        assert!(parse_env_json("[]").is_none());
        assert!(parse_env_json(r#"{"A":1}"#).is_none());
        assert!(parse_env_json(r#"{"A=B":"x"}"#).is_none());
        assert!(parse_env_json(r#"{"":"x"}"#).is_none());
        assert!(parse_env_json(r#"{"A":"x\u0000"}"#).is_none());
        assert_eq!(parse_env_json("{}").unwrap().len(), 0);
    }

    #[test]
    fn apply_to_reports_sets_and_removals() {
        // The only test touching the global, to avoid cross-test races.
        let mut vars = BTreeMap::new();
        vars.insert("VIX_T667".to_string(), Some("1".to_string()));
        vars.insert("VIX_T667_GONE".to_string(), None);
        set_active(Some(ProjectEnv::new(Source::Venv, vars)));
        let mut seen = Vec::new();
        apply_to(|k, v| seen.push((k.to_string(), v.map(str::to_string))));
        assert!(seen.contains(&("VIX_T667".to_string(), Some("1".to_string()))));
        assert!(seen.contains(&("VIX_T667_GONE".to_string(), None)));
        set_active(None);
        assert!(active().is_none() && active_vars().is_empty());
    }
}
