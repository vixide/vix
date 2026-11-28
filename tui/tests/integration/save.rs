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
    // a.txt took over the empty starting tab, so it is tab 0 and b.txt tab 1.
    assert_eq!(app.editor.active, 0, "the failing tab is left active");
    assert!(app.editor.tabs[0].dirty && app.editor.tabs[1].dirty);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_onto_an_existing_file_asks_before_overwriting() {
    let dir = unique_dir("save-as-exists");
    fs::write(dir.join("target.txt"), "keep me\n").unwrap();
    let mut app = app_at(&dir);
    app.editor.new_tab_with_content("new text\n");

    save_as(&mut app, &dir.join("target.txt"));
    draw_frame(&mut app, 120, 40);
    draw_frame(&mut app, 50, 16);
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

/// T796: the bytes a cancelled Save As overwrite must leave alone, and a
/// file name outside Latin script for the prompt to show.
const KEPT: &[u8] = b"keep\r\n\xff\xfe not UTF-8 \x00\n";

#[test]
fn cancelling_a_save_as_overwrite_of_a_non_latin_path_keeps_the_file_byte_identical() {
    let dir = unique_dir("save-as-cancel-non-latin");
    let target = dir.join("日本語-файл-ملف.txt");
    fs::write(&target, KEPT).unwrap();
    let mut app = app_at(&dir);
    app.editor.new_tab_with_content("new text\n");

    for cancel in [key('n'), esc()] {
        save_as(&mut app, &target);
        draw_frame(&mut app, 120, 40);
        let message = app
            .confirm
            .as_ref()
            .expect("an overwrite confirmation is shown")
            .message
            .clone();
        assert!(message.contains(&target.display().to_string()), "{message}");
        assert!(!message.contains("%{"), "{message}");
        // Keys that are not an answer leave it asking.
        app.on_key(key('o'));
        assert!(app.confirm.is_some(), "only y, n and Esc answer it");
        app.on_key(cancel);
        assert!(app.confirm.is_none());
        assert_eq!(fs::read(&target).unwrap(), KEPT, "cancel keeps every byte");
        assert!(
            app.editor.active_tab().unwrap().path.is_none(),
            "the buffer did not adopt it"
        );
    }
    fs::remove_dir_all(&dir).ok();
}

/// T796: the unmappable-character refusal names a non-Latin path, the
/// encoding and the character, and writes nothing.
#[test]
fn an_unmappable_save_of_a_non_latin_path_names_it_and_keeps_the_file() {
    let dir = unique_dir("unmappable-non-latin");
    let path = dir.join("café-日本.txt");
    fs::write(&path, b"caf\xe9\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&path);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("caf\u{e9} \u{65e5}\n");
    app.editor.active_tab_mut().unwrap().dirty = true;
    app.run_action("file.save");
    assert_eq!(fs::read(&path).unwrap(), b"caf\xe9\n");
    let last = &app
        .messages
        .items
        .last()
        .expect("an error was reported")
        .text;
    for part in ["café-日本.txt", "windows-1252", "\u{65e5}", "U+65E5"] {
        assert!(last.contains(part), "{part} missing from {last}");
    }
    assert!(app.editor.active_tab().unwrap().dirty);
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

/// T663: a leftover pre-save copy in the recovery directory is reported in the
/// status line -- but only when asked, with an explicit directory. `App::new`
/// never reads the machine's real `recovery_dir()`, so a developer's own
/// leftovers cannot leak into any test.
#[test]
fn interrupted_save_notice_reads_only_the_directory_it_is_given() {
    let dir = unique_dir("recovery-notice");
    let recovery = dir.join("recovery");
    fs::create_dir_all(&recovery).unwrap();

    // Building an app reads no recovery directory: nothing to report, whatever
    // is in the real home.
    let mut app = app_at(&dir);
    let baseline = app.status.clone();

    // An empty temp recovery directory is silent.
    app.notice_interrupted_saves_in(&recovery);
    assert_eq!(app.status, baseline);

    // A leftover copy (magic line, original path, saved bytes) is reported.
    fs::write(
        recovery.join("1-0-a.txt.vixrecover"),
        format!("VIXRECOVER\n{}\nold\n", dir.join("a.txt").display()),
    )
    .unwrap();
    app.notice_interrupted_saves_in(&recovery);
    assert_ne!(app.status, baseline);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_chooser_wraps_and_reports_a_missing_project() {
    let mut app = app_at(Path::new("."));
    app.chooser.open_workspace(vix::app::WorkspaceChooser {
        roots: vec!["/vix-no-such-a".into(), "/vix-no-such-b".into()],
        selected: 0,
    });
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(app.chooser.workspace_mut().unwrap().selected, 1, "wraps up");
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(
        app.chooser.workspace_mut().unwrap().selected,
        0,
        "wraps down"
    );
    app.on_key(keycode(KeyCode::Char('x'))); // ignored
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(
        app.status,
        t!("status.project_missing", path = "/vix-no-such-a")
    );
    assert!(app.chooser.workspace_mut().is_none(), "the chooser closed");
}

#[test]
fn a_failed_settings_load_is_reported_as_a_warning() {
    let mut app = app_at(Path::new("."));
    let before = app.messages.items.len();
    app.warn_settings_load_failed("bad toml");
    assert_eq!(app.messages.items.len(), before + 1);
}

/// T865: arm `fault` for the rest of the scope (this thread's saves only).
fn arm(fault: fileops::fault::SaveFault) -> fileops::fault::Injected {
    fileops::fault::inject(fault)
}

/// The record a failed in-place save of `file` left in the (test-isolated)
/// recovery directory, moved into `into` as a restart would find it.
fn take_record(file: &Path, into: &Path) -> PathBuf {
    let want = fs::canonicalize(file).unwrap();
    let rec = fileops::pending_recoveries(&fileops::recovery_dir().unwrap())
        .into_iter()
        .find(|r| r.original == want)
        .expect("the failed save left a recovery record");
    fs::create_dir_all(into).unwrap();
    let moved = into.join(rec.copy.file_name().unwrap());
    fs::rename(&rec.copy, &moved).unwrap();
    moved
}

/// T865 (PQ-02): a save whose data cannot be written or synced fails with
/// the file untouched and the buffer still dirty. The fallback used to retry
/// it as a truncating in-place write.
#[test]
fn a_save_whose_data_cannot_reach_the_disk_keeps_the_file_and_stays_dirty() {
    use fileops::fault::SaveFault;
    for fault in [SaveFault::TempWrite, SaveFault::TempSync] {
        let dir = unique_dir("save-temp-fault");
        let file = dir.join("notes.txt");
        fs::write(&file, "old\n").unwrap();
        let mut app = app_at(&dir);
        open_dirty(&mut app, &file, "new\n");
        let armed = arm(fault);
        app.run_action("file.save");
        drop(armed);
        assert_eq!(fs::read_to_string(&file).unwrap(), "old\n", "{fault:?}");
        assert!(app.editor.active_tab().unwrap().dirty, "{fault:?}");
        let reported = format!("{fault:?}");
        assert!(
            app.messages
                .items
                .iter()
                .any(|m| m.text.contains(&reported)),
            "{fault:?}: the error is reported"
        );
        app.run_action("file.save");
        assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
        assert!(!app.editor.active_tab().unwrap().dirty);
        fs::remove_dir_all(&dir).ok();
    }
}

/// T865: when the save must go in place (here the rename is refused) and no
/// recovery copy can be kept, nothing is written and the buffer stays dirty.
#[test]
fn an_in_place_save_without_a_recovery_copy_leaves_the_file_alone() {
    use fileops::fault::SaveFault;
    let dir = unique_dir("save-backup-fault");
    let file = dir.join("notes.txt");
    fs::write(&file, "old\n").unwrap();
    let mut app = app_at(&dir);
    open_dirty(&mut app, &file, "new\n");
    let rename = arm(SaveFault::Rename);
    let backup = arm(SaveFault::Backup);
    app.run_action("file.save");
    drop((rename, backup));
    assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
    assert!(app.editor.active_tab().unwrap().dirty);
    // With the copy available the same in-place save succeeds and cleans up.
    let rename = arm(SaveFault::Rename);
    app.run_action("file.save");
    drop(rename);
    assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
    assert!(!app.editor.active_tab().unwrap().dirty);
    let want = fs::canonicalize(&file).unwrap();
    assert!(
        fileops::pending_recoveries(&fileops::recovery_dir().unwrap())
            .iter()
            .all(|r| r.original != want),
        "a finished save removes its copy"
    );
    fs::remove_dir_all(&dir).ok();
}

/// T865: an in-place write that fails part-way leaves the buffer dirty and
/// the original bytes in a recovery record, which a restart restores.
#[test]
fn a_failed_in_place_save_is_restored_after_a_restart() {
    use fileops::fault::SaveFault;
    let dir = unique_dir("save-inplace-fault");
    let file = dir.join("notes.txt");
    fs::write(&file, "old\n").unwrap();
    let mut app = app_at(&dir);
    open_dirty(&mut app, &file, "new\n");
    let rename = arm(SaveFault::Rename);
    let write = arm(SaveFault::InPlaceWrite);
    app.run_action("file.save");
    drop((rename, write));
    assert!(app.editor.active_tab().unwrap().dirty);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "",
        "truncated, as a crash would"
    );
    let recovery = dir.join("recovery");
    take_record(&file, &recovery);
    drop(app);

    let mut app = app_at(&dir);
    app.notice_interrupted_saves_in(&recovery);
    assert!(app.recovery_pending());
    app.on_key(key('r'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
    assert_eq!(fileops::pending_recoveries(&recovery), []);
    fs::remove_dir_all(&dir).ok();
}

/// The writable-file-in-an-unwritable-directory save still works (in place,
/// with its recovery copy).
#[test]
#[cfg(unix)]
fn a_writable_file_in_an_unwritable_directory_still_saves() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = unique_dir("save-rodir");
    let sub = dir.join("ro");
    fs::create_dir_all(&sub).unwrap();
    let file = sub.join("notes.txt");
    fs::write(&file, "old\n").unwrap();
    let mut app = app_at(&dir);
    open_dirty(&mut app, &file, "new\n");
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o555)).unwrap();
    app.run_action("file.save");
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
    assert!(!app.editor.active_tab().unwrap().dirty);
    fs::remove_dir_all(&dir).ok();
}
