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
fn select_all_then_typing_replaces_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.on_key(ctrl('a')); // Ctrl+A selects the whole buffer
    app.on_key(key('x'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "x");
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
    app.on_key(ctrl('d')); // Ctrl+D duplicates the cursor line
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["abc", "abc"]);
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
fn smart_home_toggles_first_nonblank_and_column0() {
    let dir = unique_dir("smarthome");
    let file = dir.join("h.txt");
    fs::write(&file, "    hello\n").unwrap(); // four-space indent
    let mut app = app_at(&dir);
    app.open_initial(file);

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
    app.open_initial(file);
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
    app.open_initial(file);

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
fn find_selection_jumps_between_occurrences() {
    let dir = unique_dir("findsel");
    let file = dir.join("f.txt");
    fs::write(&file, "foo bar foo baz foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(file);
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
    app.open_initial(file.clone());
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
    app.open_initial(file.clone());
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
    app.open_initial(file.clone());
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
fn vix_menu_quit_quits_program() {
    let mut app = app_at(Path::new("."));
    assert!(!app.should_quit);

    // Open the menu bar (the Vix menu is first), then walk down to "Quit".
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let vix_idx = vix::menu::MENUS
        .iter()
        .position(|m| m.name == "menu.vix")
        .expect("a Vix menu exists");
    for _ in 0..vix_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    // Walk down until "Quit" is highlighted (Down skips separators, so we cannot
    // assume the number of presses equals the item's array index).
    let item_count = vix::menu::MENUS[vix_idx].items.len();
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

    // Pick Light deterministically and apply; the choice is persisted by name.
    app.theme_chooser.as_mut().unwrap().selected = theme_choice_index(&app, "Light");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.theme_chooser.is_none(), "Enter closes the chooser");
    assert_eq!(app.settings.theme, "Light");

    // Reopen via the command action and switch back to Dark.
    app.run_action("view.theme");
    app.theme_chooser.as_mut().unwrap().selected = theme_choice_index(&app, "Dark");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, "Dark");
}

/// Index of the theme named `name` within the open theme chooser's sorted list.
fn theme_choice_index(app: &App, name: &str) -> usize {
    app.theme_chooser
        .as_ref()
        .unwrap()
        .choices
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("theme {name} is in the chooser"))
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
    app.run_action("view.theme");
    let tc = app.theme_chooser.as_ref().expect("theme chooser open");
    let names: Vec<&str> = tc.choices.iter().map(|c| c.name.as_str()).collect();
    // Dark/Light are now ordinary bundled themes, listed alongside the rest.
    for expected in ["Dark", "Light", "Dracula", "Nord", "Tokyo Night", "Gruvbox Dark"] {
        assert!(
            names.contains(&expected),
            "chooser should list bundled theme {expected}; got {names:?}"
        );
    }

    // The list is sorted alphabetically (case-insensitively) by name.
    let keys: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "theme chooser is sorted alphabetically");
}

