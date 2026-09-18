//! T125's new AI features: "Edit selection with instruction" and "Generate
//! doc comment" (the Git panel's AI commit-message generator has its own
//! throwaway-repo test in `git.rs`, alongside its siblings). These drive the
//! CLI spawn path (`ai_command`, made deterministic via `printf`) end to
//! end -- the first real test coverage `spawn_ai_cmd`/`AiReplace`/
//! `poll_ai_replace` have had at all (Annotate/Improve/Summarize/Explain/
//! Define had none before this).

use std::time::{Duration, Instant};

use crate::common::*;

/// Build an app whose `ai_command` ignores its input and always prints
/// `output` -- deterministic, no real assistant needed. `output` must not
/// contain a single quote (kept simple; every caller here controls it).
fn app_with_canned_ai_reply(dir: &Path, output: &str, ai_diff_review: bool) -> App {
    let settings = Settings {
        ai_command: format!("printf '%s' '{output}'"),
        misc: MiscSettings {
            ai_diff_review,
            ..MiscSettings::default()
        },
        ..Settings::default()
    };
    let mut app = App::new(dir.to_path_buf(), settings).with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// Poll `App::poll_ai_replace` until `pred` holds, or fail after 5 seconds --
/// generous for a local `sh -c printf`, which finishes in well under that.
fn wait_for(app: &mut App, mut pred: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !pred(app) {
        app.poll_ai_replace();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(pred(app), "timed out waiting for the AI task to finish");
}

#[test]
fn edit_with_instruction_does_nothing_without_a_selection() {
    let dir = unique_dir("ai-edit-instr-none");
    let mut app = app_with_canned_ai_reply(&dir, "unused", true);
    buffer_with(&mut app, "hello world\n", 0);
    app.run_action("ai.edit_with_instruction");
    assert!(app.prompt.is_none(), "no prompt without a selection");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_with_instruction_runs_the_typed_text_and_always_opens_a_diff() {
    let dir = unique_dir("ai-edit-instr");
    // ai_diff_review is deliberately off here: unlike Annotate/Improve,
    // "Edit selection with instruction" must force a reviewable diff
    // regardless of the setting.
    let mut app = app_with_canned_ai_reply(&dir, "REPLACED", false);
    buffer_with(&mut app, "hello world\n", 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 5); // "hello"

    app.run_action("ai.edit_with_instruction");
    let prompt = app.prompt.as_ref().expect("the instruction prompt opened");
    assert!(matches!(prompt.kind, vix::app::PromptKind::AiInstruction));
    for ch in "make it louder".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));

    wait_for(&mut app, |app| app.ai_diff_review().is_some());
    assert_eq!(app.ai_diff_review().unwrap().result(), "REPLACED");
    // The buffer itself is untouched until the diff is accepted.
    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        "hello world\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn generate_doc_comment_inserts_just_above_the_cursor_line() {
    let dir = unique_dir("ai-doc-comment");
    // ai_diff_review off: a direct, deterministic buffer assertion.
    let mut app = app_with_canned_ai_reply(&dir, "/// A comment.\n", false);
    buffer_with(&mut app, "fn foo() {}\n", 0);

    app.run_action("ai.generate_doc_comment");
    wait_for(&mut app, |app| {
        app.editor.active_tab().unwrap().editor.get_content() != "fn foo() {}\n"
    });

    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        "/// A comment.\nfn foo() {}\n",
        "the comment is a pure insertion before the original line, nothing else changes"
    );
    fs::remove_dir_all(&dir).ok();
}
