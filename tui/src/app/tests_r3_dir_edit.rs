//! Round-3 tests of the directory-editing glue: target choice, listing
//! errors, the confirm step's keys and previews, and refused conflicts.

use std::path::Path;

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::cov4::{app_with, buffer_with, content, last_message, temp};
use crate::app::{App, Focus};
use crate::settings::Settings;

fn fixture(tag: &str) -> std::path::PathBuf {
    let dir = temp(tag);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/inner.txt"), "i").unwrap();
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    std::fs::write(dir.join("b.txt"), "B").unwrap();
    dir
}

fn hard_app(dir: &Path) -> App {
    let mut settings = Settings::default();
    settings.explorer_delete = "hard".into();
    app_with(dir, settings)
}

fn press(app: &mut App, code: KeyCode) {
    app.dir_edit_confirm_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn the_listing_directory_follows_focus_and_the_open_listing() {
    let dir = fixture("de-target");
    let mut app = hard_app(&dir);
    assert!(!app.active_tab_is_dir_edit());
    // Nothing open, no explorer focus: the explorer root.
    app.open_dir_edit();
    assert_eq!(content(&mut app), "/001 sub/\n/002 a.txt\n/003 b.txt\n");
    assert!(app.active_tab_is_dir_edit());
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(
        app.status,
        t!("status.dir_edit_opened", path = ".", count = 3).to_string()
    );

    // Opening again refreshes the same tab rather than adding another.
    let tabs = app.editor.tabs.len();
    app.open_dir_edit();
    assert_eq!(app.editor.tabs.len(), tabs);

    // A highlighted file in the explorer lists its parent directory; a
    // highlighted directory lists itself.
    let mut app = hard_app(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = app
        .explorer
        .nodes
        .iter()
        .position(|n| n.name == "sub")
        .unwrap();
    app.open_dir_edit();
    assert_eq!(content(&mut app), "/001 inner.txt\n");
    // The open listing is its own target (a refresh), whatever the focus.
    app.focus = Focus::Explorer;
    app.explorer.selected = app
        .explorer
        .nodes
        .iter()
        .position(|n| n.name == "a.txt")
        .unwrap();
    app.open_dir_edit();
    assert_eq!(content(&mut app), "/001 inner.txt\n");

    // With a file open, its directory is listed.
    let mut app = hard_app(&dir);
    app.open_initial(&dir.join("sub/inner.txt"));
    app.open_dir_edit();
    assert_eq!(content(&mut app), "/001 inner.txt\n");
}

#[test]
fn an_unreadable_directory_is_reported_and_opens_nothing() {
    let dir = fixture("de-missing");
    let mut app = hard_app(&dir);
    app.explorer.root = dir.join("gone");
    app.messages.items.clear();
    app.open_dir_edit();
    assert!(app.dir_edit.is_none());
    assert!(!app.active_tab_is_dir_edit());
    assert!(last_message(&app).is_some());
}

fn open_listing(dir: &Path) -> App {
    let mut app = hard_app(dir);
    app.open_dir_edit();
    app
}

#[test]
fn bad_listings_are_explained_on_the_status_line() {
    let dir = fixture("de-errors");
    let mut app = open_listing(&dir);
    let cases = [
        (
            "/002 a.txt\n/002 b.txt\n",
            t!("status.dir_edit_duplicate_id", id = 2, line = 2).to_string(),
        ),
        (
            "/009 a.txt\n",
            t!("status.dir_edit_unknown_id", id = 9, line = 1).to_string(),
        ),
        (
            "/002 a/b.txt\n",
            t!("status.dir_edit_invalid_name", name = "a/b.txt", line = 1).to_string(),
        ),
        (
            "/002 x.txt\n/003 x.txt\n",
            t!("status.dir_edit_duplicate_name", name = "x.txt").to_string(),
        ),
    ];
    for (text, expected) in cases {
        buffer_with(&mut app, text, 0);
        app.prepare_dir_edit_confirm();
        assert_eq!(app.status, expected, "{text:?}");
        assert!(app.dir_edit_confirm.is_none());
    }
    // An unchanged listing says so.
    let original = "/001 sub/\n/002 a.txt\n/003 b.txt\n";
    buffer_with(&mut app, original, 0);
    app.prepare_dir_edit_confirm();
    assert_eq!(app.status, t!("status.dir_edit_no_changes").to_string());
}

#[test]
fn preview_words_trash_or_delete_by_setting_and_scrolls() {
    let dir = fixture("de-preview");
    for (mode, key) in [
        ("trash", "ui.dir_edit_trash"),
        ("hard", "ui.dir_edit_delete"),
    ] {
        let mut settings = Settings::default();
        settings.explorer_delete = mode.into();
        let mut app = app_with(&dir, settings);
        app.open_dir_edit();
        // Rename sub, create new.txt and fresh/, delete b.txt.
        buffer_with(&mut app, "/001 renamed/\n/002 a.txt\nnew.txt\nfresh/\n", 0);
        app.prepare_dir_edit_confirm();
        let confirm = app.dir_edit_confirm.as_ref().expect("confirm opens");
        assert_eq!(confirm.count, 4);
        assert_eq!(confirm.dir_label, ".");
        assert_eq!(
            confirm.lines,
            vec![
                t!("ui.dir_edit_rename", from = "sub", to = "renamed").to_string(),
                t!("ui.dir_edit_create", name = "new.txt").to_string(),
                t!("ui.dir_edit_create", name = "fresh/").to_string(),
                t!(key, name = "b.txt").to_string(),
            ]
        );
        // Scrolling clamps at both ends; unrelated keys are ignored.
        press(&mut app, KeyCode::Up);
        assert_eq!(app.dir_edit_confirm.as_ref().unwrap().scroll, 0);
        for _ in 0..10 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.dir_edit_confirm.as_ref().unwrap().scroll, 3);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.dir_edit_confirm.as_ref().unwrap().scroll, 2);
        press(&mut app, KeyCode::Tab);
        assert!(app.dir_edit_confirm.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.dir_edit_confirm.is_none());
        assert_eq!(app.status, t!("status.dir_edit_cancelled").to_string());
        assert!(dir.join("b.txt").exists(), "nothing was applied");
        // Keys with no confirm open do nothing.
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Char('y'));
    }
}

