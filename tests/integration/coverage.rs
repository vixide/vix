#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

const GREEN: ratatui::style::Color = ratatui::style::Color::Rgb(0x3f, 0xb9, 0x50);
const RED: ratatui::style::Color = ratatui::style::Color::Rgb(0xf8, 0x51, 0x49);

#[test]
fn load_coverage_file_opens_a_prefilled_prompt() {
    let dir = unique_dir("coverage-prompt");
    let mut app = app_with(Settings {
        coverage_path: "coverage.info".to_string(),
        ..Settings::default()
    });
    app.root = dir.clone();
    app.run_action("tools.load_coverage_file");

    let prompt = app.prompt.as_ref().expect("prompt opened");
    assert!(matches!(prompt.kind, PromptKind::LoadCoverageFile));
    assert_eq!(prompt.input, "coverage.info", "prefilled from the setting");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_coverage_file_marks_the_gutter_covered_and_uncovered() {
    let dir = unique_dir("coverage-load");
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("lib.rs");
    fs::write(&src, "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    let report = dir.join("coverage.info");
    fs::write(
        &report,
        format!("SF:{}\nDA:1,3\nDA:2,0\nend_of_record\n", src.display()),
    )
    .unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&src);
    app.run_action("tools.load_coverage_file");
    for c in report.to_string_lossy().chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.prompt.is_none(), "prompt closes on accept");
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned()
        .unwrap_or_default();
    assert_eq!(marks.len(), 2, "both recorded lines are marked: {marks:?}");
    assert!(
        marks.contains(&(0, GREEN)),
        "line 1 (0-based index 0) is covered: {marks:?}"
    );
    assert!(
        marks.contains(&(1, RED)),
        "line 2 (0-based index 1) is uncovered: {marks:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_coverage_gutter_hides_then_restores_the_cached_report() {
    let dir = unique_dir("coverage-toggle");
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("lib.rs");
    fs::write(&src, "fn a() {}\n").unwrap();
    let report = dir.join("coverage.info");
    fs::write(
        &report,
        format!("SF:{}\nDA:1,1\nend_of_record\n", src.display()),
    )
    .unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&src);
    app.run_action("tools.load_coverage_file");
    for c in report.to_string_lossy().chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_some_and(|m| !m.is_empty()),
        "loaded and shown"
    );

    app.run_action("tools.toggle_coverage_gutter");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_none_or(Vec::is_empty),
        "toggling off clears the gutter"
    );

    app.run_action("tools.toggle_coverage_gutter");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_some_and(|m| !m.is_empty()),
        "toggling back on recomputes the marks from the cached report, no reload needed"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn toggle_coverage_gutter_with_nothing_loaded_is_a_no_op() {
    let dir = unique_dir("coverage-toggle-empty");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);
    app.status.clear();

    app.run_action("tools.toggle_coverage_gutter");

    assert!(
        !app.status.is_empty(),
        "reports there's no coverage loaded yet"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_coverage_file_with_a_missing_path_reports_an_error() {
    let dir = unique_dir("coverage-missing");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);
    let before = app.messages.items.len();

    app.run_action("tools.load_coverage_file");
    for c in "does-not-exist.info".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(
        app.messages.items.len() > before,
        "an error message was added"
    );
    assert!(!app.has_coverage(), "nothing was loaded");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_coverage_file_with_empty_input_is_a_no_op() {
    let dir = unique_dir("coverage-empty-input");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    app.run_action("tools.load_coverage_file");
    app.on_key(keycode(KeyCode::Enter)); // accept with an empty prompt

    assert!(!app.has_coverage());
    assert!(app.prompt.is_none(), "the prompt closes either way");

    fs::remove_dir_all(&dir).ok();
}
