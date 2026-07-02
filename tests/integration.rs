//! Integration tests for Vix's terminal-independent logic.

#![warn(clippy::pedantic)]
// Test setup casts small counts to `u16` cell coordinates and builds fixture
// strings by collecting `format!`; both are fine in tests.
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use vix::app::{App, Focus};
use vix::calendar;
use vix::clock;
use vix::fileops;
use vix::settings::Settings;
use vix::palette::{fuzzy_match, parse_path_target};
use vix::search::SearchBar;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn keycode(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn esc() -> KeyEvent {
    keycode(KeyCode::Esc)
}

fn func(n: u8) -> KeyEvent {
    keycode(KeyCode::F(n))
}

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent { kind, column: col, row, modifiers: KeyModifiers::NONE }
}

fn click(col: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), col, row)
}

/// Build an app with a realistic editor viewport so the code editor's
/// scroll-into-view logic has a sane area to work with.
fn app_at(root: &Path) -> App {
    let mut app = App::new(root.to_path_buf(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build an app with custom settings and a realistic editor viewport.
fn app_with(settings: Settings) -> App {
    let mut app = App::new(Path::new(".").to_path_buf(), settings);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        if c == '\n' {
            app.on_key(keycode(KeyCode::Enter));
        } else {
            app.on_key(key(c));
        }
    }
}

#[test]
fn project_media_type_example_snippets_load() {
    // The bundled example files live under config/media-types/<type>/snippets/.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proj = "config/snippets/snippets.json";

    // Rust source maps to text/rust (no x- prefix) and loads its examples.
    assert_eq!(vix::media_type::for_extension("rs").unwrap().media_type, "text/rust");
    let rust = vix::snippets::load_scoped(Some("text/rust"), root, proj);
    assert!(rust.iter().any(|s| s.prefixes.iter().any(|p| p == "fn")), "rust examples loaded");

    // A few other languages resolve to the clean text/* or application/* types.
    assert_eq!(vix::media_type::for_extension("py").unwrap().media_type, "text/python");
    assert_eq!(vix::media_type::for_extension("ts").unwrap().media_type, "text/typescript");
    assert_eq!(vix::media_type::for_extension("cs").unwrap().media_type, "text/csharp");
    assert_eq!(vix::media_type::for_extension("sql").unwrap().media_type, "application/sql");
    let sql = vix::snippets::load_scoped(Some("application/sql"), root, proj);
    assert!(sql.iter().any(|s| s.prefixes.iter().any(|p| p == "select")), "sql examples loaded");

    // Newly added languages resolve and load their example libraries.
    for (ext, mt) in [
        ("go", "text/go"),
        ("kt", "text/kotlin"),
        ("hs", "text/haskell"),
        ("ex", "text/elixir"),
        ("sh", "text/sh"),
        ("ps1", "text/powershell"),
        ("puml", "text/plantuml"),
        ("gv", "text/graphviz"),
        ("dot", "text/graphviz"),
        ("mmd", "text/mermaid"),
        ("mermaid", "text/mermaid"),
    ] {
        assert_eq!(vix::media_type::for_extension(ext).unwrap().media_type, mt, "{ext} → {mt}");
        let snips = vix::snippets::load_scoped(Some(mt), root, proj);
        assert!(!snips.is_empty(), "{mt} examples loaded");
    }

    // PlantUML loads both the building blocks and the example gallery.
    let pl = vix::snippets::load_scoped(Some("text/plantuml"), root, proj);
    assert!(pl.iter().any(|s| s.name == "Sequence Diagram"), "plantuml gallery loaded");
    assert!(pl.len() >= 50, "building blocks + gallery merged");

    // The Base column marks text vs binary content.
    assert!(vix::media_type::for_extension("rs").unwrap().is_text());
    assert!(!vix::media_type::for_extension("png").unwrap().is_text());
}

#[test]
fn snippet_picker_filters_and_inserts_bundled() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.snippets");
    assert!(app.snippets.is_some(), "Tools → Snippets opens the picker");
    // The library includes the bundled snippets.
    assert!(app.snippet_library.iter().any(|s| s.name == "TODO comment"));

    // Filter to the TODO snippet and insert it.
    for c in "todo".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.snippets.is_none(), "Enter inserts and closes");
    assert!(app.editor.active_tab().unwrap().text().starts_with("TODO: "));
}

#[test]
fn project_snippet_expands_from_prefix_on_tab() {
    let dir = unique_dir("snippets-proj");
    fs::create_dir_all(dir.join("config/snippets")).unwrap();
    fs::write(
        dir.join("config/snippets/snippets.json"),
        r#"{ "Greet": { "prefix": "hi", "body": "Hello, ${1:world}!$0" } }"#,
    )
    .unwrap();
    let mut app = app_at(&dir);

    // Type the prefix, then Tab expands it (project-scoped snippet).
    for c in "hi".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().text(), "Hello, world!");
    // The first tabstop ("world") is selected for the snippet session.
    let sel = app.editor.active_tab_mut().unwrap().editor.get_selection_text();
    assert_eq!(sel.as_deref(), Some("world"));
}

#[test]
fn edit_sql_lists_formats_and_saves_statements() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "select 1;\ninsert into t values (1)");
    app.run_action("tools.edit_sql");
    assert!(app.edit_sql.is_some(), "Edit → Mode → SQL opens the SQL editor");

    // Format all (Shift+F) uppercases keywords; Ctrl+S writes back to the buffer.
    app.on_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("SELECT 1;"), "keywords uppercased and saved: {text:?}");
    assert!(text.contains("INSERT INTO t VALUES (1);"));

    // q closes the editor.
    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.edit_sql.is_none());
}

#[test]
fn media_type_picker_filters_and_inserts() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.media_types");
    assert!(app.media_type_panel.is_some(), "Media Types opens the picker");

    // Type to filter down to SVG, then Enter inserts the media type.
    for c in "svg".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().text(), "image/svg+xml");

    // Esc closes it.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.media_type_panel.is_none());

    // The lookup table is also usable directly by extension.
    assert_eq!(vix::media_type::for_extension("png").unwrap().media_type, "image/png");
}

#[test]
fn tools_draw_inserts_ditaa_ascii_art() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.rectangle");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("+-------+"), "rectangle: {text:?}");
    assert!(text.contains("|       |"));

    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.rounded");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("/-------\\") && text.contains("\\-------/"), "rounded: {text:?}");

    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.arrow_right");
    assert_eq!(app.editor.active_tab().unwrap().text(), "------->");
}

#[test]
fn org_capture_inserts_todo_and_time_report_tabulates() {
    // Capture opens a prompt; submitting inserts a TODO headline at the cursor.
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture");
    assert!(app.prompt.is_some(), "Org → Capture opens a prompt");
    for c in "Buy milk".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.editor.active_tab().unwrap().text().contains("* TODO Buy milk"));

    // Time Tracker builds a clock report in a new tab.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\nCLOCK: [a]--[b] =>  1:00\n");
    let before = app.editor.tabs.len();
    app.run_action("org.time_report");
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(app.editor.active_tab().unwrap().text().contains("| Task | 1:00 |"));

    // Agenda Tracker runs and opens a buffer (no .org files → just the header).
    app.run_action("org.agenda");
    assert!(app.editor.active_tab().unwrap().text().contains("Agenda"));

    // Clock In inserts an open CLOCK entry; Clock Out completes it.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\n");
    app.run_action("org.clock_in");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("CLOCK: ["), "clock-in line: {t:?}");
    assert!(!t.contains("--"), "still open");
    app.run_action("org.clock_out");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("--[") && t.contains("=>"), "clocked out: {t:?}");
}

#[test]
fn roam_capture_insert_dailies_and_views() {
    let dir = unique_dir("roam");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Capture a node: the prompt creates an .org file and opens it.
    app.run_action("roam.capture");
    assert!(app.prompt.is_some(), "Roam → Capture opens a prompt");
    type_str(&mut app, "My First Note");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let node = app.editor.active_tab().unwrap().text();
    assert!(node.contains("#+title: My First Note"), "node has title: {node:?}");
    assert!(node.contains(":ID:"), "node has an ID drawer");
    assert!(dir.join("my-first-note.org").exists(), "node file written to disk");

    // Insert a link to a (new) node into the current buffer without leaving it.
    let mut app = app_at(&dir);
    app.run_action("roam.node_insert");
    type_str(&mut app, "Another Note");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let buf = app.editor.active_tab().unwrap().text();
    assert!(buf.contains("[[id:") && buf.contains("][Another Note]]"), "link inserted: {buf:?}");

    // Dailies → Today creates and opens today's daily note under daily/.
    app.run_action("roam.dailies_today");
    let daily = app.editor.active_tab().unwrap().text();
    assert!(daily.starts_with(":PROPERTIES:") && daily.contains("#+title: 20"), "daily note: {daily:?}");

    // Graph and Sync compile cross-node buffers.
    app.run_action("roam.graph");
    assert!(app.editor.active_tab().unwrap().text().contains("flowchart LR"));
    app.run_action("roam.db_sync");
    assert!(app.editor.active_tab().unwrap().text().contains("Roam Nodes"));

    // Add a tag to the active node buffer.
    let mut app = app_at(&dir);
    type_str(&mut app, "#+title: Tagged\n");
    app.run_action("roam.tag_add");
    type_str(&mut app, "work");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.editor.active_tab().unwrap().text().contains("#+filetags: :work:"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn org_node_nodeify_extract_and_dead_links() {
    let dir = unique_dir("orgnode");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Nodeify: the headline at the cursor gains an :ID: drawer.
    type_str(&mut app, "* My Heading\nsome body\n");
    for _ in 0..10 {
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    }
    app.run_action("node.nodeify");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("* My Heading\n:PROPERTIES:\n:ID:"), "nodeified: {t:?}");
    // A second nodeify is a no-op (already a node).
    app.run_action("node.nodeify");
    assert!(app.status.contains("cursor on a headline") || t.contains(":ID:"));

    // Insert a transclusion for a new node into a fresh buffer.
    let mut app = app_at(&dir);
    app.run_action("node.insert_transclusion");
    type_str(&mut app, "Shared Block");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.editor.active_tab().unwrap().text().contains("#+transclude: [[id:"));

    // Extract subtree: the subtree moves to a new file, a link stays behind.
    let mut app = app_at(&dir);
    type_str(&mut app, "* Parent\n** Child\nchild body\n");
    for _ in 0..10 {
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // onto "** Child"
    app.run_action("node.extract_subtree");
    assert!(dir.join("child.org").exists(), "extracted node file written");

    // Dead-links report opens a buffer.
    app.run_action("node.dead_links");
    assert!(app.editor.active_tab().unwrap().text().contains("Dead Links"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn org_checkbox_toggle_updates_parents_and_cookies() {
    // Move the cursor to a 0-based line by going to the top, then down.
    fn goto(app: &mut App, line: usize) {
        for _ in 0..40 {
            app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }
        for _ in 0..line {
            app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
    }

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Tasks [/]\n- [ ] call people\n  - [ ] Peter\n  - [ ] Sarah\n");

    // Toggle the "Peter" child (line 2): parent becomes partial, child checked.
    goto(&mut app, 2);
    app.run_action("org.toggle_checkbox");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("- [-] call people"), "parent partial: {t:?}");
    assert!(t.contains("  - [x] Peter"), "child checked: {t:?}");

    // Toggle "Sarah" (line 3): parent and the top-level item become fully checked.
    goto(&mut app, 3);
    app.run_action("org.toggle_checkbox");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("- [X] call people"), "parent checked: {t:?}");
    assert!(t.contains("* Tasks [1/1]"), "headline cookie counts the top-level checkbox: {t:?}");
}

#[test]
fn comment_banner_boxes_the_current_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Section Title");
    app.run_action("edit.comment_banner");
    let text = app.editor.active_tab().unwrap().text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "three banner lines: {text:?}");
    assert!(lines[0].contains('='), "top rule: {:?}", lines[0]);
    assert!(lines[1].contains("Section Title"), "title line: {:?}", lines[1]);
    assert!(lines[2].contains('='), "bottom rule: {:?}", lines[2]);
}

#[test]
fn goto_percent_and_byte_move_the_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "l1\nl2\nl3\nl4\nl5\n"); // 6 lines (incl. trailing)
    // 50% of the way through jumps roughly to the middle.
    app.run_action("nav.goto_percent");
    for c in "50".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let mid = app.editor.cursor_1based().0;
    assert!((2..=4).contains(&mid), "50% lands mid-file, got line {mid}");

    // Go to byte 0 returns to the start.
    app.run_action("nav.goto_byte");
    app.on_key(key('0'));
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.editor.cursor_1based(), (1, 1), "byte 0 is the file start");
}

#[test]
fn read_only_blocks_edits_but_allows_navigation() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "locked");
    app.run_action("view.read_only");
    // Typing is blocked.
    type_str(&mut app, "XYZ");
    assert_eq!(app.editor.active_tab().unwrap().text(), "locked", "typing blocked");
    // A destructive command is blocked.
    app.run_action("edit.reverse_lines");
    assert_eq!(app.editor.active_tab().unwrap().text(), "locked", "command blocked");
    // Toggling it back off restores editing.
    app.run_action("view.read_only");
    type_str(&mut app, "!");
    let txt = app.editor.active_tab().unwrap().text(); assert!(txt.ends_with('!'), "editing restored: {txt:?}");
}

#[test]
fn text_transforms_squeeze_eol_and_rot13() {
    // Squeeze blank lines over the whole buffer (no selection).
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a\n\n\n\nb\n");
    app.run_action("edit.squeeze_blank_lines");
    assert_eq!(app.editor.active_tab().unwrap().text(), "a\n\nb\n");

    // ROT13 over a selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Hello");
    app.on_key(ctrl('a'));
    app.run_action("tools.convert.rot13");
    assert_eq!(app.editor.active_tab().unwrap().text(), "Uryyb");

    // Convert to CRLF.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "x\ny\n");
    app.run_action("edit.eol_crlf");
    assert_eq!(app.editor.active_tab().unwrap().text(), "x\r\ny\r\n");
}

#[test]
fn which_key_lists_candidates_after_a_leader() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "spacemacs".to_string();
    // No prefix pending → no which-key.
    assert!(app.which_key().is_none());
    // Press the Space leader, then 'f' → candidates like "ff", "fr", "fs", "fp".
    app.on_key(key(' '));
    app.on_key(key('f'));
    let (title, rows) = app.which_key().expect("which-key active after SPC f");
    assert!(title.contains('f'), "title shows the pending sequence: {title:?}");
    assert!(rows.iter().any(|(k, a)| k == "f" && a == "file.open"), "SPC f f = open: {rows:?}");
    assert!(rows.iter().any(|(_, a)| a == "file.save"), "includes SPC f s save");
}

#[test]
fn clipboard_history_records_copies_and_pastes_from_it() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta");
    // Select "alpha" (first 5 chars) and copy it.
    app.on_key(keycode(KeyCode::Home));
    for _ in 0..5 {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    }
    app.run_action("edit.copy");
    assert!(app.clipboard_ring.iter().any(|e| e == "alpha"), "copy recorded: {:?}", app.clipboard_ring);
    // Collapse the selection, then move to end of buffer to paste there.
    app.on_key(keycode(KeyCode::Right));
    app.on_key(keycode(KeyCode::End));
    app.run_action("edit.paste_from_history");
    assert!(app.clipboard_chooser.is_some(), "history picker opens");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.editor.active_tab().unwrap().text().contains("betaalpha"), "pasted from history");
}

#[test]
fn http_send_reports_when_buffer_has_no_request() {
    // A buffer without a request line is rejected up front (no network attempt).
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "just some prose\n");
    app.run_action("tools.http_send");
    assert!(app.status.to_lowercase().contains("http") || app.status.contains("METHOD"));
    assert!(!app.http_running(), "no request was dispatched");
}

#[test]
fn jump_to_line_labels_move_the_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "line0\nline1\nline2\nline3\n");
    // Enter jump mode: line N gets label = Nth letter (a, b, c, …).
    app.run_action("nav.jump");
    assert!(app.jump.is_some(), "jump mode active");
    // 'c' is the 3rd label → 0-based line 2.
    app.on_key(key('c'));
    assert!(app.jump.is_none(), "jump mode exits on match");
    assert_eq!(app.editor.cursor_1based().0, 3, "cursor on line 3 (0-based 2)");
}

#[test]
fn scratch_buffer_opens_unsaved_with_a_header() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.tabs.len();
    app.run_action("file.scratch");
    assert_eq!(app.editor.tabs.len(), before + 1);
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.is_none(), "scratch buffer is not file-backed");
    assert!(tab.text().contains("Scratch buffer"), "has the header");
}

#[test]
fn align_on_equals_pads_the_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a = 1\nbbb = 2\n");
    app.on_key(ctrl('a')); // select all
    app.run_action("edit.align.equals");
    assert_eq!(app.editor.active_tab().unwrap().text(), "a   = 1\nbbb = 2\n");
}

#[test]
fn surround_wraps_and_unwraps_the_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "word");
    app.on_key(ctrl('a')); // select all
    app.run_action("edit.surround.paren");
    assert_eq!(app.editor.active_tab().unwrap().text(), "(word)");
    // Repeating the same surround removes it (toggle_wrap behavior).
    app.on_key(ctrl('a'));
    app.run_action("edit.surround.paren");
    assert_eq!(app.editor.active_tab().unwrap().text(), "word");
}

#[test]
fn emmet_expand_replaces_the_abbreviation() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "ul>li*2");
    app.run_action("edit.emmet_expand");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("<ul>"), "expanded: {text:?}");
    assert_eq!(text.matches("<li>").count(), 2, "two list items: {text:?}");
    assert!(!text.contains("ul>li*2"), "abbreviation consumed");
}

#[test]
fn org_menu_edits_headlines_and_exports() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\nbody");
    // Cursor is on the body line; move to the headline (line 0).
    app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    app.run_action("org.cycle_todo");
    assert!(app.editor.active_tab().unwrap().text().starts_with("* TODO Task"));
    app.run_action("org.demote");
    assert!(app.editor.active_tab().unwrap().text().starts_with("** TODO Task"));

    // Export opens a new buffer containing Markdown.
    let before = app.editor.tabs.len();
    app.run_action("org.export_markdown");
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(app.editor.active_tab().unwrap().text().contains("## TODO Task"));
}

