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
//! use vix_project_env::resolve_with;
//!
//! // An empty directory has no environment; nothing is run.
//! let never = |_: &str, _: &[&str], _: &Path| -> Option<String> { unreachable!() };
//! let dir = std::env::temp_dir().join("vix-project-env-doctest");
//! std::fs::create_dir_all(&dir).unwrap();
//! let resolved = resolve_with(&dir, false, &never, None);
//! assert!(resolved.env.is_none() && resolved.blocked.is_none());
//! ```

#[cfg(feature = "process")]
mod run;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
pub struct ProjectEnv {
    /// Where it came from.
    pub source: Source,
    /// Variables to set (`Some`) or remove (`None`), in a stable order
    /// (`BTreeMap`, so status text and tests are deterministic).
    pub vars: BTreeMap<String, Option<String>>,
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

/// Runs an external program in a directory and returns its stdout. A seam so
/// tests can prove nothing is executed for an untrusted workspace.
pub type Runner<'a> = &'a dyn Fn(&str, &[&str], &Path) -> Option<String>;

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
            !p.is_empty()
                && !p.contains('\0')
                && path.is_relative()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
        };
        !self.name.is_empty()
            && self.name.chars().count() <= 64
            && !self.detect.is_empty()
            && self.detect.iter().all(rel_ok)
            && !self.command.is_empty()
            && self.command.iter().all(|a| !a.contains('\0'))
            && !self.command[0].is_empty()
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

