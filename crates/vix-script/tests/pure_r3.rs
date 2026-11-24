//! Round-3 tests that need no `git` and no features: the plugin lock, trust
//! digests, symlink rules, spec parsing, catalog directory loading and the
//! error text of load failures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::path::{Path, PathBuf};

use vix_script::plugins::{
    self, LockEntry, Lockfile, PluginError, PluginPaths, PluginState, parse_spec,
};
use vix_script::{Catalog, DiscoveredScript, Runtime, load_discovered};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vix-pure-{tag}-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn paths(&self) -> PluginPaths {
        PluginPaths {
            scripts_dir: self.0.join("scripts"),
            lock_path: self.0.join("plugins.lock"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn entry(name: &str) -> LockEntry {
    LockEntry {
        name: name.into(),
        url: format!("file:///nowhere/{name}.git"),
        rev: Some("v1".into()),
        commit: "a".repeat(40),
        trusted: None,
    }
}

fn install_by_hand(paths: &PluginPaths, name: &str, files: &[(&str, &str)]) {
    let dir = paths.plugin_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    for (rel, text) in files {
        std::fs::write(dir.join(rel), text).unwrap();
    }
    let mut lock = Lockfile::load(&paths.lock_path).unwrap();
    lock.plugins.push(entry(name));
    lock.save(&paths.lock_path).unwrap();
}

#[test]
fn error_display_and_source() {
    let cases: Vec<(PluginError, &str)> = vec![
        (
            PluginError::InvalidSpec("x".into()),
            "invalid plugin source: x",
        ),
        (
            PluginError::InvalidName("y".into()),
            "invalid plugin name: y",
        ),
        (
            PluginError::AlreadyInstalled("z".into()),
            "already installed: z",
        ),
        (PluginError::NotInstalled("w".into()), "not installed: w"),
        (
            PluginError::SymlinkEscape(PathBuf::from("/a/b")),
            "symlink escapes the plugin directory: /a/b",
        ),
        (PluginError::Lockfile("bad".into()), "plugins.lock: bad"),
        (PluginError::Git("boom".into()), "git: boom"),
        (
            PluginError::GitUnavailable,
            "git is not included in this build",
        ),
    ];
    for (err, text) in cases {
        assert_eq!(err.to_string(), text);
        assert!(std::error::Error::source(&err).is_none());
    }
    let io: PluginError = std::io::Error::other("disk gone").into();
    assert_eq!(io.to_string(), "disk gone");
    assert!(std::error::Error::source(&io).is_some());
}

#[test]
fn state_keys_are_distinct_i18n_keys() {
    let keys = [
        PluginState::Enabled,
        PluginState::Disabled,
        PluginState::Changed,
        PluginState::Missing,
    ]
    .map(PluginState::i18n_key);
    assert_eq!(keys[0], "ui.plugin_state.enabled");
    assert_eq!(keys[3], "ui.plugin_state.missing");
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique.len(), 4);
}

#[test]
fn lockfile_parse_load_save_and_edit() {
    assert!(matches!(
        Lockfile::parse("[[plugin]]\nname = 3"),
        Err(PluginError::Lockfile(_))
    ));
    let s = Scratch::new("lock");
    let path = s.0.join("nested/dir/plugins.lock");
    assert_eq!(Lockfile::load(&path).unwrap(), Lockfile::default());
    let mut lock = Lockfile::default();
    lock.plugins.push(entry("one"));
    lock.plugins.push(entry("two"));
    lock.save(&path).unwrap();
    assert!(
        !path.with_extension("lock.tmp").exists(),
        "temp file renamed away"
    );
    let back = Lockfile::load(&path).unwrap();
    assert_eq!(back, lock);
    assert_eq!(back.get("two").unwrap().rev.as_deref(), Some("v1"));
    assert!(back.get("three").is_none());
    let mut back = back;
    back.get_mut("one").unwrap().trusted = Some("abc".into());
    assert_eq!(back.get("one").unwrap().trusted.as_deref(), Some("abc"));
    assert!(back.get_mut("nope").is_none());
    assert_eq!(back.remove("one").unwrap().name, "one");
    assert!(back.remove("one").is_none());
    // A directory where the lock file should be: read fails with Io.
    let dir_as_lock = s.0.join("dirlock");
    std::fs::create_dir_all(&dir_as_lock).unwrap();
    assert!(matches!(
        Lockfile::load(&dir_as_lock),
        Err(PluginError::Io(_))
    ));
    // Saving through a path whose parent is a file fails with Io.
    let blocker = s.0.join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    assert!(matches!(
        lock.save(&blocker.join("plugins.lock")),
        Err(PluginError::Io(_))
    ));
}

#[test]
fn parse_spec_splits_rev_and_rejects_dangerous_input() {
    assert_eq!(
        parse_spec("file:///srv/repo.git@v1").unwrap(),
        ("file:///srv/repo.git".to_string(), Some("v1".to_string()))
    );
    assert_eq!(
        parse_spec("git@host:me/repo.git").unwrap(),
        ("git@host:me/repo.git".to_string(), None)
    );
    assert_eq!(
        parse_spec("https://h/x.git").unwrap(),
        ("https://h/x.git".to_string(), None)
    );
    assert_eq!(parse_spec("plain").unwrap(), ("plain".to_string(), None));
    for bad in [
        "",
        "-oProxy=x",
        "has space",
        "ext::sh -c x",
        "ext::cmd",
        "https://h/x.git@",
        "https://h/x.git@-bad",
        "https://h/x.git@a b",
        "ctrl\u{7}char",
    ] {
        assert!(
            matches!(parse_spec(bad), Err(PluginError::InvalidSpec(_))),
            "{bad:?} must be rejected"
        );
    }
}

#[test]
fn names_come_from_the_last_url_segment() {
    assert_eq!(
        plugins::name_from_url("https://e.com/me/vix-case.git").unwrap(),
        "vix-case"
    );
    assert_eq!(plugins::name_from_url("git@host:me/tool/").unwrap(), "tool");
    assert_eq!(plugins::name_from_url("host:tool.git").unwrap(), "tool");
    assert!(matches!(
        plugins::name_from_url("https://e.com/.hidden"),
        Err(PluginError::InvalidName(_))
    ));
    assert!(plugins::valid_name("a.b_c-d9"));
    assert!(!plugins::valid_name(""));
    assert!(!plugins::valid_name(".dot"));
    assert!(!plugins::valid_name("-dash"));
    assert!(!plugins::valid_name("bad/slash"));
    assert!(!plugins::valid_name(&"a".repeat(65)));
    assert!(plugins::valid_name(&"a".repeat(64)));
}

#[test]
fn digest_tracks_content_names_and_ignores_dot_git() {
    let s = Scratch::new("digest");
    let dir = s.0.join("p");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join("a.rhai"), "1").unwrap();
    std::fs::write(dir.join("sub/b.txt"), "2").unwrap();
    let first = plugins::digest(&dir).unwrap();
    assert_eq!(first.len(), 64);
    std::fs::write(dir.join(".git/HEAD"), "ignored").unwrap();
    assert_eq!(plugins::digest(&dir).unwrap(), first, ".git is excluded");
    std::fs::write(dir.join("sub/b.txt"), "3").unwrap();
    assert_ne!(plugins::digest(&dir).unwrap(), first, "content matters");
    assert!(matches!(
        plugins::digest(&s.0.join("missing")),
        Err(PluginError::Io(_))
    ));
}

#[cfg(unix)]
#[test]
fn symlinks_are_hashed_by_target_and_must_stay_inside() {
    use std::os::unix::fs::symlink;
    let s = Scratch::new("sym");
    let dir = s.0.join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("real.txt"), "x").unwrap();
    symlink("real.txt", dir.join("ok-link")).unwrap();
    plugins::check_symlinks(&dir).unwrap();
    let with_link = plugins::digest(&dir).unwrap();
    std::fs::remove_file(dir.join("ok-link")).unwrap();
    assert_ne!(
        plugins::digest(&dir).unwrap(),
        with_link,
        "a link is part of the identity"
    );
    // Escaping link.
    let outside = s.0.join("outside.txt");
    std::fs::write(&outside, "o").unwrap();
    symlink(&outside, dir.join("escape")).unwrap();
    match plugins::check_symlinks(&dir) {
        Err(PluginError::SymlinkEscape(p)) => assert!(p.ends_with("escape")),
        other => panic!("expected SymlinkEscape, got {other:?}"),
    }
    std::fs::remove_file(dir.join("escape")).unwrap();
    // Dangling link.
    symlink("nowhere", dir.join("dangling")).unwrap();
    assert!(matches!(
        plugins::check_symlinks(&dir),
        Err(PluginError::SymlinkEscape(_))
    ));
    assert!(matches!(
        plugins::check_symlinks(&s.0.join("absent")),
        Err(PluginError::Io(_))
    ));
}