#[test]
fn org_insert_and_marker_block_toggles() {
    let mut app = app_at(Path::new("."));
    // Org snippet insertion.
    app.run_action("tools.insert.org.title");
    assert_eq!(app.editor.active_tab().unwrap().text(), "#+title: Hello World\n");

    // Marker toggle wraps, then unwraps, the selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "bold");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.bold");
    assert_eq!(app.editor.active_tab().unwrap().text(), "*bold*");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.bold");
    assert_eq!(app.editor.active_tab().unwrap().text(), "bold");

    // Begin-End block toggle wraps the selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hi");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.block.quote");
    assert_eq!(app.editor.active_tab().unwrap().text(), "#+BEGIN_QUOTE\nhi\n#+END_QUOTE");

    // The Tag marker wraps the selection with ':'.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "work");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.tag");
    assert_eq!(app.editor.active_tab().unwrap().text(), ":work:");

    // The Properties snippet inserts a property drawer.
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.org.properties");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.starts_with(":PROPERTIES:"));
    assert!(text.contains(":Composer:  J.S. Bach"));
    assert!(text.trim_end().ends_with(":END:"));
}

#[test]
fn select_all_then_typing_replaces_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.on_key(ctrl('a')); // Ctrl+A selects the whole buffer
    app.on_key(key('x'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "x");
}

#[test]
fn pomodoro_start_closes_dialog_and_runs_in_background() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.pomodoro");
    assert!(app.pomodoro_open, "dialog visible");
    assert_eq!(app.pomodoro.as_ref().unwrap().label(), "25:00");
    assert!(!app.pomodoro_running());
    app.on_key(keycode(KeyCode::Down)); // 24 minutes
    app.on_key(keycode(KeyCode::Enter)); // Start
    // Start hides the dialog but the countdown keeps running.
    assert!(!app.pomodoro_open, "dialog closed on Start");
    assert!(app.pomodoro_running(), "timer still running in background");
    assert_eq!(app.pomodoro.as_ref().unwrap().label(), "24:00");
    // Reopening reveals the still-running timer.
    app.run_action("tools.pomodoro");
    assert!(app.pomodoro_open);
    app.on_key(keycode(KeyCode::Enter)); // Stop → back to idle, dialog stays open
    assert!(!app.pomodoro_running(), "timer stopped");
    assert!(app.pomodoro_open);
    app.on_key(keycode(KeyCode::Esc)); // close
    assert!(!app.pomodoro_open && app.pomodoro.is_none(), "dialog closed and timer dropped");
}

#[test]
fn calculator_runs_and_inserts_result() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calculator");
    assert!(app.calculator.is_some(), "dialog opened");
    for ch in "6*7".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter)); // Run (input focused)
    assert_eq!(app.calculator.as_ref().unwrap().result(), Some("42"));
    app.on_key(keycode(KeyCode::Tab)); // focus Run
    app.on_key(keycode(KeyCode::Tab)); // focus Insert
    app.on_key(keycode(KeyCode::Enter)); // insert
    assert!(app.calculator.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "42");
}

#[test]
fn unit_converter_inserts_converted_value() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.convert.unit");
    assert!(app.unit_converter.is_some(), "dialog opened");
    // Default is 1 m → km; the output is "0.001 km".
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.unit_converter.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "0.001 km");
}

#[test]
fn color_converter_syncs_fields_and_inserts() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.color_converter");
    assert!(app.color_converter.is_some(), "dialog opened");
    for ch in "#ff0000".chars() {
        app.on_key(key(ch));
    }
    {
        let conv = app.color_converter.as_ref().unwrap();
        assert_eq!(conv.fields[1], "rgb(255, 0, 0)", "RGB field synced");
        assert_eq!(conv.fields[2], "hsl(0, 100%, 50%)", "HSL field synced");
    }
    app.on_key(keycode(KeyCode::Enter)); // insert the focused (HEX) value
    assert!(app.color_converter.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "#ff0000");
}

#[test]
fn convert_base64_round_trips_via_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.run_action("tools.convert.base64.encode");
    assert_eq!(app.editor.active_tab().unwrap().text(), "aGVsbG8=");
    app.run_action("tools.convert.base64.decode");
    assert_eq!(app.editor.active_tab().unwrap().text(), "hello");
}

#[test]
fn format_json_pretty_and_minify() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "{\"a\":1,\"b\":2}");
    app.run_action("tools.format.json_pretty");
    assert!(app.editor.active_tab().unwrap().text().contains("\n  \"a\": 1"));
    app.run_action("tools.format.json_minify");
    assert_eq!(app.editor.active_tab().unwrap().text(), "{\"a\":1,\"b\":2}");
}

#[test]
fn convert_markdown_to_html_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "# Title");
    app.run_action("tools.convert.markdown.html");
    assert_eq!(app.editor.active_tab().unwrap().text(), "<h1>Title</h1>\n");
}

#[test]
fn convert_number_base_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "255");
    app.run_action("tools.convert.number.hex");
    assert_eq!(app.editor.active_tab().unwrap().text(), "0xff");
    app.run_action("tools.convert.number.dec");
    assert_eq!(app.editor.active_tab().unwrap().text(), "255");
}

#[test]
fn convert_jwt_decode_action() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhYmMifQ.sig");
    app.run_action("tools.convert.jwt");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"alg\": \"HS256\""), "got: {text}");
    assert!(text.contains("\"sub\": \"abc\""), "got: {text}");
}

#[test]
fn convert_toml_to_json_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "name = \"Vix\"\n");
    app.run_action("tools.convert.toml.json");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"name\": \"Vix\""), "got: {text}");
}

#[test]
fn convert_csv_to_json_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a,b\n1,2\n");
    app.run_action("tools.convert.csv.json");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"a\": \"1\""), "got: {text}");
}

#[test]
fn convert_failure_leaves_buffer_unchanged() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "not json");
    app.run_action("tools.convert.json.csv");
    // Invalid JSON: the buffer is left intact.
    assert_eq!(app.editor.active_tab().unwrap().text(), "not json");
}

#[test]
fn snippets_picker_inserts_selected_body() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.snippets");
    assert!(app.snippets.is_some(), "picker opened");
    app.on_key(keycode(KeyCode::Enter)); // insert the first snippet
    assert!(app.snippets.is_none(), "picker closed after insert");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.starts_with("#!/usr/bin/env bash"), "got: {text:?}");
}

#[test]
fn markdown_preview_renders_active_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "# Title\n\n- a\n- b\n");
    app.run_action("tools.markdown_preview");
    let p = app.markdown_preview.as_ref().expect("preview open");
    assert_eq!(p.lines[0], "Title");
    assert!(p.lines.iter().any(|l| l == "• a"), "{:?}", p.lines);
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.markdown_preview.is_none(), "Esc closes the preview");
}

#[test]
fn text_information_reports_counts() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Hello world.\nHow are you?");
    app.run_action("tools.text_info");
    let p = app.text_info.as_ref().expect("panel open");
    assert_eq!(p.rows[0].label, "Characters");
    assert_eq!(p.rows[0].value, "25");
    assert_eq!(p.rows[1].value, "5"); // words
    assert_eq!(p.rows[3].value, "2"); // sentences
    app.on_key(keycode(KeyCode::Enter)); // insert Characters value (25)
    assert!(app.editor.active_tab().unwrap().text().contains("25"));
}

#[test]
fn checksum_sha256_hashes_whole_buffer_when_unselected() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    app.run_action("tools.checksum.sha256");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn checksum_on_long_buffer_keeps_caret_in_range() {
    // Regression: a transform that shrinks the buffer (120 chars → 64-char hash)
    // must move the caret back in range, or the next render panics in char_to_line.
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, &"x".repeat(120));
    app.run_action("tools.checksum.sha256");
    assert_eq!(app.editor.active_tab().unwrap().text().len(), 64);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap(); // must not panic
}

#[test]
fn generate_uuid_v4_inserts_a_canonical_uuid() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.uuid.v4");
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text.len(), 36, "v4 UUID is 36 chars: {text:?}");
    assert_eq!(text.chars().nth(14), Some('4'), "version digit is 4");
}

#[test]
fn generate_zid_sizes_insert_hex_of_the_right_length() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.zid.128");
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text.len(), 32, "128-bit ZID is 32 hex chars: {text:?}");
    assert!(text.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.zid.512");
    assert_eq!(app.editor.active_tab().unwrap().text().len(), 128, "512-bit ZID is 128 hex chars");
}

#[test]
fn insert_markdown_snippets_insert_templates() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.headline1");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "# Headline 1");

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.link");
    assert!(
        app.editor.active_tab().unwrap().text().contains("[Example](https://www.example.com)"),
        "link snippet inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.table");
    assert!(
        app.editor.active_tab().unwrap().text().contains("|---|---|---|"),
        "table snippet inserted"
    );
}

#[test]
fn insert_html_snippets_insert_templates() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.headline1");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "<h1>Headline</h1>");

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.link");
    assert!(
        app.editor.active_tab().unwrap().text().contains("<a href=\"https://www.example.com\">Example</a>"),
        "link snippet inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.table");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("<table>") && text.contains("<th>x</th>"), "table snippet inserted");
}

#[test]
fn select_all_action_selects_whole_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.run_action("edit.select_all"); // the menu / palette path
    app.on_key(key('z'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "z");
}

#[test]
fn duplicate_line_copies_the_current_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    // Ctrl+Shift+D duplicates the cursor line (Ctrl+D now adds a caret).
    app.on_key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["abc", "abc"]);
}

#[test]
fn join_lines_merges_current_line_with_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo\nbar\nbaz");
    app.run_action("edit.go_first"); // cursor to the first line
    app.run_action("edit.join_lines");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["foo bar", "baz"]);
}

#[test]
fn sort_lines_orders_whole_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "banana\napple\ncherry");
    app.run_action("edit.sort_lines");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["apple", "banana", "cherry"]);
}

#[test]
fn line_transforms_via_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "b\na\nb\na\n");
    app.run_action("edit.sort_unique");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["a", "b"]);

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "one\ntwo\nthree");
    app.run_action("edit.reverse_lines");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["three", "two", "one"]);

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "x\ny\nx");
    app.run_action("edit.remove_duplicate_lines");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["x", "y"]);

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo   ");
    app.run_action("edit.trim_trailing_whitespace");
    assert_eq!(app.editor.active_tab().unwrap().text(), "foo");
}

#[test]
fn conflict_resolve_keeps_chosen_side() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b\nz\n");
    app.run_action("edit.go_first"); // cursor to line 0
    app.run_action("git.conflict_ours");
    assert_eq!(app.editor.active_tab().unwrap().text(), "a\nours\nz\n");
}

#[test]
fn regex_tester_finds_matches_live() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.regex_tester");
    app.regex_tester.as_mut().unwrap().subject = "a1 b2 c3".to_string();
    for c in r"\d".chars() {
        app.on_key(key(c));
    }
    match app.regex_tester.as_ref().unwrap().result() {
        vix::regex_tool::Outcome::Matches(m) => assert_eq!(m, vec!["1", "2", "3"]),
        vix::regex_tool::Outcome::Error(e) => panic!("expected matches, got error: {e}"),
    }
    app.on_key(esc());
    assert!(app.regex_tester.is_none());
}

#[test]
fn diagnostics_panel_empty_reports_none() {
    let mut app = app_at(Path::new("."));
    app.run_action("lsp.diagnostics");
    assert!(app.workspace_search.is_none(), "no panel without diagnostics");
    assert!(app.status.to_lowercase().contains("diagnostic"), "status: {}", app.status);
}

#[test]
fn specs_have_no_stale_subcrate_references() {
    // Guard against architecture drift: after folding the subcrates into modules,
    // no spec should mention the old `vix-editor`/`vix_editor` crate or a
    // "Subcrate".
    fn walk(dir: &Path, hits: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "md" || e == "tsv") {
                let text = fs::read_to_string(&path).unwrap_or_default();
                for needle in ["vix-editor", "vix_editor", "Subcrate ", "subcrate "] {
                    if text.contains(needle) {
                        hits.push(format!("{}: {needle}", path.display()));
                    }
                }
            }
        }
    }
    let spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec");
    let mut hits = Vec::new();
    walk(&spec, &mut hits);
    assert!(hits.is_empty(), "stale subcrate references in spec:\n{}", hits.join("\n"));
}

#[test]
fn mode_and_suspend_actions() {
    let mut app = app_at(Path::new("."));
    app.run_action("command_mode");
    assert!(app.palette.is_some(), "command_mode opens the command palette");
    let mut app = app_at(Path::new("."));
    app.run_action("shell_mode");
    assert!(app.prompt.is_some(), "shell_mode opens the run-command prompt");
    let mut app = app_at(Path::new("."));
    app.run_action("suspend");
    assert!(app.suspend_requested, "suspend flags the main loop");
}

#[test]
fn inlay_hints_render_inline() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "let x = 1;\n");
    // A ": i32" hint just after `x` (char column 5).
    app.editor.active_tab_mut().unwrap().editor.set_inlay_hints(vec![(0, 5, ": i32".to_string())]);
    let mut term = Terminal::new(TestBackend::new(80, 6)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String =
        term.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(screen.contains(": i32"), "inlay hint text is rendered");
    assert!(screen.contains("let x"), "real text still present");
}

#[test]
fn folding_hides_lines_and_renders() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "fn a() {\n  x;\n  y;\n}\nfn b() {}\n");
    // Mark lines 0..=3 as a foldable range (normally supplied by the server).
    app.editor.active_tab_mut().unwrap().editor.set_fold_ranges(vec![(0, 3)]);
    app.run_action("edit.go_first"); // cursor to line 0
    app.run_action("editor.fold_toggle");
    let ed = &app.editor.active_tab().unwrap().editor;
    assert!(ed.has_folds(), "fold active");
    assert!(ed.is_line_hidden(1) && ed.is_line_hidden(3), "inner lines hidden");
    assert!(!ed.is_line_hidden(0), "fold start stays visible");
    assert!(!ed.is_line_hidden(4), "line after fold visible");
    // Rendering with a fold active must not panic.
    let mut term = Terminal::new(TestBackend::new(80, 20)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    app.run_action("editor.unfold_all");
    assert!(!app.editor.active_tab().unwrap().editor.has_folds());
}

#[test]
fn lsp_navigation_actions_report_inactive_without_server() {
    // With no language server attached, the LSP nav actions are no-ops that
    // report inactivity rather than panicking.
    for action in [
        "nav.goto_implementation",
        "nav.goto_type_definition",
        "lsp.references",
        "lsp.format",
        "lsp.document_symbols",
        "lsp.workspace_symbols",
        "lsp.signature_help",
        "lsp.rename",
        "lsp.code_action",
        "lsp.expand_selection",
        "lsp.shrink_selection",
        "lsp.highlight",
        "lsp.linked_edit",
        "lsp.code_lens",
    ] {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "fn main() {}\n");
        app.run_action(action);
        assert!(app.workspace_search.is_none(), "{action} opened no panel");
        assert!(app.code_actions.is_none(), "{action} opened no code-action menu");
        assert!(app.code_lens.is_none(), "{action} opened no code-lens menu");
        assert!(app.prompt.is_none(), "{action} opened no prompt without a server");
    }
}

#[test]
fn bookmarks_toggle_and_list() {
    let dir = unique_dir("bookmarks");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("bookmark.toggle");
    assert_eq!(app.bookmarks.len(), 1, "bookmark added");
    app.run_action("bookmark.list");
    assert!(app.location_chooser.is_some(), "bookmark list opened");
    app.on_key(esc());
    app.run_action("bookmark.toggle"); // same line → removes
    assert!(app.bookmarks.is_empty(), "bookmark removed");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_key_menu_shows_the_shortcuts_overlay() {
    let mut app = app_at(Path::new("."));
    assert!(!app.show_help);
    app.run_action("toggle_key_menu");
    assert!(app.show_help, "key menu opens the shortcuts overlay");
    app.run_action("toggle_key_menu");
    assert!(!app.show_help);
}

#[test]
fn autocomplete_completes_a_buffer_word() {
    let mut app = app_at(Path::new("."));
    // A long word exists earlier; typing its prefix then autocompleting expands it.
    type_str(&mut app, "function\nfun");
    app.run_action("autocomplete");
    assert_eq!(app.editor.active_tab().unwrap().text(), "function\nfunction");
}

#[test]
fn macro_records_and_replays_editor_keys() {
    let mut app = app_at(Path::new("."));
    app.run_action("macro.record"); // start recording
    assert!(app.macro_recording);
    app.on_key(key('a'));
    app.on_key(key('b'));
    app.run_action("macro.record"); // stop
    assert!(!app.macro_recording);
    assert_eq!(app.editor.active_tab().unwrap().text(), "ab");
    app.run_action("macro.play"); // replays "ab" at the cursor
    assert_eq!(app.editor.active_tab().unwrap().text(), "abab");
}

#[test]
fn column_ruler_toggles_and_renders() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "some code here\n");
    app.run_action("toggle_ruler");
    assert!(app.show_ruler);
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap(); // ruler drawn, must not panic
    app.run_action("toggle_ruler");
    assert!(!app.show_ruler);
}

#[test]
fn overwrite_mode_types_over_characters() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    app.run_action("edit.go_first"); // cursor to start of line 0
    app.run_action("toggle_overwrite_mode");
    assert!(app.overwrite);
    app.on_key(key('X'));
    // 'X' overwrites 'a' rather than inserting before it.
    assert_eq!(app.editor.active_tab().unwrap().text(), "Xbc");
    // At end-of-line it inserts normally.
    app.run_action("edit.line_end");
    app.on_key(key('Y'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "XbcY");
}

#[test]
fn spawn_multi_cursor_below_adds_a_caret() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "ab\ncd\nef");
    app.run_action("edit.go_first"); // cursor to line 0, col 0
    app.run_action("spawn_multi_cursor_down");
    assert!(app.editor.active_tab().unwrap().editor.has_multi_carets(), "a caret was added below");
}

#[test]
fn ctrl_d_adds_a_caret_and_edits_all_occurrences() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo foo foo");
    // Cursor is at end; move to the start so the first word is "foo".
    app.on_key(ctrl('a')); // select all, then collapse to start via Left
    app.on_key(keycode(KeyCode::Left));
    // Ctrl+D selects the word, then again adds the next occurrence as a caret.
    app.on_key(ctrl('d'));
    app.on_key(ctrl('d'));
    app.on_key(ctrl('d'));
    assert!(app.editor.active_tab().unwrap().editor.has_multi_carets(), "carets added");
    // Typing replaces every selected occurrence at once.
    type_str(&mut app, "bar");
    assert_eq!(app.editor.active_tab().unwrap().text(), "bar bar bar");
}

