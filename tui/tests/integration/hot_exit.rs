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

/// T865: the framed (version 2) recovery record for `path` (as `kind` and
/// its encoded bytes) and `data`, written by hand from the documented layout.
fn framed_record(kind: &str, path: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = format!("VIXRECOVER 2\npath {kind} {}\n", path.len()).into_bytes();
    out.extend_from_slice(path);
    out.extend_from_slice(format!("\ndata {}\n", data.len()).as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nend\n");
    out
}

/// Save `file` (open, dirty with `text`) so that it goes in place and fails
/// part-way, then move the record it left into `recovery`, as a restart
/// would find it.
fn interrupt_save(file: &Path, text: &str, recovery: &Path) {
    use fileops::fault::{SaveFault, inject};
    let dir = file.parent().unwrap();
    let mut app = app_in(dir);
    edit(&mut app, file, text);
    let faults = (inject(SaveFault::Rename), inject(SaveFault::InPlaceWrite));
    app.run_action("file.save");
    drop(faults);
    assert!(app.editor.active_tab().unwrap().dirty, "the save failed");
    let want = fs::canonicalize(file).unwrap();
    let rec = fileops::pending_recoveries(&fileops::recovery_dir().unwrap())
        .into_iter()
        .find(|r| r.original == want)
        .expect("the failed save left a recovery record");
    fs::create_dir_all(recovery).unwrap();
    fs::rename(&rec.copy, recovery.join(rec.copy.file_name().unwrap())).unwrap();
}

fn interrupted_prompt(app: &App) -> String {
    assert!(prompt_open(app), "a recovery prompt is open");
    app.confirm.as_ref().unwrap().message.clone()
}

/// T865 (PQ-03): an ordinary legacy record is offered and restored.
#[test]
fn a_legacy_interrupted_save_is_restored_on_restart() {
    let (dir, file) = project("interrupted-legacy", "damaged");
    let file = fs::canonicalize(&file).unwrap();
    let recovery = dir.join("recovery");
    fs::create_dir_all(&recovery).unwrap();
    fs::write(
        recovery.join("7-0-notes.txt.vixrecover"),
        format!("VIXRECOVER\n{}\nold\n", file.display()),
    )
    .unwrap();
    let mut app = app_in(&dir);
    app.notice_interrupted_saves_in(&recovery);
    assert!(interrupted_prompt(&app).contains("notes.txt"));
    app.on_key(key('d'));
    assert!(app.diff_view.is_some(), "d shows the diff");
    app.diff_view = None;
    app.notice_interrupted_saves_in(&recovery); // offered once, still pending
    app.on_key(key('r'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
    assert_eq!(fileops::pending_recoveries(&recovery), []);
    assert!(!app.recovery_pending());
    fs::remove_dir_all(&dir).ok();
}

/// A newline in a Unix file name: the record a real failed save leaves
/// brings back the same file and the same bytes.
#[test]
#[cfg(unix)]
fn an_interrupted_save_of_a_file_with_a_newline_in_its_name_is_exact() {
    let dir = unique_dir("interrupted-newline");
    let _ = fs::remove_dir_all(dir.join("backup"));
    let file = dir.join("two\nlines.txt");
    fs::write(&file, "before\n").unwrap();
    let recovery = dir.join("recovery");
    interrupt_save(&file, "after\n", &recovery);
    // A file named like the legacy reading would have guessed, untouched.
    fs::write(dir.join("two"), "unrelated").unwrap();

    let mut app = app_in(&dir);
    app.notice_interrupted_saves_in(&recovery);
    let message = interrupted_prompt(&app);
    assert!(
        message.contains("two\\nlines.txt"),
        "escaped on one line: {message}"
    );
    app.on_key(key('r'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "before\n");
    assert_eq!(fs::read_to_string(dir.join("two")).unwrap(), "unrelated");
    fs::remove_dir_all(&dir).ok();
}

/// Empty contents are a real pre-save copy, restored as an empty file.
#[test]
fn an_interrupted_save_of_an_empty_file_restores_it_empty() {
    let (dir, file) = project("interrupted-empty", "");
    let recovery = dir.join("recovery");
    interrupt_save(&file, "typed\n", &recovery);
    fs::write(&file, "partial").unwrap();
    let mut app = app_in(&dir);
    app.notice_interrupted_saves_in(&recovery);
    interrupted_prompt(&app);
    app.on_key(key('r'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "");
    fs::remove_dir_all(&dir).ok();
}

/// Non-UTF-8 Unix path bytes are kept exactly. Not every file system takes
/// such a name (macOS refuses it), so where it cannot exist the copy is
/// offered and then discarded.
#[test]
#[cfg(unix)]
fn an_interrupted_save_with_non_utf8_path_bytes_names_that_exact_path() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = unique_dir("interrupted-bytes");
    let _ = fs::remove_dir_all(dir.join("backup"));
    let mut raw = dir.as_os_str().as_bytes().to_vec();
    raw.extend_from_slice(b"/caf\xe9.txt");
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(&raw));
    let recovery = dir.join("recovery");
    fs::create_dir_all(&recovery).unwrap();
    fs::write(
        recovery.join("7-0-caf_.txt.vixrecover"),
        framed_record("unix", &raw, b"old"),
    )
    .unwrap();
    let exists = fs::write(&path, "damaged").is_ok();
    let mut app = app_in(&dir);
    app.notice_interrupted_saves_in(&recovery);
    interrupted_prompt(&app);
    if exists {
        app.on_key(key('r'));
        assert_eq!(fs::read(&path).unwrap(), b"old");
    } else {
        app.on_key(key('x'));
    }
    assert_eq!(fileops::pending_recoveries(&recovery), []);
    fs::remove_dir_all(&dir).ok();
}

/// A record naming a non-ASCII Windows path: exact on Windows; elsewhere its
/// file cannot be established, so Restore opens the copy in a new unsaved
/// buffer for the person to save where they choose, and writes nothing.
#[test]
fn an_interrupted_save_of_a_non_ascii_windows_path() {
    let dir = unique_dir("interrupted-windows");
    let _ = fs::remove_dir_all(dir.join("backup"));
    let recovery = dir.join("recovery");
    fs::create_dir_all(&recovery).unwrap();
    let windows_path = if cfg!(windows) {
        dir.join("Zoë-日記.txt").display().to_string()
    } else {
        r"C:\Users\Zoë\日記.txt".to_string()
    };
    let units: Vec<u8> = windows_path
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    fs::write(
        recovery.join("7-0-x.vixrecover"),
        framed_record("windows", &units, b"old text\n"),
    )
    .unwrap();
    let mut app = app_in(&dir);
    let tabs = app.editor.tabs.len();
    app.notice_interrupted_saves_in(&recovery);
    let message = interrupted_prompt(&app);
    assert!(message.contains("Zoë"), "{message}");
    app.on_key(key('r'));
    if cfg!(windows) {
        assert_eq!(fs::read_to_string(&windows_path).unwrap(), "old text\n");
    } else {
        assert_eq!(app.editor.tabs.len(), tabs + 1, "a new buffer");
        assert_eq!(active_text(&app), "old text\n");
        assert!(
            app.editor.active_tab().unwrap().dirty,
            "unsaved: the person chooses"
        );
        assert!(app.editor.active_tab().unwrap().path.is_none());
    }
    assert_eq!(fileops::pending_recoveries(&recovery), []);
    fs::remove_dir_all(&dir).ok();
}

/// A cut-off framed record (and an unpublished `-partial` one) is never
/// offered.
#[test]
fn a_truncated_interrupted_save_record_is_not_offered() {
    let (dir, file) = project("interrupted-truncated", "now\n");
    let recovery = dir.join("recovery");
    fs::create_dir_all(&recovery).unwrap();
    let path = fs::canonicalize(&file).unwrap().display().to_string();
    let whole = framed_record("unix", path.as_bytes(), b"old\n");
    fs::write(
        recovery.join("7-0-notes.txt.vixrecover"),
        &whole[..whole.len() - 2],
    )
    .unwrap();
    fs::write(recovery.join("7-1-notes.txt.vixrecover-partial"), &whole).unwrap();
    let mut app = app_in(&dir);
    let status = app.status.clone();
    app.notice_interrupted_saves_in(&recovery);
    assert!(!prompt_open(&app));
    assert!(!app.recovery_pending());
    assert_eq!(app.status, status);
    assert_eq!(fs::read_to_string(&file).unwrap(), "now\n");
    fs::remove_dir_all(&dir).ok();
}
