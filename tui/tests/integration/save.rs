//! T654: Save All and the Save As overwrite / dangling-symlink guards;
//! T626: saving the settings file reloads it live.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

/// Open `path` and replace its buffer with `text`, marking it dirty.
fn open_dirty(app: &mut App, path: &Path, text: &str) {
    app.editor.open(path, false).unwrap();
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.dirty = true;
}

/// Run Save As on the active tab with `target` as the prompt's input.
fn save_as(app: &mut App, target: &Path) {
    app.run_action("file.save_as");
    app.prompt.as_mut().unwrap().input = target.display().to_string();
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

#[test]
fn save_all_saves_every_dirty_file_tab_and_restores_the_active_tab() {
    let dir = unique_dir("save-all");
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("b.txt"), "b\n").unwrap();
    fs::write(dir.join("c.txt"), "c\n").unwrap();
    let mut app = app_at(&dir);
    open_dirty(&mut app, &dir.join("a.txt"), "a2\n");
    open_dirty(&mut app, &dir.join("b.txt"), "b2\n");
    app.editor.open(&dir.join("c.txt"), false).unwrap();
    let active = app.editor.active;

    app.run_action("file.save_all");

    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a2\n");
    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "b2\n");
    assert_eq!(fs::read_to_string(dir.join("c.txt")).unwrap(), "c\n");
    assert!(app.editor.tabs.iter().all(|t| !t.dirty || t.path.is_none()));
    assert_eq!(app.editor.active, active);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_all_catalog_alias_skips_read_only_and_untitled_tabs() {
    let dir = unique_dir("save-all-skip");
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("ro.txt"), "ro\n").unwrap();
    let mut app = app_at(&dir);
    // An untitled, dirty buffer: Save All must not open a Save As prompt.
    app.editor.new_tab_with_content("scratch");
    app.editor.active_tab_mut().unwrap().dirty = true;
    open_dirty(&mut app, &dir.join("ro.txt"), "ro2\n");
    app.editor.active_tab_mut().unwrap().read_only = true;
    open_dirty(&mut app, &dir.join("a.txt"), "a2\n");

    app.run_action("save_all");

    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a2\n");
    assert_eq!(fs::read_to_string(dir.join("ro.txt")).unwrap(), "ro\n");
    assert!(app.prompt.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_all_stops_at_the_first_failure() {
    let dir = unique_dir("save-all-fail");
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("b.txt"), "b\n").unwrap();
    let mut app = app_at(&dir);
    open_dirty(&mut app, &dir.join("a.txt"), "a2\n");
    // The first tab's file can no longer be written: its path is now a
    // directory.
    let a = app.editor.active_tab().unwrap().path.clone().unwrap();
    fs::remove_file(&a).unwrap();
    fs::create_dir(&a).unwrap();
    open_dirty(&mut app, &dir.join("b.txt"), "b2\n");

    app.run_action("file.save_all");

    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "b\n");
    assert_eq!(app.editor.active, 1, "the failing tab is left active");
    assert!(app.editor.tabs[1].dirty && app.editor.tabs[2].dirty);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_onto_an_existing_file_asks_before_overwriting() {
    let dir = unique_dir("save-as-exists");
    fs::write(dir.join("target.txt"), "keep me\n").unwrap();
    let mut app = app_at(&dir);
    app.editor.new_tab_with_content("new text\n");

    save_as(&mut app, &dir.join("target.txt"));
    assert!(app.confirm.is_some(), "an overwrite confirmation is shown");
    assert_eq!(
        fs::read_to_string(dir.join("target.txt")).unwrap(),
        "keep me\n"
    );
    app.on_key(key('n'));
    assert!(app.confirm.is_none());
    assert_eq!(
        fs::read_to_string(dir.join("target.txt")).unwrap(),
        "keep me\n"
    );

    save_as(&mut app, &dir.join("target.txt"));
    app.on_key(key('y'));
    assert_eq!(
        fs::read_to_string(dir.join("target.txt")).unwrap(),
        "new text\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_to_a_new_path_saves_without_asking() {
    let dir = unique_dir("save-as-new");
    let mut app = app_at(&dir);
    app.editor.new_tab_with_content("hello\n");
    save_as(&mut app, &dir.join("fresh.txt"));
    assert!(app.confirm.is_none());
    assert_eq!(
        fs::read_to_string(dir.join("fresh.txt")).unwrap(),
        "hello\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn save_as_refuses_a_dangling_symlink_target() {
    let dir = unique_dir("save-as-dangling");
    std::os::unix::fs::symlink(dir.join("missing.txt"), dir.join("link.txt")).unwrap();
    let mut app = app_at(&dir);
    app.editor.new_tab_with_content("hello\n");
    save_as(&mut app, &dir.join("link.txt"));
    assert!(app.confirm.is_none());
    assert!(
        !dir.join("missing.txt").exists(),
        "nothing written through it"
    );
    fs::remove_dir_all(&dir).ok();
}

/// An app whose settings file is `config` inside a temp dir (never the
/// user's real `config.toml`), with that file opened in the active tab.
fn app_editing_config(dir: &Path, config: &Path) -> App {
    Settings::default().save_to(config).unwrap();
    let mut app = app_at(dir).with_settings_path(config);
    app.editor.open(config, false).unwrap();
    app
}

/// The serialized text of default settings with `keymap` set to `id`.
fn config_text_with_keymap(dir: &Path, id: &str) -> String {
    let scratch = dir.join("scratch.toml");
    let mut settings = Settings::default();
    settings.keymap = id.to_string();
    settings.save_to(&scratch).unwrap();
    fs::read_to_string(&scratch).unwrap()
}

#[test]
fn saving_the_settings_file_applies_a_changed_keymap_live() {
    // T626: no restart needed after editing config.toml.
    let dir = unique_dir("settings-reload");
    let config = dir.join("config.toml");
    let mut app = app_editing_config(&dir, &config);
    assert_eq!(app.settings.keymap, "apple");
    let text = config_text_with_keymap(&dir, "sublime");
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(&text);
    tab.dirty = true;

    app.run_action("file.save");

    assert_eq!(app.settings.keymap, "sublime", "the saved keymap is live");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_a_broken_settings_file_keeps_the_current_settings() {
    let dir = unique_dir("settings-reload-bad");
    let config = dir.join("config.toml");
    let mut app = app_editing_config(&dir, &config);
    app.settings.keymap = "eclipse".to_string();
    let before = app.messages.items.len();
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("keymap = [unclosed\n");
    tab.dirty = true;

    app.run_action("file.save");

    assert_eq!(
        app.settings.keymap, "eclipse",
        "a parse error keeps the in-memory settings, not the defaults"
    );
    assert!(app.messages.items.len() > before, "the failure is reported");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reload_settings_action_rereads_the_file_without_a_save() {
    let dir = unique_dir("settings-reload-action");
    let config = dir.join("config.toml");
    let mut app = app_editing_config(&dir, &config);
    let text = config_text_with_keymap(&dir, "eclipse");
    fs::write(&config, text).unwrap();

    app.run_action("vix.reload_settings");

    assert_eq!(app.settings.keymap, "eclipse");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_some_other_toml_file_does_not_reload_settings() {
    let dir = unique_dir("settings-reload-other");
    let config = dir.join("config.toml");
    let mut app = app_editing_config(&dir, &config);
    let other = dir.join("other.toml");
    fs::write(&other, config_text_with_keymap(&dir, "sublime")).unwrap();
    app.editor.open(&other, false).unwrap();
    let tab = app.editor.active_tab_mut().unwrap();
    tab.dirty = true;

    app.run_action("file.save");

    assert_eq!(app.settings.keymap, "apple");
    fs::remove_dir_all(&dir).ok();
}
