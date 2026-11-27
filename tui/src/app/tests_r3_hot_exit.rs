//! Round-3 tests of hot exit: untitled buffers, forced flush on quit,
//! failing stores, the recovery prompt's every answer, and cleanup.

use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::Recovery;
use crate::app::cov4::{app_at, key, temp};
use crate::app::{App, Confirm, ConfirmKind};

fn app_in(dir: &Path) -> App {
    app_at(dir).with_backup_dir(dir.join("backup"))
}

fn backups(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("backup")).map_or(0, Iterator::count)
}

fn dirty_untitled(app: &mut App, text: &str) {
    app.editor.new_tab_with_content(text);
    app.editor.active_tab_mut().unwrap().dirty = true;
}

fn prompt_open(app: &App) -> bool {
    app.confirm
        .as_ref()
        .is_some_and(|c| c.kind == ConfirmKind::RecoverBackup)
}

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

#[test]
fn an_untitled_buffer_is_backed_up_and_offered_back_next_start() {
    let dir = temp("he-untitled");
    let mut app = app_in(&dir);
    dirty_untitled(&mut app, "scratch notes");
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    assert_eq!(backups(&dir), 1);
    assert!(app.editor.active_tab().unwrap().backup.session_id.is_some());
    // Unchanged text is not written again.
    let before = app.editor.active_tab().unwrap().backup.written_at;
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    assert_eq!(app.editor.active_tab().unwrap().backup.written_at, before);
    drop(app);

    // Next start: the offer, then `r` brings the text back in a new tab.
    let mut app = app_in(&dir);
    assert!(!app.recovery_pending());
    app.offer_untitled_recovery();
    assert!(app.recovery_pending());
    assert!(prompt_open(&app));
    assert_eq!(
        app.confirm.as_ref().unwrap().message,
        t!("confirm.hot_exit_untitled", count = 1).to_string()
    );
    app.hot_exit_confirm_key(key('r'));
    assert!(!prompt_open(&app));
    assert_eq!(text(&app), "scratch notes");
    assert_eq!(
        app.status,
        t!("status.hot_exit_recovered_untitled", count = 1).to_string()
    );
    // Offering again does not re-offer the buffer that is already open.
    app.offer_untitled_recovery();
    assert!(!app.recovery_pending());
}

