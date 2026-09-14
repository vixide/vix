#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]

use crate::common::*;

// Each call needs its own `unique_dir` tag -- these tests run in parallel by
// default, and `unique_dir("x")` is `vix-x-<pid>`, identical for every call
// with the same tag in one process, so two tests sharing one would race on
// the same file path (caught exactly this way: cross-test garbage in a
// buffer's content).
fn vi_app(tag: &str, text: &str, modal_engine: bool) -> App {
    let mut app = app_with(Settings {
        keymap: "vi".to_string(),
        modal_engine,
        ..Settings::default()
    });
    let dir = unique_dir(tag);
    let file = dir.join("a.txt");
    fs::write(&file, text).unwrap();
    app.open_initial(&file);
    app
}

fn selection(app: &mut App) -> Option<(usize, usize)> {
    app.editor
        .active_tab_mut()
        .and_then(|t| t.editor.get_selection())
        .map(|s| (s.start, s.end))
}

#[test]
fn modal_engine_off_leaves_v_as_a_no_op() {
    // Matches vix-modal/spec/index.md's own audit: "no v/V/Ctrl-V binding"
    // today. With the setting off (the default), that must stay true.
    let mut app = vi_app("modal-off-noop", "hello world\n", false);
    app.on_key(key('v'));
    assert!(
        selection(&mut app).is_none(),
        "'v' should do nothing with the setting off"
    );
}

#[test]
fn v_enters_visual_mode_and_hl_extend_the_selection() {
    let mut app = vi_app("modal-hl-extend", "hello world\n", true);
    app.on_key(key('v'));
    app.on_key(key('l'));
    app.on_key(key('l'));
    let (start, end) = selection(&mut app).expect("a selection should exist after v + l l");
    assert_eq!(
        (start, end),
        (0, 2),
        "anchor at 0, cursor moved right twice"
    );
}

#[test]
fn escape_from_visual_collapses_the_selection_and_returns_to_normal() {
    let mut app = vi_app("modal-esc-collapse", "hello world\n", true);
    app.on_key(key('v'));
    app.on_key(key('l'));
    app.on_key(esc());
    assert!(
        selection(&mut app).is_none(),
        "clear_selection leaves no selection at all"
    );

    // Back in Normal mode: 'v' should be able to start a fresh selection.
    app.on_key(key('v'));
    app.on_key(key('l'));
    let (start2, end2) = selection(&mut app).expect("a fresh selection after re-entering Visual");
    assert_ne!(start2, end2, "really back in Visual mode, not stuck");
}

#[test]
fn capital_v_enters_visual_line_and_selects_whole_lines() {
    let mut app = vi_app("modal-vline", "one\ntwo\nthree\n", true);
    app.on_key(key('V'));
    let text = app.editor.active_tab().unwrap().text();
    let (start, end) = selection(&mut app).expect("V selects the current line immediately");
    assert_eq!(&text[start..end], "one\n");

    // Extending down should grow the selection by whole lines, not just by
    // the cursor's own column movement.
    app.on_key(key('j'));
    let (start, end) = selection(&mut app).unwrap();
    assert_eq!(&text[start..end], "one\ntwo\n");
}

#[test]
fn visual_mode_does_not_intercept_keys_it_does_not_recognize() {
    // 'x' isn't a modal-engine key at all (T114's job) -- it must keep
    // falling through to the existing vim_normal_key table unchanged.
    let mut app = vi_app("modal-x-fallthrough", "hello\n", true);
    app.on_key(key('v'));
    app.on_key(key('x'));
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(
        text, "ello\n",
        "the old hardcoded 'x' forward-delete still runs"
    );
}
