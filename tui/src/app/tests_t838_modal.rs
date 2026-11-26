//! T838: the Vi keymap's operator ranges, word classes, graphemes and
//! preferred column, checked as exact text, register and cursor.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::cov4::{app_with, content, key, temp};
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

/// Run `"a{seq}` from `at` on `text` and check the buffer, register `a`
/// and the cursor (for `c`, right after the change, still in Insert mode).
#[track_caller]
fn check(text: &str, at: usize, seq: &str, want: &str, reg: Option<&str>, cur: usize) {
    let mut app = vi("t838", text);
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
    keys(&mut app, &format!("\"a{seq}"));
    let what = format!("{seq} at {at} on {text:?}");
    assert_eq!(content(&mut app), want, "{what}: text");
    assert_eq!(
        app.modal_registers.get('a').map(|v| v.text.as_str()),
        reg,
        "{what}: register"
    );
    assert_eq!(cursor(&mut app), cur, "{what}: cursor");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
}

#[test]
fn single_character_lines() {
    let s = "a\nb\nc\n";
    check(s, 0, "x", "\nb\nc\n", Some("a"), 0);
    check(s, 0, "3x", "\nb\nc\n", Some("a"), 0);
    check(s, 0, "dw", "\nb\nc\n", Some("a"), 0);
    check(s, 0, "cw", "\nb\nc\n", Some("a"), 0);
    check(s, 0, "de", "\nc\n", Some("a\nb"), 0);
}

#[test]
fn an_identifier_and_its_punctuation_are_separate_words() {
    let s = "pkg::run(arg);\n";
    check(s, 0, "x", "kg::run(arg);\n", Some("p"), 0);
    check(s, 0, "3x", "::run(arg);\n", Some("pkg"), 0);
    check(s, 0, "dw", "::run(arg);\n", Some("pkg"), 0);
    check(s, 0, "cw", "::run(arg);\n", Some("pkg"), 0);
    check(s, 0, "de", "::run(arg);\n", Some("pkg"), 0);
    check(s, 3, "dw", "pkgrun(arg);\n", Some("::"), 3);
    check(s, 3, "cw", "pkgrun(arg);\n", Some("::"), 3);
    // The line's last word: up to the line's end, the cursor on the new
    // last character.
    check(s, 12, "dw", "pkg::run(arg\n", Some(");"), 11);
    let mut app = vi("t838-w", s);
    let mut seen = Vec::new();
    for _ in 0..6 {
        keys(&mut app, "w");
        seen.push(cursor(&mut app));
    }
    assert_eq!(seen, [3, 5, 8, 9, 12, 13]);
}

#[test]
fn x_reaches_the_last_character_of_the_line() {
    let s = "pkg::run(arg);\n";
    check(s, 0, "$x", "pkg::run(arg)\n", Some(";"), 12);
    check(s, 10, "9x", "pkg::run(a\n", Some("rg);"), 9);
    // On an empty line there is nothing to take: no change, no register.
    check("a\n\nb\n", 2, "x", "a\n\nb\n", None, 2);
}

#[test]
fn a_dot_between_names() {
    let s = "obj.field\n";
    check(s, 0, "dw", ".field\n", Some("obj"), 0);
    check(s, 3, "dw", "objfield\n", Some("."), 3);
    check(s, 0, "cw", ".field\n", Some("obj"), 0);
    check(s, 2, "de", "obfield\n", Some("j."), 2);
}

#[test]
fn adjacent_blanks() {
    let s = "a  b\n";
    check(s, 1, "x", "a b\n", Some(" "), 1);
    check(s, 1, "3x", "a\n", Some("  b"), 0);
    check(s, 1, "dw", "ab\n", Some("  "), 1);
    check(s, 1, "cw", "ab\n", Some("  "), 1);
    check(s, 1, "de", "a\n", Some("  b"), 0);
}

#[test]
fn a_decomposed_accent_stays_with_its_letter() {
    let s = "cafe\u{301} bar\n";
    check(s, 3, "x", "caf bar\n", Some("e\u{301}"), 3);
    check(s, 2, "3x", "cabar\n", Some("fe\u{301} "), 2);
    check(s, 0, "dw", "bar\n", Some("cafe\u{301} "), 0);
    check(s, 0, "cw", " bar\n", Some("cafe\u{301}"), 0);
    check(s, 0, "de", " bar\n", Some("cafe\u{301}"), 0);
    check("cafe\u{301}\n", 0, "$x", "caf\n", Some("e\u{301}"), 2);
    let mut app = vi("t838-l", s);
    keys(&mut app, "3l");
    assert_eq!(cursor(&mut app), 3);
    keys(&mut app, "l");
    assert_eq!(cursor(&mut app), 5, "l steps over the accent");
    keys(&mut app, "h");
    assert_eq!(cursor(&mut app), 3);
}

/// Where `seq` leaves the cursor, from the start of `text`.
fn after(text: &str, seq: &str) -> usize {
    let mut app = vi("t838-v", text);
    keys(&mut app, seq);
    cursor(&mut app)
}

#[test]
fn vertical_motions_carry_the_preferred_column() {
    let s = "abcdefgh\nab\nabcdefgh\n";
    assert_eq!(after(s, "4lj"), 10, "the short line's last character");
    assert_eq!(after(s, "4ljj"), 16, "back to column 4");
    assert_eq!(after(s, "4ljjkk"), 4);
    // A horizontal motion sets a new column.
    assert_eq!(after(s, "4ljhj"), 12);
    // After `$`, each line's end.
    assert_eq!(after("abc\nabcdef\n", "$j"), 9);
    // A tab (drawn as one cell) and a wide character (two cells).
    assert_eq!(after("\tabcde\nx\n0123456789\n", "4ljj"), 13);
    let wide = "0123456789\nab\u{6f22}\u{5b57}cd\n0123456789\n";
    assert_eq!(
        after(wide, "5lj"),
        14,
        "column 5 is in the second wide character"
    );
    assert_eq!(after(wide, "5ljj"), 23);
    assert_eq!(after(wide, "3lj"), 13, "column 3 is in the first one");
}
