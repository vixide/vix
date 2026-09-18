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
        subsystems: SubsystemSettings {
            modal_engine,
            ..SubsystemSettings::default()
        },
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

fn buffer_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
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

// ----- T114: operators, registers, x, p/P -------------------------------
//
// Every test below selects a named register ("a) before an operator or
// paste, *except* the one dedicated to the unnamed register -- named
// registers are per-`App` state, but the unnamed register mirrors the real
// (process-global, in-memory-in-tests) `vix_clipboard`, which parallel
// tests could in principle race on. Routing everything else through "a
// keeps every other test's outcome fully deterministic regardless of test
// execution order.

#[test]
fn d_w_deletes_the_exclusive_range_up_to_the_next_word() {
    let mut app = vi_app("modal-dw", "foo bar\n", true);
    type_str(&mut app, "\"adw");
    assert_eq!(buffer_text(&app), "bar\n");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn d_e_deletes_the_inclusive_range_through_the_word_end() {
    let mut app = vi_app("modal-de", "foo bar\n", true);
    type_str(&mut app, "\"ade");
    assert_eq!(
        buffer_text(&app),
        " bar\n",
        "'foo' is gone, the space stays"
    );
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn dd_deletes_the_whole_line_and_lands_on_the_first_non_blank() {
    let mut app = vi_app("modal-dd", "one\n  two\nthree\n", true);
    type_str(&mut app, "\"add");
    assert_eq!(buffer_text(&app), "  two\nthree\n");
    assert_eq!(cursor(&mut app), 2, "on the 't' of 'two', not column 0");
}

#[test]
fn a_count_before_dd_deletes_that_many_lines() {
    let mut app = vi_app("modal-dd-count", "one\ntwo\nthree\nfour\n", true);
    type_str(&mut app, "\"a2dd");
    assert_eq!(buffer_text(&app), "three\nfour\n");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn c_w_deletes_and_enters_insert_mode() {
    let mut app = vi_app("modal-cw", "foo bar\n", true);
    type_str(&mut app, "\"acw");
    assert_eq!(buffer_text(&app), "bar\n");
    assert!(
        matches!(app.mode_indicator().as_deref(), Some("-- INSERT --")),
        "c enters Insert mode"
    );
    type_str(&mut app, "XYZ");
    assert_eq!(buffer_text(&app), "XYZbar\n", "typed text lands at the cut");
}

#[test]
fn x_is_sugar_for_d_plus_one_char_right() {
    let mut app = vi_app("modal-x-sugar", "hello\n", true);
    type_str(&mut app, "\"ax");
    assert_eq!(buffer_text(&app), "ello\n");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn yy_copies_the_line_without_changing_the_buffer_then_p_pastes_it_below() {
    let mut app = vi_app("modal-yy-p", "one\ntwo\nthree\n", true);
    type_str(&mut app, "\"ayy");
    assert_eq!(
        buffer_text(&app),
        "one\ntwo\nthree\n",
        "y never touches the buffer"
    );
    app.on_key(key('j')); // onto "two"
    type_str(&mut app, "\"ap");
    assert_eq!(buffer_text(&app), "one\ntwo\none\nthree\n");
    assert_eq!(cursor(&mut app), 8, "the 'o' of the pasted 'one'");
}

#[test]
fn p_and_capital_p_paste_a_char_wise_register_after_and_before_the_cursor() {
    let mut app = vi_app("modal-p-char", "abc\n", true);
    type_str(&mut app, "\"ayl"); // yank 'a' (exclusive char-wise)
    assert_eq!(buffer_text(&app), "abc\n", "y never touches the buffer");
    type_str(&mut app, "\"aP");
    assert_eq!(buffer_text(&app), "aabc\n", "P inserts right at the cursor");
    assert_eq!(cursor(&mut app), 0);
    type_str(&mut app, "\"ap");
    assert_eq!(
        buffer_text(&app),
        "aaabc\n",
        "p inserts right after the cursor"
    );
    assert_eq!(cursor(&mut app), 1);
}

#[test]
fn an_operator_composes_with_a_pending_find_motion() {
    let mut app = vi_app("modal-d-find", "a.b.c\n", true);
    type_str(&mut app, "\"adf.");
    assert_eq!(buffer_text(&app), "b.c\n", "d f . deletes through the '.'");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn a_count_before_the_operator_and_before_the_motion_multiply() {
    let mut app = vi_app("modal-count-multiply", "a b c d e f g h\n", true);
    type_str(&mut app, "\"a2d3w");
    assert_eq!(
        buffer_text(&app),
        "g h\n",
        "2d3w deletes 6 words, matching real Vim's count1*count2 rule"
    );
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn an_unrecognized_key_cancels_the_pending_operator() {
    let mut app = vi_app("modal-operator-cancel", "hello\n", true);
    type_str(&mut app, "\"adz"); // 'z' isn't a motion
    assert_eq!(buffer_text(&app), "hello\n", "nothing was deleted");
    app.on_key(key('l'));
    assert_eq!(
        cursor(&mut app),
        1,
        "'l' moves the cursor -- d wasn't stuck pending"
    );
}

#[test]
fn y_and_p_with_no_register_prefix_use_the_unnamed_clipboard_register() {
    let mut app = vi_app("modal-unnamed-register", "hi\n", true);
    type_str(&mut app, "yy");
    type_str(&mut app, "p");
    assert_eq!(buffer_text(&app), "hi\nhi\n");
    assert_eq!(cursor(&mut app), 3);
}

#[test]
fn operators_still_fall_through_to_the_old_table_for_unbound_starts() {
    // Nothing here claims 'g'/'d' etc. as bare keys outside T113/T114's
    // vocabulary -- anything truly unrecognized still reaches
    // `vim_normal_key` untouched, matching every earlier slice's own
    // fallthrough test.
    let mut app = vi_app("modal-operators-fallthrough", "hello\n", true);
    app.on_key(key('u')); // undo -- still the old table, not a modal-engine key
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text, "hello\n", "undo on an unmodified buffer is a no-op");
}

// ----- T115: text objects + dot-repeat -----------------------------------

#[test]
fn diw_deletes_the_inner_word() {
    let mut app = vi_app("modal-diw", "foo bar\n", true);
    type_str(&mut app, "\"adiw");
    assert_eq!(buffer_text(&app), " bar\n");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn daw_deletes_the_word_plus_its_trailing_space() {
    let mut app = vi_app("modal-daw", "foo bar\n", true);
    type_str(&mut app, "\"adaw");
    assert_eq!(buffer_text(&app), "bar\n");
}

#[test]
fn di_paren_deletes_inside_the_pair_delimiters_excluded() {
    let mut app = vi_app("modal-di-paren", "(bar)\n", true);
    type_str(&mut app, "\"adi(");
    assert_eq!(buffer_text(&app), "()\n");
    assert_eq!(cursor(&mut app), 1);
}

#[test]
fn da_quote_deletes_the_quoted_text_and_the_quotes() {
    let mut app = vi_app("modal-da-quote", "say \"hi\" now\n", true);
    type_str(&mut app, "\"bda\"");
    assert_eq!(buffer_text(&app), "say  now\n");
}

#[test]
fn c_i_paren_deletes_inside_and_enters_insert_mode() {
    let mut app = vi_app("modal-ci-paren", "(bar)\n", true);
    type_str(&mut app, "\"aci(");
    assert_eq!(buffer_text(&app), "()\n");
    assert!(
        matches!(app.mode_indicator().as_deref(), Some("-- INSERT --")),
        "c enters Insert mode, same as with an ordinary motion"
    );
    type_str(&mut app, "X");
    assert_eq!(buffer_text(&app), "(X)\n");
}

#[test]
fn an_unmatched_text_object_cancels_the_operator_cleanly() {
    let mut app = vi_app("modal-textobj-miss", "abc\n", true);
    type_str(&mut app, "\"adi("); // no parens anywhere in the buffer
    assert_eq!(buffer_text(&app), "abc\n", "nothing was deleted");
    app.on_key(key('l'));
    assert_eq!(
        cursor(&mut app),
        1,
        "'l' moves the cursor -- not stuck pending"
    );
}

#[test]
fn dot_repeats_the_last_delete_operator_motion() {
    let mut app = vi_app("modal-dot-dw", "one two three\n", true);
    type_str(&mut app, "dw");
    assert_eq!(buffer_text(&app), "two three\n");
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "three\n",
        ". repeats dw from the current cursor"
    );
}

#[test]
fn dot_repeats_the_last_text_object_delete() {
    let mut app = vi_app("modal-dot-textobj", "(a)(b)(c)\n", true);
    type_str(&mut app, "di(");
    assert_eq!(buffer_text(&app), "()(b)(c)\n");
    app.on_key(key('l'));
    app.on_key(key('l'));
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "()()(c)\n",
        ". repeats di( at the new cursor position"
    );
}

#[test]
fn dot_repeats_a_named_register_paste() {
    let mut app = vi_app("modal-dot-paste", "a\nb\nc\n", true);
    type_str(&mut app, "\"ayy");
    app.on_key(key('j'));
    app.on_key(key('j'));
    type_str(&mut app, "\"ap");
    assert_eq!(buffer_text(&app), "a\nb\nc\na\n");
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "a\nb\nc\na\na\n",
        ". repeats the \"ap paste (register 'a' still holds 'a\\n')"
    );
}

#[test]
fn a_count_before_dot_overrides_the_recorded_leading_count() {
    let mut app = vi_app("modal-dot-count-override", "a b c d e f\n", true);
    type_str(&mut app, "\"a2dw");
    assert_eq!(buffer_text(&app), "c d e f\n", "2dw deleted 2 words");
    type_str(&mut app, "3.");
    assert_eq!(
        buffer_text(&app),
        "f\n",
        "3. replaces the recorded count of 2 with 3, not 2*3"
    );
}

#[test]
fn yank_never_updates_what_dot_repeats() {
    // Matches real Vim: y never modifies the buffer, so it was never
    // dot-repeatable there either.
    let mut app = vi_app("modal-dot-yank-noop", "one two\n", true);
    type_str(&mut app, "dw"); // establish a real recorded change first
    assert_eq!(buffer_text(&app), "two\n");
    type_str(&mut app, "yy"); // yank -- must NOT become the new dot target
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "",
        ". still repeats the earlier dw (deleting the rest of the buffer), not a no-op from yy"
    );
}
