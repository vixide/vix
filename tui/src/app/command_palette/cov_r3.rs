//! Behavior tests for the palette glue: key handling beyond typing, query
//! history, `palette.resume`, buffer switching, the symbol scan and the
//! preview pane. Every app keeps its settings inside its own temp directory.

use super::*;
use crate::app::cov4::{app_at as base_app_at, code, key, temp};
use vix_input::KeyCode;

fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

/// Type `text` into the open palette.
fn type_into_palette(app: &mut App, text: &str) {
    for c in text.chars() {
        app.palette_key(key(c));
    }
}

fn input(app: &App) -> String {
    app.palette.as_ref().map_or_default(|p| p.input.clone())
}

#[test]
fn tab_accepts_the_first_entry_and_backspace_edits_the_query() {
    let d = temp("pal-tab");
    std::fs::write(d.join("alpha.txt"), "a\n").unwrap();
    std::fs::write(d.join("beta.txt"), "b\n").unwrap();
    let mut app = app_at(&d);
    app.open_palette();
    type_into_palette(&mut app, "betax");
    app.palette_key(code(KeyCode::Backspace));
    assert_eq!(input(&app), "beta");
    // Tab opens the best match even though the highlight had moved.
    app.palette_key(code(KeyCode::Tab));
    assert!(app.palette.is_none());
    assert_eq!(
        app.editor.active_tab().unwrap().path.as_deref(),
        Some(d.join("beta.txt").as_path())
    );
    // Accepting with nothing to accept does nothing at all.
    app.open_palette();
    type_into_palette(&mut app, "no-such-file-anywhere");
    app.palette_key(code(KeyCode::Enter));
    assert!(app.palette.is_some());
    // Other keys are ignored.
    app.palette_key(code(KeyCode::F(5)));
    // And with no palette open the handlers are inert.
    app.palette = None;
    app.palette_key(code(KeyCode::Enter));
    app.palette_key(code(KeyCode::Tab));
    app.palette_key(key('x'));
    app.recompute_palette();
    assert!(app.palette.is_none());
}

#[test]
fn queries_are_recalled_with_up_and_down_and_escape_remembers_the_palette() {
    let d = temp("pal-history");
    std::fs::write(d.join("alpha.txt"), "a\n").unwrap();
    let mut app = app_at(&d);
    app.open_palette();
    type_into_palette(&mut app, "alpha");
    app.palette_key(code(KeyCode::Enter));
    assert!(app.palette.is_none());
    // On an empty input, Up recalls the last query; Down steps back out.
    app.open_palette();
    app.palette_key(code(KeyCode::Up));
    assert_eq!(input(&app), "alpha");
    app.palette_key(code(KeyCode::Down));
    assert_eq!(input(&app), "");
    // Typing something else ends the recall: arrows move the selection.
    type_into_palette(&mut app, "al");
    app.palette_key(code(KeyCode::Up));
    assert_eq!(input(&app), "al");
    // Esc remembers the picker so palette.resume brings it back.
    app.palette_key(code(KeyCode::Esc));
    assert!(app.palette.is_none());
    assert_eq!(app.picker_memory.last, Some(PickerKind::Palette));
    app.resume_picker();
    assert_eq!(input(&app), "al");
}

#[test]
fn resuming_with_nothing_remembered_says_so_and_search_everywhere_is_restored() {
    let d = temp("pal-resume");
    let mut app = app_at(&d);
    app.resume_picker();
    assert_eq!(app.status, t!("status.picker_resume_none").to_string());
    // A remembered kind whose state is missing is ignored.
    app.picker_memory.last = Some(PickerKind::Palette);
    app.resume_picker();
    assert!(app.palette.is_none());
    app.picker_memory.last = Some(PickerKind::WorkspaceSearch);
    app.resume_picker();
    assert!(app.workspace_search.is_none());

    app.open_palette();
    app.palette_everywhere = true;
    app.recompute_palette();
    app.palette_key(code(KeyCode::Esc));
    app.palette_everywhere = false;
    app.resume_picker();
    assert!(app.palette.is_some());
    assert!(app.palette_everywhere);
}

