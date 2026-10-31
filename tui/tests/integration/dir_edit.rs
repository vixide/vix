//! Explorer → Edit Directory (T636): the listing buffer diffs back into
//! renames (including a swap), creates and deletes on save, applied only
//! after the confirm step, with open buffers following renamed files.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn edit(app: &mut App, content: &str) {
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(content);
    tab.dirty = true;
}

/// A temp directory holding `docs/`, `a.txt`, `b.txt` and `old.txt`.
fn fixture(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("docs/readme.md"), "r\n").unwrap();
    fs::write(dir.join("a.txt"), "A").unwrap();
    fs::write(dir.join("b.txt"), "B").unwrap();
    fs::write(dir.join("old.txt"), "O").unwrap();
    dir
}

#[test]
fn saving_the_listing_swaps_renames_creates_and_deletes_after_confirm() {
    let dir = fixture("dir-edit-apply");
    let mut app = app_at(&dir);
    app.settings.explorer_delete = "hard".to_string(); // no OS trash in tests
    app.open_initial(&dir.join("a.txt"));
    app.run_action("explorer.edit_directory");
    assert_eq!(
        text(&app),
        "/001 docs/\n/002 a.txt\n/003 b.txt\n/004 old.txt\n"
    );

    // Swap a/b, rename docs, drop old.txt, add a file and a directory.
    edit(
        &mut app,
        "/001 manual/\n/002 b.txt\n/003 a.txt\nnew.txt\nsub/\n",
    );
    app.run_action("file.save");
    // Nothing happens before the confirm step is answered.
    assert!(dir.join("old.txt").exists());
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "A");
    app.on_key(key('y'));

    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "B");
    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "A");
    assert!(dir.join("manual/readme.md").is_file());
    assert!(!dir.join("docs").exists());
    assert!(!dir.join("old.txt").exists());
    assert_eq!(fs::read_to_string(dir.join("new.txt")).unwrap(), "");
    assert!(dir.join("sub").is_dir());
    let leftovers: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "temporary names left: {leftovers:?}");

    // The listing is re-read, so saving again changes nothing.
    assert_eq!(
        text(&app),
        "/001 manual/\n/002 sub/\n/003 a.txt\n/004 b.txt\n/005 new.txt\n"
    );
    assert!(!app.editor.active_tab().unwrap().dirty);

    // The buffer that was open on a.txt followed its file to b.txt.
    let followed = app
        .editor
        .tabs
        .iter()
        .find(|t| t.text() == "A")
        .and_then(|t| t.path.clone())
        .unwrap();
    assert!(followed.ends_with("b.txt"), "{followed:?}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn cancelling_or_an_invalid_listing_changes_nothing() {
    let dir = fixture("dir-edit-cancel");
    let mut app = app_at(&dir);
    app.settings.explorer_delete = "hard".to_string();
    app.run_action("explorer.edit_directory");

    // `n` cancels the confirm step.
    edit(&mut app, "/001 docs/\n/002 renamed.txt\n");
    app.run_action("file.save");
    app.on_key(key('n'));
    assert!(dir.join("a.txt").exists());
    assert!(dir.join("old.txt").exists());
    assert!(!dir.join("renamed.txt").exists());

    // Two lines with one name never reach a confirm step: `y` is just typed.
    edit(&mut app, "/002 a.txt\n/003 a.txt\n");
    app.run_action("file.save");
    app.on_key(key('y'));
    assert!(dir.join("b.txt").exists());
    assert!(dir.join("docs").is_dir());
    assert!(text(&app).contains('y'));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_explorer_selection_picks_the_directory() {
    let dir = fixture("dir-edit-target");
    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "docs");
    app.run_action("explorer.edit_directory");
    assert_eq!(text(&app), "/001 readme.md\n");
    assert_eq!(app.focus, Focus::Editor);
    fs::remove_dir_all(&dir).ok();
}
