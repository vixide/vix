//! Tree-sitter textobjects in the App (T616): the Edit → Select and
//! Go → Function / Class actions, the offline shrink, and the Vi modal
//! engine's `if`/`af`/`ic`/`ac`/`ia`/`aa` and `]f`/`[f` bindings. Rust is
//! the grammar every default build carries; the per-language queries are
//! tested in `vix-editor-core`'s own `textobject` tests.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

const SRC: &str = "fn one() {\n    let f = |x| {\n        x + 1\n    };\n}\n\nfn two(a: i32, b: i32) -> i32 {\n    a + b\n}\n";

fn open_rust(tag: &str, settings: Settings) -> App {
    let mut app = app_with(settings);
    let dir = unique_dir(tag);
    let file = dir.join("lib.rs");
    fs::write(&file, SRC).unwrap();
    app.open_initial(&file);
    app
}

fn rust_app(tag: &str) -> App {
    open_rust(tag, Settings::default())
}

fn vi_app(tag: &str) -> App {
    let mut s = Settings::default();
    s.keymap = "vi".to_string();
    s.subsystems.modal_engine = true;
    open_rust(tag, s)
}

fn set_cursor(app: &mut App, pos: usize) {
    let t = app.editor.active_tab_mut().unwrap();
    t.editor.clear_selection();
    t.editor.set_cursor(pos);
}

fn cursor(app: &mut App) -> usize {
    app.editor.active_tab_mut().unwrap().editor.get_cursor()
}

fn selected(app: &mut App) -> Option<String> {
    let t = app.editor.active_tab_mut().unwrap();
    let s = t.editor.get_selection()?;
    let (a, b) = (s.start.min(s.end), s.start.max(s.end));
    Some(t.text().chars().skip(a).take(b - a).collect())
}

fn at(needle: &str) -> usize {
    SRC.find(needle).unwrap()
}

#[test]
fn select_function_around_then_repeat_grows_outward() {
    let mut app = rust_app("to-grow");
    set_cursor(&mut app, at("x + 1"));
    app.run_action("edit.select_function_around");
    assert_eq!(
        selected(&mut app).as_deref(),
        Some("|x| {\n        x + 1\n    }"),
        "the closure is the innermost function"
    );
    app.run_action("edit.select_function_around");
    assert!(
        selected(&mut app).is_some_and(|s| s.starts_with("fn one()")),
        "repeating grows to the enclosing fn"
    );
}

#[test]
fn select_function_inside_drops_the_braces() {
    let mut app = rust_app("to-inside");
    set_cursor(&mut app, at("a + b"));
    app.run_action("edit.select_function_inside");
    assert_eq!(selected(&mut app).as_deref(), Some("a + b"));
}

#[test]
fn select_parameter_inside_and_around() {
    let mut app = rust_app("to-param");
    set_cursor(&mut app, at("a: i32"));
    app.run_action("edit.select_parameter_inside");
    assert_eq!(selected(&mut app).as_deref(), Some("a: i32"));
    set_cursor(&mut app, at("a: i32"));
    app.run_action("edit.select_parameter_around");
    assert_eq!(selected(&mut app).as_deref(), Some("a: i32, "));
}

#[test]
fn next_and_previous_function_wrap_around_the_file() {
    let mut app = rust_app("to-nav");
    set_cursor(&mut app, 0);
    app.run_action("nav.next_function");
    assert_eq!(
        cursor(&mut app),
        at("|x|"),
        "the closure is the next function"
    );
    app.run_action("nav.next_function");
    assert_eq!(cursor(&mut app), at("fn two"));
    app.run_action("nav.next_function");
    assert_eq!(cursor(&mut app), 0, "wraps to the first function");
    app.run_action("nav.prev_function");
    assert_eq!(cursor(&mut app), at("fn two"), "wraps back to the last one");
}

#[test]
fn expand_then_shrink_round_trips_offline() {
    let mut app = rust_app("to-shrink");
    let start = at("x + 1");
    set_cursor(&mut app, start);
    app.run_action("lsp.expand_selection");
    let first = selected(&mut app).expect("first expansion");
    app.run_action("lsp.expand_selection");
    let second = selected(&mut app).expect("second expansion");
    assert!(second.len() > first.len());
    app.run_action("lsp.shrink_selection");
    assert_eq!(selected(&mut app), Some(first));
    app.run_action("lsp.shrink_selection");
    assert_eq!(selected(&mut app), None, "back to the bare caret");
    assert_eq!(cursor(&mut app), start);
    // Nothing left to shrink: nothing changes.
    app.run_action("lsp.shrink_selection");
    assert_eq!(selected(&mut app), None);
}

#[test]
fn textobject_selection_shrinks_back_too() {
    let mut app = rust_app("to-shrink-obj");
    set_cursor(&mut app, at("x + 1"));
    app.run_action("edit.select_function_around");
    let inner = selected(&mut app).expect("closure");
    app.run_action("edit.select_function_around");
    app.run_action("lsp.shrink_selection");
    assert_eq!(selected(&mut app), Some(inner));
}

