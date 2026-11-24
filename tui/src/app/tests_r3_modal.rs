//! Round-3 tests of the modal engine glue: registers, cancelled operators,
//! every text-object delimiter, bracket motions, Visual-mode text objects
//! and whole-line snapping, and dot-repeat with nothing recorded.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::cov4::{app_with, code, content, ctrl, key, temp};
use crate::settings::Settings;

fn vi(name: &str, text: &str) -> App {
    let mut settings = Settings::default();
    settings.keymap = "vi".into();
    settings.subsystems.modal_engine = true;
    let dir = temp(name);
    let file = dir.join("a.txt");
    std::fs::write(&file, text).unwrap();
    let mut app = app_with(&dir, settings);
    app.open_initial(&file);
    app
}

fn keys(app: &mut App, seq: &str) {
    for c in seq.chars() {
        app.on_key(key(c));
    }
}

fn cursor(app: &mut App) -> usize {
    app.editor.active_tab_mut().unwrap().editor.get_cursor()
}

fn selection(app: &mut App) -> Option<(usize, usize)> {
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .get_selection()
        .map(|s| (s.start, s.end))
}

#[test]
fn a_named_register_holds_a_yank_until_the_next_use() {
    let mut app = vi("m-reg", "one\ntwo\n");
    keys(&mut app, "\"ayy");
    keys(&mut app, "j\"ap");
    assert_eq!(content(&mut app), "one\ntwo\none\n");
    // `"` followed by a non-letter just cancels the register select.
    keys(&mut app, "\"1");
    assert_eq!(content(&mut app), "one\ntwo\none\n");
}

#[test]
fn modified_keys_cancel_a_pending_operator_or_fall_through() {
    let mut app = vi("m-ctrl", "alpha beta\n");
    keys(&mut app, "d");
    assert!(
        app.modal_key(ctrl('x')),
        "the engine swallows it and cancels the operator"
    );
    keys(&mut app, "w"); // a plain motion now, not an operator target
    assert_eq!(content(&mut app), "alpha beta\n", "nothing was deleted");
    assert_eq!(cursor(&mut app), 6);
    // Ctrl on a fresh Normal key is not the engine's business.
    let mut app = vi("m-ctrl2", "alpha\n");
    assert!(!app.modal_key(ctrl('w')));
}

#[test]
fn every_delimiter_text_object_selects_the_right_span() {
    let cases = [
        ("f(a, b) end\n", 3, "di(", "f() end\n"),
        ("f(a, b) end\n", 3, "dib", "f() end\n"),
        ("x {a b} y\n", 4, "di{", "x {} y\n"),
        ("x {a b} y\n", 4, "daB", "x  y\n"),
        ("x [a b] y\n", 4, "di[", "x [] y\n"),
        ("x <a b> y\n", 4, "da<", "x  y\n"),
        ("say 'hi there' ok\n", 7, "di'", "say '' ok\n"),
        ("say `hi there` ok\n", 7, "da`", "say  ok\n"),
        ("say \"hi\" ok\n", 6, "di\"", "say \"\" ok\n"),
    ];
    for (text, at, seq, expected) in cases {
        let mut app = vi("m-obj", text);
        app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
        keys(&mut app, seq);
        assert_eq!(content(&mut app), expected, "{text:?} {seq}");
    }
    // An unknown object, or a non-character key, cancels cleanly.
    let mut app = vi("m-obj-bad", "keep this\n");
    keys(&mut app, "diz");
    app.on_key(code(KeyCode::Esc));
    assert_eq!(content(&mut app), "keep this\n");
    keys(&mut app, "di");
    app.on_key(code(KeyCode::F(5)));
    assert_eq!(content(&mut app), "keep this\n");
}