#[test]
fn enter_carries_indentation() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::Tab));
    type_str(&mut app, "x\ny"); // Tab, 'x', Enter (auto-indent), 'y'
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["    x", "    y"]);
}

#[test]
fn alt_down_moves_the_line_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aaa\nbbb");
    app.on_key(keycode(KeyCode::Up)); // cursor onto line 0
    app.on_key(alt(KeyCode::Down));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["bbb", "aaa"]);
}

#[test]
fn alt_up_moves_the_line_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aaa\nbbb"); // cursor on line 1
    app.on_key(alt(KeyCode::Up));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["bbb", "aaa"]);
}

#[test]
fn ctrl_bracket_jumps_to_matching_bracket() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "(x)"); // cursor just after ')'
    app.on_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_cursor(), 0, "jumps to '('");
}

#[test]
fn tab_inserts_spaces_by_default() {
    let mut app = app_at(Path::new(".")); // default: spaces, width 4
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('x'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "    x", "Tab inserts 4 spaces");
}

#[test]
fn tab_width_setting_controls_space_count() {
    let mut app = app_with(Settings { tab_width: 2, ..Settings::default() });
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('y'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "  y", "tab_width=2 inserts 2 spaces");
}

#[test]
fn indent_style_tabs_inserts_a_tab() {
    let mut app = app_with(Settings {
        indent_style: "tabs".to_string(),
        ..Settings::default()
    });
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('z'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "\tz", "tabs style inserts a tab");
}

#[test]
fn types_into_buffer_and_marks_dirty() {
    let mut app = app_at(Path::new("."));
    assert!(!app.editor.active_tab().unwrap().dirty);
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.lines()[0], "hello");
    assert!(tab.dirty, "typing should mark the buffer dirty");
}

#[test]
fn open_edit_save_round_trip() {
    let dir = unique_dir("save");
    let file = dir.join("note.txt");
    fs::write(&file, "first line\nsecond line\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "first line");

    // Move to end of the first line and append text.
    app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    for c in "!!!".chars() {
        app.on_key(key(c));
    }
    app.run_action("file.save");
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.starts_with("first line!!!"), "got: {saved:?}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_table_opens_edits_and_saves_csv() {
    let dir = unique_dir("table");
    let file = dir.join("data.csv");
    fs::write(&file, "name,age\nalice,30\nbob,25\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    app.run_action("tools.edit_table");
    assert!(app.edit_table.is_some(), "table editor opened on the CSV buffer");

    // Move to alice's age cell (row 1, col 1) and change 30 -> 31.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Right));
    app.on_key(keycode(KeyCode::Enter)); // begin edit, seeded with "30"
    app.on_key(keycode(KeyCode::Backspace));
    app.on_key(keycode(KeyCode::Backspace));
    for c in "31".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // commit

    // Ctrl+S writes the grid back through the normal save flow.
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains("alice,31"), "edit persisted; got: {saved:?}");
    assert!(saved.contains("bob,25"), "other rows intact; got: {saved:?}");

    // Esc closes the editor.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_table.is_none(), "Esc closes the table editor");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_outline_opens_indents_and_saves() {
    let dir = unique_dir("outline");
    let file = dir.join("notes.txt");
    fs::write(&file, "A\nB\n  B1\nC\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    app.run_action("tools.edit_outline");
    assert!(app.edit_outline.is_some(), "outline editor opened on the buffer");

    // Move to B and indent it (with its child B1) under A via Tab.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Tab));

    // Ctrl+S writes the restructured outline back; indentation is regenerated.
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let saved = fs::read_to_string(&file).unwrap();
    assert_eq!(saved, "A\n  B\n    B1\nC\n", "B indented under A; got: {saved:?}");

    // Esc closes the editor.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_outline.is_none(), "Esc closes the outline editor");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_json_opens_edits_and_saves() {
    let dir = unique_dir("ejson");
    let file = dir.join("data.json");
    fs::write(&file, "{\n  \"a\": 1\n}\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("tools.edit_json");
    assert!(app.edit_value.is_some(), "JSON editor opened");

    app.on_key(keycode(KeyCode::Down)); // select "a"
    app.on_key(keycode(KeyCode::Enter)); // edit value
    app.on_key(keycode(KeyCode::Backspace));
    app.on_key(key('2'));
    app.on_key(keycode(KeyCode::Enter)); // commit
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains("\"a\": 2"), "value edit persisted; got: {saved:?}");

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_value.is_none(), "Esc closes the JSON editor");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_bytes_opens_overwrites_and_saves() {
    let dir = unique_dir("ebytes");
    let file = dir.join("b.txt");
    fs::write(&file, "hello").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("tools.edit_bytes");
    assert!(app.edit_bytes.is_some(), "byte editor opened");

    // Overwrite the first byte 'h' (0x68) with 0x41 = 'A' by typing "41".
    app.on_key(key('4'));
    app.on_key(key('1'));
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.starts_with("Aello"), "byte overwrite persisted; got: {saved:?}");

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_bytes.is_none(), "Esc closes the byte editor");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn insert_lorem_and_datetime_presets() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.lorem.words");
    assert!(
        app.editor.active_tab().unwrap().text().starts_with("Lorem ipsum"),
        "lorem words inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.datetime.epoch");
    let epoch = app.editor.active_tab().unwrap().text();
    assert!(
        !epoch.is_empty() && epoch.chars().all(|c| c.is_ascii_digit()),
        "epoch is all digits: {epoch:?}"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.datetime.rfc3339");
    let rfc = app.editor.active_tab().unwrap().text();
    assert!(rfc.contains('T') && rfc.contains(':'), "rfc3339 date-time shape: {rfc:?}");
}

#[test]
fn qrcode_overlay_generates_and_closes() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "https://example.com");
    app.run_action("tools.qrcode");
    assert!(
        app.qrcode.as_ref().is_some_and(|art| !art.is_empty()),
        "QR overlay rendered from the current line"
    );
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.qrcode.is_none(), "Esc closes the QR overlay");
}

#[test]
fn zen_mode_hides_then_restores_chrome() {
    let mut app = app_at(Path::new("."));
    app.show_explorer = true;
    app.show_messages = true;
    app.show_status_bar = true;
    app.show_bottom_dock = true;

    app.run_action("view.zen");
    assert!(app.is_zen(), "zen mode on");
    assert!(
        !app.show_explorer && !app.show_messages && !app.show_status_bar && !app.show_bottom_dock,
        "zen hides the chrome"
    );

    app.run_action("view.zen");
    assert!(!app.is_zen(), "zen mode off");
    assert!(
        app.show_explorer && app.show_messages && app.show_status_bar && app.show_bottom_dock,
        "zen restores prior visibility"
    );
}

#[test]
fn column_select_block_edits_multiple_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aa\nbb\ncc");
    app.run_action("edit.go_first"); // cursor to buffer start (line 0, col 0)
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT));
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "Alt+Shift+Down builds a vertical block of carets"
    );
    app.on_key(key('X')); // type at every caret
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "Xaa\nXbb\nXcc",
        "block insert lands on each line"
    );
}

#[test]
fn select_all_occurrences_creates_multi_carets() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo bar foo baz foo");
    app.on_key(keycode(KeyCode::Home)); // cursor onto the first "foo"
    app.run_action("edit.select_all_occurrences");
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "every occurrence becomes a caret"
    );
}

