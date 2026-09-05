#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn help_overlay_includes_the_active_keymap_chords() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "spacemacs".to_string();
    app.run_action("help.shortcuts");
    let h = app.help.as_ref().expect("open");
    assert!(
        h.rows.iter().any(|s| s.keys == "SPC f f"),
        "Spacemacs leader chords listed: {:?}",
        h.rows.iter().map(|s| &s.keys).collect::<Vec<_>>()
    );
    app.help = None;
    app.settings.keymap = "emacs".to_string();
    app.run_action("help.shortcuts");
    let h = app.help.as_ref().expect("open");
    assert!(
        h.rows.iter().any(|s| s.keys == "Ctrl X Ctrl F"),
        "Emacs Ctrl X chords listed"
    );
    // The chord's second key is a bare, unprefixed token ("b", not
    // "C-b") -- it must display lowercase and unchanged, not uppercased
    // to "B" (improvement plan T145: modifier_token_display now renders
    // every keymap's chords, including Emacs's, and must only uppercase
    // a key when a modifier prefix was actually found).
    assert!(
        h.rows.iter().any(|s| s.keys == "Ctrl X b"),
        "Emacs C-x b chord shows its bare second key lowercase: {:?}",
        h.rows.iter().map(|s| &s.keys).collect::<Vec<_>>()
    );
}

#[test]
fn spacemacs_normal_mode_shares_the_vi_vocabulary() {
    // Spacemacs Normal mode delegates to the same handler as the Vi keymap, so
    // the operators (gg / G / dd) work identically.
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "spacemacs".to_string();
    app.on_key(key('i'));
    for c in "one\ntwo".chars() {
        if c == '\n' {
            app.on_key(keycode(KeyCode::Enter));
        } else {
            app.on_key(key(c));
        }
    }
    app.on_key(esc());
    app.on_key(key('g'));
    app.on_key(key('g'));
    assert_eq!(app.editor.cursor_1based().0, 1, "gg goes to the top");
    app.on_key(key('d'));
    app.on_key(key('d'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "two",
        "dd cuts the line"
    );
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
    assert_eq!(
        app.mode_indicator().as_deref(),
        Some("SPC "),
        "leader pending"
    );
    app.on_key(key('w'));
    app.on_key(key('/'));
    assert!(app.editor.is_split(), "SPC w / split the editor");
    assert_eq!(
        app.mode_indicator().as_deref(),
        Some("-- NORMAL --"),
        "leader cleared"
    );
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

    for id in [
        "vscode-windows",
        "intellij-macos",
        "intellij-windows",
        "eclipse",
        "sublime",
    ] {
        app.run_action(&format!("view.keymap:{id}"));
        assert_eq!(app.settings.keymap, id);
    }

    // An unknown id is ignored.
    app.run_action("view.keymap:nope");
    assert_eq!(app.settings.keymap, "sublime");
}

#[test]
fn an_unrecognized_persisted_keymap_id_is_reported_and_corrected_at_startup() {
    // T146: `App::set_keymap` (the `view.keymap:*` path above) only ever
    // writes an id `vix_keymap_model::by_id` already accepted, so the only
    // way `settings.keymap` holds an unrecognized id at all is a
    // hand-edited (or otherwise corrupted) `settings.toml` loaded fresh —
    // simulated here by constructing `Settings` directly.
    let settings = Settings {
        keymap: "intellij-mac".to_string(), // a real, once-made typo (T104d)
        ..Settings::default()
    };
    let app = app_with(settings);
    assert_eq!(
        app.settings.keymap, "apple",
        "an unknown persisted keymap id falls back to the default"
    );
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Error)),
        "the unknown id is reported, not silently swallowed"
    );
}

#[test]
fn sublime_keymap_signature_bindings() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "sublime".to_string();
    // Ctrl+Shift+D duplicates the current line.
    type_str(&mut app, "solo");
    app.on_key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[..2],
        ["solo", "solo"],
        "Ctrl+Shift+D duplicates"
    );
    // Ctrl+J joins the two lines (from the first line).
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Home));
    app.on_key(ctrl('j'));
    assert!(
        app.editor.active_tab().unwrap().lines()[0].contains("solo solo"),
        "Ctrl+J joins lines: {:?}",
        app.editor.active_tab().unwrap().lines()
    );
    // Ctrl+Shift+P opens the command palette.
    app.on_key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(app.palette.is_some(), "Ctrl+Shift+P opens the palette");
}