#[test]
fn the_prompt_can_discard_or_postpone_and_ignores_other_keys() {
    let dir = temp("he-answers");
    let mut app = app_in(&dir);
    dirty_untitled(&mut app, "keep me maybe");
    app.flush_hot_exit();
    assert_eq!(backups(&dir), 1, "quit flushes even outside the idle pass");
    drop(app);

    let mut app = app_in(&dir);
    app.offer_untitled_recovery();
    // An unrelated key leaves the prompt up; `d` has no diff for untitled.
    assert!(app.hot_exit_confirm_key(key('q')));
    assert!(prompt_open(&app));
    assert!(app.hot_exit_confirm_key(key('d')));
    assert!(app.recovery_pending());
    // Later: the backup stays and the offer is dropped for now.
    assert!(app.hot_exit_confirm_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(!prompt_open(&app));
    assert_eq!(app.status, t!("status.hot_exit_later").to_string());
    assert_eq!(backups(&dir), 1);

    app.offer_untitled_recovery();
    assert!(app.hot_exit_confirm_key(key('n')));
    app.offer_untitled_recovery();
    assert!(app.hot_exit_confirm_key(key('x')));
    assert_eq!(app.status, t!("status.hot_exit_discarded").to_string());
    assert!(!app.recovery_pending());
    // The store's per-workspace folder may remain, but no backup is left to offer.
    app.offer_untitled_recovery();
    assert!(!app.recovery_pending(), "discarded for good");
}

#[test]
fn keys_do_nothing_without_the_prompt_and_an_empty_queue_closes_it() {
    let dir = temp("he-noprompt");
    let mut app = app_in(&dir);
    assert!(!app.hot_exit_confirm_key(key('r')));
    app.confirm = Some(Confirm {
        message: "x".into(),
        paths: Vec::new(),
        kind: ConfirmKind::RecoverBackup,
    });
    assert!(app.hot_exit_confirm_key(key('r')), "consumed");
    assert!(
        app.confirm.is_none(),
        "a prompt with nothing queued just closes"
    );
}

#[test]
fn another_dialog_defers_the_offer_and_a_closed_tab_drops_a_file_offer() {
    let dir = temp("he-defer");
    let mut app = app_in(&dir);
    app.confirm = Some(Confirm {
        message: "other".into(),
        paths: Vec::new(),
        kind: ConfirmKind::Delete,
    });
    app.hot_exit
        .pending
        .push_back(Recovery::Untitled(vec!["a".into()]));
    app.poll_hot_exit();
    assert_eq!(
        app.confirm.as_ref().unwrap().kind,
        ConfirmKind::Delete,
        "not replaced"
    );
    app.confirm = None;
    // A file offer whose tab is gone is skipped, then the next one shows.
    app.hot_exit.pending.push_front(Recovery::File {
        path: PathBuf::from("/nowhere/closed.txt"),
        identity: "id".into(),
    });
    app.poll_hot_exit();
    assert!(prompt_open(&app));
    assert_eq!(app.hot_exit.pending.len(), 1, "the dead offer was dropped");
}

#[test]
fn a_failing_store_is_reported_once_and_hot_exit_off_never_writes() {
    let dir = temp("he-failing");
    // The backup "directory" is a file, so every write fails.
    std::fs::write(dir.join("blocker"), "x").unwrap();
    let mut app = app_at(&dir).with_backup_dir(dir.join("blocker"));
    dirty_untitled(&mut app, "text");
    app.messages.items.clear();
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    let warnings = app.messages.items.len();
    assert_eq!(warnings, 1, "reported");
    assert!(app.editor.active_tab().unwrap().backup.revision.is_none());
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    assert_eq!(app.messages.items.len(), warnings, "only once per session");

    let dir = temp("he-off");
    let mut app = app_in(&dir);
    app.settings.startup.hot_exit = false;
    dirty_untitled(&mut app, "text");
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    app.flush_hot_exit();
    app.offer_untitled_recovery();
    app.check_backup_on_open(&dir.join("x.txt"));
    assert_eq!(backups(&dir), 0);
    assert!(!app.recovery_pending());
}

#[test]
fn saving_drops_the_backup_under_its_old_and_new_names() {
    let dir = temp("he-drop");
    let file = dir.join("notes.txt");
    std::fs::write(&file, "disk\n").unwrap();
    let mut app = app_in(&dir);
    app.open_initial(&file);
    {
        let tab = app.editor.active_tab_mut().unwrap();
        tab.editor.set_content("changed\n");
        tab.dirty = true;
    }
    app.flush_hot_exit();
    assert_eq!(backups(&dir), 1);
    app.drop_active_backup(None);
    assert_eq!(backups(&dir), 0, "the file's backup is gone");
    assert!(app.editor.active_tab().unwrap().backup.revision.is_none());

    // An untitled buffer's backup (with a prior name) goes too.
    dirty_untitled(&mut app, "untitled");
    app.flush_hot_exit();
    assert!(backups(&dir) >= 1);
    app.drop_active_backup(Some(dir.join("was-named.txt")));
    assert!(app.editor.active_tab().unwrap().backup.session_id.is_none());
    app.offer_untitled_recovery();
    assert!(!app.recovery_pending());
}

#[test]
fn a_backup_equal_to_the_file_is_stale_and_removed_on_open() {
    let dir = temp("he-stale");
    let file = dir.join("same.txt");
    std::fs::write(&file, "same\n").unwrap();
    let mut crashed = app_in(&dir);
    crashed.open_initial(&file);
    {
        let tab = crashed.editor.active_tab_mut().unwrap();
        tab.editor.set_content("different\n");
        tab.dirty = true;
    }
    crashed.flush_hot_exit();
    assert_eq!(backups(&dir), 1);
    drop(crashed);
    // The file was later saved with the same text another way.
    std::fs::write(&file, "different\n").unwrap();
    let mut app = app_in(&dir);
    app.open_initial(&file);
    assert!(!app.recovery_pending());
    assert_eq!(backups(&dir), 0, "the stale backup is deleted");
    // A path with no backup at all is a no-op, as is one not open.
    app.check_backup_on_open(&dir.join("never-opened.txt"));
}

#[test]
fn recovering_a_file_whose_backup_or_tab_vanished_does_nothing() {
    let dir = temp("he-vanish");
    let mut app = app_in(&dir);
    let untouched = app.status.clone();
    app.recover(Recovery::File {
        path: dir.join("no.txt"),
        identity: "missing".into(),
    });
    assert_eq!(app.status, untouched);
    app.recover(Recovery::Untitled(vec!["gone".into()]));
    assert_eq!(
        app.status,
        t!("status.hot_exit_recovered_untitled", count = 0).to_string()
    );
    app.show_backup_diff(&dir.join("no.txt"), "missing");
    assert!(app.diff_view.is_none());
}

/// Text over the 4 MiB line where backups move to the worker (T859), with
/// characters JSON must escape.
fn large_text(tag: &str) -> String {
    let mut text = format!("{tag}\n");
    while text.len() <= super::LARGE_BYTES {
        text.push_str("let s = \"quoted\\path\";\t// \u{1} é 日本\n");
    }
    text
}

/// The bytes of the one backup in `dir`'s store.
fn backup_bytes(dir: &Path) -> Vec<u8> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir.join("backup"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1, "one backup: {entries:?}");
    std::fs::read(&entries[0]).unwrap()
}

