#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

// Each call needs its own `unique_dir` tag -- these tests run in parallel by
// default, and `unique_dir("x")` is `vix-x-<pid>`, identical for every call
// with the same tag in one process, so two tests sharing one would race on
// the same file path (caught exactly this way: cross-test garbage in a
// buffer's content).
fn vi_app(tag: &str, text: &str, modal_engine: bool) -> App {
    let mut app = app_with({
        let mut s = Settings::default();
        s.keymap = "vi".to_string();
        s.subsystems.modal_engine = modal_engine;
        s
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
    // 'x' in Visual mode is Vim's Visual 'd' (T840): with nothing extended
    // yet, the selection is the one character under the cursor.
    let mut app = vi_app("modal-x-fallthrough", "hello\n", true);
    app.on_key(key('v'));
    app.on_key(key('x'));
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(
        text, "ello\n",
        "Visual x deletes the character under the cursor"
    );
}

// ----- T113: motions + counts, wired into Normal mode -----------------

#[test]
fn a_count_multiplies_h_and_l() {
    // The old table has no count support at all -- `docs/vix-for-vim-users`'s
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
    // Vim's `cw` special case (T837): the space after the word stays.
    assert_eq!(buffer_text(&app), " bar\n");
    assert!(
        matches!(app.mode_indicator().as_deref(), Some("-- INSERT --")),
        "c enters Insert mode"
    );
    type_str(&mut app, "XYZ");
    assert_eq!(
        buffer_text(&app),
        "XYZ bar\n",
        "typed text lands at the cut"
    );
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

fn ex(app: &mut App, cmd: &str) {
    app.on_key(key(':'));
    for c in cmd.chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
}

#[test]
fn ex_force_quit_closes_only_the_current_dirty_tab() {
    let mut app = vi_app("ex-qbang-one", "first\n", true);
    app.editor.new_tab_with_content("second\n");
    app.editor.active_tab_mut().unwrap().dirty = true;
    let before = app.editor.tabs.len();
    ex(&mut app, "q!");
    assert!(
        !app.flags.contains(AppFlags::SHOULD_QUIT),
        ":q! must not end the session"
    );
    assert_eq!(app.editor.tabs.len(), before - 1, "the current tab closed");
    assert!(
        app.unsaved.is_none(),
        ":q! abandons edits without prompting"
    );
}

#[test]
fn ex_qa_bang_quits_the_whole_session() {
    let mut app = vi_app("ex-qabang", "x\n", true);
    app.editor.active_tab_mut().unwrap().dirty = true;
    ex(&mut app, "qa!");
    assert!(app.flags.contains(AppFlags::SHOULD_QUIT));
}

#[test]
fn ex_qa_prompts_for_a_dirty_tab_instead_of_quitting() {
    let mut app = vi_app("ex-qa", "x\n", true);
    app.editor.active_tab_mut().unwrap().dirty = true;
    ex(&mut app, "qa");
    assert!(!app.flags.contains(AppFlags::SHOULD_QUIT));
    assert!(app.unsaved.is_some());
}

// ----- T840: Visual operators and counted put ----------------------------

fn type_keys(app: &mut App, seq: &str) {
    for c in seq.chars() {
        app.on_key(key(c));
    }
}

#[test]
fn visual_d_deletes_the_selection_including_the_cursor_character() {
    // Vim's Visual selection is inclusive: `vld` deletes two characters.
    let mut app = vi_app("modal-t840-vld", "hello world\n", true);
    type_keys(&mut app, "vld");
    assert_eq!(buffer_text(&app), "llo world\n");
    assert_eq!(cursor(&mut app), 0);
    assert!(selection(&mut app).is_none(), "back in Normal mode");
    // Normal mode again: `l` moves, it does not extend.
    type_keys(&mut app, "l");
    assert!(selection(&mut app).is_none());
}

#[test]
fn visual_line_d_deletes_whole_lines_into_a_linewise_register() {
    let mut app = vi_app("modal-t840-vjd", "one\ntwo\nthree\n", true);
    type_keys(&mut app, "Vjd");
    assert_eq!(buffer_text(&app), "three\n");
    // The register is line-wise: `p` puts it below the current line.
    type_keys(&mut app, "p");
    assert_eq!(buffer_text(&app), "three\none\ntwo\n");
}

#[test]
fn a_one_character_visual_selection_deletes_that_character() {
    let mut app = vi_app("modal-t840-vd", "abc\n", true);
    type_keys(&mut app, "lvd");
    assert_eq!(buffer_text(&app), "ac\n");
}

#[test]
fn a_backward_selection_across_a_newline_includes_both_ends() {
    // Cursor on `t` of `two`, `v`, then `k` up to `o` of `one`: Vim
    // deletes `one\nt`, both the anchor and the cursor character.
    let mut app = vi_app("modal-t840-vkd", "one\ntwo\n", true);
    type_keys(&mut app, "jvkd");
    assert_eq!(buffer_text(&app), "wo\n");
    assert_eq!(cursor(&mut app), 0);
}

#[test]
fn a_reversed_selection_on_one_line_deletes_from_cursor_to_anchor() {
    let mut app = vi_app("modal-t840-vhd", "abcdef\n", true);
    type_keys(&mut app, "lllvhhd");
    assert_eq!(buffer_text(&app), "aef\n");
    assert_eq!(cursor(&mut app), 1);
}

#[test]
fn visual_x_is_visual_d() {
    let mut app = vi_app("modal-t840-vx", "abcdef\n", true);
    type_keys(&mut app, "vlx");
    assert_eq!(buffer_text(&app), "cdef\n");
}

#[test]
fn visual_y_copies_character_wise_and_leaves_the_buffer() {
    let mut app = vi_app("modal-t840-vy", "abc def\n", true);
    // `"a` chosen inside Visual mode, as Vim allows.
    type_keys(&mut app, "wv\"ay");
    assert_eq!(buffer_text(&app), "abc def\n");
    assert!(selection(&mut app).is_none());
    type_keys(&mut app, "0\"aP");
    assert_eq!(buffer_text(&app), "dabc def\n");
}

#[test]
fn visual_y_of_two_characters_pastes_inline() {
    let mut app = vi_app("modal-t840-vy-two", "abc def\n", true);
    type_keys(&mut app, "vly$p");
    assert_eq!(buffer_text(&app), "abc defab\n");
}

#[test]
fn visual_c_deletes_and_enters_insert_mode() {
    let mut app = vi_app("modal-t840-vc", "abc def\n", true);
    type_keys(&mut app, "vllcXY");
    assert_eq!(buffer_text(&app), "XY def\n");
}

#[test]
fn visual_line_c_keeps_an_empty_line_to_type_into() {
    let mut app = vi_app("modal-t840-line-c", "one\ntwo\nthree\n", true);
    type_keys(&mut app, "VjcX");
    assert_eq!(buffer_text(&app), "X\nthree\n");
}

#[test]
fn a_visual_text_object_is_operated_on_exactly() {
    let mut app = vi_app("modal-t840-viwd", "abc def ghi\n", true);
    type_keys(&mut app, "wviwd");
    assert_eq!(buffer_text(&app), "abc  ghi\n");
}

#[test]
fn a_count_before_p_puts_that_many_copies() {
    let mut app = vi_app("modal-t840-3p", "one\ntwo\n", true);
    type_keys(&mut app, "\"ayy\"a3p");
    assert_eq!(buffer_text(&app), "one\none\none\none\ntwo\n");
    let mut app = vi_app("modal-t840-3p-char", "ab\n", true);
    type_keys(&mut app, "yl2p");
    assert_eq!(buffer_text(&app), "aaab\n");
    assert_eq!(cursor(&mut app), 2, "on the last pasted character");
}

#[test]
fn a_put_with_an_absurd_count_is_refused_not_attempted() {
    // The count saturates; the put must warn and leave the buffer alone, not
    // try to allocate the copies.
    let mut app = vi_app("modal-t840-hugep", "ab\n", true);
    let before = app.messages.items.len();
    type_keys(&mut app, "yl99999999999999999999999p");
    assert_eq!(buffer_text(&app), "ab\n");
    assert!(app.messages.items.len() > before, "a warning was added");
}

#[test]
fn cut_two_lines_switch_files_paste_three_copies_undo_once() {
    let mut app = vi_app("modal-t840-accept", "keep\ncut one\ncut two\ntail\n", true);
    type_keys(&mut app, "j\"aVjd");
    assert_eq!(buffer_text(&app), "keep\ntail\n");
    let other = unique_dir("modal-t840-accept-b").join("b.txt");
    fs::write(&other, "first\nlast\n").unwrap();
    app.open_initial(&other);
    assert_eq!(buffer_text(&app), "first\nlast\n", "now on the other file");
    type_keys(&mut app, "\"a3p");
    assert_eq!(
        buffer_text(&app),
        "first\ncut one\ncut two\ncut one\ncut two\ncut one\ncut two\nlast\n"
    );
    type_keys(&mut app, "u");
    assert_eq!(
        buffer_text(&app),
        "first\nlast\n",
        "one undo removes all three copies"
    );
}

// ----- T837: a change is one unit of repeat and undo ----------------------

fn modal_app(tag: &str, text: &str, keymap: &str) -> App {
    let mut app = app_with({
        let mut s = Settings::default();
        s.keymap = keymap.to_string();
        s.subsystems.modal_engine = true;
        s
    });
    let dir = unique_dir(tag);
    let file = dir.join("a.txt");
    fs::write(&file, text).unwrap();
    app.open_initial(&file);
    app
}

/// `ciw` on a short identifier with typed Unicode and a corrected typo,
/// `.` on a longer one, then one `u` per change.
fn ciw_then_dot_on_a_longer_word(keymap: &str, tag: &str) {
    let mut app = modal_app(tag, "ab longer_name\n", keymap);
    type_keys(&mut app, "ciwnëx");
    app.on_key(keycode(KeyCode::Backspace));
    type_keys(&mut app, "w→");
    app.on_key(esc());
    assert_eq!(
        buffer_text(&app),
        "nëw→ longer_name\n",
        "{keymap}: the first change"
    );
    type_keys(&mut app, "w.");
    assert_eq!(
        buffer_text(&app),
        "nëw→ nëw→\n",
        "{keymap}: . re-ran ciw on the longer word with the same typed text"
    );
    app.on_key(key('u'));
    assert_eq!(
        buffer_text(&app),
        "nëw→ longer_name\n",
        "{keymap}: one u undoes the repeat"
    );
    app.on_key(key('u'));
    assert_eq!(
        buffer_text(&app),
        "ab longer_name\n",
        "{keymap}: one u undoes the first change"
    );
}

#[test]
fn ciw_is_repeated_and_undone_as_one_change_under_vi() {
    ciw_then_dot_on_a_longer_word("vi", "modal-t837-ciw-vi");
}

#[test]
fn ciw_is_repeated_and_undone_as_one_change_under_spacemacs() {
    ciw_then_dot_on_a_longer_word("spacemacs", "modal-t837-ciw-spacemacs");
}

#[test]
fn a_change_after_a_delete_replaces_it_as_the_dot_target_and_undoes_separately() {
    for keymap in ["vi", "spacemacs"] {
        let mut app = modal_app(
            &format!("modal-t837-after-delete-{keymap}"),
            "one two three four\n",
            keymap,
        );
        type_keys(&mut app, "dw");
        assert_eq!(buffer_text(&app), "two three four\n");
        type_keys(&mut app, "ciwX");
        app.on_key(esc());
        assert_eq!(buffer_text(&app), "X three four\n");
        // A cancelled operator in between changes nothing and is not a change.
        app.on_key(key('d'));
        app.on_key(esc());
        app.on_key(key('c'));
        app.on_key(esc());
        type_keys(&mut app, "w.");
        assert_eq!(
            buffer_text(&app),
            "X X four\n",
            "{keymap}: . repeats ciw, not dw"
        );
        app.on_key(key('u'));
        assert_eq!(buffer_text(&app), "X three four\n", "{keymap}");
        app.on_key(key('u'));
        assert_eq!(buffer_text(&app), "two three four\n", "{keymap}");
        app.on_key(key('u'));
        assert_eq!(buffer_text(&app), "one two three four\n", "{keymap}");
    }
}

#[test]
fn cw_keeps_the_white_space_after_the_word() {
    let mut app = modal_app("modal-t837-cw-sep", "old value\n", "vi");
    type_keys(&mut app, "cwnew");
    app.on_key(esc());
    assert_eq!(
        buffer_text(&app),
        "new value\n",
        "Vim's cw changes only the word"
    );
}

#[test]
fn a_count_before_dot_replaces_the_count_not_a_digit_typed_in_insert() {
    let mut app = modal_app("modal-t837-count-dot", "a b c d\n", "vi");
    type_keys(&mut app, "cwx1");
    app.on_key(esc());
    assert_eq!(buffer_text(&app), "x1 b c d\n");
    type_keys(&mut app, "w2.");
    assert_eq!(
        buffer_text(&app),
        "x1 x1 d\n",
        "2. is c2w with the same typed text"
    );
}

#[test]
fn an_open_line_session_is_one_change() {
    let mut app = modal_app("modal-t837-open-line", "top\n", "vi");
    type_keys(&mut app, "ofoo");
    app.on_key(esc());
    assert_eq!(buffer_text(&app), "top\nfoo\n");
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "top\nfoo\nfoo\n",
        ". opens and types again"
    );
    app.on_key(key('u'));
    app.on_key(key('u'));
    assert_eq!(buffer_text(&app), "top\n", "each session undid as a whole");
}

#[test]
fn an_insert_session_that_cannot_be_recorded_invalidates_dot() {
    let mut app = modal_app("modal-t837-unrecordable", "one two three\n", "vi");
    type_keys(&mut app, "dw");
    assert_eq!(buffer_text(&app), "two three\n");
    // A cursor key inside Insert mode is not something . can replay.
    type_keys(&mut app, "iZ");
    app.on_key(keycode(KeyCode::Right));
    type_keys(&mut app, "Y");
    app.on_key(esc());
    assert_eq!(buffer_text(&app), "ZtYwo three\n");
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "ZtYwo three\n",
        ". no longer replays the older dw"
    );
}

#[test]
fn a_visual_delete_invalidates_dot() {
    let mut app = modal_app("modal-t837-visual-invalidates", "one two three\n", "vi");
    type_keys(&mut app, "dw");
    type_keys(&mut app, "vd");
    assert_eq!(buffer_text(&app), "wo three\n");
    app.on_key(key('.'));
    assert_eq!(
        buffer_text(&app),
        "wo three\n",
        ". does not replay the older dw"
    );
}
