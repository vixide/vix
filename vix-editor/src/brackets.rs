//! Bracket matching: find the partner of the bracket at (or just before) the
//! cursor. Vix-owned code, held to the crate's `clippy::pedantic`.

use crate::editor::Editor;

/// Bracket pairs Vix matches.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// Maximum characters scanned in one direction (bounds pathological input where a
/// bracket has no partner).
const SCAN_LIMIT: usize = 50_000;

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
        let cur = self.cursor.min(self.code_ref().len_chars());
        if let Some(c) = self.char_at(cur)
            && let Some(m) = self.bracket_partner(cur, c)
        {
            return Some(m);
        }
        if cur > 0
            && let Some(c) = self.char_at(cur - 1)
            && let Some(m) = self.bracket_partner(cur - 1, c)
        {
            return Some(m);
        }
        None
    }

    fn char_at(&self, i: usize) -> Option<char> {
        let code = self.code_ref();
        if i >= code.len_chars() {
            return None;
        }
        code.char_slice(i, i + 1).chars().next()
    }

    fn bracket_partner(&self, pos: usize, c: char) -> Option<usize> {
        if let Some(&(_, close)) = PAIRS.iter().find(|&&(open, _)| open == c) {
            return self.scan_forward(pos, c, close);
        }
        if let Some(&(open, _)) = PAIRS.iter().find(|&&(_, close)| close == c) {
            return self.scan_backward(pos, c, open);
        }
        None
    }

    fn scan_forward(&self, pos: usize, open: char, close: char) -> Option<usize> {
        let end = (pos + SCAN_LIMIT).min(self.code_ref().len_chars());
        let mut depth = 1usize;
        let mut i = pos + 1;
        while i < end {
            match self.char_at(i) {
                Some(ch) if ch == open => depth += 1,
                Some(ch) if ch == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn scan_backward(&self, pos: usize, close: char, open: char) -> Option<usize> {
        let start = pos.saturating_sub(SCAN_LIMIT);
        let mut depth = 1usize;
        let mut i = pos;
        while i > start {
            i -= 1;
            match self.char_at(i) {
                Some(ch) if ch == close => depth += 1,
                Some(ch) if ch == open => {
                    depth -= 1;
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
