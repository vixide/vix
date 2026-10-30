//! Document synchronization (T615): the server's negotiated sync kind, and the
//! incremental `didChange` content change that turns one document text into
//! the next.
//!
//! The change is computed by diffing the text the server last received
//! against the current text — the common prefix and suffix are kept, the
//! differing middle is replaced — rather than by replaying the editor's own
//! edit history. A diff is correct by construction for every way a buffer can
//! change (typing, multi-cursor edits, undo/redo, formatting, a Cowork peer's
//! remote edits, a reload from disk), because it only ever compares two whole
//! texts. Positions are expressed in the server's negotiated [`Encoding`] and
//! count every LSP line break (`\n`, `\r\n`, `\r`), and a change never starts
//! or ends between the `\r` and `\n` of a CRLF pair.

use serde_json::Value;

use crate::{Encoding, Position, Range};

/// How a server wants document changes delivered
/// (`capabilities.textDocumentSync`, as a bare number or a
/// `TextDocumentSyncOptions.change`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncKind {
    /// The whole text on every change (`1`). Also used for `0` (none) and an
    /// absent capability: sending the full text is always safe.
    #[default]
    Full,
    /// Only the changed range (`2`).
    Incremental,
}

impl SyncKind {
    /// Read the sync kind from an `initialize` result.
    #[must_use]
    pub fn from_initialize(result: &Value) -> Self {
        let Some(sync) = result
            .get("capabilities")
            .and_then(|c| c.get("textDocumentSync"))
        else {
            return SyncKind::Full;
        };
        let kind = sync
            .as_i64()
            .or_else(|| sync.get("change").and_then(Value::as_i64));
        if kind == Some(2) {
            SyncKind::Incremental
        } else {
            SyncKind::Full
        }
    }
}

/// One `TextDocumentContentChangeEvent` with a range: replace `range` of the
/// old text with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentChange {
    /// The replaced range of the old text, in the negotiated encoding.
    pub range: Range,
    /// The replacement text.
    pub text: String,
}