#[test]
fn theme_chooser_esc_reverts() {
    let mut app = app_at(Path::new("."));
    // Apply Light first so we have a known baseline.
    app.run_action("view.theme");
    app.theme_chooser.as_mut().unwrap().selected = theme_choice_index(&app, "Light");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings.theme, "Light");

    // Open again, move the highlight to Dark, then cancel with Esc: the
    // persisted theme must stay Light.
    app.run_action("view.theme");
    app.theme_chooser.as_mut().unwrap().selected = theme_choice_index(&app, "Dark");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.theme_chooser.is_none());
    assert_eq!(app.settings.theme, "Light", "Esc must not persist a change");
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
    app.open_initial(file);
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
    app.open_initial(file);
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
    app.open_initial(file);
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

    app.open_initial(dir.join("a.txt"));
    app.open_initial(dir.join("b.txt"));
    assert_eq!(app.settings.recent_files.len(), 2);
    assert!(app.settings.recent_files[0].ends_with("b.txt"), "most-recent first");
    assert!(app.settings.recent_files[1].ends_with("a.txt"));

    // Reopening a recorded file moves it to the front without duplicating.
    app.open_initial(dir.join("a.txt"));
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
    app.open_initial(file);

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
    app.open_initial(dir.join("c.txt"));
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
fn reopen_closed_tab_restores_the_last_closed_file() {
    let dir = unique_dir("reopen");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(dir.join("a.txt"));
    app.open_initial(dir.join("b.txt")); // active: b.txt

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
    app.open_initial(dir.join("a.txt"));
    app.open_initial(dir.join("b.txt"));
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
    app.open_initial(file);

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
fn literal_replace_all_after_preview() {
    let dir = unique_dir("litrep");
    let file = dir.join("l.txt");
    fs::write(&file, "foo foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(file);

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
    // the Find field, row 1 the Replace field.
    app.layout.search = Rect::new(0, 5, 40, 4);
    app.on_mouse(click(2, 6)); // click the Replace row

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

    // The nav arrows sit on the month-header row (row 4): ◀ at col 0, ▶ at col 20.
    app.on_mouse(click(cal.x + 20, cal.y + 4)); // ▶ next month
    assert_ne!(app.calendar.title(), title, "▶ advanced to the next month");
    app.on_mouse(click(cal.x, cal.y + 4)); // ◀ previous month
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

    // Click the first info line (local date-time) → inserts a date-time string.
    app.on_mouse(click(cal.x + 1, cal.y));
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains(':') && text.contains('-'), "inserted a date-time: {text:?}");
    assert!(app.show_calendar, "an in-box click keeps the calendar open");

    // Click a populated day cell (cells are 3 columns wide; the grid starts at
    // row 6 of the box).
    let grid = app.calendar.grid();
    let (wk, col) = grid
        .weeks
        .iter()
        .enumerate()
        .find_map(|(w, week)| week.iter().position(Option::is_some).map(|c| (w, c)))
        .unwrap();
    let before = app.editor.active_tab().unwrap().text().len();
    app.on_mouse(click(cal.x + col as u16 * 3 + 1, cal.y + 6 + wk as u16));
    let after = app.editor.active_tab().unwrap().text();
    assert!(after.len() > before, "clicking a day inserted a date");

    // A click outside the box closes it.
    app.on_mouse(click(0, 23));
    assert!(!app.show_calendar, "an outside click closes the calendar");
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
    let out = app.bottom_dock.lines.join("\n");
    assert!(out.contains("$ echo hello-vix"), "echoes the command: {out:?}");
    assert!(out.contains("hello-vix"), "shows the output: {out:?}");
    assert!(out.contains("[exit 0]"), "shows the exit code: {out:?}");
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
fn menu_dropdown_keeps_a_gap_before_shortcuts() {
    // The dropdown rect must be wide enough that every item with a shortcut keeps
    // at least one space between its label and the right-aligned shortcut.
    let bar = Rect::new(0, 0, 200, 1);
    let frame = Rect::new(0, 0, 200, 40);
    for (i, m) in vix::menu::MENUS.iter().enumerate() {
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
        // (find-related items now live in the Edit → Find submenu; the editor
        // display toggles now live in the View → Editor submenu — so the
        // top-level separators precede the groups/submenus that remain.)
        ("menu.file", &["file.open", "file.close"]),
        ("menu.vix", &["file.quit"]),
        ("menu.edit", &["edit.cut", "edit.toggle_comment"]),
        ("menu.view", &["view.left_dock"]),
    ];
    for (menu, befores) in cases {
        let items = vix::menu::MENUS
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
    let edit_idx = vix::menu::MENUS.iter().position(|m| m.name == "menu.edit").unwrap();
    for _ in 0..edit_idx {
        app.on_key(keycode(KeyCode::Right));
    }
    // Walk down to the Find submenu parent.
    let edit_items = vix::menu::MENUS[edit_idx].items;
    let find_parent = edit_items.iter().position(vix::menu::Item::has_submenu).unwrap();
    for _ in 0..=edit_items.len() {
        if app.menu.item == find_parent {
            break;
        }
        app.on_key(keycode(KeyCode::Down));
    }
    assert_eq!(app.menu.item, find_parent);
    assert!(app.menu.sub.is_none(), "submenu starts closed");

    // Right opens the submenu; its first item is a find action.
    app.on_key(keycode(KeyCode::Right));
    assert!(app.menu.sub.is_some(), "Right opens the submenu");
    assert_eq!(app.menu.selected_action(), Some("edit.find"));

    // Enter runs the nested action (opens the find box) and closes the menu.
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.menu.open.is_none(), "the menu closed");
    assert!(app.search.is_some(), "Find opened the search box");
}

#[test]
fn view_editor_submenu_rolls_up_the_editor_toggles() {
    let view = vix::menu::MENUS.iter().find(|m| m.name == "menu.view").unwrap();
    let editor = view
        .items
        .iter()
        .find(|it| it.has_submenu())
        .and_then(|it| it.submenu)
        .expect("View has an Editor submenu");
    let actions: Vec<&str> = editor.iter().map(|it| it.action).collect();
    assert_eq!(
        actions,
        vec!["view.line_numbers", "view.whitespace", "view.scrollbar", "view.soft_wrap"]
    );
}

#[test]
fn menu_navigation_skips_separators() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let file_idx = vix::menu::MENUS.iter().position(|m| m.name == "menu.file").unwrap();
    for _ in 0..file_idx {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    // Walking the whole menu with Down must never land on (or commit) a separator.
    let len = vix::menu::MENUS[file_idx].items.len();
    for _ in 0..=len {
        let action = app.menu.selected_action().expect("never a separator");
        assert_ne!(action, vix::menu::SEPARATOR);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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
    for m in &vix::menu::MENUS[..idx] {
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
    assert_eq!(app.menu.item, 2, "hover highlights the item under the pointer");
    assert!(app.menu.is_open(), "hover must not commit or close");
    // Move back up to the first item.
    app.on_mouse(mouse(MouseEventKind::Moved, dd.x + 2, dd.y + 1));
    assert_eq!(app.menu.item, 0);
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
fn click_theme_chooser_row_highlights_it() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme");
    let count = app.theme_chooser.as_ref().unwrap().choices.len();
    assert!(count >= 3, "need a few themes to click");
    // The list rect is normally recorded during render; set it directly.
    app.layout.chooser = Rect::new(10, 5, 34, count as u16);
    // Click the third row (index 2).
    app.on_mouse(click(12, 7));
    assert!(app.theme_chooser.is_some(), "a click highlights, it does not close");
    assert_eq!(
        app.theme_chooser.as_ref().unwrap().selected,
        2,
        "clicking a row highlights that theme"
    );
    // A click below the list (out of bounds) is ignored.
    app.on_mouse(click(12, 5 + count as u16 + 3));
    assert_eq!(app.theme_chooser.as_ref().unwrap().selected, 2, "out-of-list click ignored");
}

#[test]
fn view_keyway_chooser_opens_navigates_and_selects() {
    let mut app = app_at(Path::new("."));
    assert!(app.keyway_chooser.is_none());
    assert_eq!(app.settings.keyway, "apple", "default keyway");

    app.run_action("view.keyway");
    assert!(app.keyway_chooser.is_some(), "View -> Keyway opens the chooser");

    // Down moves Apple → Emacs; Enter commits and persists it.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.keyway_chooser.is_none(), "Enter closes the chooser");
    assert_eq!(app.settings.keyway, "emacs");

    // Reopen, move, then Esc reverts (the committed keyway is unchanged).
    app.run_action("view.keyway");
    app.on_key(keycode(KeyCode::Down));
    app.on_key(esc());
    assert!(app.keyway_chooser.is_none(), "Esc closes the chooser");
    assert_eq!(app.settings.keyway, "emacs", "Esc does not change the keyway");
}

#[test]
fn click_keyway_chooser_row_highlights_it() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.keyway");
    assert!(app.keyway_chooser.is_some());
    app.layout.chooser = Rect::new(10, 5, 34, 3);
    // Click the second row (index 1 → Emacs).
    app.on_mouse(click(12, 6));
    assert_eq!(app.keyway_chooser.as_ref().unwrap().selected, 1, "click highlights the row");
    // Committing with Enter persists the clicked keyway.
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.settings.keyway, "emacs");
}

#[test]
fn emacs_keyway_ctrl_movement() {
    let mut app = app_at(Path::new("."));
    app.settings.keyway = "emacs".to_string();
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
fn emacs_keyway_chords_open_find_and_quit() {
    let mut app = app_at(Path::new("."));
    app.settings.keyway = "emacs".to_string();
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
fn vim_keyway_normal_mode_is_modal() {
    let mut app = app_at(Path::new("."));
    app.settings.keyway = "vim".to_string();
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
fn vim_keyway_command_line_quits() {
    let mut app = app_at(Path::new("."));
    app.settings.keyway = "vim".to_string();
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
fn switching_keyway_resets_vim_to_normal() {
    let mut app = app_at(Path::new("."));
    app.settings.keyway = "vim".to_string();
    app.on_key(key('i')); // enter Insert
    assert_eq!(app.mode_indicator().as_deref(), Some("-- INSERT --"));
    // Choose the Vim keyway again via the chooser; modes reset to Normal.
    app.run_action("view.keyway");
    // Move selection to Vim and commit.
    app.keyway_chooser.as_mut().unwrap().selected = 2;
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.settings.keyway, "vim");
    assert_eq!(app.mode_indicator().as_deref(), Some("-- NORMAL --"), "reset to Normal");
}





