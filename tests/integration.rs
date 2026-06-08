//! Integration tests for STRIDE's terminal-independent logic.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use stride::app::{App, Focus};
use stride::datetime;
use stride::fileops;
use stride::palette::{fuzzy_match, parse_path_target};
use stride::search::SearchBar;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Build an app with a realistic editor viewport so the code editor's
/// scroll-into-view logic has a sane area to work with.
fn app_at(root: &Path) -> App {
    let mut app = App::new(root.to_path_buf());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stride-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
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
    app.open_initial(file.clone());
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
    app.open_initial(file);
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
fn line_number_toggle() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.line_numbers;
    app.run_action("tools.line_numbers");
    assert_ne!(before, app.editor.line_numbers);
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
    app.open_initial(file.clone());

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
fn interactive_query_replace_y_n_y() {
    let dir = unique_dir("qr");
    let file = dir.join("q.txt");
    fs::write(&file, "foo foo foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(file);

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
    app.open_initial(file);

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
    let now = datetime::now_local();
    let utc = datetime::utc_iso(&now);
    assert_eq!(utc.len(), 20, "{utc}"); // YYYY-MM-DDTHH:MM:SSZ
    assert!(utc.ends_with('Z'));
    assert_eq!(&utc[4..5], "-");
    assert_eq!(&utc[10..11], "T");

    let week = datetime::iso_week_date(&now);
    assert!(week.contains("-W"), "{week}"); // YYYY-Www-D
    let day = week.chars().last().unwrap();
    assert!(('1'..='7').contains(&day), "weekday digit: {week}");

    let clock = datetime::local_clock(&now);
    assert_eq!(clock.len(), 8, "{clock}"); // HH:MM:SS

    let grid = datetime::month_grid(&now);
    let count: usize = grid.weeks.iter().flatten().filter(|c| c.is_some()).count();
    assert!((28..=31).contains(&count), "days in month: {count}");
}

#[test]
fn narrow_editor_does_not_panic() {
    // The code editor's focus() underflows on tiny widths; the app clamps the
    // viewport it hands over, so typing into a 5-column editor must not panic.
    let mut app = App::new(PathBuf::from("."));
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
    app.open_initial(dir.join("a.txt")); // buffer open on the file
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
    app.open_initial(dir.join("a.txt"));
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
fn project_search_finds_matches_across_files() {
    let dir = unique_dir("psearch");
    fs::write(dir.join("a.txt"), "alpha beta\nbeta gamma\n").unwrap();
    fs::write(dir.join("b.txt"), "delta beta\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.project");
    for c in "beta".chars() {
        app.on_key(key(c));
    }
    let ps = app.project_search.as_ref().unwrap();
    assert_eq!(ps.hits.len(), 3, "two in a.txt, one in b.txt");
    let expected = ps.selected_hit().unwrap();
    let expected_name = expected.path.file_name().unwrap().to_owned();
    let expected_line = expected.line;

    // Enter opens the selected match and jumps to it.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.project_search.is_none());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.as_ref().unwrap().ends_with(&expected_name));
    assert_eq!(app.editor.cursor_1based().0, expected_line);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn project_replace_rewrites_files() {
    let dir = unique_dir("preplace");
    fs::write(dir.join("a.txt"), "beta and beta\n").unwrap();
    fs::write(dir.join("b.txt"), "gamma beta\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("search.project_replace");
    for c in "beta".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to replace field
    for c in "ZZ".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // replace all in project

    let a = fs::read_to_string(dir.join("a.txt")).unwrap();
    let b = fs::read_to_string(dir.join("b.txt")).unwrap();
    assert_eq!(a, "ZZ and ZZ\n");
    assert_eq!(b, "gamma ZZ\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detects_image_extensions() {
    use stride::editor::is_image_path;
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
    app.open_initial(dir.join("pic.png"));
    // No image tab opened, but the user is told why, and nothing panicked.
    assert!(app.editor.active_tab().map(|t| !t.is_image()).unwrap_or(true));
    assert!(app.messages.items.len() > before, "a warning was added");
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