/// The single ranged change that turns `old` into `new`, or `None` when the
/// two texts are equal.
#[must_use]
pub fn diff(old: &str, new: &str, enc: Encoding) -> Option<ContentChange> {
    if old == new {
        return None;
    }
    let ob = old.as_bytes();
    let nb = new.as_bytes();
    let mut prefix = ob.iter().zip(nb).take_while(|(a, b)| a == b).count();
    // The bytes before `prefix` are identical, so a char boundary in `old` is
    // one in `new` too.
    while !old.is_char_boundary(prefix) {
        prefix -= 1;
    }
    // Never start between the `\r` and `\n` of a CRLF pair.
    if prefix > 0
        && ob[prefix - 1] == b'\r'
        && (ob.get(prefix) == Some(&b'\n') || nb.get(prefix) == Some(&b'\n'))
    {
        prefix -= 1;
    }
    let max_suffix = (ob.len() - prefix).min(nb.len() - prefix);
    let mut suffix = ob
        .iter()
        .rev()
        .zip(nb.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(ob.len() - suffix) {
        suffix -= 1;
    }
    // Never end between the `\r` and `\n` of a CRLF pair.
    if suffix > 0 {
        let old_end = ob.len() - suffix;
        let new_end = nb.len() - suffix;
        let cr_before = |bytes: &[u8], end: usize| end > 0 && bytes[end - 1] == b'\r';
        if ob[old_end] == b'\n' && (cr_before(ob, old_end) || cr_before(nb, new_end)) {
            suffix -= 1;
        }
    }
    let old_end = ob.len() - suffix;
    let new_end = nb.len() - suffix;
    Some(ContentChange {
        range: Range {
            start: position_at(old, prefix, enc),
            end: position_at(old, old_end, enc),
        },
        text: new[prefix..new_end].to_string(),
    })
}

/// The LSP position of byte offset `byte` in `text` (clamped to the text's
/// length and backed off to a char boundary).
#[must_use]
pub fn position_at(text: &str, byte: usize, enc: Encoding) -> Position {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let mut line = 0_u32;
    let mut character = 0_u32;
    let mut prev_cr = false;
    for ch in text[..byte].chars() {
        match ch {
            '\n' if prev_cr => prev_cr = false,
            '\n' => {
                line += 1;
                character = 0;
            }
            '\r' => {
                line += 1;
                character = 0;
                prev_cr = true;
            }
            _ => {
                character += enc.units(ch);
                prev_cr = false;
            }
        }
    }
    Position { line, character }
}

/// The byte offset of LSP position `pos` in `text`, clamped as the protocol
/// says: a line past the end is the end of the text, and a `character` past
/// the end of its line is the end of that line (before its line break).
#[must_use]
pub fn offset_at(text: &str, pos: Position, enc: Encoding) -> usize {
    // Find the start of line `pos.line`.
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut line = 0;
    while line < pos.line {
        let Some(i) = bytes[start..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
        else {
            return text.len();
        };
        let at = start + i;
        start = if bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
            at + 2
        } else {
            at + 1
        };
        line += 1;
    }
    let mut units = 0_u32;
    for (i, ch) in text[start..].char_indices() {
        if units >= pos.character || ch == '\n' || ch == '\r' {
            return start + i;
        }
        units += enc.units(ch);
    }
    text.len()
}

/// Apply `change` to `text`, exactly as a server does on receiving it — the
/// reference the client's own [`diff`] is tested against.
#[must_use]
pub fn apply(text: &str, change: &ContentChange, enc: Encoding) -> String {
    let start = offset_at(text, change.range.start, enc);
    let end = offset_at(text, change.range.end, enc).max(start);
    let mut out = String::with_capacity(text.len() + change.text.len());
    out.push_str(&text[..start]);
    out.push_str(&change.text);
    out.push_str(&text[end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    const ENCODINGS: [Encoding; 3] = [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32];

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn equal_texts_need_no_change() {
        assert_eq!(diff("abc", "abc", Encoding::Utf16), None);
    }

    #[test]
    fn an_insert_is_an_empty_range() {
        let c = diff("let x;\n", "let xy;\n", Encoding::Utf16).unwrap();
        assert_eq!(c.range.start, pos(0, 5));
        assert_eq!(c.range.end, pos(0, 5));
        assert_eq!(c.text, "y");
    }

    #[test]
    fn a_multi_line_delete_spans_lines() {
        let c = diff("a\nbb\nccc\n", "a\nccc\n", Encoding::Utf16).unwrap();
        assert_eq!(apply("a\nbb\nccc\n", &c, Encoding::Utf16), "a\nccc\n");
        assert_eq!(c.text, "");
        assert_eq!(c.range.end.line - c.range.start.line, 1);
    }

    #[test]
    fn astral_columns_follow_the_encoding() {
        // "💡" is 2 UTF-16 units and 4 UTF-8 bytes before the insert point.
        let old = "💡x";
        let new = "💡yx";
        assert_eq!(
            diff(old, new, Encoding::Utf16).unwrap().range.start,
            pos(0, 2)
        );
        assert_eq!(
            diff(old, new, Encoding::Utf8).unwrap().range.start,
            pos(0, 4)
        );
        assert_eq!(
            diff(old, new, Encoding::Utf32).unwrap().range.start,
            pos(0, 1)
        );
    }

    #[test]
    fn a_change_inside_a_multibyte_char_backs_off_to_its_boundary() {
        // "é" (C3 A9) and "è" (C3 A8) share their first byte.
        let c = diff("aé", "aè", Encoding::Utf16).unwrap();
        assert_eq!(c.range.start, pos(0, 1));
        assert_eq!(c.range.end, pos(0, 2));
        assert_eq!(c.text, "è");
    }

    #[test]
    fn a_crlf_pair_is_never_split() {
        for (old, new) in [("a\rb", "a\r\nb"), ("x\n", "x\r\n"), ("a\r\nb", "a\rb")] {
            for enc in ENCODINGS {
                let c = diff(old, new, enc).unwrap();
                assert_eq!(apply(old, &c, enc), new, "{old:?} -> {new:?}");
            }
        }
    }

    #[test]
    fn positions_count_every_line_break_kind() {
        let text = "a\r\nb\rc\nd";
        assert_eq!(position_at(text, 3, Encoding::Utf16), pos(1, 0));
        assert_eq!(position_at(text, 5, Encoding::Utf16), pos(2, 0));
        assert_eq!(position_at(text, 7, Encoding::Utf16), pos(3, 0));
        assert_eq!(offset_at(text, pos(3, 0), Encoding::Utf16), 7);
        assert_eq!(offset_at(text, pos(0, 99), Encoding::Utf16), 1);
        assert_eq!(offset_at(text, pos(99, 0), Encoding::Utf16), text.len());
    }

    #[test]
    fn sync_kind_reads_a_number_or_options() {
        let kind = |sync: Value| {
            SyncKind::from_initialize(&json!({ "capabilities": { "textDocumentSync": sync } }))
        };
        assert_eq!(kind(json!(2)), SyncKind::Incremental);
        assert_eq!(
            kind(json!({ "openClose": true, "change": 2 })),
            SyncKind::Incremental
        );
        assert_eq!(kind(json!(1)), SyncKind::Full);
        assert_eq!(kind(json!(0)), SyncKind::Full);
        assert_eq!(SyncKind::from_initialize(&json!({})), SyncKind::Full);
    }

    /// Text drawn from ASCII, multi-byte, astral chars and every line break.
    fn text() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec!["a", "b", "é", "中", "💡", "\n", "\r", "\r\n", " "]),
            0..24,
        )
        .prop_map(|parts| parts.concat())
    }

    /// One edit: replace chars `[start, start + len)` with `insert`.
    fn edit() -> impl Strategy<Value = (usize, usize, String)> {
        (0..30_usize, 0..6_usize, text())
    }

    /// Apply a char-indexed edit, like the editor's own buffer does.
    fn splice(text: &str, (start, len, insert): &(usize, usize, String)) -> String {
        let chars: Vec<char> = text.chars().collect();
        let start = (*start).min(chars.len());
        let end = (start + len).min(chars.len());
        let mut out: String = chars[..start].iter().collect();
        out.push_str(insert);
        out.extend(&chars[end..]);
        out
    }

    proptest! {
        /// Whatever the two texts, the server's copy ends up equal to the
        /// client's — the guarantee a full re-sync gives.
        #[test]
        fn applying_the_diff_reproduces_the_new_text(old in text(), new in text()) {
            for enc in ENCODINGS {
                let got = diff(&old, &new, enc).map_or_else(|| old.clone(), |c| apply(&old, &c, enc));
                prop_assert_eq!(&got, &new);
            }
        }

        /// A sequence of edits (several carets, undo restoring an earlier
        /// text), each synced incrementally, keeps the server's copy equal to
        /// the buffer after every step.
        #[test]
        fn a_sequence_of_edits_stays_in_sync(
            start in text(),
            edits in prop::collection::vec(edit(), 1..8),
        ) {
            for enc in ENCODINGS {
                let mut history = vec![start.clone()];
                let mut server = start.clone();
                for e in &edits {
                    let Some(current) = history.last() else { break };
                    let next = splice(current, e);
                    if let Some(c) = diff(&server, &next, enc) {
                        server = apply(&server, &c, enc);
                    }
                    prop_assert_eq!(&server, &next);
                    history.push(next);
                }
                // Undo all the way back to the start.
                while let Some(previous) = history.pop() {
                    if let Some(c) = diff(&server, &previous, enc) {
                        server = apply(&server, &c, enc);
                    }
                    prop_assert_eq!(&server, &previous);
                }
            }
        }
    }
}
