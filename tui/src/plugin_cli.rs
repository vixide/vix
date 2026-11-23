//! `vix --plugin …` (T637): install, list, update and remove Rhai plugins
//! from the command line, without starting the editor. The work is
//! `vix_script::plugins`; this module only parses the arguments and turns
//! each result into translated lines. See `crates/vix-script/spec/index.md`,
//! "Plugins".

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

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
            lines.push(t!("msg.plugin_up_to_date", name = o.name).to_string());
        } else {
            lines.push(
                t!(
                    "msg.plugin_updated",
                    name = o.name,
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

#[cfg(all(test, feature = "script"))]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use vix_script::plugins::{LockEntry, Lockfile, PluginError, PluginPaths};

    /// A fresh temp directory unique to this test.
    fn temp(tag: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vix-plugcli-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn paths_in(dir: &Path) -> PluginPaths {
        PluginPaths {
            scripts_dir: dir.join("scripts"),
            lock_path: dir.join("plugins.lock"),
        }
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    fn entry(name: &str) -> LockEntry {
        LockEntry {
            name: name.into(),
            url: format!("https://example.invalid/{name}.git"),
            rev: None,
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            trusted: None,
        }
    }

    #[test]
    fn report_failed_carries_one_line_and_is_not_ok() {
        let r = Report::failed("boom".into());
        assert!(!r.ok);
        assert_eq!(r.lines, vec!["boom".to_string()]);
        let r = Report::ok(vec!["a".into(), "b".into()]);
        assert!(r.ok);
        assert_eq!(r.lines.len(), 2);
    }

    #[test]
    fn short_truncates_to_twelve_characters_and_keeps_short_input() {
        assert_eq!(short("0123456789abcdef"), "0123456789ab");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn bad_or_incomplete_commands_print_the_usage_line() {
        let dir = temp("usage");
        let paths = paths_in(&dir);
        let usage = t!("msg.plugin_usage").to_string();
        for bad in [
            args(&[]),
            args(&["install"]),
            args(&["remove"]),
            args(&["list", "extra"]),
            args(&["frobnicate"]),
        ] {
            let r = run(&bad, &paths);
            assert!(!r.ok, "{bad:?}");
            assert_eq!(r.lines, vec![usage.clone()], "{bad:?}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_prints_one_line_per_plugin_with_state_short_commit_and_url() {
        let dir = temp("list");
        let paths = paths_in(&dir);
        std::fs::create_dir_all(paths.plugin_dir("alpha")).unwrap();
        Lockfile {
            plugins: vec![entry("alpha"), entry("ghost")],
        }
        .save(&paths.lock_path)
        .unwrap();
        let r = run(&args(&["list"]), &paths);
        assert!(r.ok);
        assert_eq!(
            r.lines,
            vec![
                format!(
                    "alpha  {}  0123456789ab  https://example.invalid/alpha.git",
                    t!("ui.plugin_state.disabled")
                ),
                format!(
                    "ghost  {}  0123456789ab  https://example.invalid/ghost.git",
                    t!("ui.plugin_state.missing")
                ),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_with_no_plugins_says_none_and_a_corrupt_lock_fails() {
        let dir = temp("list-empty");
        let paths = paths_in(&dir);
        let r = run(&args(&["list"]), &paths);
        assert!(r.ok);
        assert_eq!(r.lines, vec![t!("msg.plugin_none").to_string()]);
        std::fs::write(&paths.lock_path, "this is [not toml").unwrap();
        let r = run(&args(&["list"]), &paths);
        assert!(!r.ok);
        let prefix = t!("msg.plugin_failed", error = "").to_string();
        assert!(r.lines[0].starts_with(prefix.trim_end()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_deletes_the_directory_and_the_lock_entry() {
        let dir = temp("remove");
        let paths = paths_in(&dir);
        std::fs::create_dir_all(paths.plugin_dir("alpha")).unwrap();
        std::fs::write(paths.plugin_dir("alpha").join("a.rhai"), "").unwrap();
        Lockfile {
            plugins: vec![entry("alpha")],
        }
        .save(&paths.lock_path)
        .unwrap();
        let r = run(&args(&["remove", "alpha"]), &paths);
        assert!(r.ok);
        assert_eq!(r.lines, vec![t!("msg.plugin_removed", name = "alpha").to_string()]);
        assert!(!paths.plugin_dir("alpha").exists());
        assert!(Lockfile::load(&paths.lock_path).unwrap().plugins.is_empty());
        // Removing it again names the missing plugin.
        let r = run(&args(&["remove", "alpha"]), &paths);
        assert!(!r.ok);
        assert!(r.lines[0].contains("alpha"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_with_an_unknown_name_fails_naming_it_and_with_no_plugins_succeeds_silently() {
        let dir = temp("update");
        let paths = paths_in(&dir);
        let r = run(&args(&["update", "nope"]), &paths);
        assert!(!r.ok);
        assert!(r.lines[0].contains("nope"));
        if cfg!(feature = "git") {
            let r = run(&args(&["update"]), &paths);
            assert!(r.ok);
            assert!(r.lines.is_empty());
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_rejects_a_bad_spec_and_an_already_installed_name_without_cloning() {
        let dir = temp("install");
        let paths = paths_in(&dir);
        let r = run(&args(&["install", "-oops"]), &paths);
        assert!(!r.ok);
        assert!(r.lines[0].contains("-oops"));
        Lockfile {
            plugins: vec![entry("alpha")],
        }
        .save(&paths.lock_path)
        .unwrap();
        let r = run(&args(&["install", "https://example.invalid/alpha.git"]), &paths);
        assert!(!r.ok);
        assert!(r.lines[0].contains("alpha"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn error_text_names_the_offending_value_for_every_variant() {
        let cases = [
            (PluginError::InvalidSpec("spec-x".into()), "spec-x"),
            (PluginError::InvalidName("name-x".into()), "name-x"),
            (PluginError::AlreadyInstalled("dup-x".into()), "dup-x"),
            (PluginError::NotInstalled("missing-x".into()), "missing-x"),
            (PluginError::SymlinkEscape(PathBuf::from("/esc/x")), "/esc/x"),
            (PluginError::Lockfile("lock-detail".into()), "lock-detail"),
            (PluginError::Git("git-detail".into()), "git-detail"),
        ];
        for (e, needle) in cases {
            assert!(error_text(&e).contains(needle), "{e:?}");
        }
        let io = PluginError::Io(std::io::Error::other("disk-detail"));
        assert!(error_text(&io).contains("disk-detail"));
        assert_eq!(
            error_text(&PluginError::GitUnavailable),
            t!("msg.plugin_error.git_unavailable").to_string()
        );
    }

    #[test]
    fn run_cli_without_a_config_dir_or_with_one_always_returns_a_report() {
        // The real config directory is never written by `list`; whatever the
        // machine has, the result is a well-formed report.
        let r = run_cli(&args(&["bogus-command"]));
        assert!(!r.ok);
        assert_eq!(r.lines.len(), 1);
    }
}
