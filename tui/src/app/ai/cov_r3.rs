//! Behavior tests for the AI glue that need no assistant: replies are fed to
//! `poll_ai_replace` through a channel the test owns, so nothing is spawned
//! and no network or CLI is involved.

use super::*;
use crate::app::cov4::{
    alt, app_at as base_app_at, buffer_with, code, content, key, last_message, temp,
};
use std::sync::mpsc;
use vix_input::KeyCode;

/// An app rooted at `d` that persists its settings inside `d`, never in the
/// user's real configuration directory.
fn app_at(d: &std::path::Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

/// Install an in-flight request owned by the test and return its sender.
fn pending(app: &mut App, dest: AiDest) -> mpsc::Sender<AiMsg> {
    let (tx, rx) = mpsc::channel();
    app.ai_replace = Some(AiReplace {
        rx,
        dest,
        label: "Label".to_string(),
    });
    tx
}

fn text(app: &mut App) -> String {
    content(app)
}

#[test]
fn a_reply_replaces_the_whole_buffer_or_just_the_recorded_range() {
    let d = temp("ai-replace");
    let mut app = app_at(&d);
    buffer_with(&mut app, "hello world", 0);
    let before = app.messages.items.len();
    let tx = pending(
        &mut app,
        AiDest::Replace {
            tab: 0,
            target: AiTarget::Range(0, 5),
        },
    );
    tx.send(AiMsg::Done("howdy\n\n".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(text(&mut app), "howdy world");
    assert!(app.ai_replace.is_none());
    assert_eq!(
        app.status,
        t!("status.ai_done", action = "Label").to_string()
    );
    // Only the chat panel and DB workbench have their own inline status; a
    // buffer replacement is also noted in Messages.
    assert_eq!(app.messages.items.len(), before + 1);

    let tx = pending(
        &mut app,
        AiDest::Replace {
            tab: 0,
            target: AiTarget::Whole,
        },
    );
    tx.send(AiMsg::Done("all new".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(text(&mut app), "all new");
    // A range past the end is clamped, and a vanished tab is ignored.
    app.apply_ai_replace(0, AiTarget::Range(3, 99), "!");
    assert_eq!(text(&mut app), "all!");
    app.apply_ai_replace(7, AiTarget::Whole, "ignored");
    assert_eq!(text(&mut app), "all!");
}

#[test]
fn a_reply_can_open_a_new_tab_or_feed_the_chat_panel() {
    let d = temp("ai-dest");
    let mut app = app_at(&d);
    let tabs = app.editor.tabs.len();
    let tx = pending(&mut app, AiDest::NewTab);
    tx.send(AiMsg::Done("fresh".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert_eq!(text(&mut app), "fresh");
    assert_eq!(
        last_message(&app),
        Some(t!("status.ai_done", action = "Label").to_string())
    );

    app.open_ai_panel();
    app.ai_panel.as_mut().unwrap().busy = true;
    let notes = app.messages.items.len();
    let tx = pending(&mut app, AiDest::Panel);
    tx.send(AiMsg::Done("an answer".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(app.messages.items.len(), notes, "the panel reports inline");
    let panel = app.ai_panel.as_ref().unwrap();
    assert!(!panel.busy);
    assert_eq!(panel.last_assistant(), Some("an answer"));
}

#[test]
fn an_empty_or_failed_reply_is_reported_on_every_surface() {
    let d = temp("ai-fail");
    let mut app = app_at(&d);
    // Empty output counts as a failure.
    let tx = pending(&mut app, AiDest::NewTab);
    tx.send(AiMsg::Done("\n".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(
        app.status,
        t!("status.ai_failed", action = "Label").to_string()
    );
    assert!(app.ai_replace.is_none());
    // A reason is shown when the provider gave one.
    let tx = pending(&mut app, AiDest::NewTab);
    tx.send(AiMsg::Failed(Some("rate limited".to_string())))
        .unwrap();
    app.poll_ai_replace();
    assert_eq!(
        app.status,
        t!(
            "status.ai_failed_detail",
            action = "Label",
            reason = "rate limited"
        )
        .to_string()
    );
    // A worker that vanished without answering is a failure too.
    let tx = pending(&mut app, AiDest::NewTab);
    drop(tx);
    app.poll_ai_replace();
    assert_eq!(
        app.status,
        t!("status.ai_failed", action = "Label").to_string()
    );
    // Nothing yet: the request stays in flight.
    let _tx = pending(&mut app, AiDest::NewTab);
    app.poll_ai_replace();
    assert!(app.ai_replace.is_some());
    // The chat panel gets an inline error and stops being busy.
    app.ai_replace = None;
    app.open_ai_panel();
    app.ai_panel.as_mut().unwrap().busy = true;
    let tx = pending(&mut app, AiDest::Panel);
    tx.send(AiMsg::Failed(None)).unwrap();
    app.poll_ai_replace();
    assert!(!app.ai_panel.as_ref().unwrap().busy);
    assert!(
        app.ai_panel
            .as_ref()
            .unwrap()
            .turns
            .last()
            .is_some_and(|t| t.role == crate::ai_panel::Role::Error)
    );
}

#[test]
fn a_reply_to_insert_opens_a_diff_or_applies_directly_per_the_setting() {
    let d = temp("ai-insert");
    let mut app = app_at(&d);
    buffer_with(&mut app, "fn x() {}\n", 0);
    app.settings.misc.ai_diff_review = true;
    let tx = pending(&mut app, AiDest::InsertBeforeLine { tab: 0, at: 0 });
    tx.send(AiMsg::Done("/// Doc.".to_string())).unwrap();
    app.poll_ai_replace();
    assert!(app.ai_diff_review().is_some());
    assert_eq!(
        text(&mut app),
        "fn x() {}\n",
        "nothing applied until accepted"
    );
    app.on_key(code(KeyCode::Esc));
    assert!(app.ai_diff_review().is_none());

    app.settings.misc.ai_diff_review = false;
    let tx = pending(&mut app, AiDest::InsertBeforeLine { tab: 0, at: 0 });
    tx.send(AiMsg::Done("/// Doc.".to_string())).unwrap();
    app.poll_ai_replace();
    assert_eq!(text(&mut app), "/// Doc.\nfn x() {}\n");
}

#[test]
fn a_diff_review_moves_toggles_and_accepts_by_key() {
    let d = temp("ai-diffkeys");
    let mut app = app_at(&d);
    buffer_with(&mut app, "a\nb\nc\nd\ne\nf\n", 0);
    app.open_ai_diff(0, AiTarget::Whole, "A\nb\nc\nd\ne\nF\n");
    let review = app.ai_diff_review().unwrap();
    assert_eq!(review.change_count(), 2);
    let all = review.accepted_count();
    app.on_key(key('r'));
    assert_eq!(app.ai_diff_review().unwrap().accepted_count(), 0);
    app.on_key(key('a'));
    assert_eq!(app.ai_diff_review().unwrap().accepted_count(), all);
    // Space toggles the highlighted hunk; arrows move between hunks.
    app.on_key(code(KeyCode::Down));
    app.on_key(key(' '));
    assert_eq!(app.ai_diff_review().unwrap().accepted_count(), all - 1);
    app.on_key(code(KeyCode::Up));
    app.on_key(key(' '));
    assert_eq!(app.ai_diff_review().unwrap().accepted_count(), all - 2);
    app.on_key(key('x'));
    assert!(app.ai_diff_review().is_some());
    // Accept the first hunk only and apply.
    app.on_key(key(' '));
    app.on_key(code(KeyCode::Enter));
    assert!(app.ai_diff_review().is_none());
    assert_eq!(text(&mut app), "A\nb\nc\nd\ne\nf\n");
}

#[test]
fn identical_proposals_and_missing_tabs_open_no_review() {
    let d = temp("ai-nochange");
    let mut app = app_at(&d);
    buffer_with(&mut app, "same\n", 0);
    app.open_ai_diff(0, AiTarget::Whole, "same\n");
    assert!(app.ai_diff_review().is_none());
    assert_eq!(app.status, t!("status.ai_no_change").to_string());
    app.open_ai_diff(9, AiTarget::Whole, "x");
    assert!(app.ai_diff_review().is_none());
    // Range targets diff just that slice.
    buffer_with(&mut app, "hello world\n", 0);
    app.open_ai_diff(0, AiTarget::Range(0, 5), "howdy");
    assert!(app.ai_diff_review().is_some());
}

#[test]
fn the_chat_panel_edits_scrolls_and_hands_replies_on() {
    let d = temp("ai-panel");
    let mut app = app_at(&d);
    app.open_ai_panel();
    for c in "hi!".chars() {
        app.ai_panel_key(key(c));
    }
    app.ai_panel_key(code(KeyCode::Backspace));
    assert_eq!(app.ai_panel.as_ref().unwrap().input, "hi");
    app.ai_panel_key(code(KeyCode::Tab));
    assert_eq!(app.ai_panel.as_ref().unwrap().input, "hi");
    {
        let panel = app.ai_panel.as_mut().unwrap();
        for i in 0..40 {
            panel.push(crate::ai_panel::Role::Assistant, format!("reply {i}"));
        }
        panel.scroll = 30;
    }
    // Scrolling "up" moves back through older turns (a larger offset).
    app.ai_panel_key(code(KeyCode::Up));
    assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 31);
    app.ai_panel_key(code(KeyCode::Down));
    assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 30);
    app.ai_panel_key(code(KeyCode::PageUp));
    assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 40);
    app.ai_panel_key(code(KeyCode::PageDown));
    assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 30);
    // Alt+C copies the last reply; Alt+T opens it in a new tab and closes the panel.
    app.ai_panel_key(alt('c'));
    assert_eq!(app.status, t!("status.ai_copied").to_string());
    let tabs = app.editor.tabs.len();
    app.ai_panel_key(alt('t'));
    assert!(app.ai_panel.is_none());
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert_eq!(text(&mut app), "reply 39");
    // With no reply there is nothing to copy or open.
    app.open_ai_panel();
    app.status.clear();
    app.ai_panel.as_mut().unwrap().turns.clear();
    app.ai_panel_key(alt('c'));
    assert_eq!(app.status, "");
    app.ai_panel_key(alt('t'));
    assert!(app.ai_panel.is_some());
    app.ai_panel_key(code(KeyCode::Esc));
    assert!(app.ai_panel.is_none());
    // Keys with no panel are harmless.
    app.ai_panel_key(key('z'));
    app.ai_panel_key(code(KeyCode::Backspace));
    app.ai_panel_key(code(KeyCode::Up));
    app.ai_panel_key(code(KeyCode::PageDown));
}

#[test]
fn sending_a_chat_line_needs_input_and_a_free_assistant() {
    let d = temp("ai-send");
    let mut app = app_at(&d);
    app.ai_panel_send(); // no panel: nothing
    app.open_ai_panel();
    app.ai_panel_send(); // empty input: nothing
    assert!(app.ai_replace.is_none());
    // Busy: a second request is refused with a status.
    app.ai_panel.as_mut().unwrap().input = "question".to_string();
    let _tx = pending(&mut app, AiDest::Panel);
    app.ai_panel_send();
    assert_eq!(app.status, t!("status.ai_busy").to_string());
    assert_eq!(app.ai_panel.as_ref().unwrap().input, "question");
    // With AI switched off the line is consumed, the assistant refuses, and
    // the panel shows the failure instead of staying busy.
    app.ai_replace = None;
    app.settings.ai = "off".to_string();
    app.ai_panel_send();
    let panel = app.ai_panel.as_ref().unwrap();
    assert!(!panel.busy);
    assert_eq!(panel.input, "");
    assert!(
        panel
            .turns
            .iter()
            .any(|t| t.role == crate::ai_panel::Role::Error)
    );
    assert!(app.ai_replace.is_none());
}

#[test]
fn ai_actions_guard_against_busy_missing_tabs_and_a_disabled_assistant() {
    let d = temp("ai-guards");
    let mut app = app_at(&d);
    buffer_with(&mut app, "some text here\n", 0);
    // Busy: every replacing action says so and starts nothing new.
    let _tx = pending(&mut app, AiDest::NewTab);
    for action in [
        "ai.improve",
        "ai.annotate",
        "ai.edit_with_instruction",
        "ai.generate_doc_comment",
    ] {
        app.status.clear();
        assert!(app.run_ai_action(action), "{action}");
        assert_eq!(app.status, t!("status.ai_busy").to_string(), "{action}");
    }
    app.ai_replace = None;
    // AI off: the assistant refuses and nothing is in flight.
    app.settings.ai = "off".to_string();
    for action in [
        "ai.summarize",
        "ai.explain",
        "ai.define",
        "ai.improve",
        "ai.annotate",
        "ai.generate_doc_comment",
    ] {
        assert!(app.run_ai_action(action), "{action}");
        assert!(app.ai_replace.is_none(), "{action}");
    }
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.ai_off"))
    );
    // No tab at all: the replacing actions return quietly.
    app.settings.ai = "on".to_string();
    app.editor.tabs.clear();
    for action in [
        "ai.improve",
        "ai.annotate",
        "ai.edit_with_instruction",
        "ai.generate_doc_comment",
        "ai.define",
        "ai.summarize",
    ] {
        assert!(app.run_ai_action(action), "{action}");
    }
    assert!(app.ai_replace.is_none() && app.prompt.is_none());
    assert_eq!(app.selected_or_word_text(), "");
    assert!(!app.run_ai_action("ai.nope"));
}

#[test]
fn replacing_actions_capture_the_selection_and_a_disabled_assistant_starts_nothing() {
    let d = temp("ai-selection");
    let mut app = app_at(&d);
    app.settings.ai = "off".to_string();
    buffer_with(&mut app, "hello world\n", 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 5);
    app.ai_improve();
    assert!(app.ai_replace.is_none());
    assert_eq!(text(&mut app), "hello world\n");
    // The instruction flow remembers the selection and asks for the text.
    app.ai_begin_edit_with_instruction();
    assert!(matches!(
        app.ai_instruction_target,
        Some((0, AiTarget::Range(0, 5)))
    ));
    assert!(matches!(
        app.prompt.as_ref().map(|p| p.kind),
        Some(PromptKind::AiInstruction)
    ));
    // An empty instruction is refused; a stale target is ignored.
    app.submit_ai_instruction("   ");
    assert_eq!(app.status, t!("status.ai_no_input").to_string());
    app.submit_ai_instruction("anything");
    app.ai_instruction_target = Some((9, AiTarget::Whole));
    app.submit_ai_instruction("make it louder");
    assert!(app.ai_replace.is_none());
    // A real target with the assistant off reports and starts nothing.
    app.ai_instruction_target = Some((0, AiTarget::Whole));
    app.submit_ai_instruction("make it louder");
    app.ai_instruction_target = Some((0, AiTarget::Range(0, 5)));
    app.submit_ai_instruction("make it louder");
    assert!(app.ai_replace.is_none());
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.ai_off"))
    );
    // Without a selection the instruction needs one.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection(None);
    app.ai_begin_edit_with_instruction();
    assert_eq!(app.status, t!("status.ai_selection_required").to_string());
}

#[test]
fn blank_input_never_reaches_the_assistant() {
    let d = temp("ai-blank");
    let mut app = app_at(&d);
    buffer_with(&mut app, "  \n\n", 0);
    app.status.clear();
    assert!(app.spawn_ai("prompt", "  \n").is_none());
    assert_eq!(app.status, t!("status.ai_no_input").to_string());
    // The doc-comment action needs code under the cursor.
    app.status.clear();
    app.ai_generate_doc_comment();
    assert_eq!(app.status, t!("status.ai_no_input").to_string());
    assert!(app.ai_replace.is_none());
    // With code but the assistant off, the request is refused before sending.
    buffer_with(&mut app, "fn f() {}\n", 0);
    app.settings.ai = "off".to_string();
    app.ai_generate_doc_comment();
    assert!(app.ai_replace.is_none());
    assert!(app.spawn_ghost_ai("a", "b", "a").is_none());
}
