//! Read-only buffer access for motions and operators (T843).
//!
//! A motion used to take the whole buffer as one `&str` and decode it into
//! a `Vec<char>` (and often every line or word range) before looking at the
//! cursor, so even `l` cost time in proportion to the file. [`Text`] is
//! the small interface the motions in [`crate::motion`] and the range
//! helpers in [`crate::operator`] now read through instead: a length and
//! two char iterators, forward from an offset and backward from one. Each
//! motion walks only as far as its answer needs (the current line for
//! `h`/`l`/`0`/`^`/`$`/`f`, the next word boundary for `w`/`b`/`e`,
//! the lines it crosses for `j`/`k`/`{`/`}`), so its cost no longer
//! depends on how much unrelated text the buffer holds.
//!
//! Implemented for `str` and `String` (every existing caller and test
//! keeps passing text as before) and for [`ropey::Rope`] and
//! [`ropey::RopeSlice`], the editor's own buffer, which the host hands over
//! as an O(1) clone instead of copying the text out.
//!
//! Offsets are **character** offsets, as everywhere in this crate.

use ropey::{Rope, RopeSlice};

/// A read-only run of characters a motion can walk (see the module docs).
pub trait Text {
    /// The number of characters.
    fn len_chars(&self) -> usize;

    /// The characters from offset `from` to the end, in order; empty when
    /// `from` is at or past the end.
    fn chars_from(&self, from: usize) -> impl Iterator<Item = char> + '_;

    /// The characters before offset `before` (clamped to the end), nearest
    /// first: `before - 1`, `before - 2`, …, `0`.
    fn chars_before(&self, before: usize) -> impl Iterator<Item = char> + '_;

    /// The character at offset `at`, or `None` past the end.
    fn char_at(&self, at: usize) -> Option<char> {
        self.chars_from(at).next()
    }

    /// The characters of `[start, end)` as a `String`, both ends clamped
    /// to the text (an inverted range is empty).
    fn range_string(&self, start: usize, end: usize) -> String {
        let end = end.min(self.len_chars());
        let start = start.min(end);
        // `start <= end` was just established, so this cannot fail.
        self.chars_from(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    /// The whole text as a `String`, for the few motions that still need it
    /// all (`(`/`)`, whose sentence rules look at the whole text).
    fn whole_string(&self) -> String {
        self.range_string(0, self.len_chars())
    }
}

impl Text for str {
    fn len_chars(&self) -> usize {
        self.chars().count()
    }

    fn chars_from(&self, from: usize) -> impl Iterator<Item = char> + '_ {
        self.chars().skip(from)
    }

    fn chars_before(&self, before: usize) -> impl Iterator<Item = char> + '_ {
        let byte = self
            .char_indices()
            .nth(before)
            .map_or(self.len(), |(i, _)| i);
        self.get(..byte).unwrap_or(self).chars().rev()
    }

    fn whole_string(&self) -> String {
        self.to_string()
    }
}

impl Text for String {
    fn len_chars(&self) -> usize {
        self.as_str().len_chars()
    }

    fn chars_from(&self, from: usize) -> impl Iterator<Item = char> + '_ {
        self.as_str().chars_from(from)
    }

    fn chars_before(&self, before: usize) -> impl Iterator<Item = char> + '_ {
        self.as_str().chars_before(before)
    }

    fn whole_string(&self) -> String {
        self.clone()
    }
}

impl Text for RopeSlice<'_> {
    fn len_chars(&self) -> usize {
        RopeSlice::len_chars(self)
    }

    fn chars_from(&self, from: usize) -> impl Iterator<Item = char> + '_ {
        self.chars_at(from.min(RopeSlice::len_chars(self)))
    }

    fn chars_before(&self, before: usize) -> impl Iterator<Item = char> + '_ {
        let mut chars = self.chars_at(before.min(RopeSlice::len_chars(self)));
        std::iter::from_fn(move || chars.prev())
    }

    fn char_at(&self, at: usize) -> Option<char> {
        self.get_char(at)
    }

    fn range_string(&self, start: usize, end: usize) -> String {
        let end = end.min(RopeSlice::len_chars(self));
        let start = start.min(end);
        let slice = self.slice(start..end);
        let mut out = String::with_capacity(slice.len_bytes());
        for chunk in slice.chunks() {
            out.push_str(chunk);
        }
        out
    }
}

impl Text for Rope {
    fn len_chars(&self) -> usize {
        Rope::len_chars(self)
    }

    fn chars_from(&self, from: usize) -> impl Iterator<Item = char> + '_ {
        self.chars_at(from.min(Rope::len_chars(self)))
    }

    fn chars_before(&self, before: usize) -> impl Iterator<Item = char> + '_ {
        let mut chars = self.chars_at(before.min(Rope::len_chars(self)));
        std::iter::from_fn(move || chars.prev())
    }

    fn char_at(&self, at: usize) -> Option<char> {
        self.get_char(at)
    }

    fn range_string(&self, start: usize, end: usize) -> String {
        self.slice(..).range_string(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_impl_walks_the_same_chars() {
        let s = "héllo\nwörld";
        let rope = Rope::from_str(s);
        let owned = s.to_string();
        for at in 0..=s.chars().count() + 2 {
            let fwd: String = s.chars_from(at).collect();
            let back: String = s.chars_before(at).collect();
            assert_eq!(
                fwd,
                rope.chars_from(at).collect::<String>(),
                "forward from {at}"
            );
            assert_eq!(
                fwd,
                owned.chars_from(at).collect::<String>(),
                "forward from {at}"
            );
            assert_eq!(
                back,
                rope.chars_before(at).collect::<String>(),
                "backward from {at}"
            );
            assert_eq!(
                back,
                owned.chars_before(at).collect::<String>(),
                "backward from {at}"
            );
            assert_eq!(s.char_at(at), rope.char_at(at));
        }
        assert_eq!(rope.range_string(1, 4), "éll");
        assert_eq!(s.range_string(1, 4), "éll");
        assert_eq!(rope.range_string(4, 1), "", "an inverted range is empty");
        assert_eq!(rope.whole_string(), s);
        assert_eq!(s.chars_before(2).collect::<String>(), "éh", "nearest first");
    }
}
