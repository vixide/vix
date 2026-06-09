//! Integration tests for Vix's terminal-independent logic.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use vix::app::{App, Focus};
use vix::calendar;
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
fn file_menu_quit_quits_program() {
    let mut app = app_at(Path::new("."));
    assert!(!app.should_quit);

    // Open the menu bar, move right to the File menu, then walk down to "Quit".
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let file_idx = vix::menu::MENUS
        .iter()
        .position(|m| m.name == "menu.file")
        .expect("a File menu exists");
    for _ in 0..file_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    let quit_idx = vix::menu::MENUS[file_idx]
        .items
        .iter()
        .position(|i| i.action == "file.quit")
        .expect("File menu has a Quit item");
    for _ in 0..quit_idx {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // The main loop (main.rs) breaks out as soon as this flag is set, so
    // choosing File -> Quit really does end the program.
    assert!(app.should_quit, "File -> Quit must request exit");
}

#[test]
fn view_themes_menu_switches_theme() {
    let mut app = app_at(Path::new("."));
    assert!(app.theme_chooser.is_none());

    // Open the menu bar, move right to the View menu, and run its first item
    // ("Themes…"), which opens the theme chooser.
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let view_idx = vix::menu::MENUS
        .iter()
        .position(|m| m.name == "menu.view")
        .expect("a View menu exists");
    for _ in 0..view_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.theme_chooser.is_some(), "Themes… opens the chooser");

    // Pick Light deterministically and apply; the choice is persisted. The list
    // is sorted (and includes bundled themes), so find Light's index.
    app.theme_chooser.as_mut().unwrap().selected = builtin_choice_index(&app, vix::theme::Mode::Light);
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.theme_chooser.is_none(), "Enter closes the chooser");
    assert_eq!(app.settings.theme, "light");

    // Reopen via the command action and switch back to Dark.
    app.run_action("view.themes");
    app.theme_chooser.as_mut().unwrap().selected = builtin_choice_index(&app, vix::theme::Mode::Dark);
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, "dark");
}

/// Index of the built-in `mode` within the open theme chooser's sorted choices.
fn builtin_choice_index(app: &App, mode: vix::theme::Mode) -> usize {
    app.theme_chooser
        .as_ref()
        .unwrap()
        .choices
        .iter()
        .position(|c| c.builtin() == Some(mode))
        .expect("built-in mode is in the chooser")
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
    assert!(web.body.contains("github.com/joelparkerhenderson/vix"));
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
fn view_locale_chooser_opens_and_cancels() {
    let mut app = app_at(Path::new("."));
    assert!(app.locale_chooser.is_none());
    app.run_action("view.locale");
    assert!(app.locale_chooser.is_some(), "View -> Locale opens the chooser");
    // Esc cancels without persisting a change (and leaves the global locale as
    // it was, so concurrent tests are unaffected).
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.locale_chooser.is_none(), "Esc closes the chooser");
}

#[test]
fn theme_chooser_lists_bundled_themes() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.themes");
    let tc = app.theme_chooser.as_ref().expect("theme chooser open");
    let names: Vec<&str> = tc.choices.iter().filter_map(|c| c.custom_name()).collect();
    for expected in ["Dracula", "Nord", "Tokyo Night", "Gruvbox Dark", "Solarized Dark"] {
        assert!(
            names.contains(&expected),
            "chooser should list bundled theme {expected}; got {names:?}"
        );
    }
    // The bundled "Dark"/"Light" themes are dropped in favor of the built-in modes.
    assert!(
        !names.iter().any(|n| n.eq_ignore_ascii_case("dark") || n.eq_ignore_ascii_case("light")),
        "built-in modes shadow same-named custom themes; got {names:?}"
    );

    // Every choice — built-ins included — is sorted alphabetically by canonical
    // name (built-ins use their id "dark"/"light").
    let keys: Vec<String> = tc
        .choices
        .iter()
        .map(|c| match c.builtin() {
            Some(m) => m.name().to_string(),
            None => c.custom_name().unwrap().to_lowercase(),
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "theme chooser is sorted alphabetically");
}

