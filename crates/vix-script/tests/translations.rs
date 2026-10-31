//! T735: a plugin ships its own strings per locale in `<name>/locales/`,
//! and its scripts read them with `tr`. Plugins are laid out by hand in a
//! temp directory — no git, no network — and enabled through the real
//! `plugins.lock` trust path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::path::PathBuf;

use vix_script::plugins::{self, LockEntry, Lockfile, PluginPaths, PluginState};
use vix_script::{HostMessage, HostState, InvokeOutcome, Runtime};

/// A scratch directory unique per test, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vix-script-i18n-{tag}-{}-{n}", std::process::id()));
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

/// Write `files` (relative path, contents) into plugin `name`, lock it and
/// enable it.
fn enabled_plugin(paths: &PluginPaths, name: &str, files: &[(&str, &str)]) {
    let dir = paths.plugin_dir(name);
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut lock = Lockfile::load(&paths.lock_path).unwrap();
    lock.plugins.push(LockEntry {
        name: name.into(),
        url: format!("file:///nowhere/{name}.git"),
        rev: None,
        commit: "0".repeat(40),
        trusted: None,
    });
    lock.save(&paths.lock_path).unwrap();
    plugins::enable(paths, name).unwrap();
}

const SCRIPT: &str = r#"
register_command("upper", tr("cmd.upper"), "on_upper");
fn on_upper() {
    message(tr("msg.done", #{ count: 3 }));
    message(tr("only.english"));
    message(tr("not.translated"));
    message(locale());
}
"#;

const EN: &str = r#"
only.english = "English only"
[cmd]
upper = "Uppercase"
[msg]
done = "Changed %{count} words"
"#;

const FR: &str = r#"
[cmd]
upper = "Majuscules"
[msg]
done = "%{count} mots modifiés"
"#;

fn load(paths: &PluginPaths, locale: &str) -> (Vec<vix_script::LoadedScript>, Runtime) {
    let runtime = Runtime::new();
    runtime.set_locale(locale);
    let found = plugins::discover_enabled(paths).unwrap();
    let (loaded, errors) = vix_script::load_discovered(&runtime, found);
    assert_eq!(errors, Vec::new());
    (loaded, runtime)
}

fn messages(runtime: &Runtime, script: &vix_script::LoadedScript) -> Vec<HostMessage> {
    let InvokeOutcome::Ran(state) =
        runtime.invoke(script, "on_upper", vec![], HostState::default())
    else {
        panic!("handler should run")
    };
    state.messages
}

fn info(text: &str) -> HostMessage {
    HostMessage::Info(text.to_string())
}

/// The label and the handler's messages are in the active locale, with the
/// host's fallback chain (tag truncation, then English, then the key).
#[test]
fn plugin_strings_follow_the_active_locale() {
    let s = Scratch::new("follow");
    let paths = s.paths();
    enabled_plugin(
        &paths,
        "case",
        &[
            ("upper.rhai", SCRIPT),
            ("locales/en.toml", EN),
            ("locales/fr.toml", FR),
        ],
    );

    let (loaded, runtime) = load(&paths, "fr");
    assert_eq!(loaded[0].stem, "case/upper");
    assert_eq!(loaded[0].commands[0].label, "Majuscules");
    assert_eq!(
        messages(&runtime, &loaded[0]),
        vec![
            info("3 mots modifiés"),
            info("English only"),
            info("not.translated"),
            info("fr"),
        ]
    );

    // `fr-CA` truncates to `fr`; a locale with no table falls back to English.
    let (loaded, _) = load(&paths, "fr-CA");
    assert_eq!(loaded[0].commands[0].label, "Majuscules");
    let (loaded, runtime) = load(&paths, "de");
    assert_eq!(loaded[0].commands[0].label, "Uppercase");
    assert_eq!(messages(&runtime, &loaded[0])[0], info("Changed 3 words"));
}

/// Each plugin reads only its own catalog, and a loose script has none:
/// `tr` returns the key.
#[test]
fn catalogs_are_per_plugin() {
    let s = Scratch::new("per-plugin");
    let paths = s.paths();
    let other = r#"register_command("x", tr("cmd.upper"), "h"); fn h() {}"#;
    enabled_plugin(&paths, "a", &[("a.rhai", SCRIPT), ("locales/en.toml", EN)]);
    enabled_plugin(&paths, "b", &[("b.rhai", other)]);
    std::fs::write(paths.scripts_dir.join("loose.rhai"), other).unwrap();

    let runtime = Runtime::new();
    let mut found = vix_script::discover(Some(&paths.scripts_dir), None);
    found.extend(plugins::discover_enabled(&paths).unwrap());
    let (loaded, errors) = vix_script::load_discovered(&runtime, found);
    assert_eq!(errors, Vec::new());
    let label = |stem: &str| {
        loaded.iter().find(|l| l.stem == stem).unwrap().commands[0]
            .label
            .clone()
    };
    assert_eq!(label("a/a"), "Uppercase");
    assert_eq!(label("b/b"), "cmd.upper");
    assert_eq!(label("loose"), "cmd.upper");
    // `a`'s handler still reads `a`'s catalog after `b` and `loose` loaded.
    let a = loaded.iter().find(|l| l.stem == "a/a").unwrap();
    assert_eq!(messages(&runtime, a)[0], info("Changed 3 words"));
}

/// `locales/` is data: a `.rhai` file in it never loads, a broken catalog
/// file is reported under the plugin's name while the scripts still load,
/// and the translations are part of the trusted digest.
#[test]
fn locales_are_data_reported_and_trusted() {
    let s = Scratch::new("data");
    let paths = s.paths();
    enabled_plugin(
        &paths,
        "case",
        &[
            ("upper.rhai", SCRIPT),
            ("locales/en.toml", EN),
            (
                "locales/evil.rhai",
                r#"register_command("evil", "Evil", "h"); fn h() {}"#,
            ),
            ("locales/de.toml", "[cmd]\nupper = 3\n"),
        ],
    );

    let found = plugins::discover_enabled(&paths).unwrap();
    assert_eq!(
        found.iter().map(|f| f.stem.as_str()).collect::<Vec<_>>(),
        vec!["case/upper"]
    );
    let runtime = Runtime::new();
    runtime.set_locale("de");
    let (loaded, errors) = vix_script::load_discovered(&runtime, found);
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].commands[0].label, "Uppercase",
        "bad de falls back"
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].stem, "case");
    assert!(
        errors[0].message.starts_with("locales/de.toml"),
        "{errors:?}"
    );

    // Editing a translation after enabling withdraws trust, like code.
    std::fs::write(
        paths.plugin_dir("case").join("locales/en.toml"),
        "[cmd]\nupper = \"Tampered\"\n",
    )
    .unwrap();
    let lock = Lockfile::load(&paths.lock_path).unwrap();
    assert_eq!(
        plugins::state(&paths, lock.get("case").unwrap()),
        PluginState::Changed
    );
    assert_eq!(plugins::discover_enabled(&paths).unwrap().len(), 0);
}
