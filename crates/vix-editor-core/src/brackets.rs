//! Bracket matching: find the partner of the bracket at (or just before) the
//! cursor. Vix-owned code, held to the crate's `clippy::pedantic`.
//!
//! Only `()`, `[]`, `{}` are matched, by a plain depth-counting scan over the
//! rope: it is *not* syntax-aware, so brackets inside strings and comments
//! count. The scan is capped at [`SCAN_LIMIT`] characters so a lone bracket in
//! a huge buffer cannot stall a frame. Used for the bracket-highlight in
//! [`crate::render`] and the jump-to-partner command. See
//! `crates/vix-editor-core/spec/index.md`.

use crate::editor::Editor;

/// Bracket pairs Vix matches.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// Maximum characters scanned in one direction (bounds pathological input where a
/// bracket has no partner).
const SCAN_LIMIT: usize = 50_000;

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

impl Editor {
    /// Public accessor for the matching bracket offset, used by the host to jump
    /// the cursor to a bracket's partner (`Ctrl+]` / the palette).
    #[must_use]
    pub fn matching_bracket_offset(&self) -> Option<usize> {
        self.matching_bracket()
    }

    /// Character offset of the bracket matching the one at (or immediately
    /// before) the cursor, or `None` when the cursor is not adjacent to a bracket
    /// or no partner is found within [`SCAN_LIMIT`].
    #[must_use]
    pub(crate) fn matching_bracket(&self) -> Option<usize> {
        // Prefer the bracket *under* the cursor, then the one just before it
        // (the usual "cursor after the closing paren" position).
        let cur = self.cursor.min(self.code_ref().len_chars());
        if let Some(c) = self.char_at(cur)
            && let Some(m) = self.bracket_partner(cur, c)
        {
            return Some(m);
        }
        if let Some(before) = cur.checked_sub(1)
            && let Some(c) = self.char_at(before)
            && let Some(m) = self.bracket_partner(before, c)
        {
            return Some(m);
        }
        None
    }

    /// The character at char offset `i`, or `None` past the end of the buffer.
    pub(crate) fn char_at(&self, i: usize) -> Option<char> {
        let code = self.code_ref();
        if i >= code.len_chars() {
            return None;
        }
        code.char_slice(i, add_or(i, 1, usize::MAX)).chars().next()
    }

    /// Partner of the bracket `c` found at `pos`: scan forward for an opener,
    /// backward for a closer; `None` if `c` is not one of [`PAIRS`] or has no
    /// partner in range.
    fn bracket_partner(&self, pos: usize, c: char) -> Option<usize> {
        if let Some(&(_, close)) = PAIRS.iter().find(|&&(open, _)| open == c) {
            return self.scan_forward(pos, c, close);
        }
        if let Some(&(open, _)) = PAIRS.iter().find(|&&(_, close)| close == c) {
            return self.scan_backward(pos, c, open);
        }
        None
    }

    /// Scan right from the opener at `pos` for its balanced `close`, tracking
    /// nesting depth; gives up after [`SCAN_LIMIT`] characters.
    fn scan_forward(&self, pos: usize, open: char, close: char) -> Option<usize> {
        // Clamp to the buffer so the loop bound is also the end-of-text guard.
        let end = add_or(pos, SCAN_LIMIT, usize::MAX).min(self.code_ref().len_chars());
        let mut depth = 1usize;
        let mut i = add_or(pos, 1, usize::MAX);
        while i < end {
            match self.char_at(i) {
                Some(ch) if ch == open => depth = add_or(depth, 1, usize::MAX),
                Some(ch) if ch == close => {
                    depth = sub_or(depth, 1, 0);
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i = add_or(i, 1, usize::MAX);
        }
        None
    }

    /// Mirror of [`Self::scan_forward`]: scan left from the closer at `pos`
    /// for its balanced `open`.
    fn scan_backward(&self, pos: usize, close: char, open: char) -> Option<usize> {
        let start = sub_or(pos, SCAN_LIMIT, 0);
        let mut depth = 1usize;
        let mut i = pos;
        while i > start {
            i = sub_or(i, 1, 0);
            match self.char_at(i) {
                Some(ch) if ch == close => depth = add_or(depth, 1, usize::MAX),
                Some(ch) if ch == open => {
                    depth = sub_or(depth, 1, 0);
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;

    #[test]
    fn matches_in_both_directions() {
        let mut ed = Editor::new("text", "a(bc)d", Vec::new()).unwrap();
        ed.set_cursor(1); // on '(' → ')' at 4
        assert_eq!(ed.matching_bracket(), Some(4));
        ed.set_cursor(4); // on ')' → '(' at 1
        assert_eq!(ed.matching_bracket(), Some(1));
        ed.set_cursor(5); // just after ')' → '(' at 1
        assert_eq!(ed.matching_bracket(), Some(1));
        ed.set_cursor(0); // on 'a' (not a bracket)
        assert_eq!(ed.matching_bracket(), None);
    }

    #[test]
    fn respects_nesting() {
        let mut ed = Editor::new("text", "([x])", Vec::new()).unwrap();
        ed.set_cursor(0); // '(' → ')' at 4
        assert_eq!(ed.matching_bracket(), Some(4));
        ed.set_cursor(1); // '[' → ']' at 3
        assert_eq!(ed.matching_bracket(), Some(3));
    }

    #[test]
    fn unmatched_returns_none() {
        let mut ed = Editor::new("text", "a(b", Vec::new()).unwrap();
        ed.set_cursor(1);
        assert_eq!(ed.matching_bracket(), None);
    }
}
