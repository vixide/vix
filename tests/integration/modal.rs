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

fn cursor(app: &mut App) -> usize {
    app.editor
        .active_tab_mut()
        .map_or(0, |t| t.editor.get_cursor())
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

// ----- T113: motions + counts, wired into Normal mode -----------------

#[test]
fn a_count_multiplies_h_and_l() {
    // The old table has no count support at all -- `docs/for-vim-users`'s
    // own gap list says so -- so this is real new behavior, not a
    // pre-existing path.
    let mut app = vi_app("modal-count-hl", "hello world\n", true);
    app.on_key(key('3'));
    app.on_key(key('l'));
    assert_eq!(cursor(&mut app), 3, "3l moves 3 chars right");
    app.on_key(key('2'));
    app.on_key(key('h'));
    assert_eq!(cursor(&mut app), 1, "2h moves 2 chars back");
}

#[test]
fn zero_and_caret_are_distinct_motions() {
    // The old table conflated 0/^ into one "smart Home" toggle
    // (`vix-modal/spec/index.md`'s audit); T113 gives them their real,
    // separate meanings.
    let mut app = vi_app("modal-zero-caret", "  abc\n", true);
    for _ in 0..4 {
        app.on_key(key('l'));
    }
    assert_eq!(cursor(&mut app), 4);
    app.on_key(key('0'));
    assert_eq!(cursor(&mut app), 0, "0 always goes to column 0");
    app.on_key(key('l'));
    app.on_key(key('l'));
    app.on_key(key('^'));
    assert_eq!(cursor(&mut app), 2, "^ goes to the first non-blank");
}

#[test]
fn gg_with_a_count_and_g_uppercase_go_to_lines() {
    let mut app = vi_app("modal-gg-g-upper", "one\ntwo\nthree\n", true);
    app.on_key(key('2'));
    app.on_key(key('g'));
    app.on_key(key('g'));
    assert_eq!(cursor(&mut app), 4, "2gg -> start of line 2");
    app.on_key(key('G'));
    assert_eq!(cursor(&mut app), 8, "G with no count -> the last line");
    app.on_key(key('1'));
    app.on_key(key('G'));
    assert_eq!(cursor(&mut app), 0, "1G -> the first line");
}

#[test]
fn a_count_survives_the_wait_for_gg_s_second_key() {
    let mut app = vi_app("modal-gg-count-survives", "a\nb\nc\nd\n", true);
    app.on_key(key('3'));
    app.on_key(key('g'));
    // The count must still be there once the pending 'g' resolves, not
    // reset by the first 'g' itself.
    app.on_key(key('g'));
    assert_eq!(cursor(&mut app), 4, "3gg -> line 3");
}

#[test]
fn word_motions_w_b_e_move_by_word_with_counts() {
    let mut app = vi_app("modal-word-forward", "foo bar baz\n", true);
    app.on_key(key('w'));
    assert_eq!(cursor(&mut app), 4, "w -> start of 'bar'");
    app.on_key(key('2'));
    app.on_key(key('w'));
    assert_eq!(cursor(&mut app), 12, "2w with no more words -> buffer end");
    app.on_key(key('b'));
    assert_eq!(cursor(&mut app), 8, "b -> start of 'baz'");

    let mut app2 = vi_app("modal-word-end", "foo bar baz\n", true);
    app2.on_key(key('e'));
    assert_eq!(cursor(&mut app2), 2, "e -> end of 'foo'");
}

#[test]
fn f_t_capital_f_search_the_current_line_with_a_count() {
    let mut app = vi_app("modal-find-forward", "a.b.c.d\n", true);
    app.on_key(key('f'));
    app.on_key(key('.'));
    assert_eq!(cursor(&mut app), 1, "f. finds the first '.'");
    app.on_key(key('2'));
    app.on_key(key('f'));
    app.on_key(key('.'));
    assert_eq!(
        cursor(&mut app),
        5,
        "2f. from there finds the 2nd '.' after it"
    );

    let mut app2 = vi_app("modal-till-forward", "a.b.c\n", true);
    app2.on_key(key('t'));
    app2.on_key(key('.'));
    assert_eq!(cursor(&mut app2), 0, "t. lands right before the '.'");

    let mut app3 = vi_app("modal-find-backward", "a.b.c.d\n", true);
    app3.on_key(key('$')); // 'd', index 6
    app3.on_key(key('F'));
    app3.on_key(key('.'));
    assert_eq!(
        cursor(&mut app3),
        5,
        "F. finds the nearest '.' before the cursor"
    );
    app3.on_key(key('F'));
    app3.on_key(key('.'));
    assert_eq!(cursor(&mut app3), 3, "another F. finds the next one back");
    app3.on_key(key('T'));
    app3.on_key(key('.'));
    assert_eq!(
        cursor(&mut app3),
        2,
        "T. lands just after the '.' before the cursor"
    );
}

#[test]
fn an_unmatched_find_char_leaves_the_cursor_untouched() {
    let mut app = vi_app("modal-find-miss", "abc\n", true);
    app.on_key(key('f'));
    app.on_key(key('z'));
    assert_eq!(
        cursor(&mut app),
        0,
        "no 'z' on the line -- cursor doesn't move"
    );
}

#[test]
fn paragraph_and_sentence_motions_move_by_block() {
    let mut app = vi_app("modal-paragraph", "one\ntwo\n\nthree\nfour\n", true);
    app.on_key(key('}'));
    assert_eq!(
        cursor(&mut app),
        9,
        "'}}' moves to the start of the next paragraph"
    );
    app.on_key(key('{'));
    assert_eq!(
        cursor(&mut app),
        0,
        "'{{' moves back to the first paragraph's start"
    );

    let mut app2 = vi_app("modal-sentence", "One. Two. Three.\n", true);
    app2.on_key(key(')'));
    assert_eq!(
        cursor(&mut app2),
        5,
        ") moves to the start of the next sentence"
    );
    app2.on_key(key('('));
    assert_eq!(
        cursor(&mut app2),
        0,
        "( moves back to the first sentence's start"
    );
}

#[test]
fn operators_still_fall_through_to_the_old_table_untouched() {
    // T114's job, not T113's -- 'd'/'y'/'x'/'p' must keep working exactly as
    // they did before the engine existed.
    let mut app = vi_app("modal-operators-fallthrough", "hello\n", true);
    app.on_key(key('x'));
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text, "ello\n", "the old hardcoded 'x' still runs");
}
