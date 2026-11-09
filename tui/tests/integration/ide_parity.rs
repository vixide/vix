//! IDE-parity features (T720-T727): Local History, Search Everywhere, Code
//! Cleanup, run configurations, Peek Definition and postfix completion.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

fn enter() -> KeyEvent {
    keycode(KeyCode::Enter)
}

/// An app on a fresh temp project with Local History pointed at a private
/// store, so a test never touches the developer's real config directory.
fn app_with_store(tag: &str) -> (App, PathBuf) {
    let dir = unique_dir(tag);
    let mut app = app_at(&dir);
    app.local_history_root = Some(dir.join(".history-store"));
    (app, dir)
}

fn open_and_set(app: &mut App, path: &Path, text: &str) {
    app.editor.open(path, false).unwrap();
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.dirty = true;
}

fn active_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

#[test]
fn saving_snapshots_and_the_history_overlay_diffs_and_restores() {
    let (mut app, dir) = app_with_store("lh-restore");
    let f = dir.join("notes.txt");
    fs::write(&f, "one\n").unwrap();
    open_and_set(&mut app, &f, "one\n");
    app.run_action("file.save");
    // Distinct ids need distinct milliseconds.
    std::thread::sleep(Duration::from_millis(5));
    open_and_set(&mut app, &f, "two\n");
    app.run_action("file.save");

    app.run_action("file.local_history");
    let open = app.choice_list.as_ref().expect("history overlay opens");
    assert_eq!(open.panel.len(), 2, "two saves, two snapshots");

    // Down to the older snapshot ("one"), then `r` restores the whole file.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(key('r'));
    assert!(app.choice_list.is_none(), "an action closes the overlay");
    assert_eq!(active_text(&app), "one\n");
    assert!(app.editor.active_tab().unwrap().dirty, "restore is unsaved");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn enter_on_a_snapshot_opens_a_read_only_diff() {
    let (mut app, dir) = app_with_store("lh-diff");
    let f = dir.join("a.txt");
    fs::write(&f, "alpha\n").unwrap();
    open_and_set(&mut app, &f, "alpha\n");
    app.run_action("file.save");
    open_and_set(&mut app, &f, "beta\n");

    app.run_action("file.local_history");
    app.on_key(enter());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only, "the diff tab is read-only");
    let text = tab.text();
    assert!(text.contains("-alpha") && text.contains("+beta"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn labelling_forces_a_snapshot_and_shows_in_the_list() {
    let (mut app, dir) = app_with_store("lh-label");
    let f = dir.join("a.txt");
    fs::write(&f, "x\n").unwrap();
    open_and_set(&mut app, &f, "x\n");
    app.run_action("file.save");
    app.run_action("file.local_history_label");
    app.prompt.as_mut().unwrap().input = "milestone".into();
    app.on_key(enter());

    app.run_action("file.local_history");
    let rows = &app.choice_list.as_ref().unwrap().panel.rows;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.contains("milestone")), "{rows:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn history_is_off_when_the_setting_is_off() {
    let dir = unique_dir("lh-off");
    let mut settings = Settings::default();
    settings.editor_behavior.local_history_keep = 0;
    let mut app = App::new(dir.clone(), settings).with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.local_history_root = Some(dir.join(".history-store"));
    let f = dir.join("a.txt");
    fs::write(&f, "x\n").unwrap();
    open_and_set(&mut app, &f, "x\n");
    app.run_action("file.save");
    app.run_action("file.local_history");
    assert!(app.choice_list.is_none(), "no snapshots were taken");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn search_everywhere_mixes_files_and_commands_and_keeps_prefixes() {
    let dir = unique_dir("everywhere");
    fs::write(dir.join("zebra_notes.txt"), "hi\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("nav.search_everywhere");
    for c in "zebra".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert!(
        p.entries.iter().any(|e| e.label.contains("zebra_notes")),
        "the file is found"
    );

    // A command query in the same box (no `>` needed).
    app.on_key(esc());
    app.run_action("nav.search_everywhere");
    for c in "save all".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert!(
        p.entries.iter().any(|e| e.label.starts_with("> ")),
        "commands appear alongside files"
    );

    // An explicit prefix still narrows to one kind.
    app.on_key(esc());
    app.run_action("nav.search_everywhere");
    app.on_key(key('>'));
    let p = app.palette.as_ref().unwrap();
    assert!(p.entries.iter().all(|e| e.label.starts_with("> ")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_plain_palette_is_not_everywhere() {
    let dir = unique_dir("not-everywhere");
    fs::write(dir.join("zebra.txt"), "hi\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("nav.search_everywhere");
    app.on_key(esc());
    app.run_action("tools.palette");
    for c in "save all".chars() {
        app.on_key(key(c));
    }
    let p = app.palette.as_ref().unwrap();
    assert!(
        p.entries.iter().all(|e| !e.label.starts_with("> ")),
        "an ordinary Ctrl+P query searches files only"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn code_cleanup_trims_whitespace_and_fixes_the_final_newline() {
    let (mut app, dir) = app_with_store("cleanup");
    let f = dir.join("a.txt");
    fs::write(&f, "x\n").unwrap();
    open_and_set(&mut app, &f, "a  \nb\t\n\n\n");
    app.run_action("edit.cleanup_code");
    assert_eq!(active_text(&app), "a\nb\n");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "tasks")]
#[test]
fn run_configuration_runs_its_before_step_first() {
    let (mut app, dir) = app_with_store("runconfig");
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(
        dir.join(".vix/run.toml"),
        "[[config]]\nname='prep'\ncommand='echo prep-ran'\n\
         [[config]]\nname='go'\ncommand='echo go-ran'\nbefore=['prep']\n",
    )
    .unwrap();
    app.run_action("project.run_config");
    let open = app.choice_list.as_ref().expect("picker opens");
    assert_eq!(open.panel.len(), 2);
    // Pick `go` (second row).
    app.on_key(keycode(KeyCode::Down));
    app.on_key(enter());
    assert!(app.choice_list.is_none());
    wait_until(
        &mut app,
        App::poll_command,
        |a| a.bottom_dock.lines.iter().any(|l| l == "go-ran"),
        Duration::from_secs(10),
    );
    let lines: Vec<String> = app.bottom_dock.lines.clone();
    // Trimmed: cmd.exe's `echo prep-ran && …` keeps the space before `&&`.
    let prep = lines
        .iter()
        .position(|l| l.trim_end() == "prep-ran")
        .unwrap();
    let go = lines.iter().position(|l| l.trim_end() == "go-ran").unwrap();
    assert!(prep < go, "before-launch step ran first: {lines:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "tasks")]
#[test]
fn a_broken_run_file_is_reported_not_run() {
    let (mut app, dir) = app_with_store("runconfig-bad");
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(dir.join(".vix/run.toml"), "[[config]]\nname='a'\n").unwrap();
    app.run_action("project.run_config");
    assert!(app.choice_list.is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(feature = "tasks")]
#[test]
fn edit_run_configurations_creates_a_starter_file() {
    let (mut app, dir) = app_with_store("runconfig-edit");
    app.run_action("project.run_config_edit");
    assert!(dir.join(".vix/run.toml").exists());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.as_ref().is_some_and(|p| p.ends_with("run.toml")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn peek_definition_shows_the_definition_without_leaving_the_file() {
    let (mut app, dir) = app_with_store("peek");
    fs::write(
        dir.join("lib.rs"),
        "// header\nfn target_fn() {\n    body();\n}\n",
    )
    .unwrap();
    let main = dir.join("main.rs");
    fs::write(&main, "fn main() {\n    target_fn();\n}\n").unwrap();
    app.editor.open(&main, false).unwrap();
    let area = app.layout.editor;
    app.editor.goto(2, Some(7), area);

    app.run_action("nav.peek_definition");
    let open = app.choice_list.as_ref().expect("peek opens");
    assert!(
        open.panel.rows[open.panel.selected].contains("fn target_fn"),
        "{:?}",
        open.panel.rows
    );
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.clone())
            .is_some_and(|p| p.ends_with("main.rs")),
        "still in the original file"
    );

    // Enter opens the definition.
    app.on_key(enter());
    assert!(app.choice_list.is_none());
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.clone())
            .is_some_and(|p| p.ends_with("lib.rs"))
    );
    assert_eq!(app.editor.cursor_line(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn postfix_completion_wraps_the_expression_on_tab() {
    let (mut app, dir) = app_with_store("postfix");
    let f = dir.join("a.rs");
    fs::write(&f, "").unwrap();
    app.editor.open(&f, false).unwrap();
    for c in "ready.if".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Tab));
    let text = active_text(&app);
    assert!(text.starts_with("if ready {"), "{text:?}");
    assert!(!text.contains(".if"), "{text:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refactor_and_organize_need_a_language_server() {
    let (mut app, dir) = app_with_store("refactor-nolsp");
    let f = dir.join("a.rs");
    fs::write(&f, "fn main() {}\n").unwrap();
    app.editor.open(&f, false).unwrap();
    app.run_action("lsp.refactor");
    assert!(app.code_actions.is_none());
    app.run_action("lsp.organize_imports");
    assert!(app.code_actions.is_none());
    let _ = fs::remove_dir_all(&dir);
}

/// The whole screen, as text.
#[cfg(feature = "tasks")]
fn screen(app: &mut App) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(160, 50)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// T726: a file's tests get a local "Run test" lens drawn on its own row
/// above the test's `fn` line (no language server needed), the buffer text is
/// untouched, `lsp.code_lens_run` on that line runs the test, and View → Code
/// Lens turns the rows off.
#[test]
#[cfg(feature = "tasks")]
fn local_test_lenses_are_drawn_above_their_line_and_run_from_it() {
    let (mut app, dir) = app_with_store("lens-local");
    app.layout.editor = Rect::new(0, 0, 80, 12);
    let src = "fn helper() {}\n\n#[test]\nfn it_works() {\n    assert!(true);\n}\n";
    app.editor.new_tab_with_content(src);
    app.editor.active_tab_mut().unwrap().path = Some(dir.join("src/lib.rs"));
    app.poll_code_lens();

    let ed = &app.editor.active_tab().unwrap().editor;
    assert_eq!(ed.code_lens_at(3), Some("Run test"), "above `fn it_works`");
    assert_eq!(active_text(&app), src, "a lens is not buffer text");
    let shown = screen(&mut app);
    let lens_row = shown.lines().position(|l| l.contains("Run test"));
    let fn_row = shown.lines().position(|l| l.contains("fn it_works"));
    assert!(
        matches!((lens_row, fn_row), (Some(l), Some(f)) if l + 1 == f),
        "lens row directly above its line:\n{shown}"
    );

    // Not on a lensed line: nothing to run.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(0);
    app.run_action("lsp.code_lens_run");
    assert!(!app.command_running());
    // On the test's line: the lens runs the test.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(3);
    app.run_action("lsp.code_lens_run");
    assert!(
        app.bottom_dock
            .lines
            .iter()
            .any(|l| l == "$ cargo test -- --exact it_works"),
        "{:?}",
        app.bottom_dock.lines
    );
    app.run_action("tools.cancel_command");

    // `lsp.code_lens` lists every lens in the file in the pick-one overlay.
    app.run_action("lsp.code_lens");
    assert_eq!(app.choice_list.as_ref().map(|c| c.panel.len()), Some(1));
    app.on_key(keycode(KeyCode::Esc));

    app.run_action("view.code_lens");
    assert!(!app.editor.active_tab().unwrap().editor.has_code_lenses());
    let _ = fs::remove_dir_all(&dir);
}

/// T726: edits re-request lenses only once typing pauses (the debounce), so
/// a lens added by an edit shows up after the wait, not on every keystroke.
#[test]
fn lens_refresh_after_an_edit_waits_for_the_debounce() {
    let (mut app, dir) = app_with_store("lens-debounce");
    let f = dir.join("t.py");
    fs::write(&f, "x = 1\n").unwrap();
    app.editor.open(&f, false).unwrap();
    app.poll_code_lens();
    assert!(!app.editor.active_tab().unwrap().editor.has_code_lenses());
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("def test_a():\n    pass\n");
    app.poll_code_lens();
    assert!(
        !app.editor.active_tab().unwrap().editor.has_code_lenses(),
        "not yet: the debounce is still running"
    );
    std::thread::sleep(Duration::from_millis(600));
    app.poll_code_lens();
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .code_lens_at(0)
            .is_some(),
        "refreshed once the edit settled"
    );
    let _ = fs::remove_dir_all(&dir);
}
