//! T618: inline ghost-text completion from the AI source. The "model" is a
//! fake `ai_command` that prints a canned reply, so nothing here touches a
//! network or a real assistant. The language-server source has its own test
//! against a mock server in `tui/tests/lsp_smoke.rs`.

use ratatui::{Terminal, backend::TestBackend};

use crate::integration::common::*;

/// An app whose AI "model" always answers with `reply`, using the AI source.
fn ghost_app(dir: &Path, reply: &str) -> App {
    let canned = dir.join("canned-ghost-reply.txt");
    fs::write(&canned, reply).unwrap();
    let mut s = Settings::default();
    s.ai_command = if cfg!(windows) {
        format!("type {}", canned.display())
    } else {
        format!("cat {}", canned.display())
    };
    s.inline_completion = "ai".to_string();
    let mut app = app_at_with(dir, s);
    app.layout.editor = Rect::new(0, 0, 100, 20);
    buffer_with(&mut app, "", 0);
    // A first tick records the starting state: only later edits ask.
    app.tick();
    app
}

fn screen(app: &mut App) -> String {
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    term.backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().editor.get_content()
}

/// Tick until the screen shows `needle`, or give up after a few seconds.
fn show(app: &mut App, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        app.tick();
        if screen(app).contains(needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Tick for a while and report whether the screen ever showed `needle`.
fn ever_shows(app: &mut App, needle: &str) -> bool {
    let until = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < until {
        app.tick();
        if screen(app).contains(needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn an_ai_suggestion_is_cleaned_drawn_dropped_on_typing_and_accepted() {
    let dir = unique_dir("ghost-ai");
    let mut app = ghost_app(&dir, "```\nlet x = 1 + 2;\n```\n");
    type_str(&mut app, "let x = ");
    // The fence and the echoed line prefix are gone; the rest is drawn.
    assert!(show(&mut app, "let x = 1 + 2;"), "the suggestion is drawn");
    assert_eq!(text(&app), "let x = ", "drawing inserts nothing");

    // Typing drops it at once (the next one is asked for after a pause).
    type_str(&mut app, "9");
    assert!(
        !screen(&mut app).contains("let x = 91 + 2;"),
        "a suggestion for older text is not drawn"
    );

    // The reply is not an echo of this line, so it is drawn whole; Tab takes it.
    assert!(show(&mut app, "let x = 9let x = 1 + 2;"));
    app.on_key(keycode(KeyCode::Tab));
    assert_eq!(text(&app), "let x = 9let x = 1 + 2;");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn accepting_a_word_keeps_the_rest_and_escape_dismisses() {
    let dir = unique_dir("ghost-ai-word");
    let mut app = ghost_app(&dir, "alpha beta gamma");
    type_str(&mut app, "x ");
    assert!(show(&mut app, "x alpha beta gamma"));
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(text(&app), "x alpha");
    assert!(
        screen(&mut app).contains("x alpha beta gamma"),
        "rest stays"
    );
    app.on_key(keycode(KeyCode::Esc));
    assert!(!screen(&mut app).contains("x alpha beta"));
    assert_eq!(text(&app), "x alpha", "Esc inserts nothing");
    // With nothing on show, Tab is an ordinary Tab again.
    app.on_key(keycode(KeyCode::Tab));
    assert_ne!(text(&app), "x alpha");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_actions_accept_and_dismiss_the_same_suggestion() {
    let dir = unique_dir("ghost-ai-actions");
    let mut app = ghost_app(&dir, "one two");
    type_str(&mut app, "n ");
    assert!(show(&mut app, "n one two"));
    app.run_action("inline_completion.accept_word");
    assert_eq!(text(&app), "n one");
    app.run_action("inline_completion.accept");
    assert_eq!(text(&app), "n one two");
    app.run_action("inline_completion.trigger");
    assert!(show(&mut app, "n one twoone two"));
    app.run_action("inline_completion.dismiss");
    assert!(!screen(&mut app).contains("n one twoone two"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn nothing_is_asked_when_off_or_when_ai_is_off() {
    let dir = unique_dir("ghost-ai-off");
    let mut app = ghost_app(&dir, "SHOULD NOT SHOW");
    app.settings.inline_completion = "off".to_string();
    type_str(&mut app, "q ");
    assert!(!ever_shows(&mut app, "SHOULD NOT SHOW"), "setting off");
    app.run_action("inline_completion.trigger");
    assert!(
        !app.status.is_empty(),
        "the trigger says the feature is off"
    );

    app.settings.inline_completion = "ai".to_string();
    app.settings.ai = "off".to_string();
    type_str(&mut app, "r ");
    assert!(!ever_shows(&mut app, "SHOULD NOT SHOW"), "ai = off");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn no_suggestion_while_text_follows_the_cursor() {
    let dir = unique_dir("ghost-ai-mid");
    let mut app = ghost_app(&dir, "MIDLINE");
    buffer_with(&mut app, "ab", 1);
    app.tick();
    type_str(&mut app, "x");
    assert!(!ever_shows(&mut app, "MIDLINE"), "text after the cursor");
    fs::remove_dir_all(&dir).ok();
}
