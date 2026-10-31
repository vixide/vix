//! Hot exit (T629): unsaved buffers are backed up while idle and offered
//! back, with a diff, when their file is next opened.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

/// A fresh project with a file holding `disk`, and an app whose backups go
/// to a private directory (never the developer's config).
fn project(tag: &str, disk: &str) -> (PathBuf, PathBuf) {
    let dir = unique_dir(tag);
    let _ = fs::remove_dir_all(dir.join("backup"));
    let file = dir.join("notes.txt");
    fs::write(&file, disk).unwrap();
    (dir, file)
}

fn app_in(dir: &Path) -> App {
    app_at(dir).with_backup_dir(dir.join("backup"))
}

/// Open `file` and type over it, leaving the buffer dirty.
fn edit(app: &mut App, file: &Path, text: &str) {
    app.open_initial(file);
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.dirty = true;
}

fn backups(dir: &Path) -> usize {
    fs::read_dir(dir.join("backup")).map_or(0, Iterator::count)
}

fn active_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn prompt_open(app: &App) -> bool {
    app.confirm
        .as_ref()
        .is_some_and(|c| c.kind == vix::app::ConfirmKind::RecoverBackup)
}

#[test]
fn a_crashed_buffer_round_trips_through_its_backup() {
    let (dir, file) = project("hot-exit-round", "one\n");
    let mut crashed = app_in(&dir);
    edit(&mut crashed, &file, "two\n");
    crashed.poll_hot_exit();
    assert_eq!(backups(&dir), 1, "the idle pass wrote one backup");
    drop(crashed); // a crash: no save, no quit prompt

    let mut app = app_in(&dir);
    app.open_initial(&file);
    assert!(prompt_open(&app), "reopening offers the backup");
    assert_eq!(active_text(&app), "one\n", "never applied on its own");
    app.on_key(key('r'));
    assert!(!prompt_open(&app));
    assert_eq!(active_text(&app), "two\n");
    assert!(
        app.editor.active_tab().unwrap().dirty,
        "recovered, not saved"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");
    assert!(!app.recovery_pending());
}

#[test]
fn a_stale_backup_is_shown_as_a_diff_then_discarded() {
    let (dir, file) = project("hot-exit-diff", "alpha\nbeta\n");
    let mut crashed = app_in(&dir);
    edit(&mut crashed, &file, "alpha\ngamma\n");
    crashed.poll_hot_exit();
    drop(crashed);

    let mut app = app_in(&dir);
    app.open_initial(&file);
    app.on_key(key('d'));
    let view = app.diff_view.as_ref().expect("d opens the diff");
    assert_ne!(view.lines.len(), 0, "the diff has lines");
    assert!(!prompt_open(&app), "the diff is on top");
    app.on_key(esc());
    assert!(app.diff_view.is_none());
    app.poll_hot_exit();
    assert!(prompt_open(&app), "the offer comes back after the diff");
    app.on_key(key('x'));
    assert!(!prompt_open(&app));
    assert_eq!(backups(&dir), 0, "x deletes the backup");
    assert_eq!(active_text(&app), "alpha\nbeta\n");
}

#[test]
fn a_clean_save_deletes_the_backup() {
    let (dir, file) = project("hot-exit-save", "one\n");
    let mut app = app_in(&dir);
    edit(&mut app, &file, "two\n");
    app.poll_hot_exit();
    assert_eq!(backups(&dir), 1);
    app.run_action("file.save");
    assert_eq!(fs::read_to_string(&file).unwrap(), "two\n");
    assert_eq!(backups(&dir), 0, "saved text needs no backup");
}

#[test]
fn an_untitled_buffer_is_recovered_with_its_session_id() {
    let dir = unique_dir("hot-exit-untitled");
    let _ = fs::remove_dir_all(dir.join("backup"));
    let mut crashed = app_in(&dir);
    crashed.open_stdin_buffer("draft text\n");
    crashed.poll_hot_exit();
    let id = crashed
        .editor
        .active_tab()
        .unwrap()
        .backup
        .session_id
        .clone()
        .expect("the first backup gives an untitled buffer a session id");
    assert_eq!(backups(&dir), 1);
    drop(crashed);

    // Another workspace never sees it.
    let other = unique_dir("hot-exit-untitled-other");
    let mut elsewhere = app_at(&other).with_backup_dir(dir.join("backup"));
    elsewhere.offer_untitled_recovery();
    assert!(!elsewhere.recovery_pending());

    let mut app = app_in(&dir);
    app.offer_untitled_recovery();
    assert!(prompt_open(&app));
    let before = app.editor.tabs.len();
    app.on_key(key('r'));
    assert_eq!(app.editor.tabs.len(), before + 1);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "draft text\n");
    assert!(tab.path.is_none() && tab.dirty);
    assert_eq!(tab.backup.session_id.as_deref(), Some(id.as_str()));
}

#[test]
fn only_changed_buffers_are_rewritten() {
    let (dir, file) = project("hot-exit-changed", "one\n");
    let mut app = app_in(&dir).with_settings_path(dir.join("config.toml"));
    edit(&mut app, &file, "two\n");
    app.poll_hot_exit();
    let backup = fs::read_dir(dir.join("backup"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&backup).unwrap();
    // Same revision: the flush on quit has nothing new to write.
    app.on_exit();
    assert!(!backup.exists(), "an unchanged buffer is not written again");
}

#[test]
fn a_backup_matching_the_file_is_dropped_silently() {
    let (dir, file) = project("hot-exit-same", "one\n");
    let mut crashed = app_in(&dir);
    edit(&mut crashed, &file, "two\n");
    crashed.poll_hot_exit();
    drop(crashed);
    // The edit reached the file some other way.
    fs::write(&file, "two\n").unwrap();
    let mut app = app_in(&dir);
    app.open_initial(&file);
    assert!(!prompt_open(&app));
    assert_eq!(backups(&dir), 0);
}

#[test]
fn discarding_in_the_close_prompt_deletes_the_backup() {
    let (dir, file) = project("hot-exit-discard", "one\n");
    let mut app = app_in(&dir);
    edit(&mut app, &file, "two\n");
    app.poll_hot_exit();
    assert_eq!(backups(&dir), 1);
    app.run_action("file.close");
    app.on_key(key('d'));
    assert_eq!(backups(&dir), 0);
}

#[test]
fn hot_exit_off_writes_and_offers_nothing() {
    let (dir, file) = project("hot-exit-off", "one\n");
    let mut settings = Settings::default();
    settings.startup.hot_exit = false;
    let mut app = app_at_with(&dir, settings).with_backup_dir(dir.join("backup"));
    edit(&mut app, &file, "two\n");
    app.poll_hot_exit();
    assert_eq!(backups(&dir), 0);
}

#[test]
fn without_an_explicit_directory_backups_follow_the_session_file() {
    // `app_at` points the session at a private temp file; the backups must
    // land beside it, never in the developer's config directory.
    let (dir, file) = project("hot-exit-derived", "one\n");
    let mut app = app_at(&dir);
    edit(&mut app, &file, "two\n");
    app.poll_hot_exit();
    assert_eq!(backups(&dir), 0, "not the explicit directory");
    let store = app.session_path.clone().unwrap().with_extension("backup");
    assert_eq!(fs::read_dir(&store).map_or(0, Iterator::count), 1);
    let _ = fs::remove_dir_all(&store);
    let mut again = app_at(&dir);
    again.open_initial(&file);
    assert!(
        !prompt_open(&again),
        "a different session file, a different store"
    );
}