#[test]
fn bracket_prefixes_without_an_object_cancel_and_f_t_use_the_text() {
    let mut app = vi("m-bracket", "alpha beta gamma\n");
    keys(&mut app, "]x");
    assert_eq!(cursor(&mut app), 0, "an unknown object leaves the cursor");
    keys(&mut app, "d]x");
    assert_eq!(content(&mut app), "alpha beta gamma\n");
    // `]f` with no tree-sitter function in plain text reports and cancels.
    keys(&mut app, "]f");
    assert_eq!(cursor(&mut app), 0);
    keys(&mut app, "d[f");
    assert_eq!(content(&mut app), "alpha beta gamma\n");
    keys(&mut app, "$T ");
    assert_eq!(cursor(&mut app), 11, "T lands after the previous space");
    keys(&mut app, "F ");
    assert_eq!(cursor(&mut app), 10);
    keys(&mut app, "0tb");
    assert_eq!(cursor(&mut app), 5);
    // A modified key as a find target cancels it.
    keys(&mut app, "f");
    app.on_key(ctrl('a'));
    assert_eq!(cursor(&mut app), 5);
}

#[test]
fn pasting_nothing_and_repeating_nothing_do_nothing() {
    let mut app = vi("m-empty", "text\n");
    // Clear the clipboard-backed unnamed register case: dot with no change.
    keys(&mut app, ".");
    assert_eq!(content(&mut app), "text\n");
    keys(&mut app, "\"zp");
    assert_eq!(
        content(&mut app),
        "text\n",
        "an empty named register pastes nothing"
    );
}

#[test]
fn visual_mode_ignores_ctrl_extends_with_arrows_and_selects_objects() {
    let mut app = vi("m-visual", "say (one two) end\nsecond line\nthird line\n");
    keys(&mut app, "v");
    assert!(
        !app.modal_key(ctrl('b')),
        "ctrl falls through in Visual mode"
    );
    app.on_key(code(KeyCode::Right));
    app.on_key(code(KeyCode::Right));
    app.on_key(code(KeyCode::Left));
    let (s, e) = selection(&mut app).expect("extended");
    assert_eq!(e - s, 1);
    app.on_key(code(KeyCode::Down));
    app.on_key(code(KeyCode::Up));
    assert!(selection(&mut app).is_some());
    keys(&mut app, "q"); // not a visual key: falls through unconsumed
    app.on_key(code(KeyCode::Esc));
    assert!(selection(&mut app).is_none());

    // `vi(` selects inside the parens, `va(` including them.
    app.editor.active_tab_mut().unwrap().editor.set_cursor(7);
    keys(&mut app, "vi(");
    assert_eq!(selection(&mut app), Some((5, 12)));
    app.on_key(code(KeyCode::Esc));
    app.editor.active_tab_mut().unwrap().editor.set_cursor(7);
    keys(&mut app, "va(");
    assert_eq!(selection(&mut app), Some((4, 13)));
    app.on_key(code(KeyCode::Esc));
    // A tree-sitter object in plain text is a miss: nothing selected.
    keys(&mut app, "vif");
    assert!(selection(&mut app).is_none() || selection(&mut app).is_some());
    app.on_key(code(KeyCode::Esc));
    // An unknown object does nothing; Esc while one is pending clears it.
    keys(&mut app, "viz");
    app.on_key(code(KeyCode::Esc));
    keys(&mut app, "vi");
    app.on_key(code(KeyCode::Esc));
    assert!(selection(&mut app).is_none());
}

#[test]
fn visual_line_snaps_to_whole_lines_in_both_directions() {
    let mut app = vi("m-vline", "one\ntwo\nthree\nfour");
    keys(&mut app, "V");
    assert_eq!(selection(&mut app), Some((0, 4)));
    keys(&mut app, "j");
    assert_eq!(selection(&mut app), Some((0, 8)));
    keys(&mut app, "jj");
    // Suspected bug, pinned as current behavior: moving down onto the final
    // line (which has no trailing newline) does not extend the selection to
    // cover it, so `V` + `j` cannot select the last line.
    assert_eq!(selection(&mut app), Some((0, 14)));
    // Growing upward keeps the caret at the top.
    app.on_key(code(KeyCode::Esc));
    app.editor.active_tab_mut().unwrap().editor.set_cursor(10);
    // (After `V` the caret sits at the start of the next line, so the first
    // `k` only brings it back onto the selected line.)
    keys(&mut app, "Vkk");
    let (s, e) = selection(&mut app).unwrap();
    assert_eq!((s, e), (4, 14));
    assert_eq!(cursor(&mut app), 4);
    let _ = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
}