#[test]
fn the_worker_writes_a_large_buffer_byte_for_byte_like_the_old_path() {
    let dir = temp("he-worker-bytes");
    let mut app = app_in(&dir);
    let text = large_text("bytes");
    dirty_untitled(&mut app, &text);
    app.hot_exit.make_pass_due();
    app.poll_hot_exit();
    assert!(
        app.hot_exit.worker.is_some(),
        "a large buffer goes to the worker"
    );
    app.settle_backups();
    let streamed = backup_bytes(&dir);

    // The old path: the whole text copied, then serialized and written.
    let id = app
        .editor
        .active_tab()
        .unwrap()
        .backup
        .session_id
        .clone()
        .unwrap();
    let workspace = app.session_key();
    let key = vix_undo_store::hot_exit::BackupKey::Untitled {
        workspace: &workspace,
        id: &id,
    };
    let old = vix_undo_store::hot_exit::BackupStore::at(dir.join("old"));
    old.write(key, &text).unwrap();
    assert_eq!(streamed, std::fs::read(old.path_of(key)).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let path = app.backup_store().unwrap().path_of(key);
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "a backup stays owner-only");
    }
}

#[test]
fn a_newer_snapshot_supersedes_one_in_flight_and_a_removal_waits_for_it() {
    let dir = temp("he-worker-supersede");
    let file = dir.join("big.txt");
    std::fs::write(&file, "disk\n").unwrap();
    let mut app = app_in(&dir);
    app.open_initial(&file);
    let first = large_text("first");
    {
        let tab = app.editor.active_tab_mut().unwrap();
        tab.editor.set_content(&first);
        tab.dirty = true;
    }
    app.flush_hot_exit();
    assert!(app.hot_exit.in_flight.is_empty(), "the quit flush waited");
    // Two snapshots queued back to back: the first is cancelled, and the
    // backup on disk is the second, whichever order the worker got to them.
    let second = large_text("second");
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&first);
    app.backup_dirty_tabs(true);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&second);
    // Whether the first write was still queued (and so got cancelled) or had
    // already finished depends on the worker's timing; what must hold either
    // way is that the backup on disk ends up as the second one.
    app.backup_dirty_tabs(true);
    app.settle_backups();
    let store = app.backup_store().unwrap();
    let identity = app.backup_identity(&file);
    let key = vix_undo_store::hot_exit::BackupKey::File(&identity);
    assert_eq!(store.read(key).unwrap().text, second);

    // A save while a write is in flight: the write cannot bring the
    // backup back after the save deleted it.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&first);
    app.backup_dirty_tabs(true);
    // Usually still in flight here; if the worker already finished, the
    // removal below must still win.
    app.drop_active_backup(None);
    app.settle_backups();
    assert_eq!(backups(&dir), 0, "the removal ran after the write");
}

#[test]
fn quit_waits_for_the_worker_to_write_a_large_buffer() {
    let dir = temp("he-worker-quit");
    let mut app = app_in(&dir);
    let text = large_text("quit");
    dirty_untitled(&mut app, &text);
    app.flush_hot_exit();
    assert!(app.hot_exit.worker.is_some());
    assert!(app.hot_exit.in_flight.is_empty());
    drop(app);
    let mut app = app_in(&dir);
    app.offer_untitled_recovery();
    app.hot_exit_confirm_key(key('r'));
    assert_eq!(self::text(&app), text);
}