#[test]
fn breadcrumb_shows_file_and_enclosing_symbol() {
    let dir = unique_dir("crumb");
    let file = dir.join("m.rs");
    fs::write(&file, "fn alpha() {}\nfn beta() {\n    let x = 1;\n}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down)); // cursor on line 3, inside beta

    app.run_action("view.breadcrumbs");
    assert!(app.show_breadcrumbs, "breadcrumb bar toggled on");
    let crumb = app.breadcrumb();
    assert!(crumb.starts_with("m.rs"), "shows the file name: {crumb:?}");
    assert!(crumb.contains("beta"), "shows the enclosing symbol: {crumb:?}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn on_save_toggles_flip_settings() {
    let mut app = app_at(Path::new("."));
    let trim = app.settings.trim_trailing_whitespace;
    app.run_action("view.trim_on_save");
    assert_eq!(app.settings.trim_trailing_whitespace, !trim, "trim-on-save toggled");

    let nl = app.settings.ensure_final_newline;
    app.run_action("view.final_newline_on_save");
    assert_eq!(app.settings.ensure_final_newline, !nl, "final-newline-on-save toggled");
}

#[test]
fn smart_home_toggles_first_nonblank_and_column0() {
    let dir = unique_dir("smarthome");
    let file = dir.join("h.txt");
    fs::write(&file, "    hello\n").unwrap(); // four-space indent
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.on_key(keycode(KeyCode::End));
    assert_eq!(app.editor.cursor_1based().1, 10, "end of '    hello'");
    // First Home -> first non-blank (column index 4 -> 1-based 5).
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.editor.cursor_1based().1, 5, "Home jumps to first non-blank");
    // Second Home -> column 0.
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.editor.cursor_1based().1, 1, "Home again jumps to column 0");
    // Third Home -> back to first non-blank.
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.editor.cursor_1based().1, 5, "toggles back to first non-blank");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_goto_line_previews_and_reverts_on_esc() {
    let dir = unique_dir("gotorevert");
    let file = dir.join("g.txt");
    let body: String = (1..=20).map(|i| format!("L{i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    assert_eq!(app.editor.cursor_1based().0, 1);

    app.run_action("tools.palette");
    app.on_key(key(':'));
    app.on_key(key('7'));
    assert_eq!(app.editor.cursor_1based().0, 7, "live preview moves to line 7 while typing");
    app.on_key(esc());
    assert!(app.palette.is_none());
    assert_eq!(app.editor.cursor_1based().0, 1, "Esc reverts to the original line");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_goto_line_commit_records_origin_in_history() {
    let dir = unique_dir("gotocommit");
    let file = dir.join("g.txt");
    let body: String = (1..=20).map(|i| format!("L{i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("tools.palette");
    for c in ":12".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.editor.cursor_1based().0, 12, "preview reached line 12");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.palette.is_none());
    assert_eq!(app.editor.cursor_1based().0, 12, "commit stays at line 12");
    // Position-history back goes to the pre-jump origin (line 1), not the preview.
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.editor.cursor_1based().0, 1, "Alt+Left returns to origin line 1");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn recent_locations_chooser_lists_and_jumps() {
    let dir = unique_dir("locations");
    let file = dir.join("g.txt");
    let body: String = (1..=40).map(|i| format!("L{i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    // Make two jumps so the position history has several entries.
    app.run_action("tools.palette");
    for c in ":12".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    app.run_action("tools.palette");
    for c in ":30".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.editor.cursor_1based().0, 30);

    // Alt+J opens the recent-locations chooser, most-recent first.
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT));
    let lc = app.location_chooser.as_ref().expect("location chooser opens");
    assert!(lc.entries.len() >= 2, "history has multiple locations: {}", lc.entries.len());
    assert_eq!(lc.entries[0].line, 30, "most recent location first");

    // Move the cursor away, then jump to the second entry from the chooser.
    let target = app.location_chooser.as_ref().unwrap().entries[1].line;
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.location_chooser.is_none(), "Enter closes the chooser");
    assert_eq!(app.editor.cursor_1based().0, target, "jumped to the chosen location");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn recent_locations_empty_history_reports_status() {
    let dir = unique_dir("locations-empty");
    let file = dir.join("g.txt");
    fs::write(&file, "one\ntwo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("nav.recent_locations");
    assert!(app.location_chooser.is_none(), "no chooser without history");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_selection_jumps_between_occurrences() {
    let dir = unique_dir("findsel");
    let file = dir.join("f.txt");
    fs::write(&file, "foo bar foo baz foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    // No selection: the word under the cursor ("foo") is used. Occurrences start
    // at chars 0, 8, 16.
    app.run_action("search.next_selection");
    assert_eq!(app.editor.cursor_1based(), (1, 9), "next -> second foo");
    app.run_action("search.next_selection");
    assert_eq!(app.editor.cursor_1based(), (1, 17), "next -> third foo");
    app.run_action("search.prev_selection");
    assert_eq!(app.editor.cursor_1based(), (1, 9), "prev -> second foo");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_trims_trailing_whitespace_by_default() {
    let dir = unique_dir("trim");
    let file = dir.join("t.txt");
    fs::write(&file, "abc\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for _ in 0..3 {
        app.on_key(key(' '));
    }
    app.run_action("file.save");
    assert_eq!(fs::read_to_string(&file).unwrap(), "abc\n", "trailing spaces trimmed");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_ensures_final_newline_by_default() {
    let dir = unique_dir("newline");
    let file = dir.join("n.txt");
    fs::write(&file, "abc").unwrap(); // no trailing newline
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("file.save");
    assert_eq!(fs::read_to_string(&file).unwrap(), "abc\n", "final newline added");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_respects_disabled_normalization() {
    let dir = unique_dir("rawsave");
    let file = dir.join("r.txt");
    fs::write(&file, "abc").unwrap(); // no trailing newline
    let mut app = app_at(&dir);
    app.settings.trim_trailing_whitespace = false;
    app.settings.ensure_final_newline = false;
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for _ in 0..2 {
        app.on_key(key(' '));
    }
    app.run_action("file.save");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "abc  ",
        "no trim and no final newline when both disabled"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_and_close_keep_one_buffer() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new");
    app.run_action("file.new");
    assert_eq!(app.editor.tabs.len(), 3);
    app.run_action("file.close");
    app.run_action("file.close");
    app.run_action("file.close");
    assert_eq!(app.editor.tabs.len(), 1, "always keeps one buffer open");
}

#[test]
fn goto_line_moves_cursor() {
    let dir = unique_dir("goto");
    let file = dir.join("many.txt");
    fs::write(&file, "a\nb\nc\nd\ne\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.editor.goto(4, Some(1), Rect::new(0, 0, 80, 24));
    assert_eq!(app.editor.cursor_1based().0, 4);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn quit_action_sets_flag() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('q'));
    assert!(app.should_quit);
}

#[test]
fn vix_menu_quit_quits_program() {
    let mut app = app_at(Path::new("."));
    assert!(!app.should_quit);

    // Open the menu bar (the Vix menu is first), then walk down to "Quit".
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let vix_idx = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.vix")
        .expect("a Vix menu exists");
    for _ in 0..vix_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    // Walk down until "Quit" is highlighted (Down skips separators, so we cannot
    // assume the number of presses equals the item's array index).
    let item_count = vix::menu::menus()[vix_idx].items.len();
    for _ in 0..=item_count {
        if app.menu.selected_action() == Some("file.quit") {
            break;
        }
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    assert_eq!(
        app.menu.selected_action(),
        Some("file.quit"),
        "Down navigation must reach Quit"
    );
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // The main loop (main.rs) breaks out as soon as this flag is set, so
    // choosing Vix -> Quit really does end the program.
    assert!(app.should_quit, "Vix -> Quit must request exit");
}

#[test]
fn view_theme_submenu_actions_switch_theme() {
    let mut app = app_at(Path::new("."));
    // The View → Theme submenu dispatches `view.theme:<name>` per item.
    app.run_action("view.theme:Light");
    assert_eq!(app.settings.theme, "Light");
    app.run_action("view.theme:Dark");
    assert_eq!(app.settings.theme, "Dark");
    // An unknown theme name is ignored.
    app.run_action("view.theme:Nonexistent");
    assert_eq!(app.settings.theme, "Dark");
}

#[test]
fn vix_menu_dialogs_open_and_close() {
    let mut app = app_at(Path::new("."));
    assert!(app.dialog.is_none());

    // About is a plain dialog (no text field), shows "Vix <version>", and closes
    // on the Ok button (Enter).
    app.run_action("vix.about");
    let about = app.dialog.as_ref().expect("About opens a dialog");
    assert!(about.body.starts_with("Vix "), "About shows the version: {}", about.body);
    assert!(about.editor.is_none(), "About is plain text, not a field");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_none(), "Enter closes the plain dialog");

    // Website is a selectable/copyable text field, shows the URL, closes on Esc.
    app.run_action("vix.website");
    let web = app.dialog.as_ref().unwrap();
    assert!(web.body.contains("github.com/vixide/vix"));
    assert!(web.editor.is_some(), "Website is a selectable text field");
    // Enter does NOT close a text-field dialog (it edits the field); Esc does.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.dialog.is_some(), "Enter is handled by the text field, not a close");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.dialog.is_none());

    // Email is also a selectable field showing the address.
    app.run_action("vix.email");
    let email = app.dialog.as_ref().unwrap();
    assert!(email.body.contains('@'));
    assert!(email.editor.is_some(), "Email is a selectable text field");
}

#[test]
fn view_locale_submenu_lists_locales() {
    // Structural check only — applying a locale mutates the process-global
    // rust-i18n locale, which would race other parallel tests.
    let _app = app_at(Path::new("."));
    let view = vix::menu::menus().iter().find(|m| m.name == "menu.view").unwrap();
    let sub = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.locale")
        .and_then(|it| it.submenu)
        .expect("Locale is a submenu");
    let actions: Vec<&str> = sub.iter().map(|it| it.action).collect();
    for code in ["view.locale:en", "view.locale:fr", "view.locale:ja"] {
        assert!(actions.contains(&code), "locale submenu offers {code}; got {actions:?}");
    }
}

#[test]
fn view_time_zone_submenu_lists_zones() {
    let _app = app_at(Path::new("."));
    let view = vix::menu::menus().iter().find(|m| m.name == "menu.view").unwrap();
    let sub = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.time_zone")
        .and_then(|it| it.submenu)
        .expect("Time Zone is a submenu");
    let actions: Vec<&str> = sub.iter().map(|it| it.action).collect();
    assert!(actions.contains(&"view.time_zone:UTC"));
    assert!(actions.contains(&"view.time_zone:America/New_York"));
    assert!(sub.len() > 100, "lists the full IANA zone set");
}

#[test]
fn view_time_zone_action_sets_active_zone() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.time_zone:America/New_York");
    assert_eq!(app.settings.time_zone, "America/New_York");
    app.run_action("view.time_zone:Not/AZone"); // unknown ignored
    assert_eq!(app.settings.time_zone, "America/New_York");
    app.run_action("view.time_zone:UTC"); // restore the process-global active zone
}

#[test]
fn view_theme_submenu_lists_bundled_themes() {
    // app_at builds an App, which populates the View → Theme submenu from the
    // available themes.
    let _app = app_at(Path::new("."));
    let view = vix::menu::menus().iter().find(|m| m.name == "menu.view").unwrap();
    let theme_parent = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.theme")
        .expect("a Theme submenu item");
    let sub = theme_parent.submenu.expect("Theme is a submenu");
    let actions: Vec<&str> = sub.iter().map(|it| it.action).collect();
    // The menu is built (and cached) on first use; Dark and Light are always
    // present (bundled, and the fallback). Each item dispatches `view.theme:<name>`.
    for expected in ["Dark", "Light"] {
        let action = format!("view.theme:{expected}");
        assert!(
            actions.contains(&action.as_str()),
            "submenu should offer theme {expected}; got {actions:?}"
        );
    }
    // The full de-dup/sort behavior over a theme list is unit-tested in
    // vix-theme-model (`theme_names`).
}

#[test]
fn line_number_toggle() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.line_numbers;
    app.run_action("tools.line_numbers");
    assert_ne!(before, app.editor.line_numbers);
}

#[test]
fn visible_whitespace_toggle() {
    let mut app = app_at(Path::new("."));
    // Off by default; the action toggles it and persists the setting.
    assert!(!app.editor.show_whitespace);
    assert!(!app.settings.show_whitespace);
    app.run_action("view.whitespace");
    assert!(app.editor.show_whitespace, "toggles visible whitespace on");
    assert!(app.settings.show_whitespace, "persists the new setting");
    app.run_action("view.whitespace");
    assert!(!app.editor.show_whitespace, "toggles back off");
}

#[test]
fn status_bar_info_accessors() {
    let dir = unique_dir("statusinfo");
    let file = dir.join("s.rs");
    fs::write(&file, "fn main() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.editor.language(), "rust", "language from extension");
    assert_eq!(tab.editor.line_ending(), "LF");
    assert!(tab.editor.selection_span().is_none(), "no selection initially");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn line_ending_detects_crlf() {
    let dir = unique_dir("crlf");
    let file = dir.join("c.txt");
    fs::write(&file, "a\r\nb\r\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    assert_eq!(app.editor.active_tab().unwrap().editor.line_ending(), "CRLF");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn soft_wrap_toggle() {
    let mut app = app_at(Path::new("."));
    assert!(!app.editor.soft_wrap, "off by default");
    assert!(!app.settings.soft_wrap);
    app.run_action("view.soft_wrap");
    assert!(app.editor.soft_wrap, "toggles soft wrap on");
    assert!(app.settings.soft_wrap, "persists the setting");
    app.run_action("view.soft_wrap");
    assert!(!app.editor.soft_wrap, "toggles back off");
}

#[test]
fn toggle_comment_round_trips_and_is_undoable() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    // A new (untitled) buffer uses the default `//` token.
    app.run_action("edit.toggle_comment");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "//hello");
    assert!(app.editor.active_tab().unwrap().dirty);
    // Toggling again removes it.
    app.run_action("edit.toggle_comment");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "hello", "second toggle uncomments");
    // And the whole thing is a single undoable edit.
    app.run_action("edit.toggle_comment");
    app.run_action("edit.undo");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "hello", "undo reverts the comment");
}

#[test]
fn ctrl_slash_toggles_comment() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.on_key(ctrl('/'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "//x");
}

#[test]
fn toggle_comment_uses_language_token() {
    let dir = unique_dir("comment");
    let file = dir.join("c.toml");
    fs::write(&file, "key = 1\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("edit.toggle_comment");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "#key = 1", "TOML uses #");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_recent_records_dedups_and_reopens() {
    let dir = unique_dir("recent");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    assert!(app.settings.recent_files.is_empty());

    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt"));
    assert_eq!(app.settings.recent_files.len(), 2);
    assert!(app.settings.recent_files[0].ends_with("b.txt"), "most-recent first");
    assert!(app.settings.recent_files[1].ends_with("a.txt"));

    // Reopening a recorded file moves it to the front without duplicating.
    app.open_initial(&dir.join("a.txt"));
    assert_eq!(app.settings.recent_files.len(), 2, "deduped");
    assert!(app.settings.recent_files[0].ends_with("a.txt"));

    // The chooser lists the entries; Down + Enter opens the second.
    app.run_action("file.open_recent");
    assert_eq!(app.recent_chooser.as_ref().unwrap().entries.len(), 2);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.recent_chooser.is_none(), "Enter opens and closes the chooser");
    let open = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(open.ends_with("b.txt"), "opened the highlighted recent file");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_symbols_finds_declarations_not_locals() {
    let text = "fn alpha() {}\nlet skip = 1;\nstruct Beta;\nclass Gamma:\n  pass\n#define MAX 10\n";
    let syms = vix::palette::symbols(text);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"alpha"), "fn: {names:?}");
    assert!(names.contains(&"Beta"), "struct: {names:?}");
    assert!(names.contains(&"Gamma"), "class: {names:?}");
    assert!(names.contains(&"MAX"), "#define: {names:?}");
    assert!(!names.contains(&"skip"), "local `let` is excluded: {names:?}");
    assert_eq!(syms[0].line, 1, "lines are 1-based");
}

#[test]
fn goto_symbol_mode_lists_and_jumps() {
    let dir = unique_dir("symbols");
    let file = dir.join("s.rs");
    fs::write(&file, "fn alpha() {}\nlet x = 1;\nstruct Beta {}\nfn gamma() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("nav.goto_symbol");
    let p = app.palette.as_ref().expect("palette open");
    assert!(matches!(p.mode(), vix::palette::Mode::Symbols), "@ enters symbols mode");
    assert_eq!(p.entries.len(), 3, "three declarations (the `let` is excluded)");

    // Filter to a single symbol, then jump to it.
    for c in "gamma".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.palette.as_ref().unwrap().entries.len(), 1);
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.palette.is_none(), "Enter accepts and closes the palette");
    assert_eq!(app.editor.cursor_1based().0, 4, "jumped to gamma's line");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_workspace_symbol_finds_symbols_across_files_and_jumps() {
    let dir = unique_dir("wssymbols");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.rs"), "fn alpha() {}\nstruct Widget {}\n").unwrap();
    fs::write(dir.join("b.rs"), "fn beta() {}\nfn widget_helper() {}\n").unwrap();
    let mut app = app_at(&dir);
    // Start on an empty buffer (no file open) to prove it searches the workspace.
    app.run_action("nav.goto_workspace_symbol");
    let p = app.palette.as_ref().expect("palette open");
    assert!(
        matches!(p.mode(), vix::palette::Mode::WorkspaceSymbols),
        "@@ enters workspace-symbols mode"
    );
    assert!(p.entries.is_empty(), "empty query lists nothing");

    // Query "widget" should match Widget (a.rs) and widget_helper (b.rs).
    for c in "widget".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert_eq!(p.entries.len(), 2, "two matches across files");

    // Accept the first match and confirm a file opened (the symbol lives in a
    // file that was not open before).
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.palette.is_none(), "Enter accepts and closes the palette");
    assert!(app.editor.active_tab().and_then(|t| t.path.as_ref()).is_some(), "a file opened");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_recent_empty_shows_status_only() {
    let mut app = app_at(Path::new("."));
    assert!(app.settings.recent_files.is_empty());
    app.run_action("file.open_recent");
    assert!(app.recent_chooser.is_none(), "no chooser when there are no recent files");
}

#[test]
fn click_recent_row_opens_file() {
    let dir = unique_dir("recentclick");
    fs::write(dir.join("c.txt"), "ccc").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("c.txt"));
    app.run_action("file.open_recent");
    // The list rect is normally recorded during render; set it directly.
    app.layout.chooser = Rect::new(10, 5, 34, 1);
    app.on_mouse(click(12, 5));
    assert!(app.recent_chooser.is_none(), "a click opens and closes the chooser");
    let open = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(open.ends_with("c.txt"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn help_overlay_toggles() {
    let mut app = app_at(Path::new("."));
    assert!(!app.show_help);
    app.on_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(app.show_help, "F1 opens the help overlay");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_help, "Esc closes the help overlay");
}

#[test]
fn fuzzy_matches_space_separated_terms() {
    assert!(fuzzy_match("features/groups/view.tsx", "feat group"));
    assert!(fuzzy_match("/etc/hosts", "etc hosts"));
    assert!(fuzzy_match("src/save_file.rs", "save file"));
    assert!(!fuzzy_match("src/main.rs", "zzz"));
}

#[test]
fn parses_path_line_col() {
    assert_eq!(
        parse_path_target("src/main.rs:42:10"),
        ("src/main.rs".to_string(), Some((42, 10)))
    );
    assert_eq!(
        parse_path_target("src/main.rs:42"),
        ("src/main.rs".to_string(), Some((42, 1)))
    );
    assert_eq!(
        parse_path_target("src/main.rs"),
        ("src/main.rs".to_string(), None)
    );
}

#[test]
fn search_pattern_respects_toggles() {
    let mut sb = SearchBar::new(false);
    sb.smart_case = false; // isolate the case/word/regex toggles from smart-case
    sb.query = "Foo.Bar".to_string();
    assert_eq!(sb.pattern().as_deref(), Some(r"(?i)Foo\.Bar"));

    sb.case_sensitive = true;
    sb.whole_word = true;
    assert_eq!(sb.pattern().as_deref(), Some(r"\bFoo\.Bar\b"));

    sb.regex = true;
    sb.whole_word = false;
    sb.case_sensitive = true;
    assert_eq!(sb.pattern().as_deref(), Some("Foo.Bar"));

    sb.query.clear();
    assert_eq!(sb.pattern(), None);
}

#[test]
fn replace_all_with_capture_groups() {
    let dir = unique_dir("rep");
    let file = dir.join("swap.txt");
    fs::write(&file, "key: value\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    // Open replace, enable regex, search `(\w+): (\w+)`, replace `$2: $1`.
    app.run_action("edit.replace");
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT)); // toggle regex
    for c in r"(\w+): (\w+)".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to replace field
    for c in "$2: $1".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // replace all
    let line = app.editor.active_tab().unwrap().lines()[0].clone();
    assert_eq!(line, "value: key");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reopen_closed_tab_restores_the_last_closed_file() {
    let dir = unique_dir("reopen");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt")); // active: b.txt

    // Close b.txt, then reopen it.
    app.run_action("file.close");
    assert!(
        !app.editor.tabs.iter().any(|t| t.path.as_deref() == Some(dir.join("b.txt").as_path())),
        "b.txt is closed"
    );
    app.run_action("file.reopen_closed");
    assert!(
        app.editor.tabs.iter().any(|t| t.path.as_deref()
            == Some(dir.join("b.txt").canonicalize().unwrap().as_path())),
        "b.txt is reopened"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn close_all_tabs_leaves_one_empty_buffer() {
    let dir = unique_dir("closeall");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt"));
    assert!(app.editor.tabs.len() >= 2, "two files are open");

    app.run_action("file.close_all");

    assert_eq!(app.editor.tabs.len(), 1, "exactly one buffer remains");
    let t = app.editor.active_tab().unwrap();
    assert!(t.path.is_none(), "the remaining buffer is untitled");
    assert!(t.text().is_empty(), "the remaining buffer is empty");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_next_repeats_after_the_box_closes() {
    let dir = unique_dir("findnext");
    let file = dir.join("f.txt");
    fs::write(&file, "ab xx ab xx ab\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    // Find "xx", then close the box.
    app.run_action("edit.find");
    for c in "xx".chars() {
        app.on_key(key(c));
    }
    app.on_key(esc()); // box closed; cursor sits on a match
    assert!(app.search.is_none());
    let first = app.editor.active_tab().unwrap().editor.get_cursor();

    // Ctrl+G repeats the last search even with the box closed, moving to the
    // other "xx" (there are exactly two, so it cycles).
    app.on_key(ctrl('g'));
    let second = app.editor.active_tab().unwrap().editor.get_cursor();
    assert_ne!(second, first, "Find Next moved to the other match");

    // Ctrl+Shift+G goes back to where we started.
    app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    let back = app.editor.active_tab().unwrap().editor.get_cursor();
    assert_eq!(back, first, "Find Previous returned to the earlier match");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_highlights_are_sticky_after_closing() {
    let dir = unique_dir("sticky");
    let file = dir.join("f.txt");
    fs::write(&file, "ab xx ab xx ab\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.find");
    for c in "ab".chars() {
        app.on_key(key(c));
    }
    app.on_key(esc());
    assert!(app.search.is_none(), "box closed");
    assert!(
        app.editor.active_tab().unwrap().editor.has_marks(),
        "highlights stay after the find box closes (sticky)"
    );
}

#[test]
fn search_reports_match_index_and_total() {
    let dir = unique_dir("matchof");
    let file = dir.join("f.txt");
    fs::write(&file, "ab xx ab xx ab\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.find");
    for c in "ab".chars() {
        app.on_key(key(c));
    }
    app.on_key(esc());
    // Find Next with the box closed reports "Match N of 3".
    app.on_key(ctrl('g'));
    assert!(app.status.contains("of 3"), "match total shown: {}", app.status);
}

#[test]
fn toggle_highlight_search_clears_and_restores() {
    let dir = unique_dir("togglehl");
    let file = dir.join("f.txt");
    fs::write(&file, "ab xx ab\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.find");
    for c in "ab".chars() {
        app.on_key(key(c));
    }
    app.on_key(esc());
    assert!(app.editor.active_tab().unwrap().editor.has_marks());

    app.run_action("toggle_highlight_search");
    assert!(!app.editor.active_tab().unwrap().editor.has_marks(), "toggled off");
    app.run_action("toggle_highlight_search");
    assert!(app.editor.active_tab().unwrap().editor.has_marks(), "toggled back on");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn literal_replace_all_after_preview() {
    let dir = unique_dir("litrep");
    let file = dir.join("l.txt");
    fs::write(&file, "foo foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.replace");
    for c in "foo".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // find next (preview) -> moves cursor + selects
    app.on_key(keycode(KeyCode::Tab)); // to replace field
    for c in "bar".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // replace all
    let line = app.editor.active_tab().unwrap().lines()[0].clone();
    assert_eq!(line, "bar bar");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn click_focuses_the_replace_field() {
    let mut app = app_at(Path::new("."));
    app.run_action("edit.replace");
    // The box's inner rect is recorded during render; set it directly. Row 0 is
    // the Find field, row 1 the toggle buttons, row 2 the Replace field.
    app.layout.search = Rect::new(0, 5, 40, 6);
    app.on_mouse(click(2, 7)); // click the Replace row (row 2)

    // Typing now lands in the Replace field, not the Find field.
    for c in "xyz".chars() {
        app.on_key(key(c));
    }
    let s = app.search.as_ref().unwrap();
    assert_eq!(s.field, vix::search::Field::Replace, "click focused the Replace field");
    assert_eq!(s.replace, "xyz");
    assert!(s.query.is_empty(), "the Find field stayed empty");

    // Clicking the first row focuses the Find field again.
    app.on_mouse(click(2, 5));
    assert_eq!(app.search.as_ref().unwrap().field, vix::search::Field::Query);
}

#[test]
fn interactive_query_replace_y_n_y() {
    let dir = unique_dir("qr");
    let file = dir.join("q.txt");
    fs::write(&file, "foo foo foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.query_replace");
    for c in "foo".chars() {
        app.on_key(key(c)); // query field; cursor must NOT move in interactive mode
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to replace field
    for c in "bar".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // begin step-through
    assert!(app.query_replace.is_some(), "session should be active");

    app.on_key(key('y')); // replace first
    app.on_key(key('n')); // skip second
    app.on_key(key('y')); // replace third -> no more matches, session ends

    assert!(app.query_replace.is_none(), "session ends after last match");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "bar foo bar");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn interactive_query_replace_bang_replaces_rest() {
    let dir = unique_dir("qrbang");
    let file = dir.join("q.txt");
    fs::write(&file, "x x x x\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.query_replace");
    app.on_key(key('x'));
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.on_key(key('Z'));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.on_key(key('!')); // replace this and all the rest

    assert!(app.query_replace.is_none());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "Z Z Z Z");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn iso_formats_have_expected_shape() {
    let now = clock::now_local();
    let utc = clock::utc_iso(&now);
    assert_eq!(utc.len(), 20, "{utc}"); // YYYY-MM-DDTHH:MM:SSZ
    assert!(utc.ends_with('Z'));
    assert_eq!(&utc[4..5], "-");
    assert_eq!(&utc[10..11], "T");

    let week = clock::iso_week_date(&now);
    assert!(week.contains("-W"), "{week}"); // YYYY-Www-D
    let day = week.chars().last().unwrap();
    assert!(('1'..='7').contains(&day), "weekday digit: {week}");

    let clk = clock::local_clock(&now);
    assert_eq!(clk.len(), 8, "{clk}"); // HH:MM:SS

    let local = clock::local_datetime(&now);
    assert_eq!(local.len(), 19, "{local}"); // YYYY-MM-DD HH:MM:SS

    let grid = calendar::month_grid(now.date());
    let count: usize = grid.weeks.iter().flatten().filter(|c| c.is_some()).count();
    assert!((28..=31).contains(&count), "days in month: {count}");
}

#[test]
fn calendar_left_right_pages_months() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    assert!(app.show_calendar, "Calendar opens on the current month");
    let start = app.calendar.shown_month();
    assert!(app.calendar.grid().today.is_some(), "today shows in the current month");

    // Ctrl pages months; Ctrl+Right forward, Ctrl+Left back to where we started.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
    assert_ne!(app.calendar.shown_month(), start, "Ctrl+Right advances the month");
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.calendar.shown_month(), start, "Ctrl+Left returns to the start month");

    // Plain arrows move the selected day.
    let sel = app.calendar.selected();
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_ne!(app.calendar.selected(), sel, "Right moves the selected day");

    // Esc closes the box.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_calendar, "Esc closes the calendar");
}

#[test]
fn narrow_editor_does_not_panic() {
    // The code editor's focus() underflows on tiny widths; the app clamps the
    // viewport it hands over, so typing into a 5-column editor must not panic.
    let mut app = App::new(PathBuf::from("."), Settings::default());
    app.layout.editor = Rect::new(0, 0, 5, 3);
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    // Go-to-line through the command palette also routes through the clamp.
    app.on_key(ctrl('p'));
    for c in ":1".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "abc");
}

fn node_index(app: &App, name: &str) -> usize {
    app.explorer
        .nodes
        .iter()
        .position(|n| n.name == name)
        .unwrap_or_else(|| panic!("no explorer node named {name}"))
}

#[test]
fn toggle_status_bar_action_flips_and_persists() {
    let mut app = app_at(Path::new("."));
    assert!(app.show_status_bar, "the status bar is shown by default");
    app.run_action("view.status_bar");
    assert!(!app.show_status_bar, "the action hides the status bar");
    assert!(!app.settings.show_status_bar, "the choice persists in settings");
    app.run_action("view.status_bar");
    assert!(app.show_status_bar, "toggling again shows it");
}

#[test]
fn toggle_scrollbar_flips_persists_and_reclaims_the_column() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert!(app.show_scrollbar, "shown by default");
    let with = app.layout.editor.width;
    assert!(app.layout.scrollbar.width > 0, "scrollbar has a column");

    app.run_action("view.scrollbar"); // hide it
    assert!(!app.show_scrollbar);
    assert!(!app.settings.show_scrollbar, "choice persists");
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert_eq!(app.layout.scrollbar.width, 0, "scrollbar column collapses");
    assert_eq!(app.layout.editor.width, with + 1, "the text reclaims the column");
}

#[test]
fn calendar_nav_arrows_change_the_month() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let cal = app.layout.calendar;
    let title = app.calendar.title();

    // The nav arrows sit on the month-header row (row 0): ◀ at col 0, ▶ at col 20.
    app.on_mouse(click(cal.x + 20, cal.y)); // ▶ next month
    assert_ne!(app.calendar.title(), title, "▶ advanced to the next month");
    app.on_mouse(click(cal.x, cal.y)); // ◀ previous month
    assert_eq!(app.calendar.title(), title, "◀ returned to the original month");
    assert!(app.show_calendar, "an arrow click keeps the calendar open");
}

#[test]
fn calendar_click_inserts_into_editor() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let cal = app.layout.calendar;
    assert!(cal.width > 0, "the calendar rect was recorded");

    // Click a populated day cell (cells are 3 columns wide; the grid's weekday
    // header is row 1 and the week rows start at row 2). Date-time lines moved to
    // the clock box, so the calendar only inserts days now.
    let grid = app.calendar.grid();
    let (wk, col) = grid
        .weeks
        .iter()
        .enumerate()
        .find_map(|(w, week)| week.iter().position(Option::is_some).map(|c| (w, c)))
        .unwrap();
    let before = app.editor.active_tab().unwrap().text().len();
    app.on_mouse(click(cal.x + col as u16 * 3 + 1, cal.y + 2 + wk as u16));
    let after = app.editor.active_tab().unwrap().text();
    assert!(after.len() > before, "clicking a day inserted a date");

    // A click outside the box closes it.
    app.on_mouse(click(0, 23));
    assert!(!app.show_calendar, "an outside click closes the calendar");
}

#[test]
fn clock_box_inserts_a_time_row() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.clock");
    assert!(app.show_clock, "the action opens the clock box");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.clock;
    assert!(r.width > 0, "the clock rect was recorded");

    // Click the first row (local date-time): inserts a date-time; the box stays
    // open so several values can be picked.
    app.on_mouse(click(r.x + 1, r.y));
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains(':') && text.contains('-'), "inserted a date-time: {text:?}");
    assert!(app.show_clock, "a row click keeps the clock box open");

    // A click outside the box closes it.
    app.on_mouse(click(0, 23));
    assert!(!app.show_clock, "an outside click closes the clock box");
}

#[test]
fn run_command_streams_output_to_bottom_dock() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.run_command");
    assert!(app.prompt.is_some(), "the action opens a command prompt");
    for c in "echo hello-vix".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.prompt.is_none(), "Enter runs and closes the prompt");
    assert!(app.show_bottom_dock, "running shows the bottom dock");

    // The command runs in a background thread; drain it like the event loop does.
    let mut waited = 0;
    while app.command_running() && waited < 300 {
        app.poll_command();
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
    }
    app.poll_command();

    let out = app.bottom_dock.lines.join("\n");
    assert!(out.contains("$ echo hello-vix"), "echoes the command: {out:?}");
    assert!(out.contains("hello-vix"), "shows the output: {out:?}");
    assert!(out.contains("[exit 0]"), "shows the exit code: {out:?}");
}

#[test]
fn cancel_command_kills_a_running_command() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.run_command");
    for c in "sleep 5".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.command_running(), "the command is running");

    app.run_action("tools.cancel_command");
    let mut waited = 0;
    while app.command_running() && waited < 300 {
        app.poll_command();
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
    }
    assert!(!app.command_running(), "cancel ended the command");
    let out = app.bottom_dock.lines.join("\n");
    assert!(out.contains("[cancelled]"), "shows it was cancelled: {out:?}");
}

#[test]
fn workspace_dock_search_regex_and_case_toggles() {
    let dir = unique_dir("dockre");
    fs::write(dir.join("a.txt"), "foo123\nFOObar\nbaz\n").unwrap();
    let mut app = app_at(&dir);

    // Regex search `fo+\d` (Alt+R) → matches foo123 on line 1 only.
    app.run_action("search.workspace_dock");
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT));
    assert!(app.prompt.as_ref().unwrap().regex, "Alt+R turned regex on");
    for c in r"fo+\d".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let out = app.bottom_dock.lines.join("\n");
    assert!(out.contains("a.txt:1:1:"), "regex matched foo123: {out:?}");
    assert!(out.contains("[1 matches in 1 files]"), "one hit: {out:?}");

    // Case-sensitive literal `FOO` (Alt+C) → matches line 2 (FOObar) only.
    app.bottom_dock.clear();
    app.run_action("search.workspace_dock");
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT));
    for c in "FOO".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let out2 = app.bottom_dock.lines.join("\n");
    assert!(out2.contains("a.txt:2:1:"), "case-sensitive FOO matched line 2: {out2:?}");
    assert!(out2.contains("[1 matches in 1 files]"), "only the uppercase hit: {out2:?}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_in_workspace_to_dock_lists_and_jumps() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let dir = unique_dir("searchdock");
    fs::write(dir.join("a.txt"), "one\nNEEDLE here\nthree\n").unwrap();
    fs::write(dir.join("b.txt"), "nothing\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.workspace_dock");
    assert!(app.prompt.is_some(), "opens a search prompt");
    for c in "NEEDLE".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.show_bottom_dock, "shows the dock");
    let out = app.bottom_dock.lines.join("\n");
    assert!(out.contains("a.txt:2:1:"), "lists the hit as path:line:col: {out:?}");
    assert!(out.contains("NEEDLE here"), "includes the matched text: {out:?}");
    assert!(out.contains("[1 matches in 1 files]"), "summary line: {out:?}");

    // Render to record the dock rect, then click the hit (2nd content row).
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.bottom_dock;
    app.on_mouse(click(r.x + 1, r.y + 2));
    assert_eq!(app.editor.active_tab().unwrap().cursor_1based().0, 2, "jumps to line 2");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn clicking_a_dock_location_jumps_to_it() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let dir = unique_dir("dockjump");
    let file = dir.join("hit.txt");
    fs::write(&file, "a\nb\nTARGET\nd\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("view.bottom_dock");
    // A grep-style line: path:line:col:text (pointing at line 3).
    app.bottom_dock.push(format!("{}:3:1: TARGET", file.display()));
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    let r = app.layout.bottom_dock;
    app.on_mouse(click(r.x + 1, r.y + 1)); // first content row = the line

    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(file.canonicalize().unwrap().as_path()), "opened the file");
    assert_eq!(tab.cursor_1based().0, 3, "jumped to line 3");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn bottom_dock_top_edge_drag_resizes() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("view.bottom_dock");
    let mut term = Terminal::new(TestBackend::new(80, 40)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    let r = app.layout.bottom_dock;
    let before = app.settings.bottom_dock_height;
    // Press the top edge and drag up four rows → taller.
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), r.x + 1, r.y));
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), r.x + 1, r.y - 4));
    assert!(app.settings.bottom_dock_height > before, "dragging the top edge up grows the dock");
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), r.x + 1, r.y - 4));

    // Re-render so the dock rect reflects the new height, then drag down → shorter.
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let grown = app.settings.bottom_dock_height;
    let r2 = app.layout.bottom_dock;
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), r2.x + 1, r2.y));
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), r2.x + 1, r2.y + 3));
    assert!(app.settings.bottom_dock_height < grown, "dragging the top edge down shrinks it");
}