#[test]
fn intellij_and_eclipse_keymaps_bind_find() {
    // A representative binding works under each new keymap: Ctrl+F opens
    // Find. (These ids must be the real `vix-keymap-model` ones —
    // "intellij-macos"/"intellij-windows", not "intellij-mac"/
    // "intellij-win" — or `Keymap::from_id` silently falls back to
    // `Keymap::Apple`, which happens to bind Ctrl+F to Find too, so a
    // wrong id here would still pass without testing IntelliJ at all;
    // found exactly that bug converting `intellij_key` for T104d.)
    for id in ["intellij-macos", "intellij-windows", "eclipse"] {
        let mut app = app_at(Path::new("."));
        app.settings.keymap = id.to_string();
        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search.is_some(), "Ctrl+F opens Find under {id}");
    }
}

// ----- vix-keybindings registry conversion (improvement plan T104d) -------
// intellij_key now dispatches through vix_keybindings::lookup instead of
// its own hardcoded match; these cover the platform divergence between
// intellij-macos and intellij-windows (the "go to" family uses different
// keys entirely) and the unguarded-Shift quirk faithfully preserved from
// the original dispatch, neither exercised by the test above.

#[test]
fn intellij_go_to_family_differs_by_platform() {
    // macOS: Ctrl+O goes to symbol, Ctrl+L goes to line.
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "intellij-macos".to_string();
    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.palette.is_some(), "macOS Ctrl+O opens Go to Symbol");
    app.on_key(esc());
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert!(app.palette.is_some(), "macOS Ctrl+L opens Go to Line");
    app.on_key(esc());
    // The same two keys do nothing IntelliJ-specific on Windows (no
    // binding for either at all -- falls through, and the plain char
    // still doesn't type since focus is Editor and it's Ctrl-held).
    app.settings.keymap = "intellij-windows".to_string();
    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.palette.is_none(), "Windows Ctrl+O is not bound");

    // Windows: Ctrl+N goes to symbol, Ctrl+G goes to line -- the mirror
    // image of macOS's O/L.
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert!(app.palette.is_some(), "Windows Ctrl+N opens Go to Symbol");
    app.on_key(esc());
    app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(app.palette.is_some(), "Windows Ctrl+G opens Go to Line");
}

