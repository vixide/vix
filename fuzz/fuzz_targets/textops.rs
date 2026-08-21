//! Fuzz the pure text transforms: whole-text rewrites and cursor-relative ones.
//!
//! The invariants are the ones the editor relies on: no panic on any input
//! (including lone surrogates' UTF-8 neighbours, CRLF soup, and cursors past the
//! end), and a returned cursor that is always a valid char offset into the
//! returned text — the host calls `set_cursor` with it, so an out-of-range
//! offset would panic later, far from the cause.

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Every cursor-relative rewrite, as `(name, fn)`.
type Rewrite = (&'static str, fn(&str, usize) -> Option<(String, usize)>);

const REWRITES: &[Rewrite] = &[
    ("transpose_chars", vix_textops::transpose_chars_at),
    ("transpose_words", vix_textops::transpose_words_at),
    ("transpose_lines", vix_textops::transpose_lines_at),
    ("transpose_sentences", vix_textops::transpose_sentences_at),
    ("transpose_paragraphs", vix_textops::transpose_paragraphs_at),
    ("transpose_sections", vix_textops::transpose_sections_at),
    ("delete_char", vix_textops::delete_char_at),
    ("delete_word", vix_textops::delete_word_at),
    ("delete_sentence", vix_textops::delete_sentence_at),
    ("delete_paragraph", vix_textops::delete_paragraph_at),
    ("delete_section", vix_textops::delete_section_at),
    ("smart_toggle", vix_textops::smart_toggle_at),
];

fuzz_target!(|data: &[u8]| {
    // First byte picks the cursor position, the rest is the text.
    let (head, text) = data.split_at(data.len().min(1));
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    let chars = text.chars().count();
    let cursor = head.first().map_or(0, |b| usize::from(*b)) % (chars + 1);

    for (name, f) in REWRITES {
        if let Some((out, pos)) = f(text, cursor) {
            assert!(
                pos <= out.chars().count(),
                "{name}: cursor {pos} past end of {} chars",
                out.chars().count()
            );
            assert!(out.is_char_boundary(0));
        }
    }

    // Cursor-relative rewrites that take a parameter.
    for delta in [-1_i64, 1] {
        if let Some((out, pos)) = vix_textops::bump_number_at(text, cursor, delta) {
            assert!(pos <= out.chars().count());
        }
    }
    for width in [0_usize, 1, 40] {
        if let Some((out, pos)) = vix_textops::wrap_paragraph_at(text, cursor, width) {
            assert!(pos <= out.chars().count());
        }
        let _ = vix_textops::wrap(text, width);
    }

    // Whole-text transforms: they must round-trip through UTF-8 without panic.
    let _ = vix_textops::to_lf(text);
    let _ = vix_textops::to_crlf(text);
    let _ = vix_textops::squeeze_blank_lines(text);
    let _ = vix_textops::rot13(text);
    let _ = vix_textops::sentence_starts(text);
    let _ = vix_textops::tag_column(text, "TODO");

    // Line endings: normalizing twice is the same as normalizing once.
    let lf = vix_textops::to_lf(text);
    assert_eq!(vix_textops::to_lf(&lf), lf, "to_lf is not idempotent");

    // ROT13 is its own inverse.
    assert_eq!(vix_textops::rot13(&vix_textops::rot13(text)), *text);
});
