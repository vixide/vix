//! Behavior tests for the review glue: what each action reports with no
//! review open, how comment and git failures surface, the guards on the
//! discard answers, and the two-pane view's mouse edge cases.

use super::*;
use crate::app::git::tests::{app_at as base_app_at, ch, click, git, key, repo, temp};
use ratatui::layout::Rect;
use std::path::Path;
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

/// An app rooted at `d` that persists its settings inside `d`, never in the
/// user's real configuration directory.
fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10,1 +10,2 @@\n ctx\n+more\n";

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn error_count(app: &App) -> usize {
    app.messages.items.len()
}

#[test]
fn every_review_action_without_a_review_only_reports_it() {
    let d = repo("rv-none", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    for action in [
        "git.review_comment",
        "git.review_export",
        "git.review_next",
        "git.review_prev",
        "git.review_stage_hunk",
        "git.review_stage_all",
        "git.review_stage_lines",
        "git.review_discard_lines",
        "git.review_discard_hunk",
        "git.review_discard_all",
        "git.review_toggle_view",
    ] {
        app.status.clear();
        assert!(app.run_git_review_action(action), "{action}");
        assert_eq!(
            app.status,
            t!("status.git_review_none").to_string(),
            "{action}"
        );
        assert!(app.confirm.is_none() && app.prompt.is_none(), "{action}");
    }
    assert!(!app.run_git_review_action("git.commit"));
    // The `v` toggle key is not consumed without a review.
    assert!(!app.review_view_key(key(KeyCode::Char('v'))));
}

#[test]
fn asking_for_a_review_needs_a_repository_and_a_range_git_can_diff() {
    let plain = temp("rv-norepo");
    let mut app = app_at(&plain);
    app.run_git_review_action("git.review");
    assert!(app.prompt.is_none());
    app.open_review("a..b");
    assert!(app.review.is_none());

    let d = repo("rv-range", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    app.run_git_review_action("git.review");
    assert!(matches!(
        app.prompt.as_ref().map(|p| p.kind),
        Some(PromptKind::GitReview)
    ));
    app.open_review("");
    assert!(app.review.is_none());
    let before = error_count(&app);
    app.open_review("--not-a-ref");
    assert_eq!(error_count(&app), before + 1);
    assert!(app.review.is_none());
    // A range that is valid but has no changes opens nothing and says so.
    app.open_review("HEAD..HEAD");
    assert_eq!(
        app.status,
        t!("status.git_review_empty", range = "HEAD..HEAD").to_string()
    );
    assert!(app.review.is_none());
}

#[test]
fn commenting_on_a_comment_row_edits_that_comment() {
    let d = temp("rv-comment");
    let mut app = app_at(&d);
    app.open_review_from_diff("a..b", DIFF);
    // Stale answers with nothing pending are ignored.
    app.accept_review_comment("ignored");
    assert_eq!(app.review.as_ref().unwrap().review.comments, []);

    app.editor.goto(7, None, app.layout.editor);
    app.run_git_review_action("git.review_comment");
    assert_eq!(app.prompt.as_ref().unwrap().input, "");
    app.accept_review_comment("why rename?");
    assert_eq!(app.review.as_ref().unwrap().review.comments.len(), 1);
    assert!(
        d.join(format!(
            ".vix/review/{}.json",
            vix_review::store_name("a..b")
        ))
        .is_file()
    );

    // The comment's own row (right under the line) re-opens it for editing.
    app.editor.goto(8, None, app.layout.editor);
    app.run_git_review_action("git.review_comment");
    assert_eq!(app.prompt.as_ref().unwrap().input, "why rename?");
    assert_eq!(
        app.review.as_ref().unwrap().pending,
        Some((0, Side::New, 2))
    );

    // Export writes both files and reports the count.
    app.run_git_review_action("git.review_export");
    assert_eq!(
        app.status,
        t!(
            "status.git_review_exported",
            count = 1,
            path = d.join(".vix/review/a..b.md").display().to_string()
        )
        .to_string()
    );
    // With the pending target gone the answer is dropped, not misapplied.
    app.review.as_mut().unwrap().pending = None;
    app.accept_review_comment("late");
    assert_eq!(
        app.review.as_ref().unwrap().review.comments[0].text,
        "why rename?"
    );
}

#[test]
fn a_blocked_review_folder_is_reported_for_comments_and_export() {
    let d = temp("rv-blocked");
    // `.vix` is a file, so the review folder cannot be created.
    std::fs::write(d.join(".vix"), "not a directory").unwrap();
    let mut app = app_at(&d);
    app.open_review_from_diff("a..b", DIFF);
    app.editor.goto(7, None, app.layout.editor);
    app.run_git_review_action("git.review_comment");
    let before = error_count(&app);
    app.accept_review_comment("note");
    assert_eq!(error_count(&app), before + 1);
    app.run_git_review_action("git.review_export");
    assert_eq!(error_count(&app), before + 2);
}

#[test]
fn staging_a_hunk_or_everything_after_the_index_changed_reports_git() {
    let d = repo("rv-stage-fail", &[("a.txt", "one\n")]);
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    let diff = git(&d, &["diff"]);
    let mut app = app_at(&d);
    app.open_review_from_diff("work", &diff);
    // Someone staged the file already, so the review's patch no longer fits.
    git(&d, &["add", "a.txt"]);
    let hunk_row = app.review.as_ref().unwrap().rendered.hunk_rows()[0];
    app.editor.goto(hunk_row + 2, None, app.layout.editor);
    let before = error_count(&app);
    app.run_git_review_action("git.review_stage_hunk");
    assert_eq!(error_count(&app), before + 1);
    app.run_git_review_action("git.review_stage_all");
    assert_eq!(error_count(&app), before + 2);
}

#[test]
fn discard_answers_without_something_pending_do_nothing() {
    let d = repo("rv-answers", &[("a.txt", "one\n")]);
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    let mut app = app_at(&d);
    // No review at all.
    app.review_discard_answered(true);
    app.review_discard_lines_answered(true);
    app.review_discard_all_answered(true);
    assert_eq!(
        std::fs::read_to_string(d.join("a.txt")).unwrap(),
        "one\ntwo\n"
    );

    app.open_review_from_diff("work", &git(&d, &["diff"]));
    // A review but nothing pending, or a "no" answer, changes nothing.
    app.review_discard_answered(true);
    app.review_discard_lines_answered(true);
    app.review_discard_all_answered(false);
    app.review.as_mut().unwrap().pending_hunk = Some((0, 0));
    app.review_discard_answered(false);
    assert!(app.review.as_ref().unwrap().pending_hunk.is_none());
    // A pending hunk that no longer exists is dropped quietly.
    app.review.as_mut().unwrap().pending_hunk = Some((9, 9));
    app.review_discard_answered(true);
    assert_eq!(
        std::fs::read_to_string(d.join("a.txt")).unwrap(),
        "one\ntwo\n"
    );
}

#[test]
fn discarding_everything_when_the_tree_changed_reports_the_failure() {
    let d = repo("rv-discard-fail", &[("a.txt", "one\n")]);
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    let mut app = app_at(&d);
    app.open_review_from_diff("work", &git(&d, &["diff"]));
    app.run_git_review_action("git.review_discard_all");
    assert_eq!(
        app.confirm.as_ref().unwrap().kind,
        ConfirmKind::GitReviewDiscardAll
    );
    app.confirm = None;
    // The working tree moved on, so reverting the patch cannot apply.
    std::fs::write(d.join("a.txt"), "totally different\n").unwrap();
    let before = error_count(&app);
    app.review_discard_all_answered(true);
    assert_eq!(error_count(&app), before + 1);
    assert_eq!(
        std::fs::read_to_string(d.join("a.txt")).unwrap(),
        "totally different\n"
    );
}

#[test]
fn line_selections_that_git_cannot_express_are_explained() {
    for (e, key) in [
        (SelectError::NoLines, "status.git_review_no_lines"),
        (SelectError::SpansHunks, "status.git_review_lines_span"),
        (SelectError::NoNewline, "status.git_review_lines_newline"),
    ] {
        let shown = review_select_error(e);
        assert_eq!(shown, t!(key).to_string());
    }
}

#[test]
fn v_toggles_the_view_for_the_active_review_only() {
    let d = temp("rv-toggle");
    let mut app = app_at(&d);
    app.open_review_from_diff("a..b", DIFF);
    assert!(!app.review.as_ref().unwrap().side_by_side);
    // Other keys, and chords with Ctrl, are not consumed.
    assert!(!app.review_view_key(key(KeyCode::Char('x'))));
    assert!(!app.review_view_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL)));
    assert!(app.review_view_key(key(KeyCode::Char('v'))));
    assert!(app.review.as_ref().unwrap().side_by_side);
    assert_eq!(
        app.status,
        t!("status.git_review_view_side_by_side").to_string()
    );
    assert!(app.review_view_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::SHIFT)));
    assert_eq!(app.status, t!("status.git_review_view_inline").to_string());
    // Once another tab is active the key falls through to it.
    app.open_readonly_text_tab("other", "text");
    assert!(!app.review_view_key(key(KeyCode::Char('v'))));
}