#[test]
fn intellij_unguarded_shift_quirk_is_preserved() {
    // Neither the original macOS Ctrl+N arm nor the original Windows
    // Ctrl+G arm checked Shift at all, so the Shift variant does the same
    // thing as the plain one on each platform -- not a bug T104d's
    // conversion introduced, a faithful transcription of it.
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "intellij-macos".to_string();
    let tabs_before = app.editor.tabs.len();
    app.on_key(KeyEvent::new(
        KeyCode::Char('n'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before + 1,
        "Ctrl+Shift+N still resolves to file.new on macOS (a new tab), not Go to File"
    );

    app.settings.keymap = "intellij-windows".to_string();
    app.on_key(KeyEvent::new(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(
        app.palette.is_some(),
        "Ctrl+Shift+G still opens Go to Line on Windows, same as plain Ctrl+G"
    );
}

// ----- vix-keybindings registry conversion (improvement plan T104e) -------
// eclipse_key now dispatches through vix_keybindings::lookup instead of its
// own hardcoded match; these cover the Shift-bit-vs-char-case
// disambiguation (same subtlety as T104c/T104d) and the Alt-only word
// completion binding, neither exercised by
// `intellij_and_eclipse_keymaps_bind_find` above.

#[test]
fn eclipse_keymap_distinguishes_ctrl_from_ctrl_shift() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "eclipse".to_string();
    // Plain Ctrl+W closes the active tab; Ctrl+Shift+W closes all tabs --
    // a terminal reports Ctrl+Shift+w as a lowercase 'w' with the Shift
    // bit set, not an uppercase 'W', so these must not collide.
    app.run_action("file.new");
    app.run_action("file.new");
    let tabs_before = app.editor.tabs.len();
    assert!(tabs_before >= 2);
    app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before - 1,
        "Ctrl+W closes just the active tab"
    );
    app.on_key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(
        app.editor.tabs.len(),
        1,
        "Ctrl+Shift+W closes all tabs, leaving exactly the one empty buffer \
         file.close_all always leaves behind"
    );
}

#[test]
fn eclipse_keymap_alt_slash_completes_a_word_distinct_from_ctrl_slash() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "eclipse".to_string();
    // Alt+/ (word completion) is a distinct binding from Ctrl+/ (toggle
    // comment) -- Alt is only examined once Ctrl is confirmed absent.
    type_str(&mut app, "function\nfun");
    app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::ALT));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "function\nfunction",
        "Alt+/ completes the word, same as the autocomplete action"
    );
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
    // C-x C-f opens the file browser (find-file).
    app.on_key(ctrl('x'));
    app.on_key(ctrl('f'));
    assert!(app.file_browser.is_some(), "C-x C-f opens the file browser");
    app.on_key(esc());
    assert!(app.file_browser.is_none());
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
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "ello",
        "x deletes a char"
    );
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
fn vim_keymap_operators_and_motions() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    app.on_key(key('i'));
    for c in "one\ntwo\nthree".chars() {
        if c == '\n' {
            app.on_key(keycode(KeyCode::Enter));
        } else {
            app.on_key(key(c));
        }
    }
    app.on_key(esc());

    // `gg` jumps to the first line, `G` to the last.
    app.on_key(key('g'));
    app.on_key(key('g'));
    assert_eq!(app.editor.cursor_1based().0, 1, "gg goes to the top");
    app.on_key(key('G'));
    assert_eq!(app.editor.cursor_1based().0, 3, "G goes to the bottom");

    // `dd` cuts the current line; `u` undoes it.
    app.on_key(key('g'));
    app.on_key(key('g'));
    app.on_key(key('d'));
    app.on_key(key('d'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "two",
        "dd cut line one"
    );
    app.on_key(key('u'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "one",
        "u undoes the cut"
    );

    // `w` moves to the next word start.
    app.on_key(key('g'));
    app.on_key(key('g'));
    app.on_key(key('w'));
    assert_eq!(
        app.editor.cursor_1based(),
        (2, 1),
        "w jumps to the next word (line 2)"
    );
}

// ----- vix-keybindings registry conversion (improvement plan T104b) -------
// vim_normal_key now dispatches through vix_keybindings::lookup instead of
// its own hardcoded match; these cover the insert-mode-entry compound
// actions (a/A/I/o/O -- each now its own "vim.*" action id, T104b) and
// yy, none of which the existing Vim tests above happened to exercise.

#[test]
fn vim_keymap_insert_entry_variants_and_yank() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    app.on_key(key('i'));
    type_str(&mut app, "abc");
    app.on_key(esc());

    // `A` (vim.append_end): enters Insert at the end of the line.
    app.on_key(key('A'));
    assert_eq!(app.mode_indicator().as_deref(), Some("-- INSERT --"));
    app.on_key(key('!'));
    app.on_key(esc());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "abc!");

    // `0` back to column 1, `I` (vim.insert_line_start): enters Insert
    // before the first character.
    app.on_key(key('0'));
    app.on_key(key('I'));
    app.on_key(key('>'));
    app.on_key(esc());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], ">abc!");

    // `a` (vim.append): enters Insert one char to the right of the cursor.
    app.on_key(key('0'));
    app.on_key(key('a'));
    app.on_key(key('X'));
    app.on_key(esc());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], ">Xabc!");

    // `o` (vim.open_below) / `O` (vim.open_above): open a new line and
    // enter Insert on it.
    app.on_key(key('o'));
    app.on_key(key('2'));
    app.on_key(esc());
    app.on_key(key('O'));
    app.on_key(key('1'));
    app.on_key(esc());
    let lines = app.editor.active_tab().unwrap().lines();
    assert_eq!(
        lines,
        vec![">Xabc!".to_string(), "1".to_string(), "2".to_string()]
    );

    // `yy` copies the current line without changing the buffer — unlike
    // `dd`, already covered above. (The paste round-trip isn't asserted
    // here: the clipboard is process-shared, so parallel tests could race
    // it, the same reason `emacs_keymap_meta_and_window_chords` skips it.)
    let before = app.editor.active_tab().unwrap().text();
    app.on_key(key('y'));
    app.on_key(key('y'));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        before,
        "yy doesn't mutate"
    );
}