#[test]
fn a_target_that_appeared_meanwhile_is_never_overwritten() {
    let dir = fixture("de-conflict");
    let mut app = open_listing(&dir);
    buffer_with(&mut app, "/001 sub/\n/002 c.txt\n/003 b.txt\n", 0);
    app.prepare_dir_edit_confirm();
    // Someone creates c.txt after the plan was made.
    std::fs::write(dir.join("c.txt"), "theirs").unwrap();
    app.messages.items.clear();
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.status, t!("status.dir_edit_stopped").to_string());
    assert_eq!(
        std::fs::read_to_string(dir.join("c.txt")).unwrap(),
        "theirs"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("a.txt")).unwrap(),
        "A",
        "the source stays"
    );
    assert!(
        last_message(&app).is_some(),
        "the failure is in the message log"
    );

    // The same for a create whose name appeared.
    let mut app = open_listing(&dir);
    buffer_with(
        &mut app,
        "/001 sub/\n/002 a.txt\n/003 b.txt\n/004 c.txt\nlate.txt\n",
        0,
    );
    app.prepare_dir_edit_confirm();
    std::fs::write(dir.join("late.txt"), "mine").unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.status, t!("status.dir_edit_stopped").to_string());
    assert_eq!(
        std::fs::read_to_string(dir.join("late.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn a_clean_apply_renames_deletes_creates_and_relists() {
    let dir = fixture("de-apply");
    let mut app = open_listing(&dir);
    app.open_initial(&dir.join("b.txt"));
    app.open_dir_edit();
    buffer_with(
        &mut app,
        "/001 sub/\n/003 renamed.txt\n/004 session.toml\nmade/\n",
        0,
    );
    app.prepare_dir_edit_confirm();
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.status,
        t!("status.dir_edit_applied", count = 3).to_string()
    );
    assert!(!dir.join("a.txt").exists());
    assert!(dir.join("renamed.txt").is_file());
    assert!(dir.join("made").is_dir());
    assert_eq!(
        content(&mut app),
        "/001 made/\n/002 sub/\n/003 renamed.txt\n/004 session.toml\n"
    );
    // The open buffer on b.txt followed the rename.
    assert!(app.editor.tabs.iter().any(|t| {
        t.path
            .as_deref()
            .is_some_and(|p| p.ends_with("renamed.txt"))
    }));
}
