//! `vix --plugin …` (T637): install, list, update and remove Rhai plugins
//! from the command line, without starting the editor. The work is
//! `vix_script::plugins`; this module only parses the arguments and turns
//! each result into translated lines. See `crates/vix-script/spec/index.md`,
//! "Plugins".

/// What a `--plugin` command printed, and whether it succeeded (the binary
/// exits non-zero when it didn't).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Lines for standard output (or standard error, when `ok` is false).
    pub lines: Vec<String>,
    /// Whether the command succeeded.
    pub ok: bool,
}

impl Report {
    /// A successful report carrying `lines` for standard output.
    #[cfg(feature = "script")]
    fn ok(lines: Vec<String>) -> Self {
        Self { lines, ok: true }
    }

    /// A failed report with a single diagnostic `line` (printed on stderr, and
    /// the binary then exits with status 1).
    fn failed(line: String) -> Self {
        Self {
            lines: vec![line],
            ok: false,
        }
    }
}

/// The real plugin locations: `Settings::scripts_dir()` and
/// `Settings::plugins_lock_path()`, or `None` when the config directory
/// can't be determined.
#[cfg(feature = "script")]
#[must_use]
pub fn default_paths() -> Option<vix_script::plugins::PluginPaths> {
    Some(vix_script::plugins::PluginPaths {
        scripts_dir: crate::settings::Settings::scripts_dir()?,
        lock_path: crate::settings::Settings::plugins_lock_path()?,
    })
}

/// Run `vix --plugin <args>` against `paths`.
#[cfg(feature = "script")]
#[must_use]
pub fn run(args: &[String], paths: &vix_script::plugins::PluginPaths) -> Report {
    use vix_script::plugins;
    let arg = args.get(1).map(String::as_str);
    let result = match (args.first().map(String::as_str), arg) {
        (Some("install"), Some(spec)) => install(paths, spec),
        (Some("list"), None) => list(paths),
        (Some("update"), name) => update(paths, name),
        (Some("remove"), Some(name)) => plugins::remove(paths, name)
            .map(|e| vec![t!("msg.plugin_removed", name = e.name).to_string()]),
        _ => return Report::failed(t!("msg.plugin_usage").to_string()),
    };
    match result {
        Ok(lines) => Report::ok(lines),
        Err(e) => Report::failed(t!("msg.plugin_failed", error = error_text(&e)).to_string()),
    }
}

/// Run `vix --plugin <args>` against the real config directory — what the
/// binary calls. Without the `script` feature there are no plugins, and it
/// says so.
#[must_use]
pub fn run_cli(args: &[String]) -> Report {
    #[cfg(feature = "script")]
    {
        match default_paths() {
            Some(paths) => run(args, &paths),
            // No config directory at all (no home directory): nothing to do.
            None => Report::failed(
                t!(
                    "msg.plugin_failed",
                    error = error_text(&vix_script::plugins::PluginError::Io(
                        std::io::ErrorKind::NotFound.into()
                    ))
                )
                .to_string(),
            ),
        }
    }
    #[cfg(not(feature = "script"))]
    {
        let _ = args;
        Report::failed(t!("msg.plugin_unavailable").to_string())
    }
}

/// The first 12 characters of a commit, for display.
#[cfg(feature = "script")]
fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

/// `vix --plugin install <spec>`: clone and pin a plugin, reporting its name
/// and short commit. This variant needs the `git` capability to clone.
#[cfg(all(feature = "script", feature = "git"))]
fn install(
    paths: &vix_script::plugins::PluginPaths,
    spec: &str,
) -> Result<Vec<String>, vix_script::plugins::PluginError> {
    let e = vix_script::plugins::install(paths, spec)?;
    Ok(vec![
        t!(
            "msg.plugin_installed",
            name = e.name,
            commit = short(&e.commit)
        )
        .to_string(),
    ])
}

