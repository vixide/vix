//! Behaviour tests for ghost-text completion state handling.

use super::*;
use crate::app::cov4::{app_at, buffer_with, code, content, ctrl, key, temp};
use vix_input::{KeyCode, KeyModifiers};

fn lsp_app(tag: &str, text: &str) -> App {
    let dir = temp(tag);
    let mut app = app_at(&dir);
    app.settings.inline_completion = "lsp".to_string();
    buffer_with(&mut app, text, text.chars().count());
    app
}

/// Install `suggestion` as the current ghost for the active buffer state.
fn show(app: &mut App, suggestion: &str) {
    let c = app.ghost_cursor().expect("a plain buffer");
    app.ghost.ghost = Ghost::new(suggestion, c.revision, c.cursor);
}

#[test]
fn accepting_the_whole_suggestion_inserts_it_and_clears_the_ghost() {
    let mut app = lsp_app("ghost-accept", "let x = ");
    show(&mut app, "compute(1)");
    assert!(app.ghost_accept(false));
    assert_eq!(content(&mut app), "let x = compute(1)");
    assert!(app.ghost.ghost.is_none());
    assert!(app.ghost.due.is_none());
}

#[test]
fn accepting_a_word_keeps_the_rest_as_the_new_suggestion() {
    let mut app = lsp_app("ghost-word", "let ");
    show(&mut app, "alpha beta gamma");
    assert!(app.ghost_accept(true));
    assert_eq!(content(&mut app), "let alpha");
    let rest = app.ghost.ghost.as_ref().expect("the remainder stays");
    assert_eq!(rest.text(), " beta gamma");
    // The remainder is anchored to the new cursor, so it is still current.
    assert!(app.ghost_overlay().is_some());
    assert!(app.ghost_accept(true));
    assert_eq!(content(&mut app), "let alpha beta");
}

#[test]
fn a_suggestion_the_buffer_moved_away_from_is_dropped_not_inserted() {
    let mut app = lsp_app("ghost-stale", "abc");
    show(&mut app, "def");
    app.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    assert!(!app.ghost_accept(false));
    assert!(app.ghost.ghost.is_none());
    assert_eq!(content(&mut app), "abc");
    // With no suggestion at all there is nothing to accept either.
    assert!(!app.ghost_accept(true));
}

#[test]
fn accepting_with_a_selection_or_read_only_buffer_is_refused() {
    let mut app = lsp_app("ghost-ro", "abc");
    show(&mut app, "def");
    app.editor.active_tab_mut().unwrap().read_only = true;
    assert!(app.ghost_cursor().is_none());
    assert!(!app.ghost_accept(false));
    assert_eq!(content(&mut app), "abc");
    assert!(app.ghost_overlay().is_none());

    let mut app = lsp_app("ghost-sel", "abc");
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 2);
    assert!(app.ghost_cursor().is_none());
}

#[test]
fn keys_accept_with_tab_one_word_with_ctrl_right_and_dismiss_with_esc() {
    let mut app = lsp_app("ghost-keys", "x");
    assert!(
        !app.ghost_key(code(KeyCode::Tab)),
        "no ghost, key not taken"
    );
    show(&mut app, "one two");
    assert!(!app.ghost_key(key('z')), "unrelated keys are left alone");
    assert!(app.ghost.ghost.is_some());
    assert!(
        !app.ghost_key(vix_input::KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)),
        "Shift+Tab is not accept"
    );
    assert!(app.ghost_key(vix_input::KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::CONTROL
    )));
    assert_eq!(content(&mut app), "xone");
    assert!(app.ghost_key(code(KeyCode::Tab)));
    assert_eq!(content(&mut app), "xone two");
    show(&mut app, "more");
    assert!(app.ghost_key(code(KeyCode::Esc)));
    assert!(app.ghost.ghost.is_none());
    // A plain Ctrl+letter is not a ghost key either.
    show(&mut app, "more");
    assert!(!app.ghost_key(ctrl('a')));
}

#[test]
fn ghost_keys_yield_to_a_focus_elsewhere() {
    let mut app = lsp_app("ghost-focus", "x");
    show(&mut app, "tail");
    app.focus = Focus::Explorer;
    assert!(!app.ghost_key(code(KeyCode::Tab)));
    assert!(app.ghost_overlay().is_none());
}