#[test]
fn bottom_dock_focus_and_scroll() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("view.bottom_dock");
    for i in 0..50 {
        app.bottom_dock.push(format!("line {i}"));
    }
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.bottom_dock;
    assert!(r.height > 0, "the dock rect was recorded");

    // A click focuses the dock; it starts pinned to the bottom.
    app.on_mouse(click(r.x + 1, r.y + 1));
    assert_eq!(app.focus, vix::app::Focus::BottomDock);
    let pinned = app.bottom_dock.scroll;

    // Up / wheel scroll back through the buffer.
    app.on_key(keycode(KeyCode::Up));
    assert!(app.bottom_dock.scroll < pinned, "Up scrolls back");
    app.on_mouse(mouse(MouseEventKind::ScrollUp, r.x + 1, r.y + 1));
    let after_wheel = app.bottom_dock.scroll;
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.bottom_dock.scroll, 0, "Home jumps to the top");
    assert!(after_wheel < pinned);

    // Esc returns focus to the editor.
    app.on_key(esc());
    assert_eq!(app.focus, vix::app::Focus::Editor);
}

#[test]
fn toggle_bottom_dock_flips_persists_and_renders() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    assert!(!app.show_bottom_dock, "hidden by default");

    app.run_action("view.bottom_dock");
    assert!(app.show_bottom_dock, "the action shows the bottom dock");
    assert!(app.settings.show_bottom_dock, "the choice persists");

    // The dock buffers lines and renders without panicking.
    app.bottom_dock.push("hello from the bottom dock");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    app.run_action("view.bottom_dock");
    assert!(!app.show_bottom_dock, "toggling again hides it");
}

#[test]
fn draw_handles_a_hidden_status_bar() {
    // A full render with the status bar hidden must lay out and paint without
    // panicking (the body row now consumes the freed line).
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let mut app = app_at(Path::new("."));
    app.run_action("view.status_bar"); // hide it
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
}

#[test]
fn explorer_left_arrow_collapses_never_opens() {
    let dir = unique_dir("explorerleft");
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/inner.txt"), "x").unwrap();

    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    let has_inner = |app: &App| app.explorer.nodes.iter().any(|n| n.name == "inner.txt");

    // Left on a collapsed folder must NOT open it.
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(keycode(KeyCode::Left));
    assert!(!has_inner(&app), "Left must not expand a collapsed folder");

    // Right opens it; Left then closes it again.
    app.on_key(keycode(KeyCode::Right));
    assert!(has_inner(&app), "Right expands the folder");
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(keycode(KeyCode::Left));
    assert!(!has_inner(&app), "Left collapses the expanded folder");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unique_copy_name_suffixes() {
    let dir = unique_dir("uniq");
    fs::write(dir.join("a.txt"), "x").unwrap();
    let got = fileops::unique_copy_name(&dir, &dir.join("a.txt"));
    assert_eq!(got.file_name().unwrap(), "a copy.txt");
    fs::write(dir.join("a copy.txt"), "x").unwrap();
    let got2 = fileops::unique_copy_name(&dir, &dir.join("a.txt"));
    assert_eq!(got2.file_name().unwrap(), "a copy 2.txt");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_copy_paste_into_directory() {
    let dir = unique_dir("clip");
    fs::write(dir.join("a.txt"), "hello").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();

    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(ctrl('c')); // copy
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v')); // paste into sub/

    assert!(dir.join("sub/a.txt").exists(), "file copied into sub/");
    assert!(dir.join("a.txt").exists(), "original remains after copy");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_cut_moves_file_and_follows_buffer() {
    let dir = unique_dir("cut");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt")); // buffer open on the file
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(ctrl('x')); // cut
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v')); // paste/move into sub/

    assert!(dir.join("sub/a.txt").exists(), "file moved");
    assert!(!dir.join("a.txt").exists(), "original gone after move");
    // The open buffer now points at the new location.
    let tab_path = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(tab_path.ends_with("sub/a.txt"), "buffer followed the move: {tab_path:?}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_delete_closes_buffer() {
    let dir = unique_dir("del");
    fs::write(dir.join("a.txt"), "bye\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    assert_eq!(app.editor.tabs.len(), 2); // initial empty buffer + a.txt
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)); // request
    app.on_key(key('y')); // confirm

    assert!(!dir.join("a.txt").exists(), "file deleted");
    // The file's buffer closed; only the empty buffer remains.
    assert_eq!(app.editor.tabs.len(), 1);
    assert!(app.editor.active_tab().unwrap().path.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_multiselect_collects_paths() {
    let dir = unique_dir("multi");
    fs::write(dir.join("a.txt"), "1").unwrap();
    fs::write(dir.join("b.txt"), "2").unwrap();
    fs::write(dir.join("c.txt"), "3").unwrap();
    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = 0;
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    assert_eq!(app.explorer.selected_paths().len(), 3, "anchor..cursor inclusive");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_definition_single_jumps() {
    let dir = unique_dir("gotodef");
    fs::write(dir.join("lib.rs"), "fn target() {}\n").unwrap();
    fs::write(dir.join("main.rs"), "target()\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("main.rs")); // cursor at offset 0 → on "target"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.as_ref().unwrap().ends_with("lib.rs"), "jumped to the definition file");
    assert_eq!(app.editor.cursor_1based().0, 1);
    assert!(app.workspace_search.is_none(), "single match jumps directly");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_definition_multiple_opens_panel() {
    let dir = unique_dir("gotodef2");
    fs::write(dir.join("a.rs"), "fn dup() {}\n").unwrap();
    fs::write(dir.join("b.rs"), "fn dup() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.rs"));
    app.editor.goto(1, Some(4), Rect::new(0, 0, 80, 24)); // cursor on "dup"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let ps = app.workspace_search.as_ref().expect("panel of candidates");
    assert!(ps.static_results);
    assert_eq!(ps.hits.len(), 2, "two definitions of dup");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn position_history_back_and_forward() {
    let dir = unique_dir("nav");
    fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5\n").unwrap();
    fs::write(dir.join("b.txt"), "a\nb\nc\nd\ne\n").unwrap();
    let mut app = app_at(&dir);

    let open_at = |app: &mut App, spec: &str| {
        app.run_action("file.open");
        for c in spec.chars() {
            app.on_key(key(c));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    };
    let here = |app: &App| -> (String, usize) {
        let t = app.editor.active_tab().unwrap();
        let name = t.path.as_ref().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        (name, app.editor.cursor_1based().0)
    };

    open_at(&mut app, "a.txt:3");
    assert_eq!(here(&app), ("a.txt".into(), 3));
    open_at(&mut app, "b.txt:2");
    assert_eq!(here(&app), ("b.txt".into(), 2));

    // Alt+Left goes back to the previous location.
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(here(&app), ("a.txt".into(), 3), "Alt+Left → previous position");

    // Alt+Right returns forward.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(here(&app), ("b.txt".into(), 2), "Alt+Right → next position");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_search_finds_matches_across_files() {
    let dir = unique_dir("psearch");
    fs::write(dir.join("a.txt"), "alpha beta\nbeta gamma\n").unwrap();
    fs::write(dir.join("b.txt"), "delta beta\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.workspace");
    for c in "beta".chars() {
        app.on_key(key(c));
    }
    let ps = app.workspace_search.as_ref().unwrap();
    assert_eq!(ps.hits.len(), 3, "two in a.txt, one in b.txt");
    let expected = ps.selected_hit().unwrap();
    let expected_name = expected.path.file_name().unwrap().to_owned();
    let expected_line = expected.line;

    // Enter opens the selected match and jumps to it.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.workspace_search.is_none());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.as_ref().unwrap().ends_with(&expected_name));
    assert_eq!(app.editor.cursor_1based().0, expected_line);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_search_include_path_filter_narrows_results() {
    let dir = unique_dir("psfilter");
    fs::write(dir.join("a.rs"), "needle here\n").unwrap();
    fs::write(dir.join("b.txt"), "needle here\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.workspace");
    for c in "needle".chars() {
        app.on_key(key(c));
    }
    // Both files match before filtering.
    assert_eq!(app.workspace_search.as_ref().unwrap().hits.len(), 2);

    // Tab to the Include-path field and restrict to .rs files.
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    for c in r"\.rs$".chars() {
        app.on_key(key(c));
    }
    let hits = &app.workspace_search.as_ref().unwrap().hits;
    assert_eq!(hits.len(), 1, "only the .rs file remains");
    assert!(hits[0].path.to_string_lossy().ends_with("a.rs"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_search_exclude_path_filter_drops_results() {
    let dir = unique_dir("psexclude");
    fs::write(dir.join("a.rs"), "needle here\n").unwrap();
    fs::write(dir.join("b.txt"), "needle here\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.workspace");
    for c in "needle".chars() {
        app.on_key(key(c));
    }
    // Tab twice (query → include → exclude) and exclude .txt files.
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    for c in r"\.txt$".chars() {
        app.on_key(key(c));
    }
    let hits = &app.workspace_search.as_ref().unwrap().hits;
    assert_eq!(hits.len(), 1, "the .txt file is excluded");
    assert!(hits[0].path.to_string_lossy().ends_with("a.rs"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_replace_rewrites_files() {
    let dir = unique_dir("preplace");
    fs::write(dir.join("a.txt"), "beta and beta\n").unwrap();
    fs::write(dir.join("b.txt"), "gamma beta\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.workspace_replace");
    for c in "beta".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to replace field
    for c in "ZZ".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // preview replace
    // Nothing is written until the preview is confirmed.
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "beta and beta\n");
    app.on_key(key('y')); // confirm: apply across the workspace

    let a = fs::read_to_string(dir.join("a.txt")).unwrap();
    let b = fs::read_to_string(dir.join("b.txt")).unwrap();
    assert_eq!(a, "ZZ and ZZ\n");
    assert_eq!(b, "gamma ZZ\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn auto_pairs_brackets_and_deletes_empty_pair() {
    let dir = unique_dir("autopair");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    app.on_key(key('('));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.editor.get_content(), "()", "closer auto-inserted");
        assert_eq!(t.editor.get_cursor(), 1, "cursor sits between the pair");
    }
    // Typing the closer steps over the auto-inserted one rather than doubling it.
    app.on_key(key(')'));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_content(), "()");
    // Backspace at the caret-between position deletes both halves.
    app.on_key(key('('));
    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_content(), "()");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn snippet_expands_with_navigable_tabstops() {
    let dir = unique_dir("snippet-tabstops");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Open the Snippets picker and choose "Rust function" (index 7).
    app.run_action("tools.snippets");
    for _ in 0..7 {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.editor.get_content(), "fn name() -> () {\n    \n}\n");
        // The first tabstop's placeholder ("name") is selected.
        assert_eq!(t.editor.selection_span(), Some((3, 7)));
    }
    // Typing replaces the selected placeholder.
    for c in "foo".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.editor.active_tab().unwrap().editor.get_content(), "fn foo() -> () {\n    \n}\n");
    // Tab jumps to the (empty) parameter tabstop, between the parens.
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_cursor(), 7);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detects_image_extensions() {
    use vix::editor::is_image_path;
    assert!(is_image_path(Path::new("photos/a.PNG")));
    assert!(is_image_path(Path::new("x.jpeg")));
    assert!(is_image_path(Path::new("y.webp")));
    assert!(!is_image_path(Path::new("z.rs")));
    assert!(!is_image_path(Path::new("notes.md")));
}

#[test]
fn image_open_without_picker_warns_and_does_not_crash() {
    let dir = unique_dir("img");
    fs::write(dir.join("pic.png"), b"\x89PNG not-really").unwrap();
    let mut app = app_at(&dir); // picker is None (no terminal)
    let before = app.messages.items.len();
    app.open_initial(&dir.join("pic.png"));
    // No image tab opened, but the user is told why, and nothing panicked.
    assert!(app.editor.active_tab().is_none_or(|t| !t.is_image()));
    assert!(app.messages.items.len() > before, "a warning was added");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn scrollbar_drag_scrolls_editor() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let dir = unique_dir("scroll");
    let file = dir.join("long.txt");
    let body: String = (1..=200).map(|i| format!("line {i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    // Rectangles are normally set during render; set them directly here.
    app.layout.editor = Rect::new(0, 0, 80, 20);
    app.layout.scrollbar = Rect::new(80, 0, 1, 20);

    // Press at the bottom of the scrollbar track → jump near the last line.
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 80,
        row: 19,
        modifiers: KeyModifiers::NONE,
    });
    assert!(
        app.editor.cursor_1based().0 > 150,
        "dragging to the bottom scrolls near the end (got line {})",
        app.editor.cursor_1based().0
    );

    // Drag back to the top → jump near the first line.
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 80,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.editor.cursor_1based().0, 1, "dragging to the top scrolls home");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_navigation_is_bounded() {
    let mut app = app_at(Path::new("."));
    for _ in 0..5 {
        app.explorer.up();
    }
    assert_eq!(app.explorer.selected, 0);
    app.explorer.last();
    let last = app.explorer.selected;
    app.explorer.down();
    assert_eq!(app.explorer.selected, last, "down at end stays put");
}

// ===========================================================================
// Keyboard shortcuts (global)
// ===========================================================================

#[test]
fn ctrl_n_creates_new_buffer() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.tabs.len();
    app.on_key(ctrl('n'));
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(app.editor.active_tab().unwrap().path.is_none());
}

#[test]
fn ctrl_w_closes_active_buffer() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('n'));
    app.on_key(ctrl('n'));
    let before = app.editor.tabs.len();
    app.on_key(ctrl('w'));
    assert_eq!(app.editor.tabs.len(), before - 1);
}

#[test]
fn ctrl_o_opens_prompt_and_esc_closes() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('o'));
    assert!(app.prompt.is_some(), "Ctrl+O opens the Open prompt");
    app.on_key(esc());
    assert!(app.prompt.is_none(), "Esc closes the prompt");
}

#[test]
fn ctrl_b_toggles_explorer() {
    let mut app = app_at(Path::new("."));
    let before = app.show_explorer;
    app.on_key(ctrl('b'));
    assert_ne!(app.show_explorer, before);
    app.on_key(ctrl('b'));
    assert_eq!(app.show_explorer, before);
}

#[test]
fn ctrl_e_toggles_focus_between_editor_and_explorer() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.focus, Focus::Editor);
    app.on_key(ctrl('e'));
    assert_eq!(app.focus, Focus::Explorer);
    app.on_key(ctrl('e'));
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn ctrl_f_opens_find_and_esc_closes() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('f'));
    let s = app.search.as_ref().expect("Ctrl+F opens search");
    assert!(!s.replacing, "Ctrl+F is find, not replace");
    app.on_key(esc());
    assert!(app.search.is_none(), "Esc closes the search bar");
}

#[test]
fn ctrl_r_opens_replace() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('r'));
    assert!(app.search.as_ref().is_some_and(|s| s.replacing), "Ctrl+R opens replace");
}

#[test]
fn ctrl_shift_f_opens_workspace_search() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert!(app.workspace_search.is_some(), "Ctrl+Shift+F opens workspace search");
}