#[test]
fn a_remembered_workspace_search_comes_back_with_its_query() {
    let d = temp("pal-ws");
    std::fs::write(d.join("hay.txt"), "needle in a haystack\n").unwrap();
    let mut app = app_at(&d);
    app.remember_workspace_search(); // no panel: nothing is stored
    assert!(app.picker_memory.workspace.is_none());
    app.run_action("search.workspace");
    for c in "needle".chars() {
        app.on_key(key(c));
    }
    app.remember_workspace_search();
    assert_eq!(app.picker_memory.last, Some(PickerKind::WorkspaceSearch));
    app.workspace_search = None;
    app.resume_picker();
    let ps = app.workspace_search.as_ref().unwrap();
    assert_eq!(ps.query, "needle");
    // A static result list is never remembered.
    app.workspace_search
        .as_mut()
        .unwrap()
        .flags
        .insert(WorkspaceFlags::STATIC_RESULTS);
    app.picker_memory.workspace = None;
    app.remember_workspace_search();
    assert!(app.picker_memory.workspace.is_none());
}

#[test]
fn the_buffer_switcher_activates_the_chosen_tab() {
    let d = temp("pal-buffers");
    std::fs::write(d.join("one.txt"), "1\n").unwrap();
    std::fs::write(d.join("two.txt"), "2\n").unwrap();
    let mut app = app_at(&d);
    app.editor.open(&d.join("one.txt"), false).unwrap();
    app.editor.open(&d.join("two.txt"), false).unwrap();
    let two = app.editor.active;
    app.open_palette_seeded("#");
    let row = app
        .palette
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|e| e.label.ends_with("one.txt"))
        .unwrap();
    app.palette.as_mut().unwrap().selected = row;
    app.palette_key(code(KeyCode::Enter));
    assert_ne!(app.editor.active, two);
    assert_eq!(
        app.editor.active_tab().unwrap().path.as_deref(),
        Some(d.join("one.txt").as_path())
    );
    // An empty query lists every buffer.
    app.open_palette_seeded("#");
    assert!(app.palette.as_ref().unwrap().entries.len() >= 2);
}

#[test]
fn goto_line_previews_and_a_command_accepts_as_a_recent() {
    let d = temp("pal-commands");
    let mut app = app_at(&d);
    app.editor.new_tab_with_content("a\nb\nc\nd\n");
    app.open_palette();
    type_into_palette(&mut app, ":3");
    assert_eq!(app.editor.cursor_line(), 2);
    app.palette_key(code(KeyCode::Backspace));
    app.palette_key(key('4'));
    assert_eq!(app.editor.cursor_line(), 3);
    app.palette_key(code(KeyCode::Enter));
    assert_eq!(app.editor.cursor_line(), 3);

    // Running a command from the palette records it first in the recents list.
    app.open_palette_seeded(&format!(">{}", t!("cmd.toggle_line_numbers")));
    let action = "view.line_numbers".to_string();
    let row = app
        .palette
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|e| matches!(&e.action, PAction::RunCommand(a) if *a == action))
        .unwrap();
    app.palette.as_mut().unwrap().selected = row;
    app.palette_key(code(KeyCode::Enter));
    assert_eq!(app.command_recents.first(), Some(&action));
    // With recents present, an empty `>` query lists the recent command first.
    app.open_palette_seeded(">");
    let first = app
        .palette
        .as_ref()
        .unwrap()
        .entries
        .first()
        .unwrap()
        .clone();
    assert!(matches!(first.action, PAction::RunCommand(ref a) if *a == action));
}

