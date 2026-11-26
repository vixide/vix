//! Problem matchers over task output and Go → Next/Previous Error (T632).
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
#[cfg(all(unix, feature = "tasks"))]
use vix::editor_core::virtual_text::VirtualTextLayer;

/// A project with `a.c` and `sub/b.c`, and a `tasks.toml` whose `build` task
/// prints two gcc-style problems (one for a file that does not exist) with
/// the given `matcher` line.
#[cfg(all(unix, feature = "tasks"))]
fn project(tag: &str, matcher: &str) -> PathBuf {
    let dir = unique_dir(tag);
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("a.c"), "int a;\nint b;\nint c;\n").unwrap();
    fs::write(dir.join("sub/b.c"), "x\ny\nzzzzzz\n").unwrap();
    fs::write(
        dir.join("tasks.toml"),
        format!(
            "[[task]]\nname = \"build\"\n\
             command = \"printf 'a.c:2:1: error: first\\\\nnoise\\\\nsub/b.c:3:4: warning: second\\\\nmissing.c:1:1: error: third\\\\n'\"\n\
             {matcher}\n"
        ),
    )
    .unwrap();
    dir
}

/// Run the `build` task from Tools → Tasks… and wait for it to finish.
#[cfg(all(unix, feature = "tasks"))]
fn run_build(app: &mut App) {
    app.run_action("tools.tasks");
    app.on_key(keycode(KeyCode::Enter));
    wait_until(
        app,
        App::poll_command,
        |a| !a.command_running(),
        Duration::from_secs(10),
    );
}

fn active_ends_with(app: &App, tail: &str) -> bool {
    app.editor
        .active_tab()
        .and_then(|t| t.path.as_ref())
        .is_some_and(|p| p.ends_with(tail))
}

#[cfg(all(unix, feature = "tasks"))]
#[test]
fn next_and_previous_error_walk_the_task_problems_and_wrap() {
    let dir = project("t632-nav", "matcher = \"gcc\"");
    let mut app = app_at(&dir);
    run_build(&mut app);
    assert_eq!(app.task_problems.len(), 3, "{:?}", app.task_problems);

    app.run_action("go.next_error");
    assert!(active_ends_with(&app, "a.c"));
    assert_eq!(app.editor.cursor_line(), 1, "line 2 is zero-based 1");

    app.run_action("go.next_error");
    assert!(active_ends_with(&app, "sub/b.c"));
    assert_eq!(app.editor.cursor_line(), 2);
    let col = app.editor.active_tab().unwrap().editor.get_cursor();
    assert_eq!(col, "x\ny\n".len() + 3, "column 4 is offset 3 on the line");

    // The third names a file that does not exist: reported, not opened.
    app.run_action("go.next_error");
    assert!(active_ends_with(&app, "sub/b.c"));
    assert_eq!(app.task_problems.current(), Some(2));

    // Next wraps to the first; Previous wraps back to the last.
    app.run_action("go.next_error");
    assert!(active_ends_with(&app, "a.c"));
    assert_eq!(app.task_problems.current(), Some(0));
    app.run_action("go.prev_error");
    assert_eq!(app.task_problems.current(), Some(2));
    app.run_action("go.prev_error");
    assert_eq!(app.task_problems.current(), Some(1));
    assert!(active_ends_with(&app, "sub/b.c"));
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(all(unix, feature = "tasks"))]
#[test]
fn task_problems_show_inline_on_the_cursor_line() {
    let dir = project("t632-inline", "");
    let mut app = app_at(&dir);
    run_build(&mut app);
    app.run_action("go.next_error");
    app.tick();
    let texts = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .eol_texts(VirtualTextLayer::TaskProblems)
        .to_vec();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert_eq!((texts[0].line, texts[0].text.as_str()), (1, "first"));
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(all(unix, feature = "tasks", feature = "lsp"))]
#[test]
fn the_problems_panel_lists_task_problems() {
    let dir = project("t632-panel", "");
    let mut app = app_at(&dir);
    run_build(&mut app);
    app.run_action("lsp.diagnostics");
    let ps = app.workspace_search.as_ref().expect("problems panel");
    let rows: Vec<&str> = ps.hits.iter().map(|h| h.display.as_str()).collect();
    assert!(
        rows.contains(&"a.c:2: [error] first (gcc)"),
        "rows: {rows:?}"
    );
    assert!(rows.iter().any(|r| r.starts_with("sub/b.c:3: [warning]")));
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(all(unix, feature = "tasks"))]
#[test]
fn matcher_none_finds_nothing_and_an_invalid_one_falls_back_to_auto() {
    let dir = project("t632-none", "matcher = \"none\"");
    let mut app = app_at(&dir);
    run_build(&mut app);
    assert!(app.task_problems.is_empty());
    let tabs = app.editor.tabs.len();
    app.run_action("go.next_error");
    assert_eq!(app.editor.tabs.len(), tabs, "nothing opened");

    let dir2 = project("t632-bad", "matcher = \"(unclosed\"");
    let mut app = app_at(&dir2);
    let before = app.messages.items.len();
    run_build(&mut app);
    assert!(
        app.messages.items.len() > before,
        "the bad matcher is reported"
    );
    assert_eq!(app.task_problems.len(), 3, "auto still matched");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dir2);
}