/// `install` without the `git` capability: always
/// `PluginError::GitUnavailable`, since plugins are fetched by cloning.
#[cfg(all(feature = "script", not(feature = "git")))]
fn install(
    _paths: &vix_script::plugins::PluginPaths,
    _spec: &str,
) -> Result<Vec<String>, vix_script::plugins::PluginError> {
    Err(vix_script::plugins::PluginError::GitUnavailable)
}

/// `vix --plugin update [name]`: fast-forward one plugin (or all of them) and
/// print one line per plugin, plus a warning line when the update dropped the
/// plugin's trust (a changed commit must be re-approved before it runs).
#[cfg(all(feature = "script", feature = "git"))]
fn update(
    paths: &vix_script::plugins::PluginPaths,
    name: Option<&str>,
) -> Result<Vec<String>, vix_script::plugins::PluginError> {
    let mut lines = Vec::new();
    for o in vix_script::plugins::update(paths, name)? {
        if o.old_commit == o.new_commit {
            lines.push(t!("msg.plugin_up_to_date", name = o.name.clone()).to_string());
        } else {
            lines.push(
                t!(
                    "msg.plugin_updated",
                    name = o.name.clone(),
                    old = short(&o.old_commit),
                    new = short(&o.new_commit)
                )
                .to_string(),
            );
        }
        if o.trust_dropped {
            lines.push(t!("msg.plugin_trust_dropped", name = o.name).to_string());
        }
    }
    Ok(lines)
}

/// `update` without the `git` capability: always
/// `PluginError::GitUnavailable`, since updating means fetching.
#[cfg(all(feature = "script", not(feature = "git")))]
fn update(
    _paths: &vix_script::plugins::PluginPaths,
    _name: Option<&str>,
) -> Result<Vec<String>, vix_script::plugins::PluginError> {
    Err(vix_script::plugins::PluginError::GitUnavailable)
}

/// One line per installed plugin: name, state, short commit, URL.
#[cfg(feature = "script")]
fn list(
    paths: &vix_script::plugins::PluginPaths,
) -> Result<Vec<String>, vix_script::plugins::PluginError> {
    let lock = vix_script::plugins::Lockfile::load(&paths.lock_path)?;
    if lock.plugins.is_empty() {
        return Ok(vec![t!("msg.plugin_none").to_string()]);
    }
    Ok(lock
        .plugins
        .iter()
        .map(|e| {
            let state = vix_script::plugins::state(paths, e);
            format!(
                "{}  {}  {}  {}",
                e.name,
                t!(state.i18n_key()),
                short(&e.commit),
                e.url
            )
        })
        .collect())
}

/// A translated description of `e`, shared by the CLI and the in-editor
/// chooser.
#[cfg(feature = "script")]
#[must_use]
pub fn error_text(e: &vix_script::plugins::PluginError) -> String {
    use vix_script::plugins::PluginError as E;
    match e {
        E::InvalidSpec(spec) => t!("msg.plugin_error.invalid_spec", spec = spec.clone()),
        E::InvalidName(name) => t!("msg.plugin_error.invalid_name", name = name.clone()),
        E::AlreadyInstalled(name) => {
            t!("msg.plugin_error.already_installed", name = name.clone())
        }
        E::NotInstalled(name) => t!("msg.plugin_error.not_installed", name = name.clone()),
        E::SymlinkEscape(path) => t!(
            "msg.plugin_error.symlink_escape",
            path = path.display().to_string()
        ),
        E::Lockfile(detail) => t!("msg.plugin_error.lockfile", detail = detail.clone()),
        E::Io(err) => t!("msg.plugin_error.io", detail = err.to_string()),
        E::Git(detail) => t!("msg.plugin_error.git", detail = detail.clone()),
        E::GitUnavailable => t!("msg.plugin_error.git_unavailable"),
        // `PluginError` is `#[non_exhaustive]`; a future variant still reads.
        other => t!("msg.plugin_error.io", detail = other.to_string()),
    }
    .to_string()
}