#[test]
fn theme_chooser_esc_reverts() {
    let mut app = app_at(Path::new("."));
    // Apply Light first so we have a known baseline.
    app.run_action("view.themes");
    app.theme_chooser.as_mut().unwrap().selected = builtin_choice_index(&app, vix::theme::Mode::Light);
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, "light");

    // Open again, move the highlight to Dark, then cancel with Esc: the
    // persisted theme must stay Light.
    app.run_action("view.themes");
    app.theme_chooser.as_mut().unwrap().selected = builtin_choice_index(&app, vix::theme::Mode::Dark);
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.theme_chooser.is_none());
    assert_eq!(app.settings.theme, "light", "Esc must not persist a change");
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
    let now = calendar::now_local();
    let utc = calendar::utc_iso(&now);
    assert_eq!(utc.len(), 20, "{utc}"); // YYYY-MM-DDTHH:MM:SSZ
    assert!(utc.ends_with('Z'));
    assert_eq!(&utc[4..5], "-");
    assert_eq!(&utc[10..11], "T");

    let week = calendar::iso_week_date(&now);
    assert!(week.contains("-W"), "{week}"); // YYYY-Www-D
    let day = week.chars().last().unwrap();
    assert!(('1'..='7').contains(&day), "weekday digit: {week}");

    let clock = calendar::local_clock(&now);
    assert_eq!(clock.len(), 8, "{clock}"); // HH:MM:SS

    let local = calendar::local_datetime(&now);
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

    // Right pages forward, left pages back to where we started.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_ne!(app.calendar.shown_month(), start, "Right advances the month");
    assert!(app.calendar.grid().today.is_none(), "no today highlight off-month");
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.calendar.shown_month(), start, "Left returns to the start month");

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
fn goto_definition_single_jumps() {
    let dir = unique_dir("gotodef");
    fs::write(dir.join("lib.rs"), "fn target() {}\n").unwrap();
    fs::write(dir.join("main.rs"), "target()\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(dir.join("main.rs")); // cursor at offset 0 → on "target"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.as_ref().unwrap().ends_with("lib.rs"), "jumped to the definition file");
    assert_eq!(app.editor.cursor_1based().0, 1);
    assert!(app.project_search.is_none(), "single match jumps directly");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_definition_multiple_opens_panel() {
    let dir = unique_dir("gotodef2");
    fs::write(dir.join("a.rs"), "fn dup() {}\n").unwrap();
    fs::write(dir.join("b.rs"), "fn dup() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(dir.join("a.rs"));
    app.editor.goto(1, Some(4), Rect::new(0, 0, 80, 24)); // cursor on "dup"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let ps = app.project_search.as_ref().expect("panel of candidates");
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
    app.open_initial(dir.join("pic.png"));
    // No image tab opened, but the user is told why, and nothing panicked.
    assert!(app.editor.active_tab().map(|t| !t.is_image()).unwrap_or(true));
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
    app.open_initial(file);
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
fn ctrl_shift_f_opens_project_search() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
    assert!(app.project_search.is_some(), "Ctrl+Shift+F opens project search");
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
        vix::menu::MENUS.iter().position(|m| m.name == name).unwrap()
    };
    for (letter, name) in [('f', "menu.file"), ('e', "menu.edit"), ('v', "menu.view"), ('h', "menu.help")] {
        let mut app = app_at(Path::new("."));
        app.on_key(alt(letter));
        assert_eq!(app.menu.open, Some(menu_index(name)), "Alt+{letter} opens {name}");
    }
}

#[test]
fn ctrl_z_undoes_and_ctrl_y_redoes() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    let full = app.editor.active_tab().unwrap().lines()[0].len();
    app.on_key(ctrl('z'));
    let undone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(undone < full, "Ctrl+Z undoes typing ({undone} < {full})");
    app.on_key(ctrl('y'));
    let redone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(redone > undone, "Ctrl+Y redoes ({redone} > {undone})");
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
