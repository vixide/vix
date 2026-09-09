#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

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
fn select_all_then_typing_replaces_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.on_key(ctrl('a')); // Ctrl+A selects the whole buffer
    app.on_key(key('x'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "x");
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
fn search_pattern_respects_toggles() {
    let mut sb = SearchBar::new(false);
    sb.flags.remove(SearchFlags::SMART_CASE); // isolate the case/word/regex toggles from smart-case
    sb.query = "Foo.Bar".to_string();
    assert_eq!(sb.pattern().as_deref(), Some(r"(?i)Foo\.Bar"));

    sb.flags
        .insert(SearchFlags::CASE_SENSITIVE | SearchFlags::WHOLE_WORD);
    assert_eq!(sb.pattern().as_deref(), Some(r"\bFoo\.Bar\b"));

    sb.flags.insert(SearchFlags::REGEX);
    sb.flags.remove(SearchFlags::WHOLE_WORD);
    sb.flags.insert(SearchFlags::CASE_SENSITIVE);
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
    app.on_key(KeyEvent::new(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
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
    assert!(
        app.status.contains("of 3"),
        "match total shown: {}",
        app.status
    );
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
    assert!(
        !app.editor.active_tab().unwrap().editor.has_marks(),
        "toggled off"
    );
    app.run_action("toggle_highlight_search");
    assert!(
        app.editor.active_tab().unwrap().editor.has_marks(),
        "toggled back on"
    );

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
    assert_eq!(
        s.field,
        vix::search::Field::Replace,
        "click focused the Replace field"
    );
    assert_eq!(s.replace, "xyz");
    assert!(s.query.is_empty(), "the Find field stayed empty");

    // Clicking the first row focuses the Find field again.
    app.on_mouse(click(2, 5));
    assert_eq!(
        app.search.as_ref().unwrap().field,
        vix::search::Field::Query
    );
}

#[test]
fn ctrl_f_opens_find_and_esc_closes() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('f'));
    let s = app.search.as_ref().expect("Ctrl+F opens search");
    assert!(
        !s.flags.contains(SearchFlags::REPLACING),
        "Ctrl+F is find, not replace"
    );
    app.on_key(esc());
    assert!(app.search.is_none(), "Esc closes the search bar");
}

#[test]
fn ctrl_r_opens_replace() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('r'));
    assert!(
        app.search
            .as_ref()
            .is_some_and(|s| s.flags.contains(SearchFlags::REPLACING)),
        "Ctrl+R opens replace"
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
    assert!(
        app.search.is_some(),
        "search stays open while navigating matches"
    );
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
    let sug = app
        .spell_suggest
        .as_ref()
        .expect("popup opens on a misspelling");
    assert!(!sug.suggestions.is_empty(), "offers suggestions");
    // Apply the highlighted suggestion; the misspelling is gone.
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.spell_suggest.is_none(), "popup closes after applying");
    let line0 = app.editor.active_tab().unwrap().lines()[0].clone();
    assert!(
        !line0.contains("helllo"),
        "misspelling replaced; got: {line0:?}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_dialog_offers_replace_as_a_mode() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "alpha beta alpha", 0);

    // A plain Find, then `Alt+P` turns it into a find-and-replace in place —
    // no closing the box and hunting for a separate Replace command.
    app.run_action("edit.find");
    assert!(
        !app.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REPLACING),
        "Find opens as a find"
    );
    app.on_key(alt(KeyCode::Char('h')));
    let bar = app.search.as_ref().unwrap();
    assert!(
        bar.flags.contains(SearchFlags::REPLACING),
        "Alt+H turns on replace"
    );
    assert_eq!(
        bar.field,
        vix::search::Field::Replace,
        "and puts the cursor where the user just asked to type"
    );

    // Toggling back returns to a plain find without losing the query.
    app.search.as_mut().unwrap().query = "alpha".to_string();
    app.on_key(alt(KeyCode::Char('h')));
    let bar = app.search.as_ref().unwrap();
    assert!(!bar.flags.contains(SearchFlags::REPLACING));
    assert_eq!(bar.field, vix::search::Field::Query);
    assert_eq!(bar.query, "alpha", "the query survives the round trip");
}

#[test]
fn find_dialog_scope_option_widens_the_search() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "needle", 0);

    // Type a query into the find box, then widen the scope: the workspace panel
    // opens already carrying the query and the toggles, which is what replaced
    // the separate "Find in Files…" menu item.
    app.run_action("edit.find");
    app.on_key(alt(KeyCode::Char('h'))); // replace on, to check it carries too
    app.search.as_mut().unwrap().query = "needle".to_string();
    app.search.as_mut().unwrap().replace = "pin".to_string();
    app.search
        .as_mut()
        .unwrap()
        .flags
        .insert(SearchFlags::REGEX);
    app.on_key(alt(KeyCode::Char('i')));

    assert!(app.search.is_none(), "the find box hands over");
    let panel = app
        .workspace_search
        .as_ref()
        .expect("workspace panel opened");
    assert_eq!(panel.query, "needle", "the query came along");
    assert_eq!(panel.replace, "pin", "so did the replacement");
    assert!(
        panel.flags.contains(WorkspaceFlags::REPLACING),
        "and the replace mode"
    );
    assert!(
        panel.flags.contains(WorkspaceFlags::REGEX),
        "and the toggles"
    );

    // The panel is the "Files" stage; widening again lists into the dock.
    app.on_key(alt(KeyCode::Char('i')));
    assert!(
        app.workspace_search.is_none(),
        "the panel hands over in turn"
    );
}
