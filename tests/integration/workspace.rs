#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn project_media_type_example_snippets_load() {
    // The bundled example files live under config/media-types/<type>/snippets/.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proj = "config/snippets/snippets.json";

    // Rust source maps to text/rust (no x- prefix) and loads its examples.
    assert_eq!(
        vix::media_type::for_extension("rs").unwrap().media_type,
        "text/rust"
    );
    let rust = vix::snippets::load_scoped(Some("text/rust"), root, proj);
    assert!(
        rust.iter().any(|s| s.prefixes.iter().any(|p| p == "fn")),
        "rust examples loaded"
    );

    // A few other languages resolve to the clean text/* or application/* types.
    assert_eq!(
        vix::media_type::for_extension("py").unwrap().media_type,
        "text/python"
    );
    assert_eq!(
        vix::media_type::for_extension("ts").unwrap().media_type,
        "text/typescript"
    );
    assert_eq!(
        vix::media_type::for_extension("cs").unwrap().media_type,
        "text/csharp"
    );
    assert_eq!(
        vix::media_type::for_extension("sql").unwrap().media_type,
        "application/sql"
    );
    let sql = vix::snippets::load_scoped(Some("application/sql"), root, proj);
    assert!(
        sql.iter().any(|s| s.prefixes.iter().any(|p| p == "select")),
        "sql examples loaded"
    );

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
        assert_eq!(
            vix::media_type::for_extension(ext).unwrap().media_type,
            mt,
            "{ext} → {mt}"
        );
        let snips = vix::snippets::load_scoped(Some(mt), root, proj);
        assert!(!snips.is_empty(), "{mt} examples loaded");
    }

    // PlantUML loads both the building blocks and the example gallery.
    let pl = vix::snippets::load_scoped(Some("text/plantuml"), root, proj);
    assert!(
        pl.iter().any(|s| s.name == "Sequence Diagram"),
        "plantuml gallery loaded"
    );
    assert!(pl.len() >= 50, "building blocks + gallery merged");

    // The Base column marks text vs binary content.
    assert!(vix::media_type::for_extension("rs").unwrap().is_text());
    assert!(!vix::media_type::for_extension("png").unwrap().is_text());
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
    let sel = app
        .editor
        .active_tab_mut()
        .unwrap()
        .editor
        .get_selection_text();
    assert_eq!(sel.as_deref(), Some("world"));
}