#[test]
fn trust_lifecycle_enable_change_disable_remove() {
    let s = Scratch::new("trust");
    let paths = s.paths();
    install_by_hand(&paths, "demo", &[("a.rhai", "let x = 1;")]);
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    let e = lock.get("demo").unwrap().clone();
    assert_eq!(plugins::state(&paths, &e), PluginState::Disabled);
    assert_eq!(plugins::discover_enabled(&paths).unwrap(), Vec::new());

    plugins::enable(&paths, "demo").unwrap();
    let e = Lockfile::load(&paths.lock_path)
        .unwrap()
        .get("demo")
        .unwrap()
        .clone();
    assert_eq!(plugins::state(&paths, &e), PluginState::Enabled);
    let found = plugins::discover_enabled(&paths).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].stem, "demo/a");
    assert_eq!(
        found[0].locales_dir.as_deref(),
        Some(
            paths
                .plugin_dir("demo")
                .join(plugins::LOCALES_DIR)
                .as_path()
        )
    );

    std::fs::write(paths.plugin_dir("demo").join("a.rhai"), "let x = 2;").unwrap();
    assert_eq!(plugins::state(&paths, &e), PluginState::Changed);
    assert_eq!(plugins::discover_enabled(&paths).unwrap(), Vec::new());

    plugins::disable(&paths, "demo").unwrap();
    assert!(
        Lockfile::load(&paths.lock_path)
            .unwrap()
            .get("demo")
            .unwrap()
            .trusted
            .is_none()
    );

    let removed = plugins::remove(&paths, "demo").unwrap();
    assert_eq!(removed.name, "demo");
    assert!(!paths.plugin_dir("demo").exists());
    assert_eq!(
        Lockfile::load(&paths.lock_path).unwrap().plugins,
        Vec::new()
    );
    // A directory that vanished is Missing.
    assert_eq!(plugins::state(&paths, &e), PluginState::Missing);
}

