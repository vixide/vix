//! T627: per-project (`.vix/settings.toml`) and per-language settings.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

/// A workspace at `dir` with a project settings file holding `project`, a
/// user config file at `dir/config.toml` holding `user`, and a session that
/// records the workspace as trusted when `trusted`. Returns the app, not yet
/// started (callers run [`App::load_project_settings`] or the prompt).
fn workspace(dir: &Path, user: &Settings, project: &str, trusted: bool) -> App {
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(dir.join(".vix").join("settings.toml"), project).unwrap();
    let config = dir.join("config.toml");
    user.save_to(&config).unwrap();
    let session = isolated_session_path();
    if trusted {
        let mut s = vix_session::Session::default();
        // The session keys a workspace by its canonical path (on macOS the
        // temp dir is a /var -> /private/var symlink).
        let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        s.set_scripts_trusted(&key.to_string_lossy(), Some(true));
        s.save_to(&session).unwrap();
    }
    let mut app = App::new(dir.to_path_buf(), Settings::load_from(&config))
        .with_session_path(session)
        .with_settings_path(config);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

const PROJECT: &str = "tab_width = 2\nwrap_column = 72\n\
    [languages.python]\nsoft_wrap = true\ntab_width = 8\n\
    [languages.markdown]\ntrim_trailing_whitespace = false\n";

fn user_settings() -> Settings {
    let mut user = Settings::default();
    user.tab_width = 3;
    user.wrap_column = 100;
    user.keymap = "emacs".into();
    user
}

/// Whether any message in the drawer mentions `needle` (a key name, which
/// is data, not translated text).
fn messages_mention(app: &App, needle: &str) -> bool {
    app.messages.items.iter().any(|m| m.text.contains(needle))
}

#[test]
fn project_settings_override_user_settings_in_a_trusted_workspace() {
    let dir = unique_dir("t627-precedence");
    let mut app = workspace(&dir, &user_settings(), PROJECT, true);

    app.load_project_settings();

    assert_eq!(app.settings.tab_width, 2, "project beats user");
    assert_eq!(app.settings.wrap_column, 72, "project beats user");
    assert_eq!(app.settings.keymap, "emacs", "user beats built-in");
    assert_eq!(app.settings.locale, "en", "built-in where nobody overrides");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_untrusted_workspace_ignores_the_project_file_until_trusted() {
    let dir = unique_dir("t627-untrusted");
    let mut app = workspace(&dir, &user_settings(), PROJECT, false);

    app.load_project_settings();
    assert_eq!(
        app.settings.tab_width, 3,
        "untrusted: the user value stands"
    );

    // The trust prompt now asks about the settings file even with no scripts.
    app.maybe_prompt_script_trust();
    let prompt = app.script_trust.as_ref().expect("trust prompt queued");
    assert!(prompt.settings);
    assert_eq!(prompt.count, 0);
    app.script_trust_opened_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .or(app.script_trust_opened_at);
    app.on_key(key('n'));
    assert_eq!(app.settings.tab_width, 3, "declined: still ignored");

    app.load_project_settings();
    assert_eq!(app.settings.tab_width, 3, "a decline is remembered");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn trusting_the_workspace_applies_the_project_file_at_once() {
    let dir = unique_dir("t627-trust");
    let mut app = workspace(&dir, &user_settings(), PROJECT, false);
    app.maybe_prompt_script_trust();
    app.script_trust_opened_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .or(app.script_trust_opened_at);

    app.on_key(key('y'));

    assert!(app.script_trust.is_none());
    assert_eq!(app.settings.tab_width, 2);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unknown_keys_in_the_project_file_are_reported() {
    let dir = unique_dir("t627-unknown");
    let project = "tab_widht = 2\n[languages.rust]\ntheme = \"light\"\n";
    let mut app = workspace(&dir, &Settings::default(), project, true);

    app.load_project_settings();

    assert!(messages_mention(&app, "tab_widht"), "a typo is reported");
    assert!(
        messages_mention(&app, "languages.rust.theme"),
        "a non-language key in a language table is reported"
    );
    assert_eq!(app.settings.tab_width, 4, "the unknown key changed nothing");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_language_override_applies_only_to_that_languages_buffers() {
    let dir = unique_dir("t627-language");
    fs::write(dir.join("a.py"), "x = 1\n").unwrap();
    fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
    let mut app = workspace(&dir, &Settings::default(), PROJECT, true);
    app.load_project_settings();

    app.open_initial(&dir.join("a.py"));
    let python = app.editor.active_tab().unwrap().editor.soft_wrap_enabled();
    app.open_initial(&dir.join("b.rs"));
    let rust = app.editor.active_tab().unwrap().editor.soft_wrap_enabled();

    assert!(python, "[languages.python] soft_wrap applies to a.py");
    assert!(!rust, "and not to b.rs");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_language_override_changes_what_saving_does_to_that_buffer() {
    let dir = unique_dir("t627-language-save");
    fs::write(dir.join("a.md"), "x\n").unwrap();
    fs::write(dir.join("b.txt"), "x\n").unwrap();
    let mut app = workspace(&dir, &Settings::default(), PROJECT, true);
    app.load_project_settings();

    for name in ["a.md", "b.txt"] {
        app.open_initial(&dir.join(name));
        let tab = app.editor.active_tab_mut().unwrap();
        tab.editor.set_content("trailing  \n");
        tab.dirty = true;
        app.run_action("file.save");
    }

    assert_eq!(
        fs::read_to_string(dir.join("a.md")).unwrap(),
        "trailing  \n"
    );
    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "trailing\n");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_settings_never_copies_project_values_into_the_user_file() {
    let dir = unique_dir("t627-unmerge");
    let mut app = workspace(&dir, &user_settings(), PROJECT, true);
    app.load_project_settings();
    app.settings.gutter.line_numbers = false;

    app.run_action("vix.settings");

    let saved = Settings::load_from(&dir.join("config.toml"));
    assert_eq!(saved.tab_width, 3, "the project's tab_width stays out");
    assert!(saved.languages.is_empty(), "so do its language tables");
    assert!(!saved.gutter.line_numbers, "the in-app change is saved");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_the_project_settings_file_reloads_it_live() {
    let dir = unique_dir("t627-reload");
    let mut app = workspace(&dir, &user_settings(), PROJECT, true);
    app.load_project_settings();
    let path = dir.join(".vix").join("settings.toml");
    app.open_initial(&path);
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("tab_width = 6\n");
    tab.dirty = true;

    app.run_action("file.save");

    assert_eq!(app.settings.tab_width, 6);
    assert_eq!(
        app.settings.wrap_column, 100,
        "a removed key falls back to the user's"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_broken_project_file_is_reported_and_ignored() {
    let dir = unique_dir("t627-broken");
    let mut app = workspace(&dir, &user_settings(), "tab_width = \"wide\"\n", true);
    let before = app.messages.items.len();

    app.load_project_settings();

    assert_eq!(app.settings.tab_width, 3);
    assert!(app.messages.items.len() > before);
    fs::remove_dir_all(&dir).ok();
}