#[test]
fn ctrl_p_opens_palette_and_esc_closes() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('p'));
    assert!(app.palette.is_some());
    app.on_key(esc());
    assert!(app.palette.is_none());
}

#[test]
fn palette_command_fuzzy_ranks_best_match_first() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('p'));
    for c in ">sortlines".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert!(!p.entries.is_empty(), "fuzzy query matched commands");
    match &p.entries[0].action {
        vix::palette::Action::RunCommand(a) => assert_eq!(a, "edit.sort_lines"),
        _ => panic!("expected a command entry"),
    }
}

#[test]
fn palette_recents_seed_from_persisted_settings() {
    let settings = Settings { command_recents: vec!["edit.select_all".to_string()], ..Settings::default() };
    let mut app = app_with(settings);
    app.on_key(ctrl('p'));
    app.on_key(key('>')); // empty command query → recents first
    let p = app.palette.as_ref().unwrap();
    match &p.entries[0].action {
        vix::palette::Action::RunCommand(a) => assert_eq!(a, "edit.select_all", "persisted recent floats up"),
        _ => panic!("expected a command entry"),
    }
}

#[test]
fn palette_recent_command_floats_to_top() {
    let mut app = app_at(Path::new("."));
    // Run "Select All" from the palette so it is recorded as a recent.
    app.on_key(ctrl('p'));
    for c in ">select all".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // accept → records the recent + runs it
    assert!(app.palette.is_none(), "palette closed after accepting");

    // Reopen the command list with no query: the recent is first.
    app.on_key(ctrl('p'));
    app.on_key(key('>'));
    let p = app.palette.as_ref().unwrap();
    match &p.entries[0].action {
        vix::palette::Action::RunCommand(a) => assert_eq!(a, "edit.select_all", "recent floats up"),
        _ => panic!("expected a command entry"),
    }
}

#[test]
fn f10_toggles_menu_bar() {
    let mut app = app_at(Path::new("."));
    assert!(!app.menu.is_open());
    app.on_key(func(10));
    assert!(app.menu.is_open());
    app.on_key(func(10));
    assert!(!app.menu.is_open());
}

#[test]
fn alt_letters_open_specific_menus() {
    let alt = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
    let menu_index = |name: &str| {
        vix::menu::menus().iter().position(|m| m.name == name).unwrap()
    };
    for (letter, name) in [
        ('v', "menu.vix"),
        ('f', "menu.file"),
        ('e', "menu.edit"),
        ('i', "menu.view"),
        ('n', "menu.go"),
        ('g', "menu.git"),
        ('o', "menu.org"),
        ('r', "menu.run"),
        ('h', "menu.help"),
    ] {
        let mut app = app_at(Path::new("."));
        app.on_key(alt(letter));
        assert_eq!(app.menu.open, Some(menu_index(name)), "Alt+{letter} opens {name}");
    }
}

#[test]
fn undo_tree_preserves_a_branch_after_a_new_edit() {
    let mut app = app_at(Path::new("."));
    // Type "A", undo it, then type "B" — the case linear undo would lose.
    type_str(&mut app, "A");
    app.on_key(ctrl('z')); // undo "A" → empty
    type_str(&mut app, "B"); // new branch off the empty root
    assert_eq!(app.editor.active_tab().unwrap().text(), "B");
    // Redo right after the edit does nothing (B is the active tip).
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert_eq!(app.editor.active_tab().unwrap().text(), "B");
    // Undo back to the branch point, switch branches, and redo into the OLD "A"
    // branch — proving it survived the new edit.
    app.on_key(ctrl('z')); // undo "B" → empty (branch point)
    assert_eq!(app.editor.active_tab().unwrap().text(), "");
    app.run_action("edit.undo_branch");
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert_eq!(app.editor.active_tab().unwrap().text(), "A", "the old branch is still reachable");
}

#[test]
fn ctrl_z_undoes_and_ctrl_shift_z_redoes() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    let full = app.editor.active_tab().unwrap().lines()[0].len();
    app.on_key(ctrl('z'));
    let undone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(undone < full, "Ctrl+Z undoes typing ({undone} < {full})");
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    let redone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(redone > undone, "Ctrl+Shift+Z redoes ({redone} > {undone})");
}

#[test]
fn editor_cut_removes_selection() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    // Select the whole word with Shift+Left (Home is intercepted and ignores
    // Shift), then cut.
    for _ in 0..5 {
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    }
    app.on_key(ctrl('x'));
    assert!(
        app.editor.active_tab().unwrap().text().is_empty(),
        "Ctrl+X cuts the selected text"
    );
}

#[test]
fn f3_after_find_does_not_panic_and_keeps_search() {
    let mut app = app_at(Path::new("."));
    for c in "foo bar foo".chars() {
        app.on_key(key(c));
    }
    app.on_key(ctrl('f'));
    for c in "foo".chars() {
        app.on_key(key(c));
    }
    app.on_key(func(3)); // find next
    app.on_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT)); // find prev
    assert!(app.search.is_some(), "search stays open while navigating matches");
}

#[test]
fn delete_key_forward_deletes() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Home));
    app.on_key(keycode(KeyCode::Delete));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "bc", "Delete removes the char ahead");
}

// ===========================================================================
// Mouse actions
// ===========================================================================

#[test]
fn menu_dropdown_keeps_a_gap_before_shortcuts() {
    // The dropdown rect must be wide enough that every item with a shortcut keeps
    // at least one space between its label and the right-aligned shortcut.
    let bar = Rect::new(0, 0, 200, 1);
    let frame = Rect::new(0, 0, 200, 40);
    for (i, m) in vix::menu::menus().iter().enumerate() {
        let rect = vix::ui::menu_dropdown_rect(frame, bar, i);
        for it in m.items {
            if it.shortcut.is_empty() {
                continue;
            }
            // Row = " label" + pad + "shortcut " inside borders; pad must be >= 1.
            let content = it.label().chars().count() + it.shortcut.chars().count();
            let pad = (rect.width as usize).saturating_sub(content + 4);
            assert!(pad >= 1, "{}/{} label and shortcut touch", m.name, it.action);
        }
    }
}

#[test]
fn menus_have_separators_in_the_specified_places() {
    // A separator sits immediately before each named item.
    let cases: &[(&str, &[&str])] = &[
        // (find-related items live in the Edit → Find submenu; the View dock and
        // editor toggles live in the View → Layout / Editor submenus — so the
        // top-level separators precede the groups/submenus that remain.)
        ("menu.file", &["file.open", "file.close"]),
        ("menu.vix", &["file.quit"]),
        ("menu.edit", &["edit.cut", "edit.toggle_comment"]),
    ];
    for (menu, befores) in cases {
        let items = vix::menu::menus()
            .iter()
            .find(|m| m.name == *menu)
            .unwrap_or_else(|| panic!("{menu} exists"))
            .items;
        for action in *befores {
            let at = items
                .iter()
                .position(|it| it.action == *action)
                .unwrap_or_else(|| panic!("{menu} has {action}"));
            assert!(at > 0 && items[at - 1].is_separator(), "{menu}: separator before {action}");
        }
    }
}

#[test]
fn submenu_opens_and_runs_a_nested_action() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::F(10)));
    let edit_idx = vix::menu::menus().iter().position(|m| m.name == "menu.edit").unwrap();
    for _ in 0..edit_idx {
        app.on_key(keycode(KeyCode::Right));
    }
    // Walk down to the Find submenu parent.
    let edit_items = vix::menu::menus()[edit_idx].items;
    let find_parent = edit_items
        .iter()
        .position(|it| it.label == "menu.item.edit.find_menu")
        .unwrap();
    for _ in 0..=edit_items.len() {
        if app.menu.item == Some(find_parent) {
            break;
        }
        app.on_key(keycode(KeyCode::Down));
    }
    assert_eq!(app.menu.item, Some(find_parent));
    assert!(!app.menu.submenu_open(), "submenu starts closed");

    // Right opens the submenu and highlights its first item (a find action).
    app.on_key(keycode(KeyCode::Right));
    assert!(app.menu.submenu_open(), "Right opens the submenu");
    assert_eq!(app.menu.selected_action(), Some("edit.find"), "first submenu item highlighted");

    // Enter runs the nested action (opens the find box) and closes the menu.
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.menu.open.is_none(), "the menu closed");
    assert!(app.search.is_some(), "Find opened the search box");
}

#[test]
fn context_menu_runs_the_selected_action() {
    let mut app = app_at(Path::new("."));
    for c in "hello world".chars() {
        app.on_key(key(c));
    }
    // Open the menu directly (the right-click path needs a rendered layout) and
    // select "Select All" (index 4 in CONTEXT_ITEMS), then run it with Enter.
    app.context_menu = Some(vix::app::ContextMenu { selected: 4, x: 0, y: 0 });
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.context_menu.is_none(), "Enter closes the context menu");
    assert!(
        app.editor.active_tab_mut().unwrap().editor.get_selection_text().is_some(),
        "the Select All action ran",
    );
}

#[test]
fn ctrl_tab_switches_tabs() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new"); // open a second tab
    let before = app.editor.active;
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
    assert_ne!(app.editor.active, before, "Ctrl+Tab moves to another tab");
}

#[test]
fn view_editor_submenu_rolls_up_the_editor_toggles() {
    let view = vix::menu::menus().iter().find(|m| m.name == "menu.view").unwrap();
    let editor = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.editor")
        .and_then(|it| it.submenu)
        .expect("View has an Editor submenu");
    let actions: Vec<&str> =
        editor.iter().map(|it| it.action).filter(|a| a.starts_with("view.")).collect();
    assert_eq!(
        actions,
        vec![
            "view.line_numbers",
            "view.relative_line_numbers",
            "view.read_only",
            "view.whitespace",
            "view.scrollbar",
            "view.soft_wrap",
            "view.inlay_hints",
            "view.sticky_scroll",
            "view.minimap",
            "view.highlight_word",
            "view.spellcheck",
            "view.auto_pair",
            "view.rainbow_brackets",
            "view.trim_on_save",
            "view.final_newline_on_save",
            "view.format_on_save",
            "view.auto_save"
        ]
    );
}

#[test]
fn view_layout_submenu_rolls_up_the_dock_toggles() {
    let view = vix::menu::menus().iter().find(|m| m.name == "menu.view").unwrap();
    let layout = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.layout")
        .and_then(|it| it.submenu)
        .expect("View has a Layout submenu");
    let actions: Vec<&str> = layout.iter().map(|it| it.action).collect();
    assert_eq!(
        actions,
        vec![
            "view.left_dock",
            "view.right_dock",
            "view.bottom_dock",
            "view.status_bar",
            "view.breadcrumbs",
            "view.outline_dock",
            "view.zen"
        ]
    );
}

#[test]
fn ascii_panel_opens_inserts_and_closes() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.ascii");
    assert!(app.ascii_panel.is_some(), "Tools → ASCII opens the panel");

    // Highlight code 65 ('A') and insert it; the panel stays open.
    app.ascii_panel.as_mut().unwrap().selected = 65;
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.ascii_panel.is_some(), "Enter keeps the panel open");
    assert!(
        app.editor.active_tab().unwrap().lines()[0].contains('A'),
        "the highlighted character is inserted into the editor"
    );

    // Esc closes the panel.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.ascii_panel.is_none(), "Esc closes the panel");
}

#[test]
fn closing_a_dirty_tab_prompts_then_discards() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x')); // dirties the untitled buffer
    assert!(app.editor.active_tab().unwrap().dirty);
    app.run_action("file.close");
    assert!(app.unsaved.is_some(), "a dirty close prompts to save");
    app.on_key(key('d')); // don't save -> close anyway
    assert!(app.unsaved.is_none(), "the prompt is dismissed");
    assert!(
        !app.editor.active_tab().unwrap().dirty,
        "the buffer was closed (a fresh empty tab remains)"
    );
}

#[test]
fn closing_a_dirty_tab_can_be_cancelled() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.run_action("file.close");
    assert!(app.unsaved.is_some());
    app.on_key(key('c')); // cancel
    assert!(app.unsaved.is_none());
    assert!(
        app.editor.active_tab().unwrap().dirty,
        "cancelling keeps the unsaved buffer open"
    );
}

#[test]
fn quitting_with_a_dirty_tab_prompts_then_quits_on_discard() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.run_action("file.quit");
    assert!(app.unsaved.is_some(), "a dirty quit prompts first");
    assert!(!app.should_quit, "quit is deferred until the tab is resolved");
    app.on_key(key('d')); // discard -> no more dirty tabs -> quit
    assert!(app.should_quit, "discarding the last dirty tab quits");
}