#[test]
fn the_overlay_shows_the_first_line_and_whether_more_follow() {
    let mut app = lsp_app("ghost-overlay", "fn f() ");
    show(&mut app, "{ 1 }");
    assert_eq!(app.ghost_overlay(), Some(("{ 1 }", false)));
    show(&mut app, "{\n    1\n}");
    assert_eq!(app.ghost_overlay(), Some(("{", true)));
    // Text after the cursor on the same line hides the suggestion.
    buffer_with(&mut app, "ab", 1);
    show(&mut app, "zz");
    assert!(app.ghost_overlay().is_none());
}

#[test]
fn triggering_when_off_says_so_and_otherwise_arms_an_immediate_request() {
    let mut app = lsp_app("ghost-trigger", "a");
    app.settings.inline_completion = "off".to_string();
    app.ghost_trigger();
    assert_eq!(app.status, t!("status.inline_completion_off").to_string());
    assert!(app.ghost.due.is_none());

    app.settings.inline_completion = "lsp".to_string();
    show(&mut app, "old");
    app.ghost.ai_failed = true;
    app.ghost_trigger();
    assert!(app.ghost.ghost.is_none());
    assert!(!app.ghost.ai_failed);
    assert_eq!(app.ghost.due.map(|(_, automatic)| automatic), Some(false));
    app.ghost_dismiss();
    assert!(app.ghost.ghost.is_none());
}

#[test]
fn language_server_suggestions_are_shown_only_for_the_state_they_were_asked_for() {
    let mut app = lsp_app("ghost-lsp", "ab ");
    let path = std::path::PathBuf::from("/definitely/elsewhere.rs");
    let c = app.ghost_cursor().unwrap();
    app.ghost.lsp_pending = Some((c.revision, c.cursor));

    // A reply for another document is ignored and keeps waiting.
    app.on_ghost_lsp(&path, vec!["nope".into()]);
    assert!(app.ghost.ghost.is_none());
    assert_eq!(app.ghost.lsp_pending, Some((c.revision, c.cursor)));

    // An untitled buffer has no path, so no reply matches it.
    app.on_ghost_lsp(std::path::Path::new(""), vec!["nope".into()]);
    assert!(app.ghost.ghost.is_none());

    // Give the buffer a path: a matching reply installs the first item.
    app.editor.active_tab_mut().unwrap().path = Some(path.clone());
    let c = app.ghost_cursor().unwrap();
    app.ghost.lsp_pending = Some((c.revision, c.cursor));
    app.on_ghost_lsp(&path, vec!["first".into(), "second".into()]);
    assert_eq!(app.ghost.ghost.as_ref().map(Ghost::text), Some("first"));
    assert!(app.ghost.lsp_pending.is_none());

    // When the setting changed meanwhile the pending flag clears but nothing shows.
    app.ghost.ghost = None;
    app.ghost.lsp_pending = Some((c.revision, c.cursor));
    app.settings.inline_completion = "off".to_string();
    app.on_ghost_lsp(&path, vec!["late".into()]);
    assert!(app.ghost.ghost.is_none());
    assert!(app.ghost.lsp_pending.is_none());
}

#[test]
fn polling_with_the_feature_off_forgets_everything() {
    let mut app = lsp_app("ghost-poll-off", "a");
    show(&mut app, "b");
    app.settings.inline_completion = "off".to_string();
    app.poll_ghost();
    assert!(app.ghost.ghost.is_none());
    assert!(app.ghost.seen.is_none());
}

#[test]
fn only_typing_arms_a_request_and_moving_away_drops_the_suggestion() {
    let mut app = lsp_app("ghost-poll", "ab");
    // The first sighting of a buffer never arms a request.
    app.poll_ghost();
    assert!(app.ghost.seen.is_some());
    assert!(app.ghost.due.is_none());

    // An edit arms the automatic, debounced request.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .insert_str_action("c");
    app.poll_ghost();
    let (_, automatic) = app.ghost.due.expect("typing arms a request");
    assert!(automatic);

    // Moving the cursor drops a suggestion computed for the old position and
    // arms nothing new.
    show(&mut app, "tail");
    app.ghost.due = None;
    app.editor.active_tab_mut().unwrap().editor.set_cursor(0);
    app.poll_ghost();
    assert!(app.ghost.ghost.is_none());
    assert!(app.ghost.due.is_none());

    // A buffer that cannot take ghost text clears the state.
    app.ghost.due = Some((Instant::now(), true));
    app.editor.active_tab_mut().unwrap().read_only = true;
    app.poll_ghost();
    assert!(app.ghost.due.is_none());
}

