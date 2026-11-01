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
}

impl Source {
    /// A short stable label for the status bar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Source::Venv => "venv",
            Source::Direnv => "direnv",
            Source::Mise => "mise",
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEnv {
    /// Where it came from.
    pub source: Source,
    /// Variables to set (`Some`) or remove (`None`), in a stable order.
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
    let bin = if venv.join("Scripts").is_dir() && !venv.join("bin").is_dir() {
        venv.join("Scripts")
    } else {
        venv.join("bin")
    };
    let mut dirs = vec![bin];
    if let Some(p) = current_path {
        dirs.extend(std::env::split_paths(p));
    }
    let mut vars = BTreeMap::new();
    vars.insert(
        "VIRTUAL_ENV".to_string(),
        Some(venv.to_string_lossy().into_owned()),
    );
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
        } else {
            out.blocked = Some(source);
        }
    }
    if let Some(venv) = find_venv(root) {
        out.env = Some(venv_env(&venv, current_path));
    }
    out
}

/// The environment currently applied to spawned processes.
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