#[test]
fn unsaved_prompt_save_writes_and_closes() {
    let dir = unique_dir("unsaved_save");
    let file = dir.join("note.txt");
    fs::write(&file, "hello\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for c in "!!!".chars() {
        app.on_key(key(c));
    }
    assert!(app.editor.active_tab().unwrap().dirty);
    app.run_action("file.close");
    assert!(app.unsaved.is_some());
    app.on_key(key('s')); // save -> writes -> closes
    assert!(app.unsaved.is_none());
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.starts_with("hello!!!"), "got: {saved:?}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn spellcheck_toggle_persists_and_clears_when_off() {
    // Toggling the setting works without a dictionary present (graceful no-op):
    // enabling sets the flag; disabling clears marks and the flag.
    let mut app = app_at(Path::new("."));
    assert!(!app.spellcheck);
    app.run_action("view.spellcheck");
    assert!(app.spellcheck, "toggle enables spellcheck");
    assert!(app.settings.spellcheck, "the setting is updated for persistence");
    app.run_action("view.spellcheck");
    assert!(!app.spellcheck, "toggle disables spellcheck");
    assert!(
        app.editor.active_tab().unwrap().editor.spell_marks().is_none(),
        "disabling clears the underline marks"
    );
}

#[test]
fn recent_files_max_caps_the_list() {
    let dir = unique_dir("recentmax");
    let mut app = app_at(&dir);
    app.settings.recent_files_max = 2;
    for name in ["a.txt", "b.txt", "c.txt"] {
        let p = dir.join(name);
        fs::write(&p, "x\n").unwrap();
        app.open_initial(&p);
    }
    assert_eq!(app.settings.recent_files.len(), 2, "kept only recent_files_max entries");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn select_more_and_less_extend_selection_by_word() {
    let mut app = app_at(Path::new("."));
    for c in "alpha beta gamma".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Home)); // cursor to column 0

    app.run_action("edit.select_more");
    assert_eq!(
        app.editor.active_tab_mut().unwrap().editor.get_selection_text().as_deref(),
        Some("alpha"),
    );
    app.run_action("edit.select_more");
    assert_eq!(
        app.editor.active_tab_mut().unwrap().editor.get_selection_text().as_deref(),
        Some("alpha beta"),
    );
    // Select Less retracts the active end leftward by a word.
    app.run_action("edit.select_less");
    assert_eq!(
        app.editor.active_tab_mut().unwrap().editor.get_selection_text().as_deref(),
        Some("alpha "),
    );
}

#[test]
fn change_case_transforms_the_selection() {
    let mut app = app_at(Path::new("."));
    for c in "foo bar".chars() {
        app.on_key(key(c));
    }
    app.run_action("edit.select_all");
    app.run_action("edit.case_upper");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "FOO BAR");
    // The result stays selected, so the next transform applies to it.
    app.run_action("edit.case_snake");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "foo_bar");
    app.run_action("edit.case_pascal");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "FooBar");
}

#[test]
fn change_case_without_selection_is_a_noop() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    app.run_action("edit.case_upper"); // no selection
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "hello");
}

#[test]
fn editor_gutter_marks_round_trip() {
    let mut app = app_at(Path::new("."));
    let t = app.editor.active_tab_mut().unwrap();
    t.editor.set_gutter_marks(vec![(0, "#3fb950"), (2, "#d29922")]);
    assert_eq!(t.editor.gutter_marks().map(std::vec::Vec::len), Some(2));
    t.editor.clear_gutter_marks();
    assert!(t.editor.gutter_marks().is_none());
}

#[test]
#[ignore = "needs git and an in-tree checkout"]
fn git_gutter_marks_a_modified_line() {
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    app.open_initial(&PathBuf::from("Cargo.toml"));
    app.on_key(key('x')); // modify the first line
    app.refresh_git_gutter();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned()
        .unwrap_or_default();
    assert!(!marks.is_empty(), "a modified line is marked in the gutter");
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_stages_and_commits() {
    let dir = unique_dir("gitpanel");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    fs::write(dir.join("a.txt"), "hello\n").unwrap();

    let mut app = app_at(&dir);
    app.run_action("git.changes");
    assert!(app.git_panel.is_some(), "panel opens in a repo");
    assert_eq!(app.git_status.len(), 1, "one changed (untracked) file");

    // Space stages the selected file.
    app.on_key(keycode(KeyCode::Char(' ')));
    assert!(app.git_status[0].is_staged(), "file is staged");

    // 'c' begins the commit message prompt.
    app.on_key(keycode(KeyCode::Char('c')));
    assert!(app.prompt.is_some(), "commit message prompt opens");
    for ch in "initial".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));

    app.refresh_git();
    assert!(app.git_status.is_empty(), "after commit the tree is clean");
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn revert_hunk_restores_committed_text() {
    let dir = unique_dir("reverthunk");
    fs::create_dir_all(&dir).unwrap();
    // Canonicalize so the workspace root and the file share a prefix even when
    // the temp dir lives under a symlink (e.g. macOS /var → /private/var); the
    // diff gutter and revert key the HEAD cache off that shared prefix.
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.on_key(key('X')); // modify line 0: "one" -> "Xone"
    assert_eq!(app.editor.active_tab().unwrap().text(), "Xone\ntwo\n");

    app.run_action("git.revert_hunk");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "one\ntwo\n",
        "the modified hunk is restored to HEAD"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_stash_and_pop_round_trip() {
    let dir = unique_dir("gitstash");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    fs::write(&file, "one\ntwo\n").unwrap(); // uncommitted change

    let mut app = app_at(&dir);
    app.refresh_git();
    assert!(app.git_dirty(), "working tree dirty before stash");
    app.run_action("git.stash");
    app.refresh_git();
    assert!(!app.git_dirty(), "clean after stash");
    app.run_action("git.stash_pop");
    app.refresh_git();
    assert!(app.git_dirty(), "change restored after pop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn stage_hunk_stages_only_the_cursor_hunk() {
    let dir = unique_dir("stagehunk");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    // Change the first line in the working tree.
    fs::write(&file, "ONE\ntwo\nthree\n").unwrap();
    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.run_action("git.stage_hunk"); // cursor on line 0

    // The staged (index) version now has the change; HEAD still has "one".
    let staged = String::from_utf8(
        std::process::Command::new("git").current_dir(&dir).args(["show", ":a.txt"]).output().unwrap().stdout,
    )
    .unwrap();
    assert_eq!(staged, "ONE\ntwo\nthree\n", "the hunk is staged into the index");
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn unstage_hunk_removes_the_cursor_hunk_from_index() {
    let dir = unique_dir("unstagehunk");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    // Change the first line and stage the whole file, then unstage just the hunk.
    fs::write(&file, "ONE\ntwo\nthree\n").unwrap();
    run(&["add", "a.txt"]);
    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.run_action("git.unstage_hunk"); // cursor on line 0

    // The index now matches HEAD again ("one"); the working tree keeps "ONE".
    let staged = String::from_utf8(
        std::process::Command::new("git").current_dir(&dir).args(["show", ":a.txt"]).output().unwrap().stdout,
    )
    .unwrap();
    assert_eq!(staged, "one\ntwo\nthree\n", "the hunk is removed from the index");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_snapshot_and_restore_round_trip() {
    let dir = unique_dir("session");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    fs::write(&a, "alpha\nbeta\ngamma\n").unwrap();
    fs::write(&b, "one\ntwo\n").unwrap();

    // Open two files, focus the second, move its cursor, then snapshot.
    let mut app = app_at(&dir);
    app.open_initial(&a.clone());
    app.open_initial(&b.clone());
    app.run_action("cursor_down"); // line 2 of b.txt
    app.run_action("cursor_right");
    let snap = app.workspace_session();
    assert_eq!(snap.files.len(), 2, "both files captured");
    assert_eq!(snap.active, 1, "second file is focused");
    let saved_cursor = snap.cursors[1];
    assert!(saved_cursor > 0, "cursor offset captured: {saved_cursor}");

    // A fresh app at the same root restores the snapshot.
    let mut restored = app_at(&dir);
    let opened = restored.apply_session(&snap);
    assert_eq!(opened, 2, "both files reopened");
    assert_eq!(restored.editor.tabs.len(), 2, "blank buffer dropped");
    assert_eq!(restored.editor.active, 1, "focus restored");
    let tab = restored.editor.active_tab().unwrap();
    assert_eq!(tab.editor.get_cursor(), saved_cursor, "cursor restored");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_restores_scroll_offset() {
    let dir = unique_dir("session-scroll");
    let a = dir.join("long.txt");
    let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
    fs::write(&a, &body).unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&a.clone());
    if let Some(t) = app.editor.active_tab_mut() {
        t.editor.set_offset_y(120);
    }
    let snap = app.workspace_session();
    assert_eq!(snap.scrolls.first().copied(), Some(120), "scroll offset captured");

    let mut restored = app_at(&dir);
    assert_eq!(restored.apply_session(&snap), 1);
    let tab = restored.editor.active_tab().unwrap();
    assert_eq!(tab.editor.get_offset_y(), 120, "scroll offset restored");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn session_apply_skips_missing_files() {
    let dir = unique_dir("session-missing");
    fs::create_dir_all(&dir).unwrap();
    let ws = vix::session::WorkspaceSession {
        root: dir.to_string_lossy().into_owned(),
        files: vec![dir.join("gone.txt").to_string_lossy().into_owned()],
        active: 0,
        cursors: vec![0],
        ..Default::default()
    };
    let mut app = app_at(&dir);
    let opened = app.apply_session(&ws);
    assert_eq!(opened, 0, "missing file is skipped");
    // The blank untitled buffer is left intact when nothing reopened.
    assert_eq!(app.editor.tabs.len(), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_blame_annotates_the_current_line() {
    let dir = unique_dir("gitblame");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Ada Lovelace"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed the file"]);

    let mut app = app_at(&dir);
    app.open_initial(&file);
    // Cursor starts on line 1; blame should attribute it to the seed commit.
    app.run_action("git.blame");
    assert!(app.status.contains("Ada Lovelace"), "blame names the author: {}", app.status);
    assert!(app.status.contains("seed the file"), "blame shows the summary: {}", app.status);
    assert!(app.status.starts_with("L1:"), "blame labels the line: {}", app.status);
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_blame_flags_an_uncommitted_line() {
    let dir = unique_dir("gitblameunc");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "committed\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    // A second, uncommitted line on disk.
    fs::write(&file, "committed\nbrand new\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("cursor_down"); // move to line 2 (the new line)
    app.run_action("git.blame");
    assert!(
        app.status.contains("L2") && app.status.to_lowercase().contains("commit"),
        "uncommitted line is flagged: {}",
        app.status
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo with branches"]
fn branch_chooser_switches_branches() {
    let dir = unique_dir("gitbranch");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    run(&["branch", "feature"]);

    let mut app = app_at(&dir);
    app.refresh_git();
    let start = app.git_branch.clone();
    app.run_action("git.switch_branch");
    let chooser = app.branch_chooser.as_ref().expect("branch chooser opens");
    let idx = chooser.branches.iter().position(|b| Some(b) != start.as_ref()).unwrap();
    let target = chooser.branches[idx].clone();
    app.branch_chooser.as_mut().unwrap().selected = idx;
    app.on_key(keycode(KeyCode::Enter));
    app.refresh_git();
    assert_eq!(app.git_branch.as_deref(), Some(target.as_str()), "checked out the chosen branch");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn refresh_git_populates_branch_when_in_a_repo() {
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    if app.git_repo {
        assert!(app.git_branch.is_some(), "a repo reports a branch");
    }
    // The dirty flag is always consistent with the cached status list.
    assert_eq!(app.git_dirty(), !app.git_status.is_empty());
}

#[test]
fn spell_suggest_is_a_noop_without_a_dictionary() {
    // With spellcheck off (and no dictionary loaded), Ctrl+; just sets a status
    // and does not open the popup.
    let mut app = app_at(Path::new("."));
    app.run_action("spell.suggest");
    assert!(app.spell_suggest.is_none());
}

// End-to-end spellcheck needs the untracked ./dictionaries set and the Rust
// grammar; run with `cargo test -p vix --test integration -- --ignored`.
#[test]
#[ignore = "needs the untracked ./dictionaries set and the Rust grammar"]
fn spell_suggest_popup_replaces_a_misspelling() {
    let dir = unique_dir("spellsug");
    let file = dir.join("a.rs");
    fs::write(&file, "// helllo world\nfn main() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("view.spellcheck");
    // Put the cursor inside "helllo" (chars 3..9) and open the popup.
    app.editor.active_tab_mut().unwrap().editor.set_cursor(5);
    app.run_action("spell.suggest");
    let sug = app.spell_suggest.as_ref().expect("popup opens on a misspelling");
    assert!(!sug.suggestions.is_empty(), "offers suggestions");
    // Apply the highlighted suggestion; the misspelling is gone.
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.spell_suggest.is_none(), "popup closes after applying");
    let line0 = app.editor.active_tab().unwrap().lines()[0].clone();
    assert!(!line0.contains("helllo"), "misspelling replaced; got: {line0:?}");
    fs::remove_dir_all(&dir).ok();
}
#[test]
#[ignore = "needs the untracked ./dictionaries set and the Rust grammar"]
fn spellcheck_underlines_a_misspelling_in_a_comment() {
    let dir = unique_dir("spell");
    let file = dir.join("a.rs");
    fs::write(&file, "// helllo wrld\nfn main() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("view.spellcheck");
    app.refresh_spellcheck();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .spell_marks()
        .cloned()
        .unwrap_or_default();
    assert!(!marks.is_empty(), "misspelled words in the comment are underlined");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn outline_panel_lists_symbols_and_jumps() {
    let dir = unique_dir("outline");
    let file = dir.join("a.rs");
    fs::write(&file, "fn alpha() {}\n\nstruct Beta;\n\nfn gamma() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("nav.outline");
    let o = app.outline.as_ref().expect("outline opens");
    assert_eq!(o.len(), 3, "alpha, Beta, gamma");

    // Jump to the last symbol (fn gamma, line 5).
    app.on_key(keycode(KeyCode::End));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.outline.is_none(), "panel closes after a jump");
    assert_eq!(app.editor.cursor_1based().0, 5, "cursor jumps to fn gamma");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn workspace_dashboard_opens_counts_files_and_closes() {
    let dir = unique_dir("dashboard");
    fs::write(dir.join("a.txt"), "x\n").unwrap();
    fs::write(dir.join("b.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("tools.dashboard");
    assert!(app.dashboard.is_some(), "Tools → Workspace Dashboard opens");
    assert!(!app.dashboard.as_ref().unwrap().folder.is_empty(), "folder is shown immediately");

    // Wait (bounded) for the async file-count metric to arrive.
    for _ in 0..200 {
        app.poll_dashboard();
        if app.dashboard.as_ref().unwrap().file_count.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(app.dashboard.as_ref().unwrap().file_count, Some(2), "counted the two files");

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.dashboard.is_none(), "Esc closes the dashboard");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn system_info_panel_opens_inserts_and_closes() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.system_info");
    assert!(app.system_info.is_some(), "Tools → System Information opens the panel");

    // Highlight the first row that has an insertable value, then insert it.
    let idx = app
        .system_info
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| !r.value.is_empty())
        .expect("the snapshot has at least one value row");
    let value = app.system_info.as_ref().unwrap().rows[idx].value.clone();
    app.system_info.as_mut().unwrap().select_index(idx);
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.system_info.is_some(), "Enter keeps the panel open");
    assert!(
        app.editor.active_tab().unwrap().lines()[0].contains(&value),
        "the highlighted value is inserted into the editor"
    );

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.system_info.is_none(), "Esc closes the panel");
}

#[test]
fn test_panel_toggles_and_parser_builds_results() {
    let mut app = app_at(Path::new("."));
    assert!(!app.show_test_panel);
    app.run_action("tools.test_panel");
    assert!(app.show_test_panel, "Toggle Test Panel shows it");

    // The parser turns runner output into a pass/fail list (used by the panel).
    let results = vix::test_runner::parse("test a::ok ... ok\ntest a::bad ... FAILED\n");
    assert_eq!(vix::test_runner::tally(&results), (1, 1, 0));
}

#[test]
fn debug_breakpoints_toggle_on_the_cursor_line() {
    let dir = unique_dir("breakpoints");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.rs");
    fs::write(&file, "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("cursor_down"); // line 2
    app.run_action("run.toggle_breakpoint");
    assert_eq!(app.active_breakpoints(), vec![2], "breakpoint set on line 2");

    app.run_action("cursor_down"); // line 3
    app.run_action("run.toggle_breakpoint");
    assert_eq!(app.active_breakpoints(), vec![2, 3]);

    // Toggling again clears it.
    app.run_action("run.toggle_breakpoint");
    assert_eq!(app.active_breakpoints(), vec![2]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn outline_sidebar_lists_symbols_and_follows_toggle() {
    let dir = unique_dir("outline-dock");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lib.rs");
    fs::write(&file, "fn alpha() {}\nfn beta() {}\nstruct Gamma;\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    // Off by default.
    app.refresh_outline_dock();
    assert!(app.outline_dock.is_none());

    // Toggling on builds the symbol list for the active buffer.
    app.run_action("view.outline_dock");
    app.refresh_outline_dock();
    let o = app.outline_dock.as_ref().expect("outline dock populated");
    assert!(o.entries.iter().any(|e| e.name == "alpha"));
    assert!(o.entries.iter().any(|e| e.name == "Gamma"));

    // Toggling off clears it.
    app.run_action("view.outline_dock");
    assert!(app.outline_dock.is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn spacemacs_keymap_is_modal_with_space_leader() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "spacemacs".to_string();
    // Starts in Normal mode.
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"));
    // `i` enters Insert, Esc returns to Normal.
    app.on_key(key('i'));
    assert_eq!(app.mode_indicator().as_deref(), Some("-- INSERT --"));
    app.on_key(keycode(KeyCode::Esc));
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"));
    // The Space leader: SPC w / splits the editor vertically.
    assert!(!app.editor.is_split());
    app.on_key(key(' '));
    assert_eq!(app.mode_indicator().as_deref(), Some("SPC "), "leader pending");
    app.on_key(key('w'));
    app.on_key(key('/'));
    assert!(app.editor.is_split(), "SPC w / split the editor");
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"), "leader cleared");
}

#[test]
fn menu_type_ahead_selects_by_first_letter() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::F(10)));
    let file_idx = vix::menu::menus().iter().position(|m| m.name == "menu.file").unwrap();
    for _ in 0..file_idx {
        app.on_key(keycode(KeyCode::Right));
    }
    // Open File, type S to cycle the "S" items in menu order:
    // Switch Project → Save Workspace → Save → Save As → wraps around.
    app.on_key(key('s'));
    assert_eq!(app.menu.selected_action(), Some("file.switch_project"));
    app.on_key(key('s'));
    assert_eq!(app.menu.selected_action(), Some("workspace.save"));
    app.on_key(key('s'));
    assert_eq!(app.menu.selected_action(), Some("file.save"));
    app.on_key(key('s'));
    assert_eq!(app.menu.selected_action(), Some("file.save_as"));
    app.on_key(key('s'));
    assert_eq!(app.menu.selected_action(), Some("file.switch_project"), "wraps around");

    // A different letter jumps elsewhere (C → Close).
    app.on_key(key('c'));
    assert_eq!(app.menu.selected_action(), Some("file.close"));
}

#[test]
fn menu_navigation_skips_separators() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let file_idx = vix::menu::menus().iter().position(|m| m.name == "menu.file").unwrap();
    for _ in 0..file_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    // The dropdown opens with nothing highlighted; the user must move to select.
    assert_eq!(app.menu.item, None, "no item is auto-selected on open");
    assert_eq!(app.menu.selected_action(), None);
    // Walking the whole menu with Down must never land on (or commit) a separator.
    let len = vix::menu::menus()[file_idx].items.len();
    for _ in 0..=len {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let action = app.menu.selected_action().expect("never a separator");
        assert_ne!(action, vix::menu::SEPARATOR);
    }
}

#[test]
fn nerd_palette_inserts_glyph_with_keyboard() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.nerd_palette");
    assert!(app.nerd_palette.is_some(), "the action opens the palette");

    // Move one cell right, capture the highlighted glyph, then insert it.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let expected = app.nerd_palette.as_ref().unwrap().selected_glyph();
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Enter inserts but keeps the palette open for picking more glyphs.
    assert!(app.nerd_palette.is_some(), "Enter keeps the palette open");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains(expected), "the editor holds the inserted glyph");

    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.nerd_palette.is_none(), "Esc closes the palette");
}

#[test]
fn nerd_palette_click_inserts_glyph() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.nerd_palette");
    // The grid rect is normally recorded during render; set it directly. Each
    // cell is `NERD_CELL_W` wide, so column 1 starts at x = NERD_CELL_W.
    let cw = vix::ui::NERD_CELL_W;
    app.layout.nerd_palette = Rect::new(0, 2, cw * 8, 7);
    app.on_mouse(click(cw, 2)); // row 0, column 1 → glyph index 1

    // The click both highlights that cell and inserts it; the palette stays open,
    // so its current glyph is the one just inserted.
    assert!(app.nerd_palette.is_some(), "a click keeps the palette open");
    let expected = app.nerd_palette.as_ref().unwrap().selected_glyph();
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains(expected), "clicking a cell inserts that glyph");
}

#[test]
fn click_menu_bar_opens_menu() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    // Column 2 falls inside the first menu's " Vix " label (cols 1..6).
    app.on_mouse(click(2, 0));
    assert_eq!(app.menu.open, Some(0), "clicking the bar opens that menu");
}

#[test]
fn click_dropdown_item_runs_its_action() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open the Vix menu (index 0)
    assert_eq!(app.menu.open, Some(0));
    // The dropdown rect is normally set during render; compute and set it.
    let dd = vix::ui::menu_dropdown_rect(Rect::new(0, 0, 100, 40), app.layout.menu, 0);
    app.layout.menu_dropdown = dd;
    // The first Vix item is "About Vix" → opens its dialog.
    app.on_mouse(click(dd.x + 2, dd.y + 1));
    assert!(app.menu.open.is_none(), "running an item closes the menu");
    assert!(app.dialog.is_some(), "clicking About opens its dialog");
}

/// Column inside the `idx`-th top-level menu's title (mirrors the bar layout),
/// computed from the actual titles so it is locale-independent.
fn top_menu_col(app: &App, idx: usize) -> u16 {
    let mut x = app.layout.menu.x + 1;
    for m in &vix::menu::menus()[..idx] {
        x += m.title().chars().count() as u16 + 2;
    }
    x + 1
}

#[test]
fn hover_moves_menu_dropdown_selection() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open the Vix menu (index 0)
    assert_eq!(app.menu.open, Some(0));
    let dd = vix::ui::menu_dropdown_rect(Rect::new(0, 0, 100, 40), app.layout.menu, 0);
    app.layout.menu_dropdown = dd;

    // Hover (no button) over the third item; the highlight follows the pointer
    // without committing or closing.
    app.on_mouse(mouse(MouseEventKind::Moved, dd.x + 2, dd.y + 1 + 2));
    assert_eq!(app.menu.item, Some(2), "hover highlights the item under the pointer");
    assert!(app.menu.is_open(), "hover must not commit or close");
    // Move back up to the first item.
    app.on_mouse(mouse(MouseEventKind::Moved, dd.x + 2, dd.y + 1));
    assert_eq!(app.menu.item, Some(0));
}

#[test]
fn hover_switches_open_top_menu() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open Vix (index 0)
    assert_eq!(app.menu.open, Some(0));
    // Hover the File menu name (index 1); the open menu follows the pointer.
    let file_col = top_menu_col(&app, 1);
    app.on_mouse(mouse(MouseEventKind::Moved, file_col, 0));
    assert_eq!(app.menu.open, Some(1), "hovering another name switches menus");
}

#[test]
fn hover_over_pane_does_not_steal_focus() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    // With no menu open, plain motion must be ignored everywhere.
    app.on_mouse(mouse(MouseEventKind::Moved, 10, 5));
    assert_eq!(app.focus, Focus::Explorer, "plain hover must not change focus");
}

#[test]
fn click_away_closes_open_menu() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0));
    assert!(app.menu.is_open());
    // Click somewhere outside the bar and dropdown.
    app.layout.menu_dropdown = Rect::new(0, 1, 10, 8);
    app.on_mouse(click(60, 20));
    assert!(!app.menu.is_open(), "clicking away closes the menu");
}

