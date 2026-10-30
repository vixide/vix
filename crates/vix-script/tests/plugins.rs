//! T637: the Rhai plugin installer, end to end against real `git` — every
//! repository is a `file://` bare repo made in a temp directory, so nothing
//! here ever contacts a network host. Covers the lockfile round trip, an
//! install, and the three trust rules in `spec/index.md` § Plugins.

#![cfg(feature = "plugins")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use vix_script::plugins::{
    self, LockEntry, Lockfile, PluginError, PluginPaths, PluginState, discover_enabled,
};

/// A scratch directory unique per test, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vix-plugins-{tag}-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn paths(&self) -> PluginPaths {
        PluginPaths {
            scripts_dir: self.0.join("config/scripts"),
            lock_path: self.0.join("config/plugins.lock"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Run a fixture `git` command, isolated from the user's own git config
/// (only on the child process — the test's own environment is untouched).
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=Vix Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.symlinks=true",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An upstream: a work tree to commit in and a bare repo to install from.
struct Upstream {
    work: PathBuf,
    bare: PathBuf,
}

impl Upstream {
    /// Create `<root>/<name>.git` (bare) holding one commit of `files`.
    fn new(root: &Path, name: &str, files: &[(&str, &str)]) -> Self {
        let work = root.join(format!("{name}-work"));
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=main"]);
        let up = Self {
            work,
            bare: root.join(format!("{name}.git")),
        };
        up.commit(files, "initial");
        git(
            root,
            &[
                "clone",
                "--quiet",
                "--bare",
                up.work.to_str().unwrap(),
                up.bare.to_str().unwrap(),
            ],
        );
        up
    }

    /// Write `files`, commit them, and push to the bare repo (if it exists).
    fn commit(&self, files: &[(&str, &str)], message: &str) {
        for (rel, text) in files {
            let path = self.work.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git(&self.work, &["add", "--all"]);
        git(
            &self.work,
            &["commit", "--quiet", "--allow-empty", "-m", message],
        );
        if self.bare.exists() {
            git(
                &self.work,
                &["push", "--quiet", self.bare.to_str().unwrap(), "main"],
            );
        }
    }

    /// `file://` URL of the bare repo.
    fn url(&self) -> String {
        let p = self.bare.to_string_lossy().replace('\\', "/");
        if p.starts_with('/') {
            format!("file://{p}")
        } else {
            format!("file:///{p}")
        }
    }
}

/// No scripts.
const NONE: Vec<String> = Vec::new();

/// The stems of every script of every enabled plugin.
fn enabled_stems(paths: &PluginPaths) -> Vec<String> {
    discover_enabled(paths)
        .unwrap()
        .into_iter()
        .map(|s| s.stem)
        .collect()
}

const UPPER: &str = r#"
register_command("upper", "Upper", "on_upper");
fn on_upper() { set_selection_text(selection_text().to_upper()); }
"#;

#[test]
fn lockfile_round_trips() {
    let lock = Lockfile {
        plugins: vec![
            LockEntry {
                name: "case".into(),
                url: "https://example.invalid/me/case.git".into(),
                rev: Some("v1".into()),
                commit: "0123456789abcdef0123456789abcdef01234567".into(),
                trusted: Some("ab".repeat(32)),
            },
            LockEntry {
                name: "other".into(),
                url: "git@example.invalid:me/other.git".into(),
                rev: None,
                commit: "fedcba9876543210fedcba9876543210fedcba98".into(),
                trusted: None,
            },
        ],
    };
    let text = lock.to_toml().unwrap();
    assert!(text.contains("[[plugin]]"));
    assert!(!text.contains("trusted = \"\""), "None is omitted: {text}");
    assert_eq!(Lockfile::parse(&text).unwrap(), lock);

    let s = Scratch::new("lock");
    let path = s.paths().lock_path;
    assert_eq!(
        Lockfile::load(&path).unwrap(),
        Lockfile::default(),
        "missing = empty"
    );
    lock.save(&path).unwrap();
    assert_eq!(Lockfile::load(&path).unwrap(), lock);
    assert!(Lockfile::parse("[[plugin]]\nname = 3").is_err());
}

#[test]
fn installs_from_a_local_bare_repo_disabled_until_enabled() {
    let s = Scratch::new("install");
    let up = Upstream::new(&s.0, "vix-case", &[("upper.rhai", UPPER), ("README", "x")]);
    let paths = s.paths();

    let entry = plugins::install(&paths, &up.url()).unwrap();
    assert_eq!(entry.name, "vix-case");
    assert_eq!(entry.commit.len(), 40);
    assert_eq!(entry.trusted, None);
    assert!(paths.plugin_dir("vix-case").join("upper.rhai").is_file());
    assert!(!paths.scripts_dir.join(".vix-case.installing").exists());
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    assert_eq!(lock.get("vix-case"), Some(&entry));

    // Installed, but nothing loads until the user trusts it.
    assert_eq!(plugins::state(&paths, &entry), PluginState::Disabled);
    assert_eq!(enabled_stems(&paths), NONE);

    plugins::enable(&paths, "vix-case").unwrap();
    let found = discover_enabled(&paths).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].stem, "vix-case/upper");
    let (loaded, errors) = vix_script::load_discovered(&vix_script::Runtime::new(), found);
    assert_eq!(errors.len(), 0, "{errors:?}");
    assert_eq!(loaded[0].commands[0].id, "upper");

    // Installing again is refused, not a silent overwrite.
    assert!(matches!(
        plugins::install(&paths, &up.url()),
        Err(PluginError::AlreadyInstalled(_))
    ));
}

#[test]
fn install_pins_a_requested_rev() {
    let s = Scratch::new("rev");
    let up = Upstream::new(&s.0, "pinned", &[("a.rhai", "// v1")]);
    git(&up.work, &["tag", "v1"]);
    git(
        &up.work,
        &["push", "--quiet", up.bare.to_str().unwrap(), "v1"],
    );
    up.commit(&[("a.rhai", "// v2")], "second");
    let paths = s.paths();

    let entry = plugins::install(&paths, &format!("{}@v1", up.url())).unwrap();
    assert_eq!(entry.rev.as_deref(), Some("v1"));
    let text = std::fs::read_to_string(paths.plugin_dir("pinned").join("a.rhai")).unwrap();
    assert_eq!(text, "// v1");
}

/// Security rule 1 (ttt #992): an update that changes the code does not
/// inherit the old trust decision.
#[test]
fn update_that_changes_code_drops_trust() {
    let s = Scratch::new("update");
    let up = Upstream::new(&s.0, "upd", &[("upper.rhai", UPPER)]);
    let paths = s.paths();
    plugins::install(&paths, &up.url()).unwrap();
    plugins::enable(&paths, "upd").unwrap();

    // An update with no new commits keeps trust.
    let same = plugins::update(&paths, Some("upd")).unwrap();
    assert!(!same[0].trust_dropped);
    assert_eq!(same[0].old_commit, same[0].new_commit);
    assert_eq!(discover_enabled(&paths).unwrap().len(), 1);

    up.commit(&[("upper.rhai", "// rewritten upstream")], "change");
    let changed = plugins::update(&paths, None).unwrap();
    assert!(changed[0].trust_dropped);
    assert_ne!(changed[0].old_commit, changed[0].new_commit);
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    let entry = lock.get("upd").unwrap();
    assert_eq!(entry.trusted, None);
    assert_eq!(entry.commit, changed[0].new_commit);
    assert_eq!(plugins::state(&paths, entry), PluginState::Disabled);
    assert_eq!(enabled_stems(&paths), NONE);
}

/// A local edit after enabling also stops the plugin loading: trust is
/// pinned to the digest, checked at every load.
#[test]
fn local_edit_after_enable_is_reported_changed() {
    let s = Scratch::new("edit");
    let up = Upstream::new(&s.0, "edited", &[("upper.rhai", UPPER)]);
    let paths = s.paths();
    plugins::install(&paths, &up.url()).unwrap();
    plugins::enable(&paths, "edited").unwrap();
    std::fs::write(paths.plugin_dir("edited").join("upper.rhai"), "// tampered").unwrap();
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    assert_eq!(
        plugins::state(&paths, lock.get("edited").unwrap()),
        PluginState::Changed
    );
    assert_eq!(enabled_stems(&paths), NONE);
}

/// Security rule 2 (ttt #961/#1009): remove and disable drop the trust
/// record, so a reinstall (or re-enable) can't reuse a stale one.
#[test]
fn remove_and_disable_drop_the_trust_record() {
    let s = Scratch::new("remove");
    let up = Upstream::new(&s.0, "gone", &[("upper.rhai", UPPER)]);
    let paths = s.paths();
    plugins::install(&paths, &up.url()).unwrap();
    plugins::enable(&paths, "gone").unwrap();

    plugins::disable(&paths, "gone").unwrap();
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    assert_eq!(lock.get("gone").unwrap().trusted, None);
    assert_eq!(enabled_stems(&paths), NONE);

    plugins::enable(&paths, "gone").unwrap();
    let removed = plugins::remove(&paths, "gone").unwrap();
    assert!(removed.trusted.is_some());
    assert!(!paths.plugin_dir("gone").exists());
    assert!(
        Lockfile::load(&paths.lock_path)
            .unwrap()
            .get("gone")
            .is_none()
    );

    let again = plugins::install(&paths, &up.url()).unwrap();
    assert_eq!(again.trusted, None, "a reinstall starts untrusted");
    assert_eq!(enabled_stems(&paths), NONE);
    assert!(matches!(
        plugins::remove(&paths, "never-installed"),
        Err(PluginError::NotInstalled(_))
    ));
}

/// Security rule 3 (ttt #936): a symlink in a cloned plugin cannot point
/// outside the plugin directory — the install fails and leaves nothing.
#[cfg(unix)]
#[test]
fn symlink_escaping_the_plugin_fails_install() {
    let s = Scratch::new("symlink");
    let secret = s.0.join("secret.rhai");
    std::fs::write(&secret, "// outside").unwrap();
    let up = Upstream::new(&s.0, "evil", &[("ok.rhai", UPPER)]);
    std::os::unix::fs::symlink(&secret, up.work.join("steal.rhai")).unwrap();
    std::os::unix::fs::symlink("../../../..", up.work.join("up")).unwrap();
    up.commit(&[], "add links");
    let paths = s.paths();

    let err = plugins::install(&paths, &up.url()).unwrap_err();
    assert!(matches!(err, PluginError::SymlinkEscape(_)), "{err:?}");
    assert!(!paths.plugin_dir("evil").exists());
    assert!(!paths.scripts_dir.join(".evil.installing").exists());
    assert_eq!(
        Lockfile::load(&paths.lock_path).unwrap(),
        Lockfile::default()
    );
}

/// A symlink that stays inside the plugin is fine; an update that adds an
/// escaping one is rolled back to the previous commit.
#[cfg(unix)]
#[test]
fn symlink_inside_is_allowed_and_escaping_update_is_rolled_back() {
    let s = Scratch::new("symlink-update");
    let up = Upstream::new(&s.0, "linky", &[("lib/real.rhai", UPPER)]);
    std::os::unix::fs::symlink("lib/real.rhai", up.work.join("alias.rhai")).unwrap();
    up.commit(&[], "inside link");
    let paths = s.paths();
    let entry = plugins::install(&paths, &up.url()).unwrap();
    plugins::enable(&paths, "linky").unwrap();
    // The in-plugin symlink is not itself loaded as a script.
    assert_eq!(enabled_stems(&paths), NONE);

    std::os::unix::fs::symlink("/etc/hosts", up.work.join("hosts.rhai")).unwrap();
    up.commit(&[], "escaping link");
    let err = plugins::update(&paths, Some("linky")).unwrap_err();
    assert!(matches!(err, PluginError::SymlinkEscape(_)), "{err:?}");
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    assert_eq!(lock.get("linky").unwrap().commit, entry.commit);
    assert!(!paths.plugin_dir("linky").join("hosts.rhai").exists());
    assert_eq!(
        plugins::state(&paths, lock.get("linky").unwrap()),
        PluginState::Enabled
    );
}