#[test]
fn vim_command_line_goes_to_line() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vi".to_string();
    app.on_key(key('i'));
    for _ in 0..4 {
        app.on_key(keycode(KeyCode::Enter));
    }
    app.on_key(esc());
    // `:3` jumps to line 3.
    app.on_key(key(':'));
    app.on_key(key('3'));
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.editor.cursor_1based().0, 3, ":3 goes to line 3");
}

#[test]
fn emacs_keymap_meta_and_window_chords() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    // M-x opens the command palette.
    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT));
    assert!(app.palette.is_some(), "M-x opens the palette");
    app.on_key(esc());
    // C-x 2 splits the window; C-x 1 unsplits.
    app.on_key(ctrl('x'));
    app.on_key(key('2'));
    assert!(app.editor.split_root.is_some(), "C-x 2 splits");
    app.on_key(ctrl('x'));
    app.on_key(key('1'));
    assert!(app.editor.split_root.is_none(), "C-x 1 unsplits");
    // C-k kills the whole line. (The yank round-trip is not asserted here: the
    // clipboard is process-shared, so parallel tests could race it.)
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    app.on_key(ctrl('k'));
    assert!(
        app.editor.active_tab().unwrap().text().is_empty(),
        "C-k kills the line"
    );
    // C-x b opens the buffer switcher (palette `#` mode).
    app.on_key(ctrl('x'));
    app.on_key(key('b'));
    assert!(app.palette.is_some(), "C-x b opens the buffer switcher");
}

// ----- vix-keybindings registry conversion (improvement plan T104a) -------
// The Emacs keymap dispatch above this point now goes through
// `vix_keybindings::lookup` instead of its own hardcoded matches; these
// cover contexts/bindings the existing Emacs tests above didn't touch.

#[test]
fn emacs_keymap_meta_go_first_and_go_last() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    type_str(&mut app, "one\ntwo\nthree");
    // A-< (Meta <) goes to the document start; A-> (Meta >) to its end —
    // folded into the same top-level table as the Ctrl bindings (T104a),
    // no longer a separate hardcoded `emacs_meta_key` match.
    app.on_key(alt(KeyCode::Char('<')));
    assert_eq!(
        app.editor.cursor_1based(),
        (1, 1),
        "A-< goes to document start"
    );
    app.on_key(alt(KeyCode::Char('>')));
    assert_eq!(
        app.editor.cursor_1based(),
        (3, 6), // end of "three" (5 chars), 1-based column 6
        "A-> goes to document end"
    );
}

#[test]
fn emacs_keymap_ctrl_c_ctrl_x_footnote_chord() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    type_str(&mut app, "some text");
    if let Some(t) = app.editor.active_tab_mut() {
        t.editor.set_cursor(4);
    }
    // C-c C-x f (org.footnote) — the "C-c C-x" context, previously untested
    // even via the raw action id.
    app.on_key(ctrl('c'));
    app.on_key(ctrl('x'));
    app.on_key(key('f'));
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.starts_with("some[fn:1] text"), "{text:?}");
}

#[test]
fn emacs_keymap_ctrl_c_p_c_project_chord_resolves_to_the_real_action() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "emacs".to_string();
    // C-c p c c (project.compile) — the "C-c p c" context. This repo is a
    // real Cargo project, so the chord should resolve all the way to
    // `open_project_command_prompt` and open a real prompt.
    app.on_key(ctrl('c'));
    app.on_key(key('p'));
    app.on_key(key('c'));
    app.on_key(key('c'));
    assert!(app.prompt.is_some(), "project.compile should open a prompt");
}

