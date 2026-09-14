#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]

use crate::common::*;

fn active_tab_filename(app: &App) -> String {
    app.editor
        .active_tab()
        .and_then(|t| t.path.as_deref())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[test]
fn help_tutorial_opens_chapter_one_as_an_editable_tab() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");

    assert_eq!(active_tab_filename(&app), "01-moving-around.txt");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("Task 1"),
        "the real lesson body opened: {text}"
    );
    let tab = app.editor.active_tab().unwrap();
    assert!(!tab.read_only, "a tutor working copy is freely editable");
}

#[test]
fn tutor_next_chapter_advances_through_all_six_chapters_then_clamps() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");

    let expected = [
        "01-moving-around.txt",
        "02-editing-basics.txt",
        "03-find-and-replace.txt",
        "04-multi-cursor-and-selection.txt",
        "05-files-tabs-and-palette.txt",
        "06-git-basics.txt",
    ];
    for filename in expected {
        assert_eq!(active_tab_filename(&app), filename);
        app.run_action("tutor.next_chapter");
    }
    // One `next` past the last chapter clamps rather than wrapping back to
    // chapter 1 (`spec/index.md`'s "no wraparound").
    assert_eq!(active_tab_filename(&app), "06-git-basics.txt");
}

#[test]
fn tutor_prev_chapter_clamps_at_the_first_chapter() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");
    app.run_action("tutor.prev_chapter");
    assert_eq!(active_tab_filename(&app), "01-moving-around.txt");
}

#[test]
fn every_chapters_working_copy_opens_with_a_non_empty_body() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");
    for _ in 0..vix::tutor::CHAPTERS.len() {
        assert!(
            !app.editor.active_tab().unwrap().text().is_empty(),
            "{} opened empty",
            active_tab_filename(&app)
        );
        app.run_action("tutor.next_chapter");
    }
}

#[test]
fn tutor_next_chapter_before_the_tutorial_is_open_is_a_no_op() {
    let mut app = app_with(Settings::default());
    app.run_action("tutor.next_chapter");
    assert_ne!(
        active_tab_filename(&app),
        "01-moving-around.txt",
        "the tutorial shouldn't have opened itself"
    );
}

#[test]
fn tutor_restart_chapter_discards_edits() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");

    let pristine = app.editor.active_tab().unwrap().text();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("the learner made a mess of it");
    assert_ne!(app.editor.active_tab().unwrap().text(), pristine);

    app.run_action("tutor.restart_chapter");
    assert_eq!(app.editor.active_tab().unwrap().text(), pristine);
    assert!(
        !app.editor.active_tab().unwrap().dirty,
        "restart clears dirty too"
    );
}

#[test]
fn status_bar_progress_updates_live_as_tasks_are_completed() {
    let mut app = app_with(Settings::default());
    app.run_action("help.tutorial");
    assert!(
        app.status.contains("0/3"),
        "fresh chapter starts at 0/3: {}",
        app.status
    );

    // Complete task 2 ("delete this line") directly, the same way a real
    // edit would leave the buffer, then drive one harmless key through the
    // real `on_key` pipeline -- that's what recomputes the indicator.
    let done = app
        .editor
        .active_tab()
        .unwrap()
        .text()
        .replace("DELETE THIS ENTIRE LINE\n", "");
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&done);
    app.on_key(keycode(KeyCode::Right));

    assert!(
        app.status.contains("1/3"),
        "task 2 alone should show 1/3: {}",
        app.status
    );
}