#[test]
fn a_due_request_without_language_server_support_sends_nothing() {
    let mut app = lsp_app("ghost-due", "ab");
    app.editor.active_tab_mut().unwrap().path = Some("/nowhere/x.rs".into());
    app.poll_ghost();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .insert_str_action("c");
    app.poll_ghost();
    assert!(app.ghost.due.is_some(), "typing armed the debounce");
    app.ghost.due = Some((Instant::now(), true));
    app.poll_ghost();
    assert!(app.ghost.due.is_none(), "the due request was consumed");
    assert!(
        app.ghost.lsp_pending.is_none(),
        "no server, nothing in flight"
    );
    let c = app.ghost_cursor().unwrap();
    assert!(!app.request_ghost(Source::Lsp, &c, false));
    assert!(!app.request_ghost(Source::Off, &c, false));
}

#[test]
fn a_blank_path_buffer_cannot_ask_a_language_server() {
    let mut app = lsp_app("ghost-nopath", "ab");
    let c = app.ghost_cursor().unwrap();
    assert!(c.path.is_none());
    assert!(!app.request_ghost(Source::Lsp, &c, true));
}

#[test]
fn text_after_the_cursor_blocks_a_request() {
    let mut app = lsp_app("ghost-rest", "ab");
    buffer_with(&mut app, "abcd", 2);
    let c = app.ghost_cursor().unwrap();
    assert!(!c.rest_blank);
    assert!(!app.request_ghost(Source::Lsp, &c, true));
    assert_eq!(c.line_prefix, "ab");
}

#[test]
fn ai_replies_are_collected_when_still_current_and_discarded_otherwise() {
    let mut app = lsp_app("ghost-ai", "ab ");
    app.settings.inline_completion = "ai".to_string();
    let c = app.ghost_cursor().unwrap();

    // Still waiting: the request is kept.
    let (tx, rx) = std::sync::mpsc::channel();
    app.ghost.ai = Some(AiRequest {
        rx,
        revision: c.revision,
        cursor: c.cursor,
    });
    app.collect_ghost_ai(&c);
    assert!(app.ghost.ai.is_some());
    assert!(app.ghost.ghost.is_none());

    // A reply for the current state becomes the suggestion.
    tx.send(Some("done".to_string())).unwrap();
    app.collect_ghost_ai(&c);
    assert!(app.ghost.ai.is_none());
    assert_eq!(app.ghost.ghost.as_ref().map(Ghost::text), Some("done"));

    // A reply for an older revision is thrown away.
    app.ghost.ghost = None;
    let (tx, rx) = std::sync::mpsc::channel();
    app.ghost.ai = Some(AiRequest {
        rx,
        revision: c.revision.wrapping_add(1),
        cursor: c.cursor,
    });
    tx.send(Some("old".to_string())).unwrap();
    app.collect_ghost_ai(&c);
    assert!(app.ghost.ghost.is_none());

    // A worker that died without replying leaves no suggestion.
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    drop(tx);
    app.ghost.ai = Some(AiRequest {
        rx,
        revision: c.revision,
        cursor: c.cursor,
    });
    app.collect_ghost_ai(&c);
    assert!(app.ghost.ai.is_none());
    assert!(app.ghost.ghost.is_none());
}

#[test]
fn an_ai_request_is_refused_after_a_failure_or_when_ai_is_disabled() {
    let mut app = lsp_app("ghost-ai-off", "ab ");
    app.settings.inline_completion = "ai".to_string();
    let c = app.ghost_cursor().unwrap();
    app.ghost.ai_failed = true;
    assert!(!app.request_ghost(Source::Ai, &c, true));
    app.ghost.ai_failed = false;
    if !app.settings.ai_enabled() {
        assert!(!app.request_ghost(Source::Ai, &c, true));
        assert!(!app.ghost.ai_failed, "disabled AI is not a failure");
    }
}