#[test]
fn the_two_pane_mouse_ignores_outside_clicks_headers_and_drags() {
    let d = temp("rv-mouse");
    let mut app = app_at(&d);
    app.settings.review_view = "side_by_side".to_string();
    app.layout.editor = Rect::new(0, 0, 120, 12);
    let long: String = (0..40)
        .map(|i| [" ctx ", &i.to_string(), "\n"].concat())
        .collect();
    let diff = format!(
        "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1,40 +1,41 @@\n{long}+added\n"
    );
    app.open_review_from_diff("a..b", &diff);
    assert!(app.review.as_ref().unwrap().side_by_side);
    let line_before = app.editor.cursor_line();
    // Not the review tab's wide layout: ignored.
    app.layout.editor = Rect::new(0, 0, 20, 12);
    assert!(!app.review_mouse(click(1, 1)));
    app.layout.editor = Rect::new(0, 0, 120, 12);
    // Outside the editor: swallowed, cursor unmoved.
    assert!(app.review_mouse(click(200, 1)));
    assert_eq!(app.editor.cursor_line(), line_before);
    // The title row has no diff line behind it: swallowed.
    assert!(app.review_mouse(click(5, 0)));
    assert_eq!(app.editor.cursor_line(), line_before);
    // Drags and releases are swallowed rather than selecting hidden text.
    assert!(app.review_mouse(mouse(
        MouseEventKind::Drag(vix_input::MouseButton::Left),
        5,
        3
    )));
    assert_eq!(app.editor.cursor_line(), line_before);
    // Scrolling down moves the view past the cursor, which follows into view.
    for _ in 0..4 {
        assert!(app.review_mouse(mouse(MouseEventKind::ScrollDown, 5, 3)));
    }
    assert!(app.editor.cursor_line() > line_before);
    let moved = app.editor.cursor_line();
    for _ in 0..10 {
        app.review_mouse(mouse(MouseEventKind::ScrollUp, 5, 3));
    }
    assert!(app.editor.cursor_line() <= moved);
    assert!(app.review_panes_for(0, 120).is_some() || app.review_panes_for(1, 120).is_some());
    assert!(app.review_panes_for(0, 20).is_none());
    assert!(app.review_panes_for(99, 120).is_none());
}