#[test]
fn trust_operations_reject_unknown_and_invalid_names() {
    let s = Scratch::new("unknown");
    let paths = s.paths();
    install_by_hand(&paths, "real", &[("a.rhai", "")]);
    for name in ["ghost", "../real", ""] {
        assert!(
            matches!(
                plugins::enable(&paths, name),
                Err(PluginError::NotInstalled(_))
            ),
            "{name}"
        );
    }
    assert!(matches!(
        plugins::disable(&paths, "ghost"),
        Err(PluginError::NotInstalled(_))
    ));
    assert!(matches!(
        plugins::remove(&paths, "ghost"),
        Err(PluginError::NotInstalled(_))
    ));
    assert!(matches!(
        plugins::remove(&paths, "../real"),
        Err(PluginError::InvalidName(_))
    ));
    assert!(
        paths.plugin_dir("real").exists(),
        "an invalid name deletes nothing"
    );
    // Lock entry whose directory is gone: enable refuses, state is Missing.
    let mut lock = Lockfile::load(&paths.lock_path).unwrap();
    lock.plugins.push(entry("gone"));
    lock.save(&paths.lock_path).unwrap();
    assert!(matches!(
        plugins::enable(&paths, "gone"),
        Err(PluginError::NotInstalled(_))
    ));
    // remove() of a plugin whose dir is a plain file removes the file.
    let mut lock = Lockfile::load(&paths.lock_path).unwrap();
    lock.plugins.push(entry("filey"));
    lock.save(&paths.lock_path).unwrap();
    std::fs::write(paths.plugin_dir("filey"), "x").unwrap();
    plugins::remove(&paths, "filey").unwrap();
    assert!(!paths.plugin_dir("filey").exists());
    // Removal of an entry whose directory was already deleted is fine.
    plugins::remove(&paths, "gone").unwrap();
}

