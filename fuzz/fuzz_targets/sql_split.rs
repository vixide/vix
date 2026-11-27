//! Fuzz the SQL edit surface's statement splitter and formatter (`vix-edit-sql`).
//!
//! **Edit → Mode → SQL…** splits whatever SQL buffer the user opened on its
//! top-level `;`s, and saving joins the statements back (`Editor::to_text`).
//! A quote or comment that never closes, a `--` comment on the last line, or
//! multi-byte text must neither panic nor lose a statement on save: T831's
//! first slice found a real one, a trailing `--` comment swallowing the `;`
//! that `to_text` appended, merging two statements.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_edit_sql::{Editor, format_sql, split_statements};

fuzz_target!(|text: &str| {
    let statements = split_statements(text);
    for (i, s) in statements.iter().enumerate() {
        assert!(!s.is_empty(), "statement {i} is empty");
        assert_eq!(s.trim(), s, "statement {i} is not trimmed");
    }

    // Saving and reopening keeps exactly the same statements.
    let editor = Editor::from_text(text);
    assert_eq!(
        editor.len(),
        statements.len(),
        "Editor disagrees with split_statements"
    );
    let saved = editor.to_text();
    assert_eq!(
        split_statements(&saved),
        statements,
        "to_text then split changed the statements: {saved:?}"
    );
    // And saving is a fixed point.
    assert_eq!(
        Editor::from_text(&saved).to_text(),
        saved,
        "to_text is not a fixed point"
    );

    for i in 0..editor.len() {
        let _ = editor.kind(i);
        let _ = editor.preview(i);
        // Formatting only upper-cases ASCII keywords outside strings and
        // comments: the text is otherwise unchanged, and formatting twice is
        // the same as once.
        let s = editor.statement(i);
        let formatted = format_sql(s);
        assert!(
            formatted.eq_ignore_ascii_case(s),
            "format_sql changed more than keyword case: {s:?} -> {formatted:?}"
        );
        assert_eq!(
            format_sql(&formatted),
            formatted,
            "format_sql is not idempotent"
        );
    }
    // Out-of-range indexes are answered, not panicked on.
    let _ = editor.statement(editor.len());
});