/// Whether the workspace has any config that makes a code-running source
/// applicable (so deserves a trust prompt): an `.envrc`, a mise config, or a
/// valid user detector whose files exist.
#[must_use]
pub fn needs_trust(root: &Path) -> bool {
    // `.venv` is deliberately absent: it only reads files, so it never
    // warrants a prompt.
    detect_code_source(root).is_some() || load_detectors(root).iter().any(|d| d.applies(root))
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
    ProjectEnv {
        source: Source::Venv,
        vars,
    }
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
/// `trusted` must be the workspace-trust decision; see the module docs. This
/// may start external programs (each limited to a few seconds) and so should
/// not be called on the UI thread.
#[must_use]
pub fn resolve(root: &Path, trusted: bool) -> Resolved {
    resolve_with(
        root,
        trusted,
        &real_runner,
        std::env::var_os("PATH").as_ref(),
    )
}

/// [`resolve`] with an injectable [`Runner`] and base `PATH`.
#[must_use]
pub fn resolve_with(
    root: &Path,
    trusted: bool,
    runner: Runner<'_>,
    current_path: Option<&OsString>,
) -> Resolved {
    let mut out = Resolved::default();
    // 1. User detectors override everything built in. Only the first
    //    applicable one is tried; if it yields nothing we fall through.
    if let Some(d) = load_detectors(root).into_iter().find(|d| d.applies(root)) {
        if trusted {
            let args: Vec<&str> = d.command[1..].iter().map(String::as_str).collect();
            if let Some(vars) = runner(&d.command[0], &args, root)
                .and_then(|t| parse_env_json(&t))
                .filter(|v| !v.is_empty())
            {
                out.env = Some(ProjectEnv {
                    source: Source::Custom,
                    vars,
                });
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
                Source::Direnv => runner("direnv", &["export", "json"], root),
                _ => runner("mise", &["env", "--json"], root),
            };
            // direnv prints nothing when already loaded/empty; treat as no env.
            if let Some(vars) = text
                .and_then(|t| parse_env_json(&t))
                .filter(|v| !v.is_empty())
            {
                out.env = Some(ProjectEnv { source, vars });
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
fn real_runner(program: &str, args: &[&str], dir: &Path) -> Option<String> {
    run::run_tool(program, args, dir)
}

/// Without the `process` feature no external tool is ever started.
#[cfg(not(feature = "process"))]
fn real_runner(_program: &str, _args: &[&str], _dir: &Path) -> Option<String> {
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
            let runner = |_: &str, _: &[&str], _: &Path| {
                calls.set(calls.get() + 1);
                Some(r#"{"EVIL":"1"}"#.to_string())
            };
            let r = resolve_with(dir.path(), false, &runner, None);
            assert_eq!(calls.get(), 0, "{file}: nothing may run");
            assert!(r.env.is_none());
            assert!(r.blocked.is_some());
        }
    }

    #[test]
    fn trusted_workspace_runs_direnv_and_uses_its_json() {
        let dir = Dir::new("direnv");
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        let runner = |p: &str, a: &[&str], _: &Path| {
            assert_eq!((p, a), ("direnv", &["export", "json"][..]));
            Some(r#"{"FOO":"bar","GONE":null}"#.to_string())
        };
        let r = resolve_with(dir.path(), true, &runner, None);
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
        let runner = |p: &str, _: &[&str], _: &Path| {
            assert_eq!(p, "mise");
            Some(r#"{"A":"1"}"#.to_string())
        };
        let r = resolve_with(dir.path(), true, &runner, None);
        assert_eq!(r.env.unwrap().source, Source::Mise);
    }

    #[test]
    fn venv_works_untrusted_and_without_running_anything() {
        let dir = Dir::new("venv");
        make_venv(dir.path());
        let runner =
            |_: &str, _: &[&str], _: &Path| -> Option<String> { panic!("must not run anything") };
        let base = std::env::join_paths(["/usr/bin"]).unwrap();
        let r = resolve_with(dir.path(), false, &runner, Some(&base));
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
        let r = resolve_with(dir.path(), false, &|_, _, _| None, None);
        assert_eq!(r.blocked, Some(Source::Direnv));
        assert_eq!(r.env.unwrap().source, Source::Venv);
    }

    #[test]
    fn failed_tool_falls_back_and_empty_dir_is_empty() {
        let dir = Dir::new("empty");
        assert_eq!(
            resolve_with(dir.path(), true, &|_, _, _| None, None),
            Resolved::default()
        );
        fs::write(dir.path().join(".envrc"), "x").unwrap();
        let r = resolve_with(dir.path(), true, &|_, _, _| None, None);
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
        let runner = |p: &str, a: &[&str], _: &Path| {
            assert_eq!((p, a), ("nix-env-json", &["--all"][..]));
            Some(r#"{"NIXY":"1"}"#.to_string())
        };
        let env = resolve_with(dir.path(), true, &runner, None).env.unwrap();
        assert_eq!(env.source, Source::Custom, "custom wins over direnv");
        assert_eq!(env.vars["NIXY"].as_deref(), Some("1"));
    }

    #[test]
    fn user_detector_never_runs_untrusted_or_without_its_files() {
        let dir = Dir::new("custom-untrusted");
        write_detectors(dir.path(), NIX);
        let runner =
            |_: &str, _: &[&str], _: &Path| -> Option<String> { panic!("must not run anything") };
        // No flake.nix: not applicable, even trusted.
        assert_eq!(
            resolve_with(dir.path(), true, &runner, None),
            Resolved::default()
        );
        assert!(!needs_trust(dir.path()));
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        assert!(needs_trust(dir.path()));
        let r = resolve_with(dir.path(), false, &runner, None);
        assert!(r.env.is_none());
        assert_eq!(r.blocked, Some(Source::Custom));
    }

    #[test]
    fn failed_user_detector_falls_through_to_the_built_in_sources() {
        let dir = Dir::new("custom-fail");
        write_detectors(dir.path(), NIX);
        fs::write(dir.path().join("flake.nix"), "").unwrap();
        make_venv(dir.path());
        let r = resolve_with(dir.path(), true, &|_, _, _| None, None);
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
        let many = NIX.repeat(MAX_DETECTORS + 1);
        assert!(parse_detectors(&many).is_none());
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
        set_active(Some(ProjectEnv {
            source: Source::Venv,
            vars,
        }));
        let mut seen = Vec::new();
        apply_to(|k, v| seen.push((k.to_string(), v.map(str::to_string))));
        assert!(seen.contains(&("VIX_T667".to_string(), Some("1".to_string()))));
        assert!(seen.contains(&("VIX_T667_GONE".to_string(), None)));
        set_active(None);
        assert!(active().is_none() && active_vars().is_empty());
    }
}