#[test]
fn next_error_without_a_run_reports_and_stays_put() {
    let dir = unique_dir("t632-empty");
    let mut app = app_at(&dir);
    let tabs = app.editor.tabs.len();
    app.run_action("go.next_error");
    app.run_action("go.prev_error");
    assert_eq!(app.editor.tabs.len(), tabs, "nothing opened");
    assert!(app.task_problems.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn clicking_a_rustc_arrow_in_the_dock_jumps_to_it() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let dir = unique_dir("t632-dock");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {\n    let x = 1;\n}\n").unwrap();
    let mut app = app_at(&dir);
    app.visible.insert(vix::app::Visible::BOTTOM_DOCK);
    app.bottom_dock.push("error[E0308]: mismatched types");
    app.bottom_dock.push("  --> src/main.rs:2:9");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.bottom_dock;
    app.on_mouse(click(r.x + 1, r.y + 2));
    assert!(active_ends_with(&app, "src/main.rs"));
    assert_eq!(
        app.editor.active_tab().unwrap().cursor_1based(),
        (2, 9),
        "jumps to the arrow's line and column"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Run the Vim Ex command `cmd` through the `:` command line.
fn ex(app: &mut App, cmd: &str) {
    app.on_key(key(':'));
    type_str(app, cmd);
    app.on_key(keycode(KeyCode::Enter));
}

/// `:cn` / `:cp` walk a failing subproject build's errors and `:cope` lists
/// them (T841): the subproject compile runs in `crates/app`, whose paths the
/// problems resolve against.
#[cfg(all(unix, feature = "tasks"))]
#[test]
fn quickfix_ex_commands_walk_a_failing_subproject_build() {
    // Canonical, as the tabs' paths are (macOS's temp dir is a symlink).
    let dir = unique_dir("t841-quickfix").canonicalize().unwrap();
    let sub = dir.join("crates/app");
    fs::create_dir_all(sub.join("src")).unwrap();
    fs::write(
        sub.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        sub.join("src/lib.rs"),
        "pub fn a() {}\npub fn b() -> u8 {}\n",
    )
    .unwrap();
    fs::write(sub.join("src/main.rs"), "fn main() {\n    app::c();\n}\n").unwrap();
    let mut settings = Settings::default();
    settings.keymap = "vi".into();
    let mut app = app_at_with(&dir, settings);
    ex(&mut app, "e crates/app/src/main.rs");
    assert!(active_ends_with(&app, "crates/app/src/main.rs"));

    // Project → Subproject → Compile, its command replaced by a failing one.
    app.run_action("project.subproject.compile");
    app.prompt.as_mut().unwrap_or_else(|| panic!("no prompt: {}", app.status)).input = "printf 'src/lib.rs:2:19: error: mismatched types\\nsrc/main.rs:2:10: error: cannot find function\\n'; exit 101".into();
    app.on_key(keycode(KeyCode::Enter));
    wait_until(
        &mut app,
        App::poll_command,
        |a| !a.command_running(),
        Duration::from_secs(10),
    );
    assert_eq!(app.task_problems.len(), 2, "{:?}", app.task_problems);

    ex(&mut app, "cn");
    assert!(active_ends_with(&app, "crates/app/src/lib.rs"));
    assert_eq!(app.editor.active_tab().unwrap().cursor_1based(), (2, 19));
    ex(&mut app, "cnext");
    assert!(active_ends_with(&app, "crates/app/src/main.rs"));
    assert_eq!(app.editor.active_tab().unwrap().cursor_1based(), (2, 10));
    ex(&mut app, "cn");
    assert_eq!(
        app.task_problems.current(),
        Some(0),
        ":cn wraps to the first"
    );
    ex(&mut app, "cp");
    assert_eq!(
        app.task_problems.current(),
        Some(1),
        ":cp wraps to the last"
    );
    assert!(active_ends_with(&app, "crates/app/src/main.rs"));
    ex(&mut app, "cprevious");
    assert!(active_ends_with(&app, "crates/app/src/lib.rs"));
    ex(&mut app, "cN");
    assert_eq!(app.task_problems.current(), Some(1));

    ex(&mut app, "cope");
    let panel = app
        .workspace_search
        .as_ref()
        .expect("the task-problems view");
    let rows: Vec<&str> = panel.hits.iter().map(|h| h.display.as_str()).collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows[0].starts_with("crates/app/src/lib.rs:2: [error] mismatched types"),
        "{rows:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cope_without_a_run_reports_no_problems() {
    let dir = unique_dir("t841-cope-empty");
    let mut settings = Settings::default();
    settings.keymap = "vi".into();
    let mut app = app_at_with(&dir, settings);
    ex(&mut app, "copen");
    assert!(app.workspace_search.is_none());
    assert!(!app.status.is_empty(), "the empty list is reported");
    let _ = fs::remove_dir_all(&dir);
}
