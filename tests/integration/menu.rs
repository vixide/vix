#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn vix_menu_license_shows_trademark_info() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    // The Vix menu offers a License item dispatching `vix.license`.
    let vixm = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.vix")
        .expect("vix menu");
    assert!(
        vix::menu::menus()[vixm]
            .items
            .iter()
            .any(|it| it.action == "vix.license"),
        "Vix menu has a License item"
    );
    app.run_action("vix.license");
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        screen.contains("trademarks"),
        "the license screen shows trademark information"
    );
}

#[test]
fn view_toggle_menu_tooltips_hides_them() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    assert!(app.settings.show_menu_tooltips, "tooltips on by default");
    app.run_action("view.menu_tooltips");
    assert!(!app.settings.show_menu_tooltips, "toggled off");

    // With tooltips off, opening a menu and highlighting an item shows no help.
    let file = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.file")
        .expect("file menu");
    app.menu.open_index(file);
    app.menu.highlight_item(0);
    let prefix: String = vix::menu::menus()[file].items[0]
        .help()
        .expect("file.new help")
        .chars()
        .take(20)
        .collect();
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        !screen.contains(&prefix),
        "no tooltip is drawn while menu tooltips are toggled off"
    );
}

#[test]
fn toggle_key_menu_shows_the_shortcuts_overlay() {
    let mut app = app_at(Path::new("."));
    assert!(app.help.is_none());
    app.run_action("toggle_key_menu");
    assert!(app.help.is_some(), "key menu opens the shortcuts overlay");
    app.run_action("toggle_key_menu");
    assert!(app.help.is_none());
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
    assert!(
        about.body.starts_with("Vix "),
        "About shows the version: {}",
        about.body
    );
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
    assert!(
        app.dialog.is_some(),
        "Enter is handled by the text field, not a close"
    );
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
    let view = vix::menu::menus()
        .iter()
        .find(|m| m.name == "menu.view")
        .unwrap();
    let sub = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.locale")
        .and_then(|it| it.submenu)
        .expect("Locale is a submenu");
    let actions: Vec<&str> = sub.iter().map(|it| it.action).collect();
    for code in ["view.locale:en", "view.locale:fr", "view.locale:ja"] {
        assert!(
            actions.contains(&code),
            "locale submenu offers {code}; got {actions:?}"
        );
    }
}

#[test]
fn view_time_zone_submenu_lists_zones() {
    let _app = app_at(Path::new("."));
    let view = vix::menu::menus()
        .iter()
        .find(|m| m.name == "menu.view")
        .unwrap();
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
fn view_theme_submenu_lists_bundled_themes() {
    // app_at builds an App, which populates the View → Theme submenu from the
    // available themes.
    let _app = app_at(Path::new("."));
    let view = vix::menu::menus()
        .iter()
        .find(|m| m.name == "menu.view")
        .unwrap();
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
        vix::menu::menus()
            .iter()
            .position(|m| m.name == name)
            .unwrap()
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
        assert_eq!(
            app.menu.open,
            Some(menu_index(name)),
            "Alt+{letter} opens {name}"
        );
    }
}

#[test]
fn alt_letter_toggles_the_menu_it_names() {
    let menu_index = |name: &str| {
        vix::menu::menus()
            .iter()
            .position(|m| m.name == name)
            .unwrap()
    };
    let mut app = app_at(Path::new("."));

    // Alt+F opens File; pressing it again closes the dropdown.
    app.on_key(alt(KeyCode::Char('f')));
    assert_eq!(app.menu.open, Some(menu_index("menu.file")));
    app.on_key(alt(KeyCode::Char('f')));
    assert!(!app.menu.is_open(), "Alt+F again closes the File menu");

    // Another menu's letter switches to it rather than closing.
    app.on_key(alt(KeyCode::Char('f')));
    app.on_key(alt(KeyCode::Char('e')));
    assert_eq!(
        app.menu.open,
        Some(menu_index("menu.edit")),
        "Alt+E switches from File to Edit"
    );

    // The toggle reaches down through an open submenu too.
    let edit_items = vix::menu::menus()[menu_index("menu.edit")].items;
    let sub_row = edit_items
        .iter()
        .position(vix::menu::Item::has_submenu)
        .expect("the Edit menu has a submenu");
    app.menu.highlight_item(sub_row);
    app.menu.right();
    assert!(app.menu.submenu_open(), "the submenu is open");
    app.on_key(alt(KeyCode::Char('e')));
    assert!(
        !app.menu.is_open(),
        "Alt+E closes Edit from inside a submenu"
    );

    // A letter that names no menu is still ignored while one is open.
    app.on_key(alt(KeyCode::Char('e')));
    app.on_key(alt(KeyCode::Char('z')));
    assert_eq!(
        app.menu.open,
        Some(menu_index("menu.edit")),
        "an unassigned mnemonic leaves the open menu alone"
    );
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
            assert!(
                pad >= 1,
                "{}/{} label and shortcut touch",
                m.name,
                it.action
            );
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
            assert!(
                at > 0 && items[at - 1].is_separator(),
                "{menu}: separator before {action}"
            );
        }
    }
}

#[test]
fn submenu_opens_and_runs_a_nested_action() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::F(10)));
    let edit_idx = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.edit")
        .unwrap();
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
    assert_eq!(
        app.menu.selected_action(),
        Some("edit.find"),
        "first submenu item highlighted"
    );

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
    app.context_menu = Some(vix::app::ContextMenu {
        selected: 4,
        x: 0,
        y: 0,
    });
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.context_menu.is_none(), "Enter closes the context menu");
    assert!(
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .get_selection_text()
            .is_some(),
        "the Select All action ran",
    );
}