#[test]
fn specs_reference_crates_by_their_workspace_paths() {
    // Guard against architecture drift in the *current* direction. Vix is a
    // workspace of `crates/vix-*` members, so a spec names one by its workspace
    // path (`crates/vix-find-panel/spec/index.md`). The flat, pre-workspace
    // spelling (`find_panel/spec/index.md`) is drift: those paths resolve to
    // nothing, and they describe a layout that no longer exists.
    //
    // This replaced a test asserting the opposite — that no spec may mention
    // `vix-editor` — written while the crates were briefly folded into modules.
    // That decision was reversed; the test outlived it and started failing
    // correct documentation.
    fn walk(dir: &Path, hits: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "md" || e == "tsv") {
                let text = fs::read_to_string(&path).unwrap_or_default();
                for line in text.lines() {
                    // `some_module/spec/…` — an underscored module name is the
                    // old spelling; workspace crates are `vix-kebab-case`.
                    for (at, _) in line.match_indices("/spec/") {
                        let before = &line[..at];
                        let name: String = before
                            .chars()
                            .rev()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if name.contains('_') {
                            let name: String = name.chars().rev().collect();
                            hits.push(format!("{}: {name}/spec/", path.display()));
                        }
                    }
                    if line.contains("Subcrate ") || line.contains("subcrate ") {
                        hits.push(format!("{}: subcrate", path.display()));
                    }
                }
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    walk(&root.join("spec"), &mut hits);
    walk(&root.join("crates"), &mut hits);
    hits.sort();
    hits.dedup();
    assert!(
        hits.is_empty(),
        "specs name crates by a pre-workspace path:\n{}",
        hits.join("\n")
    );
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
    let lc = app
        .location_chooser
        .as_ref()
        .expect("location chooser opens");
    assert!(
        lc.entries.len() >= 2,
        "history has multiple locations: {}",
        lc.entries.len()
    );
    assert_eq!(lc.entries[0].line, 30, "most recent location first");

    // Move the cursor away, then jump to the second entry from the chooser.
    let target = app.location_chooser.as_ref().unwrap().entries[1].line;
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.location_chooser.is_none(), "Enter closes the chooser");
    assert_eq!(
        app.editor.cursor_1based().0,
        target,
        "jumped to the chosen location"
    );

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
fn open_recent_records_dedups_and_reopens() {
    let dir = unique_dir("recent");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    assert!(app.settings.recent_files.is_empty());

    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt"));
    assert_eq!(app.settings.recent_files.len(), 2);
    assert!(
        app.settings.recent_files[0].ends_with("b.txt"),
        "most-recent first"
    );
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
    assert!(
        app.recent_chooser.is_none(),
        "Enter opens and closes the chooser"
    );
    let open = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(
        open.ends_with("b.txt"),
        "opened the highlighted recent file"
    );

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
    assert!(
        app.palette.is_none(),
        "Enter accepts and closes the palette"
    );
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_some(),
        "a file opened"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_recent_empty_shows_status_only() {
    let mut app = app_at(Path::new("."));
    assert!(app.settings.recent_files.is_empty());
    app.run_action("file.open_recent");
    assert!(
        app.recent_chooser.is_none(),
        "no chooser when there are no recent files"
    );
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
    assert!(
        app.recent_chooser.is_none(),
        "a click opens and closes the chooser"
    );
    let open = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(open.ends_with("c.txt"));
    fs::remove_dir_all(&dir).ok();
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
    assert!(
        out2.contains("a.txt:2:1:"),
        "case-sensitive FOO matched line 2: {out2:?}"
    );
    assert!(
        out2.contains("[1 matches in 1 files]"),
        "only the uppercase hit: {out2:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_in_workspace_to_dock_lists_and_jumps() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
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
    assert!(
        out.contains("a.txt:2:1:"),
        "lists the hit as path:line:col: {out:?}"
    );
    assert!(
        out.contains("NEEDLE here"),
        "includes the matched text: {out:?}"
    );
    assert!(
        out.contains("[1 matches in 1 files]"),
        "summary line: {out:?}"
    );

    // Render to record the dock rect, then click the hit (2nd content row).
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.bottom_dock;
    app.on_mouse(click(r.x + 1, r.y + 2));
    assert_eq!(
        app.editor.active_tab().unwrap().cursor_1based().0,
        2,
        "jumps to line 2"
    );

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
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "beta and beta\n"
    );
    app.on_key(key('y')); // confirm: apply across the workspace

    let a = fs::read_to_string(dir.join("a.txt")).unwrap();
    let b = fs::read_to_string(dir.join("b.txt")).unwrap();
    assert_eq!(a, "ZZ and ZZ\n");
    assert_eq!(b, "gamma ZZ\n");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctrl_shift_f_opens_workspace_search() {
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert!(
        app.workspace_search.is_some(),
        "Ctrl+Shift+F opens workspace search"
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
    assert_eq!(
        app.settings.recent_files.len(),
        2,
        "kept only recent_files_max entries"
    );
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
    assert_eq!(
        snap.scrolls.first().copied(),
        Some(120),
        "scroll offset captured"
    );

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
fn workspace_dashboard_opens_counts_files_and_closes() {
    let dir = unique_dir("dashboard");
    fs::write(dir.join("a.txt"), "x\n").unwrap();
    fs::write(dir.join("b.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("tools.dashboard");
    assert!(app.dashboard.is_some(), "Tools → Workspace Dashboard opens");
    assert!(
        !app.dashboard.as_ref().unwrap().folder.is_empty(),
        "folder is shown immediately"
    );

    // Wait (bounded) for the async file-count metric to arrive.
    for _ in 0..200 {
        app.poll_dashboard();
        if app.dashboard.as_ref().unwrap().file_count.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        app.dashboard.as_ref().unwrap().file_count,
        Some(2),
        "counted the two files"
    );

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.dashboard.is_none(), "Esc closes the dashboard");
    fs::remove_dir_all(&dir).ok();
}
