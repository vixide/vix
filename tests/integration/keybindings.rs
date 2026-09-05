#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn keybindings_reload_action_reports_a_summary() {
    let mut app = app_at(Path::new("."));
    let before = app.messages.items.len();
    app.run_action("keybindings.reload");
    assert!(
        app.messages.items.len() > before,
        "keybindings.reload reports how many overrides ended up active"
    );
}

// ----- keybinding editor (Vix → Keybindings…, T204) -----------------------
// The success paths of a rebind/reset write through
// `vix_keybindings::user_bindings` to `Settings::keybindings_path()` — the
// real user config directory, with no test-only override (T104h's own
// crate-level tests avoid it the same way, writing straight to
// `std::env::temp_dir()` instead of through `App`). So these tests cover
// everything up to but not including that write: opening/closing/filtering
// the overlay, the Prompt-wins-over-keybinding_editor dispatch order the
// rebind flow depends on, and `accept_rebind_key`'s validation, which
// rejects an empty or unparseable token before ever touching the file.

#[test]
fn keybinding_editor_action_opens_and_closes() {
    let mut app = app_at(Path::new("."));
    assert!(app.keybinding_editor.is_none());
    app.run_action("keybindings.editor");
    assert!(
        app.keybinding_editor.is_some(),
        "Vix → Keybindings… opens the overlay"
    );
    app.on_key(esc());
    assert!(app.keybinding_editor.is_none(), "Esc closes the overlay");
}

#[test]
fn keybinding_editor_lists_built_in_rows_and_filters() {
    // Doesn't assert a specific row's `Source`: `build_keybinding_rows`
    // reads the real `Settings::keybindings_path()` (T104h has no
    // test-only override for it, see the module comment above), so on a
    // machine with a real `keybindings.toml` a builtin token could
    // legitimately show as `User` here. The row's presence and key
    // display are host-independent; its source is not.
    let mut app = app_with(Settings {
        keymap: "apple".to_string(),
        ..Settings::default()
    });
    app.run_action("keybindings.editor");
    let p = app.keybinding_editor.as_ref().expect("open");
    assert!(
        p.rows
            .iter()
            .any(|r| r.key_display == "Ctrl P" && r.action_id == "tools.palette"),
        "the active keymap's built-in Ctrl P row is listed"
    );
    let total = p.len();
    for c in "palette".chars() {
        app.on_key(key(c));
    }
    let p = app.keybinding_editor.as_ref().unwrap();
    assert!(p.len() < total, "typing filters the rows");
    assert_eq!(p.selected, 0, "filtering resets the selection");
}

#[test]
fn keybinding_editor_enter_opens_a_rebind_prompt_that_wins_over_the_editor() {
    let mut app = app_at(Path::new("."));
    app.run_action("keybindings.editor");
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        matches!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::RebindKey)
        ),
        "Enter on the selected row opens a rebind prompt"
    );
    assert!(
        app.keybinding_editor.is_some(),
        "the editor stays open underneath the prompt"
    );
    // The dispatch-order fix this depends on: `prompt` must be checked
    // before `keybinding_editor` in `try_panel_key`'s panel! table, or a
    // typed character would filter the (invisible) editor underneath
    // instead of landing in the prompt's input.
    app.on_key(key('x'));
    assert_eq!(app.prompt.as_ref().unwrap().input, "x");
    assert_eq!(
        app.keybinding_editor.as_ref().unwrap().query,
        "",
        "the keystroke went to the prompt, not the editor's filter"
    );
    app.on_key(esc());
    assert!(app.prompt.is_none(), "Esc cancels the rebind prompt");
    assert!(
        app.keybinding_editor.is_some(),
        "canceling the prompt leaves the editor open"
    );
}

#[test]
fn accept_rebind_key_rejects_an_empty_or_unparseable_token_without_writing() {
    let mut app = app_at(Path::new("."));
    app.run_action("keybindings.editor");
    app.on_key(keycode(KeyCode::Enter));
    let before = app.messages.items.len();
    app.on_key(keycode(KeyCode::Enter)); // submit an empty prompt
    assert!(app.prompt.is_none(), "the empty submission is consumed");
    assert_eq!(
        app.messages.items.len(),
        before,
        "an empty token is silently dropped, not reported as an error"
    );

    app.on_key(keycode(KeyCode::Enter)); // re-open the rebind prompt
    for c in "not a key".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Error)),
        "an unparseable token is reported as an error"
    );
}

#[test]
fn reset_selected_keybinding_is_a_no_op_on_a_built_in_row() {
    // Hand-built, not `run_action("keybindings.editor")`'s real rows: this
    // must never reach `vix_keybindings::user_bindings::remove` on the
    // real `Settings::keybindings_path()`, and a `BuiltIn` row 0 is the
    // one guarantee that has to hold regardless of host state.
    use vix_keybinding_editor_panel::{Panel, Row, Source};
    let mut app = app_at(Path::new("."));
    app.keybinding_editor = Some(Panel::open(vec![Row {
        key_token: "C-p".to_string(),
        key_display: "Ctrl P".to_string(),
        action_id: "tools.palette".to_string(),
        action_title: "Command Palette".to_string(),
        source: Source::BuiltIn,
    }]));
    let before = app.messages.items.len();
    app.on_key(keycode(KeyCode::Delete));
    assert_eq!(
        app.keybinding_editor.as_ref().map(|p| p.rows.len()),
        Some(1),
        "a built-in row isn't resettable, so nothing is removed"
    );
    assert_eq!(
        app.messages.items.len(),
        before,
        "no reset message when there was nothing to reset"
    );
}
