//! Property tests for buffer operations (T831): `Code` against a naive
//! `String` model. Inserts and removes land where the model says, the
//! character/byte/line index conversions agree with counting by hand, an
//! insert followed by removing it (and the reverse) is a no-op, the edit log
//! replayed on a copy rebuilds the text, `Code::changed_since` never misses
//! a changed character, and `Editor::edit_selections` gives the same text
//! for non-overlapping edits in any order.
//!
//! The generated text uses `\n` as its only line break: ropey also breaks
//! lines at `\r`, VT, FF, NEL, LS and PS, which the naive model does not.

use proptest::prelude::*;
use vix_editor_core::code::{Code, Splice};
use vix_editor_core::editor::Editor;
use vix_editor_core::multicursor::SelectionEdit;

/// Text with one-, two- and four-byte characters and `\n` line breaks.
const TEXT: &str = "[ab é😀\t\n]{0,24}";

/// One primitive edit: insert `text` at `at`, or remove from `at` for
/// `span` characters (both reduced modulo the buffer length when applied).
#[derive(Debug, Clone)]
enum Op {
    Insert { at: usize, text: String },
    Remove { at: usize, span: usize },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), "[ab é😀\n]{0,4}").prop_map(|(at, text)| Op::Insert { at, text }),
        (any::<usize>(), 0usize..6).prop_map(|(at, span)| Op::Remove { at, span }),
    ]
}

/// A fresh buffer over `initial` with no grammar.
fn buffer(initial: &str) -> Result<Code, TestCaseError> {
    Code::new(initial, "", None).map_err(|e| TestCaseError::fail(e.to_string()))
}