#[test]
fn symbols_and_workspace_symbols_list_declarations_and_open_them() {
    let d = temp("pal-symbols");
    std::fs::write(d.join("lib.rs"), "fn alpha_one() {}\nfn beta_two() {}\n").unwrap();
    std::fs::write(d.join("pic.png"), "not text").unwrap();
    let mut app = app_at(&d);
    app.open_initial(&d.join("lib.rs"));
    app.open_palette_seeded("@beta");
    let p = app.palette.as_ref().unwrap();
    assert_eq!(p.entries.len(), 1);
    assert!(matches!(p.entries[0].action, PAction::GotoLine(2)));
    assert!(app.palette_has_preview());
    // `@@` with an empty query lists nothing; with a name it finds the file.
    app.open_palette_seeded("@@");
    assert!(app.palette.as_ref().unwrap().entries.is_empty());
    app.open_palette_seeded("@@alpha");
    let p = app.palette.as_ref().unwrap();
    assert_eq!(p.entries.len(), 1);
    assert!(p.entries[0].label.contains("lib.rs:1"));
    app.palette_key(code(KeyCode::Enter));
    assert_eq!(app.editor.cursor_line(), 0);
}

#[test]
fn previews_come_from_open_buffers_disk_and_are_cached() {
    let d = temp("pal-preview");
    std::fs::write(d.join("lib.rs"), "line1\nline2\nline3\n").unwrap();
    std::fs::write(d.join("pic.png"), [0x89, b'P', b'N', b'G']).unwrap();
    let mut app = app_at(&d);
    // Nothing highlighted: no preview target.
    assert!(app.palette_preview().is_none());
    app.open_palette_seeded("lib");
    assert!(app.palette_has_preview());
    let (title, preview) = app.palette_preview().unwrap();
    assert_eq!(title, "lib.rs");
    let _ = preview;
    // The second call is served from the one-entry cache.
    assert!(app.picker_preview_cache.is_some());
    assert!(app.palette_preview().is_some());
    // Search Everywhere has no preview pane.
    app.palette_everywhere = true;
    assert!(!app.palette_has_preview());
    app.palette_everywhere = false;
    // An image has no preview; a missing file has none either.
    assert!(
        app.picker_preview(PreviewSource::File(d.join("pic.png")), 1)
            .is_none()
    );
    assert!(
        app.picker_preview(PreviewSource::File(d.join("gone.rs")), 1)
            .is_none()
    );
    // An open buffer is previewed from memory, including its unsaved text.
    app.palette = None;
    app.open_initial(&d.join("lib.rs"));
    app.on_key(key('Z'));
    let (_, from_buffer) = app
        .picker_preview(PreviewSource::File(d.join("lib.rs")), 1)
        .unwrap();
    assert!(format!("{from_buffer:?}").contains('Z'));
    // The active-buffer source needs a tab.
    assert!(app.picker_preview(PreviewSource::ActiveBuffer, 1).is_some());
    app.editor.tabs.clear();
    app.picker_preview_cache = None;
    assert!(app.picker_preview(PreviewSource::ActiveBuffer, 1).is_none());
    // A Cowork-style remote path is never read from disk.
    let remote = PathBuf::from(format!("{}x.rs", crate::editor::REMOTE_PATH_PREFIX));
    assert!(app.picker_preview(PreviewSource::File(remote), 1).is_none());
}

#[test]
fn file_scope_and_the_index_cap_shape_the_file_list() {
    let d = temp("pal-scope");
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub/in.txt"), "x\n").unwrap();
    std::fs::write(d.join("out.txt"), "y\n").unwrap();
    let mut app = app_at(&d);
    app.build_file_index();
    app.palette_file_scope = Some("sub".to_string());
    let entries = app.palette_file_entries("");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].label.contains("in.txt"));
    app.palette_file_scope = None;
    assert!(app.palette_file_entries("").len() >= 2);
    // A second workspace folder is indexed as well.
    let other = temp("pal-scope-other");
    std::fs::write(other.join("extra.txt"), "z\n").unwrap();
    app.workspace_folders.push(other);
    app.build_file_index();
    assert!(app.file_index.iter().any(|p| p.ends_with("extra.txt")));
}