#[test]
fn shrink_stack_is_dropped_after_an_edit() {
    let mut app = rust_app("to-shrink-edit");
    set_cursor(&mut app, at("a + b"));
    app.run_action("lsp.expand_selection");
    app.run_action("lsp.expand_selection");
    let range = |app: &mut App| {
        let s = app.editor.active_tab_mut().unwrap().editor.get_selection();
        s.map(|s| (s.start, s.end))
    };
    let before = range(&mut app);
    assert!(before.is_some());
    {
        let t = app.editor.active_tab_mut().unwrap();
        t.editor.code_mut().insert(0, "\n");
    }
    app.run_action("lsp.shrink_selection");
    assert_eq!(range(&mut app), before, "a stale stack is not trusted");
}

#[test]
fn plain_text_has_no_textobjects() {
    let mut app = app_with(Settings::default());
    app.editor.new_tab_with_content("fn one() {}\n");
    set_cursor(&mut app, 4);
    app.run_action("edit.select_function_around");
    assert_eq!(selected(&mut app), None);
    app.run_action("nav.next_function");
    assert_eq!(cursor(&mut app), 4);
}

#[test]
fn vi_daf_deletes_the_function() {
    let mut app = vi_app("to-vi-daf");
    set_cursor(&mut app, at("a + b"));
    for c in ['d', 'a', 'f'] {
        app.on_key(key(c));
    }
    let text = app.editor.active_tab().unwrap().text();
    assert!(!text.contains("fn two"), "{text:?}");
    assert!(text.contains("fn one"));
}

#[test]
fn vi_count_picks_an_outer_function() {
    let mut app = vi_app("to-vi-2yaf");
    set_cursor(&mut app, at("x + 1"));
    // `2daf`: the second enclosing function is `fn one`.
    for c in ['2', 'd', 'a', 'f'] {
        app.on_key(key(c));
    }
    let text = app.editor.active_tab().unwrap().text();
    assert!(!text.contains("fn one"), "{text:?}");
    assert!(text.contains("fn two"));
}

#[test]
fn vi_daa_deletes_a_parameter_and_its_comma() {
    let mut app = vi_app("to-vi-dia");
    set_cursor(&mut app, at("b: i32"));
    for c in ['d', 'a', 'a'] {
        app.on_key(key(c));
    }
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("fn two(a: i32) -> i32"), "{text:?}");
}

#[test]
fn vi_bracket_f_motions_take_counts() {
    let mut app = vi_app("to-vi-]f");
    set_cursor(&mut app, 0);
    app.on_key(key(']'));
    app.on_key(key('f'));
    assert_eq!(cursor(&mut app), at("|x|"));
    set_cursor(&mut app, 0);
    for c in ['2', ']', 'f'] {
        app.on_key(key(c));
    }
    assert_eq!(cursor(&mut app), at("fn two"));
    app.on_key(key('['));
    app.on_key(key('m'));
    assert_eq!(cursor(&mut app), at("|x|"), "[m is [f");
}

#[test]
fn vi_d_bracket_f_deletes_up_to_the_next_function() {
    let mut app = vi_app("to-vi-d]f");
    set_cursor(&mut app, at("\nfn two"));
    for c in ['d', ']', 'f'] {
        app.on_key(key(c));
    }
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("}\nfn two"), "{text:?}");
}

#[test]
fn vi_visual_af_selects_and_grows() {
    let mut app = vi_app("to-vi-vaf");
    set_cursor(&mut app, at("x + 1"));
    for c in ['v', 'a', 'f'] {
        app.on_key(key(c));
    }
    assert_eq!(
        selected(&mut app).as_deref(),
        Some("|x| {\n        x + 1\n    }")
    );
    app.on_key(key('a'));
    app.on_key(key('f'));
    assert!(selected(&mut app).is_some_and(|s| s.starts_with("fn one()")));
}

#[test]
fn vi_visual_iw_works_too() {
    let mut app = vi_app("to-vi-viw");
    set_cursor(&mut app, at("two"));
    for c in ['v', 'i', 'w'] {
        app.on_key(key(c));
    }
    assert_eq!(selected(&mut app).as_deref(), Some("two"));
}

#[test]
fn textobject_actions_are_in_menus_and_palette() {
    let ids = [
        "edit.select_function_inside",
        "edit.select_function_around",
        "edit.select_class_inside",
        "edit.select_class_around",
        "edit.select_parameter_inside",
        "edit.select_parameter_around",
        "nav.next_function",
        "nav.prev_function",
        "nav.next_class",
        "nav.prev_class",
    ];
    let palette: Vec<&str> = vix::palette::COMMANDS.iter().map(|(_, a)| *a).collect();
    for id in ids {
        assert!(palette.contains(&id), "{id} in the palette");
    }
}