/// The byte offset of character `c` in `s`.
fn byte_of(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

/// `s` with characters `from..to` removed.
fn model_remove(s: &str, from: usize, to: usize) -> String {
    let (a, b) = (byte_of(s, from), byte_of(s, to));
    format!("{}{}", &s[..a], &s[b..])
}

/// `s` with `text` inserted at character `at`.
fn model_insert(s: &str, at: usize, text: &str) -> String {
    let a = byte_of(s, at);
    format!("{}{text}{}", &s[..a], &s[a..])
}

/// Apply `op` to both the buffer and the model.
fn apply(code: &mut Code, model: &mut String, op: &Op) {
    let len = code.len_chars();
    match op {
        Op::Insert { at, text } => {
            let at = at % (len + 1);
            code.insert(at, text);
            *model = model_insert(model, at, text);
        }
        Op::Remove { at, span } => {
            let from = at % (len + 1);
            let to = from + span % (len - from + 1);
            code.remove(from, to);
            *model = model_remove(model, from, to);
        }
    }
}

proptest! {
    /// Every insert and remove leaves the buffer equal to the model, with the
    /// same length in characters, bytes and lines.
    #[test]
    fn edits_match_a_string_model(initial in TEXT, ops in prop::collection::vec(op(), 0..12)) {
        let mut code = buffer(&initial)?;
        let mut model = initial;
        for op in &ops {
            apply(&mut code, &mut model, op);
            prop_assert_eq!(code.get_content(), model.clone());
            prop_assert_eq!(code.len_chars(), model.chars().count());
            prop_assert_eq!(code.len_bytes(), model.len());
            prop_assert_eq!(code.len_lines(), model.matches('\n').count() + 1);
        }
    }

    /// Character, byte, line and `(row, col)` conversions agree with
    /// counting by hand, and each pair of conversions round-trips.
    #[test]
    fn index_conversions_agree_with_counting(text in TEXT) {
        let code = buffer(&text)?;
        let chars: Vec<char> = text.chars().collect();
        let mut byte = 0;
        let (mut row, mut col) = (0, 0);
        for c in 0..=chars.len() {
            prop_assert_eq!(code.char_to_byte(c), byte);
            prop_assert_eq!(code.byte_to_char(byte), c);
            prop_assert_eq!(code.char_to_line(c), row);
            prop_assert_eq!(code.byte_to_line(byte), row);
            prop_assert_eq!(code.point(c), (row, col));
            prop_assert_eq!(code.offset(row, col), c);
            if let Some(&ch) = chars.get(c) {
                byte += ch.len_utf8();
                if ch == '\n' {
                    row += 1;
                    col = 0;
                } else {
                    col += 1;
                }
            }
        }
        for (i, line) in text.split('\n').enumerate() {
            prop_assert_eq!(code.line_len(i), line.chars().count());
            let start: usize = text.split('\n').take(i).map(|l| l.chars().count() + 1).sum();
            prop_assert_eq!(code.line_to_char(i), start);
        }
    }

    /// Slices by character and by byte range return the model's text.
    #[test]
    fn slices_match_the_model(text in TEXT, a in any::<usize>(), b in any::<usize>()) {
        let code = buffer(&text)?;
        let len = code.len_chars();
        let (a, b) = (a % (len + 1), b % (len + 1));
        let (from, to) = (a.min(b), a.max(b));
        let want: String = text.chars().skip(from).take(to - from).collect();
        prop_assert!(code.slice(from, to) == want, "{want:?}");
        prop_assert!(code.char_slice(from, to) == want, "{want:?}");
        let (bf, bt) = (byte_of(&text, from), byte_of(&text, to));
        prop_assert_eq!(code.byte_slice(bf, bt).to_string(), want);
    }

    /// Inserting text and then removing exactly it restores the buffer, and
    /// removing a range and inserting what it held back does too.
    #[test]
    fn insert_and_remove_round_trip(
        text in TEXT,
        at in any::<usize>(),
        span in 0usize..8,
        insert in "[ab é😀\n]{0,6}",
    ) {
        let mut code = buffer(&text)?;
        let len = code.len_chars();
        let at = at % (len + 1);
        code.insert(at, &insert);
        code.remove(at, at + insert.chars().count());
        prop_assert!(code.get_content() == text, "{text:?}");

        let to = at + span % (len - at + 1);
        let held = code.slice(at, to);
        code.remove(at, to);
        code.insert(at, &held);
        prop_assert_eq!(code.get_content(), text);
    }

    /// The edit log, replayed with `apply_remote` on a buffer over the
    /// original text, rebuilds the edited text.
    #[test]
    fn edit_log_replays_to_the_same_text(initial in TEXT, ops in prop::collection::vec(op(), 0..12)) {
        let mut code = buffer(&initial)?;
        code.start_edit_log();
        let mut model = initial.clone();
        for op in &ops {
            apply(&mut code, &mut model, op);
        }
        let log: Vec<Splice> = code.take_edit_log();
        let mut copy = buffer(&initial)?;
        copy.apply_remote(&log);
        prop_assert_eq!(copy.get_content(), model);
        prop_assert!(code.take_edit_log().is_empty());
    }

    /// Outside the range `changed_since` reports, the text is the old text:
    /// the same prefix before it and the same suffix after it.
    #[test]
    fn changed_since_covers_every_change(initial in TEXT, ops in prop::collection::vec(op(), 0..8)) {
        let mut code = buffer(&initial)?;
        let since = code.revision();
        let mut model = initial.clone();
        for op in &ops {
            apply(&mut code, &mut model, op);
        }
        let range = code.changed_since(since);
        prop_assert!(range.is_some());
        let range = range.unwrap_or(0..0);
        let now: Vec<char> = model.chars().collect();
        let old: Vec<char> = initial.chars().collect();
        prop_assert!(range.start <= range.end && range.end <= now.len(), "{range:?}");
        prop_assert_eq!(&now[..range.start], &old[..range.start.min(old.len())]);
        let tail = now.len() - range.end;
        prop_assert!(tail <= old.len());
        prop_assert_eq!(&now[range.end..], &old[old.len() - tail..]);
    }

    /// Non-overlapping edits handed to `edit_selections` in either order,
    /// or shuffled, give the text the model gets by applying them from the
    /// last to the first, and one undo restores the original.
    #[test]
    fn non_overlapping_multi_caret_edits_commute(
        text in "[ab é😀\n]{1,24}",
        cuts in prop::collection::vec((any::<usize>(), 0usize..4, "[xy😀]{0,3}"), 1..5),
        seed in any::<u64>(),
    ) {
        let len = text.chars().count();
        // Disjoint ranges in buffer order: each starts at or after the end of
        // the one before it, and no two share a start.
        let mut starts: Vec<usize> = cuts.iter().map(|(at, _, _)| at % (len + 1)).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut edits = Vec::new();
        for (i, &start) in starts.iter().enumerate() {
            let limit = starts.get(i + 1).copied().unwrap_or(len);
            let (_, span, insert) = &cuts[i.min(cuts.len() - 1)];
            let end = (start + span).min(limit);
            edits.push(SelectionEdit::new(start, end, insert));
        }
        let mut want = text.clone();
        for e in edits.iter().rev() {
            want = model_insert(&model_remove(&want, e.start, e.end), e.start, &e.text);
        }
        let mut reversed = edits.clone();
        reversed.reverse();
        let mut shuffled = edits.clone();
        let mut s = seed;
        for i in (1..shuffled.len()).rev() {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            shuffled.swap(i, usize::try_from(s >> 33).unwrap_or(0) % (i + 1));
        }
        for order in [&edits, &reversed, &shuffled] {
            let mut ed = Editor::new("text", &text, Vec::new())
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            ed.edit_selections(order);
            prop_assert_eq!(ed.get_content(), want.clone());
            // Carets that land on the same spot (an empty edit right after
            // a deletion) merge, so there can be fewer than edits.
            prop_assert!(ed.caret_count() <= edits.len());
            ed.undo();
            prop_assert_eq!(ed.get_content(), text.clone());
        }
    }
}