#[test]
fn view_editor_submenu_rolls_up_the_editor_toggles() {
    let view = vix::menu::menus()
        .iter()
        .find(|m| m.name == "menu.view")
        .unwrap();
    let editor = view
        .items
        .iter()
        .find(|it| it.label == "menu.item.view.editor")
        .and_then(|it| it.submenu)
        .expect("View has an Editor submenu");
    let actions: Vec<&str> = editor
        .iter()
        .map(|it| it.action)
        .filter(|a| a.starts_with("view."))
        .collect();
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
    let view = vix::menu::menus()
        .iter()
        .find(|m| m.name == "menu.view")
        .unwrap();
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
fn menu_type_ahead_selects_by_first_letter() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::F(10)));
    let file_idx = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.file")
        .unwrap();
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
    assert_eq!(
        app.menu.selected_action(),
        Some("file.switch_project"),
        "wraps around"
    );

    // A different letter jumps elsewhere (C → Close).
    app.on_key(key('c'));
    assert_eq!(app.menu.selected_action(), Some("file.close"));
}

#[test]
fn menu_navigation_skips_separators() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE));
    let file_idx = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.file")
        .unwrap();
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
fn click_menu_bar_opens_menu() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    // Column 2 falls inside the first menu's " Vix " label (cols 1..6).
    app.on_mouse(click(2, 0));
    assert_eq!(app.menu.open, Some(0), "clicking the bar opens that menu");
}

#[test]
fn click_the_open_menu_name_again_closes_it() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    // Column 2 falls inside the first menu's " Vix " label (cols 1..6).
    app.on_mouse(click(2, 0));
    assert_eq!(app.menu.open, Some(0), "the first click opens the menu");
    // Clicking the same name again toggles the dropdown shut.
    app.on_mouse(click(2, 0));
    assert!(
        !app.menu.is_open(),
        "clicking the open menu's own name closes it"
    );
    // ...and a third click reopens it, rather than staying shut.
    app.on_mouse(click(2, 0));
    assert_eq!(app.menu.open, Some(0), "clicking again reopens the menu");
    // Clicking a *different* name still switches menus instead of closing.
    let second = top_menu_col(&app, 1);
    app.on_mouse(click(second, 0));
    assert_eq!(
        app.menu.open,
        Some(1),
        "clicking another name switches to that menu"
    );
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
fn transpose_submenu_actions_swap_the_units_around_the_cursor() {
    let mut app = app_at(Path::new("."));

    // Characters and words (the pre-existing pair, now under Edit → Transpose).
    buffer_with(&mut app, "ab", 1);
    app.run_action("edit.transpose_chars");
    assert_eq!(app.editor.active_tab().unwrap().text(), "ba");
    buffer_with(&mut app, "foo bar", 5);
    app.run_action("edit.transpose_words");
    assert_eq!(app.editor.active_tab().unwrap().text(), "bar foo");

    // Lines: the cursor's line swaps with the one above it.
    buffer_with(&mut app, "one\ntwo\nthree\n", 5);
    app.run_action("edit.transpose_lines");
    assert_eq!(app.editor.active_tab().unwrap().text(), "two\none\nthree\n");

    // Sentences: the separator between them stays put.
    buffer_with(&mut app, "One. Two. Three.", 5);
    app.run_action("edit.transpose_sentences");
    assert_eq!(app.editor.active_tab().unwrap().text(), "Two. One. Three.");

    // Paragraphs: blank-line delimited blocks, blank lines preserved.
    buffer_with(&mut app, "a1\na2\n\nb1\nb2\n", 6);
    app.run_action("edit.transpose_paragraphs");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "b1\nb2\n\na1\na2\n"
    );

    // Sections: two or more blank lines delimit; a single blank line does not.
    buffer_with(&mut app, "a\n\nb\n\n\nc\n", 9);
    app.run_action("edit.transpose_sections");
    assert_eq!(app.editor.active_tab().unwrap().text(), "c\n\n\na\n\nb\n");

    // No pair above the first line: a no-op rather than a scramble.
    buffer_with(&mut app, "one\ntwo\n", 0);
    app.run_action("edit.transpose_lines");
    assert_eq!(app.editor.active_tab().unwrap().text(), "one\ntwo\n");
}

#[test]
fn delete_submenu_actions_remove_the_unit_at_the_cursor() {
    let mut app = app_at(Path::new("."));

    // Character: the one under the cursor goes, the cursor stays.
    buffer_with(&mut app, "abc", 1);
    app.run_action("edit.delete.character");
    assert_eq!(app.editor.active_tab().unwrap().text(), "ac");

    // Word: taken with the spacing after it, so the line closes up.
    buffer_with(&mut app, "one two three", 4);
    app.run_action("edit.delete.word");
    assert_eq!(app.editor.active_tab().unwrap().text(), "one three");

    // Sentence: the line break after it is left alone.
    buffer_with(&mut app, "One. Two.\nThree.", 5);
    app.run_action("edit.delete.sentence");
    assert_eq!(app.editor.active_tab().unwrap().text(), "One.\nThree.");

    // Paragraph: the blank lines separating it from the next one go too.
    buffer_with(&mut app, "a1\na2\n\nb1\nb2\n", 0);
    app.run_action("edit.delete.paragraph");
    assert_eq!(app.editor.active_tab().unwrap().text(), "b1\nb2\n");

    // Section: two or more blank lines delimit; a single blank line does not.
    buffer_with(&mut app, "a\n\nb\n\n\nc\n", 0);
    app.run_action("edit.delete.section");
    assert_eq!(app.editor.active_tab().unwrap().text(), "c\n");

    // Nothing at the end of the buffer: a no-op rather than a scramble.
    buffer_with(&mut app, "abc", 3);
    app.run_action("edit.delete.character");
    assert_eq!(app.editor.active_tab().unwrap().text(), "abc");
}
