//! Fuzz the JSON/YAML edit surface's parser and writer (`vix-edit-value`).
//!
//! **Edit → Mode → JSON…/YAML…** parses the buffer into a foldable tree and
//! writes it back on save. The buffer is any file the user opened, so parsing
//! must be total, every row the tree shows must be readable, and what the tree
//! writes must read back as the same tree: saving twice must not keep changing
//! the file.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_edit_value::{Format, Tree};

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let format = if selector & 1 == 0 {
        Format::Json
    } else {
        Format::Yaml
    };
    let Ok(tree) = Tree::from_text(text, format) else {
        return;
    };

    // Every visible row is readable, and the root is the first one.
    assert!(tree.row_count() >= 1, "a parsed tree shows no rows");
    assert!(tree.sel() < tree.row_count(), "selection past the last row");
    assert_eq!(tree.depth(0), 0, "the first row is not the root");
    for i in 0..tree.row_count() {
        let _ = (
            tree.label(i),
            tree.value(i),
            tree.is_container(i),
            tree.is_collapsed(i),
        );
    }
    assert!(!tree.is_dirty(), "a freshly parsed tree is dirty");

    // Writing is a fixed point: what the tree saves reads back to a tree
    // that saves the same text. Keys keep their original form (T850), so
    // keys that render alike (`1` and `"1"`, tagged or complex keys) are
    // each written back as read; a save that would make two collide, or that
    // the YAML writer cannot express, is refused and writes nothing.
    let Ok(saved) = tree.to_text() else {
        return; // the caller refuses to save; nothing is written
    };
    let reread = Tree::from_text(&saved, format)
        .unwrap_or_else(|e| panic!("the tree's own output does not parse: {e}\n{saved}"));
    let again = reread.to_text().expect("the reread tree writes");
    assert_eq!(
        reread.row_count(),
        tree.row_count(),
        "saving changed the row count"
    );
    assert_eq!(again, saved, "saving twice changed the text");
});