#[test]
fn vscode_keymap_split_panel_and_delete_line() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vscode-macos".to_string();
    // Ctrl+\ splits the editor.
    app.on_key(ctrl('\\'));
    assert!(app.editor.split_root.is_some(), "Ctrl+\\ splits");
    // Ctrl+J toggles the bottom panel.
    let before = app.show_bottom_dock;
    app.on_key(ctrl('j'));
    assert_ne!(app.show_bottom_dock, before, "Ctrl+J toggles the panel");
    // Ctrl+Shift+K deletes the current line.
    type_str(&mut app, "doomed");
    app.on_key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(
        app.editor.active_tab().unwrap().text().is_empty(),
        "Ctrl+Shift+K deletes the line"
    );
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
    assert_eq!(
        app.mode_indicator().as_deref(),
        Some("-- NORMAL --"),
        "reset to Normal"
    );
}

#[test]
fn apple_keymap_ctrl_d_deletes_the_character_ahead() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.keymap, "apple", "default keymap");

    // macOS forward delete: the character to the right of the cursor goes, the
    // cursor stays, so a second press eats the next one.
    buffer_with(&mut app, "abc", 1);
    app.on_key(ctrl('d'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "ac");
    app.on_key(ctrl('d'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "a");
    assert!(
        !app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "Ctrl+D no longer spawns a caret in the Apple keymap"
    );

    // Nothing ahead at the end of the buffer: a no-op, not a backspace.
    buffer_with(&mut app, "abc", 3);
    app.on_key(ctrl('d'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "abc");

    // Ctrl+Shift+D still duplicates the line.
    buffer_with(&mut app, "abc", 0);
    app.on_key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(app.editor.active_tab().unwrap().text(), "abc\nabc");

    // Other keymaps keep the editor core's add-next-occurrence on Ctrl+D.
    app.settings.keymap = "vscode-macos".to_string();
    buffer_with(&mut app, "foo foo", 0);
    app.on_key(ctrl('d'));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "foo foo",
        "VS Code's Ctrl+D selects, it does not delete"
    );
}

// ----- vix-keybindings registry conversion (improvement plan T104g) -------
// apple_ctrl_key and global_shared_key now dispatch through
// vix_keybindings::lookup/lookup_shared instead of their own hardcoded
// match/if chains; these cover the two bindings that stay host-side
// (Ctrl+Alt+R, the only Alt-keyed apple_ctrl_key binding) and a shared,
// named-key token (Ctrl+BackTab) neither exercised elsewhere.

#[test]
fn apple_keymap_ctrl_alt_r_opens_query_replace() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.keymap, "apple", "default keymap");
    app.on_key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));
    // `edit.query_replace` opens the search bar in interactive mode --
    // `app.query_replace` (the step-through session) only appears once the
    // query/replace fields are submitted, so the field's `interactive` flag
    // is the right signal here, same as `edit.replace`'s own coverage.
    assert!(
        app.search.as_ref().is_some_and(|s| s.interactive),
        "Ctrl+Alt+R opens the search bar in interactive query-replace mode"
    );
}

// ----- vix-keybindings override choke point (improvement plan T104i/j) ----
// App::override_key, inserted in on_key right after org_table_key, now
// intercepts every keymap's dispatch when self.key_overrides has a
// matching entry. App::apply_key_overrides (the resolve/report/store half
// of App::resolve_key_overrides, split out so it doesn't need the real
// keybindings.toml path or a real loaded script) is the test seam --
// T104j now also feeds script bind_key requests through the same call.

#[test]
fn key_override_wins_over_a_builtin_keymap_binding() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.keymap, "apple", "default keymap");
    // Apple's built-in Ctrl+E toggles explorer/editor focus; override it
    // to open a new file instead.
    app.apply_key_overrides(vec![vix_keybindings::Override {
        key_token: "C-e".to_string(),
        action_id: "file.new".to_string(),
        source: vix_keybindings::Source::User,
    }]);
    let tabs_before = app.editor.tabs.len();
    let focus_before = app.focus;
    app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before + 1,
        "the override's file.new ran, not the built-in focus toggle"
    );
    assert_eq!(
        app.focus, focus_before,
        "the built-in view.toggle_explorer_focus never fired"
    );
    // The override also shadows a real built-in (Apple's Ctrl+E), so it's
    // reported once, informationally.
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Info)),
        "shadowing a built-in is reported informationally"
    );
}
