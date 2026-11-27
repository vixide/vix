//! The runnable migration exercise in `docs/vix-for-vim-users/index.md`
//! (T844), step by step, on a copy of `tui/examples/vim-exercise`: run the
//! test at point and watch it fail, jump to the failure with `:cn`, edit the
//! wrong argument with `ciw`, repeat it with `n.`, take a detour through
//! the undo tree, rerun the test, and stage the fix. Real `cargo` and real
//! `git`, as a user runs it, so the guide's steps cannot rot.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
#[cfg(all(unix, feature = "tasks", feature = "git"))]
use crate::integration::vim_parity::parse_keys;

/// The exercise's source, as the guide tells a user to copy it.
fn exercise_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/vim-exercise")
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
/// A fresh copy of the exercise in its own git repository, as the guide's
/// first step makes one.
fn copy_of_the_exercise() -> PathBuf {
    let dir = unique_dir("t844-vim-exercise");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let dir = dir.canonicalize().unwrap();
    // Written, not `fs::copy`d: a copy may keep the source's modification
    // time, and `cargo` would then reuse a build of an earlier run's fixed
    // copy from a shared target directory.
    for file in ["Cargo.toml", "Cargo.lock", "README.md", "src/lib.rs"] {
        let content = fs::read(exercise_source().join(file)).unwrap();
        fs::write(dir.join(file), content).unwrap();
    }
    init_git_repo(&dir, "Test");
    commit_all(&dir, "start");
    dir
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
fn keys(app: &mut App, keys: &str) {
    for k in parse_keys(keys) {
        app.on_key(k);
    }
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
/// Project -> Test at Point, then wait for `cargo test` to finish (a first
/// build of the exercise included).
fn test_at_point(app: &mut App) {
    app.bottom_dock.clear();
    app.run_action("project.test_at_point");
    assert!(app.command_running(), "status: {}", app.status);
    wait_until(
        app,
        App::poll_command,
        |a| !a.command_running(),
        Duration::from_secs(600),
    );
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
fn dock_has(app: &App, needle: &str) -> bool {
    app.bottom_dock.lines.iter().any(|l| l.contains(needle))
}

#[test]
fn the_exercise_starts_with_two_failing_tests_in_its_source() {
    // The guide's line numbers and its "two copies of one bug" story.
    let src = fs::read_to_string(exercise_source().join("src/lib.rs")).unwrap();
    assert_eq!(src.matches("ratio(part, whole, 10)").count(), 2, "{src}");
    let lines: Vec<&str> = src.lines().collect();
    assert_eq!(lines[23].trim(), "assert_eq!(percent(1, 4), 25);");
}

#[cfg(all(unix, feature = "tasks", feature = "git"))]
#[test]
fn the_migration_exercise_runs_as_the_guide_says() {
    let dir = copy_of_the_exercise();
    let lib = dir.join("src/lib.rs");
    let mut settings = Settings::default();
    settings.keymap = "vi".to_string();
    let mut app = app_at_with(&dir, settings);
    app.refresh_git();
    app.open_initial(&lib);

    // 1. Run the test at point: it fails. (After a search, `<Esc>` leaves
    //    Vix's find bar; in Vim it is a harmless no-op.)
    keys(&mut app, "/a_quarter<CR><Esc>");
    assert_eq!(app.editor.cursor_line(), 22, "on the test's fn line");
    test_at_point(&mut app);
    assert!(dock_has(&app, "1 failed"), "{:?}", app.bottom_dock.lines);
    assert!(!app.task_problems.is_empty(), "the panic is a problem");

    // 2. Jump to the failure.
    keys(&mut app, "gg:cn<CR>");
    assert_eq!(
        app.editor.active_tab().unwrap().cursor_1based(),
        (24, 9),
        "on the failing assert_eq!"
    );

    // 3. Edit the argument, 4. repeat the edit on the second copy.
    keys(&mut app, "gg/10<CR><Esc>ciw100<Esc>");
    assert_eq!(text(&app).matches("ratio(part, whole, 100)").count(), 1);
    keys(&mut app, "n.");
    assert_eq!(text(&app).matches("ratio(part, whole, 100)").count(), 2);
    let fixed = text(&app);

    // 5. A detour: undo the repeat, make a different change (a branch),
    //    then use the undo tree to go back to the state with both fixes.
    keys(&mut app, "u0f1ciw50<Esc>");
    assert!(
        text(&app).contains("ratio(part, whole, 50)"),
        "{}",
        text(&app)
    );
    app.run_action("edit.undo_tree");
    assert!(app.undo_tree.is_some(), "Edit -> Undo Tree opened");
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(text(&app), fixed, "the preview is the other branch");
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(text(&app), fixed);

    // 6. Save and rerun the test at point: it passes.
    keys(&mut app, ":w<CR>");
    assert_eq!(fs::read_to_string(&lib).unwrap(), fixed);
    keys(&mut app, "gg/a_quarter<CR><Esc>");
    test_at_point(&mut app);
    assert!(dock_has(&app, "1 passed"), "{:?}", app.bottom_dock.lines);
    assert!(app.task_problems.is_empty(), "{:?}", app.task_problems);

    // 7. Stage the fix: Git -> Stage Hunk on each changed line.
    app.refresh_git();
    for line in [":6<CR>", ":11<CR>"] {
        keys(&mut app, line);
        app.run_action("git.stage_hunk");
    }
    let staged = fixture_git(&dir, &["diff", "--cached"]);
    assert!(staged.contains("+    ratio(part, whole, 100)"), "{staged}");
    assert!(
        staged.contains("+    format!(\"{}%\", ratio(part, whole, 100))"),
        "{staged}"
    );
    assert_eq!(fixture_git(&dir, &["diff"]), "", "nothing left unstaged");
    let _ = fs::remove_dir_all(&dir);
}