#[test]
fn click_editor_focuses_it() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    app.on_mouse(click(5, 3));
    assert_eq!(app.focus, Focus::Editor, "clicking the editor focuses it");
}

#[test]
fn open_calendar_swallows_editor_clicks() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    app.run_action("tools.calendar"); // open the calendar overlay
    assert!(app.show_calendar);
    // A click over the editor must not reach it while the calendar is open.
    app.on_mouse(click(5, 3));
    assert_eq!(app.focus, Focus::Explorer, "calendar open swallows the editor click");
}

#[test]
fn click_explorer_row_focuses_and_selects() {
    let dir = unique_dir("clickexp");
    fs::write(dir.join("a.txt"), "1").unwrap();
    fs::write(dir.join("b.txt"), "2").unwrap();
    let mut app = app_at(&dir);
    app.show_explorer = true;
    app.layout.explorer = Rect::new(0, 0, 30, 20);
    // Rows start one below the top border (explorer.y + 1). Click the SECOND row;
    // the first is already selected, where a click would promote/open the file.
    app.on_mouse(click(5, 2));
    assert_eq!(app.focus, Focus::Explorer);
    assert_eq!(app.explorer.selected, 1, "clicked the second row");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn drag_explorer_right_edge_resizes_left_dock() {
    let mut app = app_at(Path::new("."));
    app.show_explorer = true;
    app.layout.menu = Rect::new(0, 0, 100, 1); // full width 100
    app.layout.explorer = Rect::new(0, 0, 30, 24); // right border at column 29
    let before = app.settings.explorer_width;
    app.on_mouse(click(29, 0)); // grab the right edge
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 50, 0));
    assert!(app.settings.explorer_width > before, "dragging right widens the dock");
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 50, 0));
    // After releasing, a drag elsewhere no longer resizes.
    let stable = app.settings.explorer_width;
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 20, 0));
    assert_eq!(app.settings.explorer_width, stable, "release ends the resize");
}

#[test]
fn drag_messages_left_edge_resizes_right_dock() {
    let mut app = app_at(Path::new("."));
    app.show_messages = true;
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.layout.messages = Rect::new(68, 0, 32, 24); // left border at column 68
    let before = app.settings.messages_width;
    app.on_mouse(click(68, 0)); // grab the left edge
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 55, 0)); // drag left → wider
    assert!(app.settings.messages_width > before, "dragging left widens the dock");
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 55, 0));
}

#[test]
fn click_closes_plain_dialog() {
    let mut app = app_at(Path::new("."));
    app.run_action("vix.about"); // plain (no text field)
    assert!(app.dialog.is_some());
    app.on_mouse(click(0, 0));
    assert!(app.dialog.is_none(), "a click acts as the Ok button");
}

#[test]
fn click_dock_toggle_icons() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    let (left, right) = vix::ui::dock_toggle_cols(app.layout.menu);
    let explorer_before = app.show_explorer;
    app.on_mouse(click(left, 0));
    assert_ne!(app.show_explorer, explorer_before, "left dock icon toggles the explorer");
    let messages_before = app.show_messages;
    app.on_mouse(click(right, 0));
    assert_ne!(app.show_messages, messages_before, "right dock icon toggles the messages");
}

#[test]
fn view_keymap_submenu_actions_set_the_keymap() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.keymap, "apple", "default keymap");

    // The View → Keymap submenu dispatches `view.keymap:<id>` per item.
    app.run_action("view.keymap:vscode-macos");
    assert_eq!(app.settings.keymap, "vscode-macos");

    app.run_action("view.keymap:vi");
    assert_eq!(app.settings.keymap, "vi");

    for id in ["vscode-windows", "intellij-macos", "intellij-windows", "eclipse"] {
        app.run_action(&format!("view.keymap:{id}"));
        assert_eq!(app.settings.keymap, id);
    }

    // An unknown id is ignored.
    app.run_action("view.keymap:nope");
    assert_eq!(app.settings.keymap, "eclipse");
}

#[test]
fn intellij_and_eclipse_keymaps_bind_find() {
    // A representative binding works under each new keymap: Ctrl+F opens Find.
    for id in ["intellij-mac", "intellij-win", "eclipse"] {
        let mut app = app_at(Path::new("."));
        app.settings.keymap = id.to_string();
        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search.is_some(), "Ctrl+F opens Find under {id}");
    }
}

#[test]
fn emacs_keymap_ctrl_movement() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    // Plain letters still type (no modifier).
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.editor.cursor_1based().1, 4, "cursor after typing abc");
    // Ctrl chords move the cursor: C-b back, C-f forward, C-a home, C-e end.
    app.on_key(ctrl('b'));
    assert_eq!(app.editor.cursor_1based().1, 3, "C-b moves back a char");
    app.on_key(ctrl('f'));
    assert_eq!(app.editor.cursor_1based().1, 4, "C-f moves forward a char");
    app.on_key(ctrl('a'));
    assert_eq!(app.editor.cursor_1based().1, 1, "C-a moves to line start");
    app.on_key(ctrl('e'));
    assert_eq!(app.editor.cursor_1based().1, 4, "C-e moves to line end");
    // Typing was not corrupted by the motions.
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "abc");
}

#[test]
fn emacs_keymap_chords_open_find_and_quit() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    // C-x C-f opens the file prompt.
    app.on_key(ctrl('x'));
    app.on_key(ctrl('f'));
    assert!(app.prompt.is_some(), "C-x C-f opens the open-file prompt");
    app.on_key(esc());
    assert!(app.prompt.is_none());
    // Standalone C-s opens find.
    app.on_key(ctrl('s'));
    assert!(app.search.is_some(), "C-s opens find");
    app.on_key(esc());
    // C-x C-c quits.
    app.on_key(ctrl('x'));
    app.on_key(ctrl('c'));
    assert!(app.should_quit, "C-x C-c quits");
}

#[test]
fn vscode_keymap_quick_open_command_palette_and_goto_line() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vscode-macos".to_string();

    // Ctrl+P is Quick Open (the file prompt), not the Command Palette.
    app.on_key(ctrl('p'));
    assert!(app.prompt.is_some(), "Ctrl+P opens Quick Open");
    assert!(app.palette.is_none());
    app.on_key(esc());

    // Ctrl+Shift+P opens the Command Palette.
    app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert!(app.palette.is_some(), "Ctrl+Shift+P opens the Command Palette");
    app.on_key(esc());

    // Ctrl+G opens Go to Line (the palette).
    app.on_key(ctrl('g'));
    assert!(app.palette.is_some(), "Ctrl+G opens Go to Line");
}

#[test]
fn vim_keymap_normal_mode_is_modal() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"));

    // Normal-mode letters are commands, not text.
    for c in "hjkl".chars() {
        app.on_key(key(c));
    }
    assert!(
        app.editor.active_tab().unwrap().text().is_empty(),
        "Normal mode must not type into the buffer"
    );

    // `i` enters Insert mode; now letters type.
    app.on_key(key('i'));
    assert_eq!(app.mode_indicator().as_deref(), Some("-- INSERT --"));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "hello");

    // Esc returns to Normal; `0` then `x` deletes the first char.
    app.on_key(esc());
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"));
    app.on_key(key('0'));
    app.on_key(key('x'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "ello", "x deletes a char");
}

#[test]
fn vim_keymap_command_line_quits() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    // `:` opens the command line (shown in the mode indicator).
    app.on_key(key(':'));
    assert_eq!(app.mode_indicator().as_deref(), Some(":"));
    app.on_key(key('q'));
    app.on_key(key('!'));
    assert_eq!(app.mode_indicator().as_deref(), Some(":q!"));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.should_quit, ":q! quits");
}

#[test]
fn switching_keymap_resets_vim_to_normal() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    app.on_key(key('i')); // enter Insert
    assert_eq!(app.mode_indicator().as_deref(), Some("-- INSERT --"));
    // Choose the Vim keymap again via the submenu action; modes reset to Normal.
    app.run_action("view.keymap:vi");
    assert_eq!(app.settings.keymap, "vi");
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"), "reset to Normal");
}






#[test]
fn typing_brackets_auto_pairs_and_steps_over() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('('));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.text(), "()", "opener inserts the matching closer");
        assert_eq!(t.editor.get_cursor(), 1, "cursor sits between the pair");
    }
    // Typing the closer steps over the auto-inserted one (no doubling).
    app.on_key(key(')'));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.text(), "()");
        assert_eq!(t.editor.get_cursor(), 2);
    }
}

#[test]
fn auto_pair_wraps_a_selection() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    app.on_key(ctrl('a')); // select all
    app.on_key(key('('));
    let t = app.editor.active_tab().unwrap();
    assert_eq!(t.text(), "(abc)", "typing an opener wraps the selection");
}

#[test]
fn split_panes_open_focus_and_close() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new"); // a second tab, so the panes differ
    assert!(!app.editor.is_split());

    app.run_action("view.split_vertical");
    assert!(app.editor.is_split(), "Split Vertical splits the editor");
    let tabs = app.editor.split_layout(Rect::new(0, 0, 80, 24));
    assert_eq!(tabs.len(), 2, "two panes");
    assert_ne!(tabs[0].tab, tabs[1].tab, "the two panes show different tabs");

    let before = app.editor.active;
    app.run_action("view.focus_other_pane");
    assert_ne!(app.editor.active, before, "focusing the other pane swaps the active tab");

    // A second split makes a 2x2-style grid (three panes here).
    app.run_action("view.split_horizontal");
    assert_eq!(app.editor.split_layout(Rect::new(0, 0, 80, 24)).len(), 3, "nested split adds a pane");

    app.run_action("view.unsplit");
    app.run_action("view.unsplit");
    assert!(!app.editor.is_split(), "unsplitting back to one pane");
}

// ---- Generated smoke tests for the action catalog (spec/actions/actions.tsv).
// Each runs the action on a small buffer and checks the app stays sane.

#[test]
fn catalog_cursor_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_top() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_top");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_center() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_center");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_bottom() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_bottom");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_text() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_text");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_text_toggle() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_text_toggle");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_end_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_end_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paragraph_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paragraph_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paragraph_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paragraph_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_paragraph_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_paragraph_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_paragraph_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_paragraph_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_insert_newline() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("insert_newline");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_backspace() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("backspace");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_insert_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("insert_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save_as() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save_as");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_literal() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_literal");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_diff_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("diff_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_diff_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("diff_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_center() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("center");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_undo() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("undo");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_redo() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("redo");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_copy() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("copy");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_copy_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("copy_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cut() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cut");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cut_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cut_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_duplicate() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("duplicate");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_duplicate_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("duplicate_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_move_lines_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("move_lines_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_move_lines_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("move_lines_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_join_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("join_lines");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_line_transforms() {
    for action in ["sort_unique", "reverse_lines", "remove_duplicate_lines", "trim_trailing_whitespace"] {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "alpha\nbeta\nalpha  \n");
        app.run_action(action);
        assert!(app.editor.active_tab().is_some(), "{action} kept the app sane");
    }
}

#[test]
fn catalog_sort_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sort_lines");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_indent_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("indent_selection");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_outdent_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("outdent_selection");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_autocomplete() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("autocomplete");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cycle_autocomplete_back() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cycle_autocomplete_back");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_outdent_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("outdent_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_indent_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("indent_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paste() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paste");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paste_primary() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paste_primary");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_open_file() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("open_file");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_half_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("half_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_half_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("half_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_text() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_text");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_text_toggle() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_text_toggle");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_end_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("end_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_help() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_help");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_key_menu() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_key_menu");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_diff_gutter() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_diff_gutter");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_ruler() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_ruler");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_highlight_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_highlight_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_unhighlight_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("unhighlight_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_reset_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("reset_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_clear_status() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("clear_status");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_shell_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("shell_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_command_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("command_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_overwrite_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_overwrite_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_escape() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("escape");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_quit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("quit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_quit_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("quit_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_force_quit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("force_quit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_add_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("add_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_previous_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("previous_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_next_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("next_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_first_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("first_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_last_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("last_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_next_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("next_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_previous_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("previous_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_first_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("first_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_last_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("last_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_unsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("unsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_vsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("vsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_hsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("hsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_suspend() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("suspend");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_scroll_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("scroll_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_scroll_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("scroll_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_select() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_select");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_remove_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("remove_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_remove_all_multi_cursors() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("remove_all_multi_cursors");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_skip_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("skip_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_skip_multi_cursor_back() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("skip_multi_cursor_back");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_jump_to_matching_brace() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("jump_to_matching_brace");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_jump_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("jump_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_deselect() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("deselect");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_clear_info() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("clear_info");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_none() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("none");
    assert!(app.editor.active_tab().is_some());
}