#[test]
fn comment_rows_that_name_a_vanished_file_offer_no_target() {
    let d = temp("rv-vanished");
    let mut app = app_at(&d);
    app.open_review_from_diff("a..b", DIFF);
    app.editor.goto(7, None, app.layout.editor);
    app.run_git_review_action("git.review_comment");
    app.accept_review_comment("note");
    // The comment's file is no longer part of the review's files.
    app.review.as_mut().unwrap().review.comments[0].file = "gone.rs".to_string();
    app.editor.goto(8, None, app.layout.editor);
    app.prompt = None;
    app.run_git_review_action("git.review_comment");
    assert!(app.prompt.is_none());
    assert_eq!(app.status, t!("status.git_review_no_line").to_string());
}

#[test]
fn a_stage_all_with_nothing_to_stage_says_there_is_no_hunk() {
    let d = temp("rv-nohunk");
    let mut app = app_at(&d);
    app.open_review_from_diff("a..b", DIFF);
    // Replace the review's diff with a header-only one: files remain, hunks do not.
    app.review
        .as_mut()
        .unwrap()
        .review
        .replace_diff("diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n");
    app.status.clear();
    app.run_git_review_action("git.review_stage_all");
    assert_eq!(app.status, t!("status.git_review_no_hunk").to_string());
    app.status.clear();
    app.run_git_review_action("git.review_discard_all");
    assert_eq!(app.status, t!("status.git_review_no_hunk").to_string());
    let _ = ch('x');
}
