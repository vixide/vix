#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

fn type_pattern_and_replacement(app: &mut App, pattern: &str, replacement: &str) {
    app.run_action("edit.structural_replace");
    for c in pattern.chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    for c in replacement.chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
}

#[test]
fn structural_replace_steps_through_matches_and_reports_the_total() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "add(1, 2); add(3, 4);", 0);

    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    let sr = app.structural_replace.as_ref().expect("session opens");
    assert_eq!(sr.replaced, 0);
    let (cs, ce) = sr.current;
    assert_eq!(
        &app.editor.active_tab().unwrap().text()[cs..ce],
        "add(1, 2)"
    );

    app.on_key(key('y')); // replace the first match
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "sum(1, 2); add(3, 4);"
    );
    assert!(
        app.structural_replace.is_some(),
        "advances to the second match"
    );

    app.on_key(key('y')); // replace the second match
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "sum(1, 2); sum(3, 4);"
    );
    assert!(
        app.structural_replace.is_none(),
        "the session ends once every match is handled"
    );
    assert!(
        app.status.contains('2'),
        "reports 2 replaced: {}",
        app.status
    );
}

#[test]
fn structural_replace_n_skips_a_match_without_changing_it() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "add(1, 2); add(3, 4);", 0);
    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    app.on_key(key('n')); // skip the first
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "add(1, 2); add(3, 4);",
        "skipping makes no change"
    );
    let sr = app.structural_replace.as_ref().expect("still open");
    let (cs, ce) = sr.current;
    assert_eq!(
        &app.editor.active_tab().unwrap().text()[cs..ce],
        "add(3, 4)",
        "advanced to the second match"
    );
}

#[test]
fn structural_replace_q_quits_without_changing_anything() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "add(1, 2); add(3, 4);", 0);
    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    app.on_key(key('q'));

    assert!(app.structural_replace.is_none());
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "add(1, 2); add(3, 4);"
    );
}

#[test]
fn structural_replace_bang_replaces_every_remaining_match() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "add(1, 2); add(3, 4); add(5, 6);", 0);
    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    app.on_key(key('!'));

    assert!(app.structural_replace.is_none());
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "sum(1, 2); sum(3, 4); sum(5, 6);"
    );
    assert!(
        app.status.contains('3'),
        "reports 3 replaced: {}",
        app.status
    );
}

#[test]
fn structural_replace_uses_the_multi_hole_to_move_a_whole_argument_list() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "old_call(a, b, c);", 0);
    type_pattern_and_replacement(&mut app, "old_call($$ARGS)", "new_call($$ARGS)");

    app.on_key(key('y'));

    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "new_call(a, b, c);"
    );
}

#[test]
fn structural_replace_scoped_to_a_selection_ignores_matches_outside_it() {
    let mut app = app_at(Path::new("."));
    let text = "add(1, 2); add(3, 4);";
    buffer_with(&mut app, text, 0);
    // Select only the first `add(1, 2)` call.
    let sel_end = text.find(';').unwrap() + 1;
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, sel_end);

    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    app.on_key(key('!')); // replace everything the session can see

    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "sum(1, 2); add(3, 4);",
        "the second call, outside the selection, is untouched"
    );
}

#[test]
fn structural_replace_with_no_matches_reports_status_and_opens_no_session() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "totally unrelated code", 0);

    type_pattern_and_replacement(&mut app, "add($X, $Y)", "sum($X, $Y)");

    assert!(app.structural_replace.is_none());
    assert!(!app.status.is_empty());
}

#[test]
fn structural_replace_with_a_blank_pattern_reports_an_error_and_opens_no_session() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "add(1, 2);", 0);
    let before = app.messages.items.len();

    app.run_action("edit.structural_replace");
    for c in "   ".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(
        app.messages.items.len() > before,
        "an error was reported for the blank pattern"
    );
    assert!(app.prompt.is_none(), "no replacement prompt follows");
    assert!(app.structural_replace.is_none());
}

#[test]
fn structural_replace_workspace_computes_a_plan_and_confirm_writes_files() {
    let dir = unique_dir("structural-replace-workspace");
    fs::write(dir.join("a.txt"), "add(1, 2);\n").unwrap();
    fs::write(dir.join("b.txt"), "add(3, 4); add(5, 6);\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("edit.structural_replace_workspace");
    for c in "add($X, $Y)".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    for c in "sum($X, $Y)".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    let rc = app.replace_confirm.as_ref().expect("plan computed");
    assert_eq!(rc.replaced, 3, "one hit in a.txt, two in b.txt");
    assert_eq!(rc.plan.len(), 2, "both files have a hit");
    // Nothing is written until the preview is confirmed.
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "add(1, 2);\n"
    );

    app.on_key(key('y'));

    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "sum(1, 2);\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("b.txt")).unwrap(),
        "sum(3, 4); sum(5, 6);\n"
    );
    assert!(app.replace_confirm.is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn structural_replace_workspace_with_no_matches_reports_status() {
    let dir = unique_dir("structural-replace-workspace-empty");
    fs::write(dir.join("a.txt"), "unrelated content\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("edit.structural_replace_workspace");
    for c in "add($X, $Y)".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    for c in "sum($X, $Y)".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.replace_confirm.is_none());
    assert!(!app.status.is_empty());

    fs::remove_dir_all(&dir).ok();
}