#[test]
fn catalog_dir_loading_reports_each_bad_file_and_keeps_good_ones() {
    let s = Scratch::new("catalog");
    let dir = s.0.join("locales");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("en.toml"), "hello = \"Hi\"\n[a]\nb = \"nested\"").unwrap();
    std::fs::write(dir.join("fr.toml"), "hello = \"Salut\"").unwrap();
    std::fs::write(dir.join("not a code.toml"), "x = \"1\"").unwrap();
    std::fs::write(dir.join("de.toml"), "broken = [").unwrap();
    std::fs::write(dir.join("es.toml"), "n = 3").unwrap();
    std::fs::write(dir.join("readme.txt"), "ignored").unwrap();
    std::fs::create_dir_all(dir.join("ja.toml")).unwrap();
    let big = vec![b'#'; usize::try_from(vix_script::MAX_FILE_BYTES).unwrap() + 1];
    std::fs::write(dir.join("it.toml"), big).unwrap();
    let (catalog, errors) = Catalog::load_dir(&dir);
    assert_eq!(catalog.locales(), vec!["en", "fr"]);
    assert!(!catalog.is_empty());
    assert_eq!(catalog.lookup("fr", "hello"), Some("Salut"));
    assert_eq!(catalog.lookup("fr-CA", "hello"), Some("Salut"));
    assert_eq!(catalog.lookup("en", "a.b"), Some("nested"));
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|e| e == "not a code.toml: not a locale code")
    );
    assert!(errors.iter().any(|e| e.starts_with("de.toml: ")));
    assert!(errors.iter().any(|e| e.starts_with("es.toml: ")));
    assert!(errors.iter().any(|e| e.starts_with("it.toml: larger than")));
    // Missing directory: empty catalog, no errors.
    let (empty, none) = Catalog::load_dir(&s.0.join("absent"));
    assert!(empty.is_empty());
    assert_eq!(none, Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn catalog_unreadable_file_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new("unreadable");
    let dir = s.0.join("locales");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("en.toml");
    std::fs::write(&file, "a = \"b\"").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read_to_string(&file).is_ok(); // running as root
    let (catalog, errors) = Catalog::load_dir(&dir);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    if !readable {
        assert!(catalog.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("en.toml: "), "{errors:?}");
    }
}

#[test]
fn load_failures_are_reported_with_stem_and_message() {
    let s = Scratch::new("load");
    std::fs::create_dir_all(&s.0).unwrap();
    let bad = s.0.join("bad.rhai");
    std::fs::write(&bad, "let = ;").unwrap();
    let unreadable = s.0.join("missing.rhai");
    let runtime = Runtime::new();
    assert_eq!(runtime.locale(), "en");
    runtime.set_locale("fr");
    assert_eq!(runtime.locale(), "fr");
    let (loaded, errors) = load_discovered(
        &runtime,
        vec![
            DiscoveredScript::new("bad", &bad),
            DiscoveredScript::new("missing", &unreadable),
        ],
    );
    assert!(loaded.is_empty());
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].stem, "bad");
    assert!(
        errors[0]
            .to_string()
            .starts_with("script 'bad' failed to load: "),
        "{}",
        errors[0]
    );
    assert_eq!(errors[1].stem, "missing");
    let _ = Runtime::default();
}

#[test]
fn bad_translation_files_are_reported_under_the_plugin_name() {
    let s = Scratch::new("badloc");
    let paths = s.paths();
    install_by_hand(&paths, "pl", &[("a.rhai", "let x = 1;")]);
    let loc = paths.plugin_dir("pl").join(plugins::LOCALES_DIR);
    std::fs::create_dir_all(&loc).unwrap();
    std::fs::write(loc.join("en.toml"), "oops = [").unwrap();
    plugins::enable(&paths, "pl").unwrap();
    let found = plugins::discover_enabled(&paths).unwrap();
    let (loaded, errors) = load_discovered(&Runtime::new(), found);
    assert_eq!(loaded.len(), 1, "the script still loads");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].stem, "pl");
    assert!(
        errors[0].message.starts_with("locales/en.toml: "),
        "{errors:?}"
    );
    let _ = Path::new("");
}
