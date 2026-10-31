#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

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
    assert_eq!(
        app.editor.cursor_1based().0,
        7,
        "live preview moves to line 7 while typing"
    );
    app.on_key(esc());
    assert!(app.palette.is_none());
    assert_eq!(
        app.editor.cursor_1based().0,
        1,
        "Esc reverts to the original line"
    );
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
    assert_eq!(
        app.editor.cursor_1based().0,
        1,
        "Alt+Left returns to origin line 1"
    );
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
    assert!(
        !names.contains(&"skip"),
        "local `let` is excluded: {names:?}"
    );
    assert_eq!(syms[0].line, 1, "lines are 1-based");
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
fn palette_files_mode_ranks_by_fuzzy_score_not_walk_order() {
    // T153: Files mode used to keep `ignore::WalkBuilder`'s raw traversal
    // order — not portable across filesystems, and not ranked at all.
    // "target.rs" is an exact match for the query; "abc_target_helper.rs"
    // only contains it mid-string, so a relevance ranking must put the
    // former first regardless of which the walk happens to visit first
    // (and regardless of alphabetical order, which would rank the "abc_"
    // file first too — the point is score, not path, breaks the tie here).
    let dir = unique_dir("palette-files-score");
    fs::write(dir.join("abc_target_helper.rs"), "").unwrap();
    fs::write(dir.join("target.rs"), "").unwrap();
    fs::write(dir.join("zzz_other.rs"), "").unwrap();
    let mut app = app_at(&dir);
    app.on_key(ctrl('p'));
    for c in "target".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert!(!p.entries.is_empty(), "fuzzy query matched files");
    assert_eq!(
        p.entries[0].label, "target.rs",
        "the exact match ranks first by score, not by path or walk order"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_files_mode_empty_query_lists_alphabetically() {
    // Every candidate scores 0 against an empty query, so the path
    // tie-break alone determines order (T153) -- deterministic and
    // portable, unlike the raw filesystem-walk order it replaces.
    let dir = unique_dir("palette-files-empty");
    fs::write(dir.join("zeta.rs"), "").unwrap();
    fs::write(dir.join("alpha.rs"), "").unwrap();
    fs::write(dir.join("mid.rs"), "").unwrap();
    let mut app = app_at(&dir);
    app.on_key(ctrl('p'));
    let p = app.palette.as_ref().unwrap();
    let labels: Vec<&str> = p.entries.iter().map(|e| e.label.as_str()).collect();
    let mut sorted = labels.clone();
    sorted.sort_unstable();
    assert_eq!(
        labels, sorted,
        "an empty query lists files alphabetically, not raw walk order"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn palette_recents_seed_from_persisted_settings() {
    let settings = {
        let mut s = Settings::default();
        s.command_recents = vec!["edit.select_all".to_string()];
        s
    };
    let mut app = app_with(settings);
    app.on_key(ctrl('p'));
    app.on_key(key('>')); // empty command query → recents first
    let p = app.palette.as_ref().unwrap();
    match &p.entries[0].action {
        vix::palette::Action::RunCommand(a) => {
            assert_eq!(a, "edit.select_all", "persisted recent floats up");
        }
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
    assert!(
        text.contains(expected),
        "the editor holds the inserted glyph"
    );

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
    assert!(
        text.contains(expected),
        "clicking a cell inserts that glyph"
    );
}

// ----- vix-keybindings registry conversion (improvement plan T104f) -------
// sublime_key now dispatches through vix_keybindings::lookup instead of its
// own hardcoded match; this covers plain Ctrl+P (Goto Anything) staying
// distinct from Ctrl+Shift+P (Command Palette, already covered above) --
// the two must not collide even though a terminal can report Ctrl+Shift+p
// as a lowercase 'p' with the Shift bit set.

#[test]
fn sublime_keymap_plain_ctrl_p_opens_the_file_browser_not_the_palette() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "sublime".to_string();
    app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert!(
        app.file_browser.is_some(),
        "plain Ctrl+P opens Goto Anything (the file browser)"
    );
    assert!(
        app.palette.is_none(),
        "plain Ctrl+P must not also open the command palette"
    );
}

#[test]
fn vscode_keymap_quick_open_command_palette_and_goto_line() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "vscode-macos".to_string();

    // Ctrl+P is Quick Open (the fuzzy file browser), not the Command Palette.
    app.on_key(ctrl('p'));
    assert!(app.file_browser.is_some(), "Ctrl+P opens Quick Open");
    assert!(app.palette.is_none());
    app.on_key(esc());

    // Ctrl+Shift+P opens the Command Palette.
    app.on_key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(
        app.palette.is_some(),
        "Ctrl+Shift+P opens the Command Palette"
    );
    app.on_key(esc());

    // Ctrl+G opens Go to Line (the palette).
    app.on_key(ctrl('g'));
    assert!(app.palette.is_some(), "Ctrl+G opens Go to Line");
}

/// Render the whole app into a `width`×`height` test terminal.
fn render(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    term.backend().buffer().clone()
}

/// The screen row (y) whose text contains `needle`, plus that row's text.
fn row_with(buf: &ratatui::buffer::Buffer, needle: &str) -> Option<(u16, String)> {
    (0..buf.area.height).find_map(|y| {
        let text: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        text.contains(needle).then_some((y, text))
    })
}

/// A 200-line file whose lines read `L1`..`L200`.
fn numbered_file(dir: &Path, name: &str) -> PathBuf {
    let file = dir.join(name);
    let body: String = (1..=200).map(|i| format!("L{i}\n")).collect();
    fs::write(&file, body).unwrap();
    file
}

/// T630: on a wide screen the file finder shows a numbered window of the
/// highlighted file centered on its target line, that line highlighted.
#[test]
fn palette_preview_shows_a_window_with_the_target_line_highlighted() {
    let dir = unique_dir("t630-preview");
    numbered_file(&dir, "p.txt");
    let mut app = app_at(&dir);
    app.run_action("tools.palette");
    for c in "p.txt:100".chars() {
        app.on_key(key(c));
    }
    let (title, preview) = app.palette_preview().expect("a preview");
    assert!(title.ends_with("p.txt"), "{title}");
    assert_eq!(preview.target, 100);
    let i = preview.target_index().expect("target in the window");
    assert_eq!(preview.lines[i], "L100");
    assert_eq!(preview.lines.len(), 2 * vix::palette::PREVIEW_RADIUS + 1);

    let buf = render(&mut app, 160, 50);
    let (target_y, target_row) = row_with(&buf, " L100 ").expect("target line drawn");
    assert!(target_row.contains("100 L100"), "numbered: {target_row}");
    let (other_y, _) = row_with(&buf, " L99 ").expect("neighbor drawn");
    // A column, not a byte offset: the row holds multi-byte border glyphs.
    let col = target_row[..target_row.find("L100").unwrap()]
        .chars()
        .count();
    let x = u16::try_from(col).unwrap();
    assert_ne!(
        buf[(x, target_y)].style(),
        buf[(x, other_y)].style(),
        "the target line is highlighted, its neighbor is not"
    );
    fs::remove_dir_all(&dir).ok();
}

/// T630: below 100 columns the preview pane is hidden.
#[test]
fn palette_preview_is_hidden_on_a_narrow_screen() {
    let dir = unique_dir("t630-narrow");
    numbered_file(&dir, "p.txt");
    let mut app = app_at(&dir);
    app.run_action("tools.palette");
    for c in "p.txt:100".chars() {
        app.on_key(key(c));
    }
    let buf = render(&mut app, 90, 40);
    assert!(row_with(&buf, "p.txt").is_some(), "the list is drawn");
    assert!(row_with(&buf, " L100").is_none(), "no preview pane");
    fs::remove_dir_all(&dir).ok();
}

/// T630: `@` previews the active buffer around the symbol, and workspace
/// search previews the highlighted hit.
#[test]
fn symbol_and_workspace_search_previews_follow_the_selection() {
    let dir = unique_dir("t630-sym");
    let file = dir.join("s.rs");
    let mut body: String = (1..=30).map(|i| format!("// c{i}\n")).collect();
    body.push_str("fn alpha() {}\n");
    fs::write(&file, &body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("nav.goto_symbol");
    let (_, preview) = app.palette_preview().expect("symbol preview");
    assert_eq!(preview.target, 31);
    assert_eq!(
        preview.lines[preview.target_index().unwrap()],
        "fn alpha() {}"
    );
    app.on_key(esc());

    app.run_action("search.workspace");
    for c in "alpha".chars() {
        app.on_key(key(c));
    }
    let buf = render(&mut app, 160, 50);
    assert!(
        row_with(&buf, "31 fn alpha() {}").is_some(),
        "the hit's line is previewed with its number"
    );
    fs::remove_dir_all(&dir).ok();
}

/// T630: `palette.resume` reopens the palette in its last mode, with its
/// query and selection.
#[test]
fn resume_restores_the_palette_mode_query_and_selection() {
    let dir = unique_dir("t630-resume");
    let mut app = app_at(&dir);
    app.run_action("palette.resume");
    assert!(app.palette.is_none(), "nothing to resume yet");

    app.run_action("tools.palette");
    for c in ">line".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down));
    app.on_key(esc());
    assert!(app.palette.is_none());

    app.run_action("palette.resume");
    let p = app.palette.as_ref().expect("palette reopened");
    assert_eq!(p.input, ">line");
    assert_eq!(p.mode(), vix::palette::Mode::Commands);
    assert_eq!(p.selected, 2);
    fs::remove_dir_all(&dir).ok();
}

/// T630: `palette.resume` after the workspace-search panel closed reopens
/// that panel, re-run, with its query and selection.
#[test]
fn resume_restores_the_workspace_search_panel() {
    let dir = unique_dir("t630-resume-ws");
    fs::write(dir.join("a.txt"), "needle one\nneedle two\nneedle three\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("search.workspace");
    for c in "needle".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Down));
    app.on_key(esc());
    assert!(app.workspace_search.is_none());

    app.run_action("palette.resume");
    let ps = app.workspace_search.as_ref().expect("panel reopened");
    assert_eq!(ps.query, "needle");
    assert_eq!(ps.hits.len(), 3);
    assert_eq!(ps.selected, 1);
    fs::remove_dir_all(&dir).ok();
}

/// T630: `Up`/`Down` on an empty palette input walk the accepted-query
/// history; once the input is edited they move the selection again.
#[test]
fn palette_up_down_recall_query_history() {
    let dir = unique_dir("t630-history");
    let file = dir.join("h.txt");
    let body: String = (1..=20).map(|i| format!("L{i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    for query in [":5", ":9"] {
        app.run_action("tools.palette");
        for c in query.chars() {
            app.on_key(key(c));
        }
        app.on_key(keycode(KeyCode::Enter));
    }

    app.run_action("tools.palette");
    let input = |app: &App| app.palette.as_ref().unwrap().input.clone();
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(input(&app), ":9");
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(input(&app), ":5");
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(input(&app), ":5", "stays at the oldest");
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(input(&app), ":9");
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(input(&app), "", "past the newest is an empty input");

    // Typed input: Up/Down move the selection, the input stays.
    app.on_key(key('>'));
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(input(&app), ">");
    assert_eq!(app.palette.as_ref().unwrap().selected, 1);
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(input(&app), ">");
    assert_eq!(app.palette.as_ref().unwrap().selected, 0);
    fs::remove_dir_all(&dir).ok();
}
