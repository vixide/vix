//! Multiple cursors ("carets").
//!
//! The editor keeps one *primary* cursor (`cursor` + `selection`); this module
//! adds zero or more *extra* carets in [`Editor::carets`]. Editing and movement
//! are applied at every caret at once, and `Ctrl+D` grows the set by selecting
//! the next occurrence of the current selection/word.
//!
//! Edits run directly on the rope in one undo transaction, processing carets in
//! ascending order with a running offset so each edit stays valid.

#![warn(clippy::pedantic)]

// Offset bookkeeping mixes `usize` positions with a signed running shift; the
// values are small in-buffer offsets, so these casts cannot realistically wrap.
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use crate::editor_core::actions::{MoveDown, MoveLeft, MoveRight, MoveUp};
use crate::editor_core::editor::Editor;
use crate::editor_core::selection::Selection;

/// An extra caret beyond the primary cursor.
#[derive(Clone, Copy, Debug)]
pub struct Caret {
    /// Cursor offset (character index).
    pub pos: usize,
    /// Selection anchor, if this caret has a selection.
    pub anchor: Option<usize>,
}

/// Which arrow movement to apply to every caret.
#[derive(Clone, Copy)]
pub enum CaretMove {
    /// Move every caret one character to the left.
    Left,
    /// Move every caret one character to the right.
    Right,
    /// Move every caret one line up.
    Up,
    /// Move every caret one line down.
    Down,
}

impl Editor {
    /// Whether more than one caret is active.
    #[must_use]
    pub fn has_multi_carets(&self) -> bool {
        !self.carets.is_empty()
    }

    /// Drop all extra carets, keeping just the primary cursor.
    pub fn clear_carets(&mut self) {
        self.carets.clear();
    }

    /// Selection ranges for every caret (primary + extras), for rendering.
    #[must_use]
    pub fn caret_selections(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if let Some(s) = self.selection.filter(|s| !s.is_empty()) {
            out.push((s.start, s.end));
        }
        for c in &self.carets {
            if let Some(a) = c.anchor.filter(|&a| a != c.pos) {
                out.push((a.min(c.pos), a.max(c.pos)));
            }
        }
        out
    }

    /// Cursor offsets for every caret (primary + extras), for rendering.
    #[must_use]
    pub fn caret_positions(&self) -> Vec<usize> {
        let mut out = vec![self.cursor];
        out.extend(self.carets.iter().map(|c| c.pos));
        out
    }

    /// All carets (primary first) as `(pos, anchor)` pairs.
    fn gather(&self) -> Vec<(usize, Option<usize>)> {
        let primary_anchor = self
            .selection
            .filter(|s| !s.is_empty())
            .map(|s| if s.end == self.cursor { s.start } else { s.end });
        let mut v = vec![(self.cursor, primary_anchor)];
        v.extend(self.carets.iter().map(|c| (c.pos, c.anchor)));
        v
    }

    /// Install carets from `(pos, anchor)` pairs: de-duplicate, sort by position,
    /// make the lowest the primary cursor, and keep the rest as extras.
    fn scatter(&mut self, mut carets: Vec<(usize, Option<usize>)>) {
        carets.sort_by_key(|&(p, _)| p);
        carets.dedup_by_key(|&mut (p, _)| p);
        let mut iter = carets.into_iter();
        let (pos, anchor) = iter.next().unwrap_or((self.cursor, None));
        self.cursor = pos;
        self.selection = anchor
            .filter(|&a| a != pos)
            .map(|a| Selection::from_anchor_and_cursor(a, pos));
        self.carets = iter.map(|(pos, anchor)| Caret { pos, anchor }).collect();
        self.reset_highlight_cache();
    }

    /// Insert `text` at every caret (replacing each selection), as one undo step.
    /// Carets are processed in ascending order with a running offset `shift`, so
    /// each edit's coordinates stay valid in the live buffer.
    pub fn multi_insert(&mut self, text: &str) {
        let added = text.chars().count() as isize;
        let mut carets = self.gather();
        carets.sort_by_key(|&(p, _)| p);
        let primary = (self.cursor, self.selection);
        let code = self.code_mut();
        code.tx();
        code.set_state_before(primary.0, primary.1);
        let mut shift: isize = 0;
        let mut result: Vec<(usize, Option<usize>)> = Vec::with_capacity(carets.len());
        for (pos, anchor) in carets {
            let (rstart, rend, base) = match anchor.filter(|&a| a != pos) {
                Some(a) => (a.min(pos), a.max(pos), a.min(pos)),
                None => (pos, pos, pos),
            };
            let at = (base as isize + shift) as usize;
            if rend > rstart {
                let rs = (rstart as isize + shift) as usize;
                let re = (rend as isize + shift) as usize;
                code.remove(rs, re);
            }
            code.insert(at, text);
            shift += added - (rend - rstart) as isize;
            result.push((at + added as usize, None));
        }
        code.commit();
        self.scatter(result);
    }

    /// Delete at every caret (each selection, else one char), as one undo step.
    pub fn multi_delete(&mut self, forward: bool) {
        let len = self.code_ref().len_chars() as isize;
        let mut carets = self.gather();
        carets.sort_by_key(|&(p, _)| p);
        let primary = (self.cursor, self.selection);
        let code = self.code_mut();
        code.tx();
        code.set_state_before(primary.0, primary.1);
        let mut shift: isize = 0;
        let mut result: Vec<(usize, Option<usize>)> = Vec::with_capacity(carets.len());
        for (pos, anchor) in carets {
            if let Some(a) = anchor.filter(|&a| a != pos) {
                let (s, e) = (a.min(pos), a.max(pos));
                let rs = (s as isize + shift) as usize;
                let re = (e as isize + shift) as usize;
                code.remove(rs, re);
                shift -= (e - s) as isize;
                result.push((rs, None));
            } else if forward {
                let p = (pos as isize + shift) as usize;
                if (pos as isize) < len {
                    code.remove(p, p + 1);
                    shift -= 1;
                }
                result.push((p, None));
            } else if pos > 0 {
                let p = (pos as isize + shift) as usize;
                code.remove(p - 1, p);
                shift -= 1;
                result.push((p - 1, None));
            } else {
                result.push(((pos as isize + shift) as usize, None));
            }
        }
        code.commit();
        self.scatter(result);
    }

    /// Move every caret in `dir` (extending its selection when `shift`).
    pub fn multi_move(&mut self, dir: CaretMove, shift: bool) {
        let carets = self.gather();
        let mut result: Vec<(usize, Option<usize>)> = Vec::with_capacity(carets.len());
        for (pos, anchor) in carets {
            // Drive the single-cursor move logic for this caret.
            self.cursor = pos;
            self.selection =
                anchor.map(|a| Selection::from_anchor_and_cursor(a, pos));
            match dir {
                CaretMove::Left => self.apply(MoveLeft { shift }),
                CaretMove::Right => self.apply(MoveRight { shift }),
                CaretMove::Up => self.apply(MoveUp { shift }),
                CaretMove::Down => self.apply(MoveDown { shift }),
            }
            let new_anchor = self
                .selection
                .filter(|s| !s.is_empty())
                .map(|s| if s.end == self.cursor { s.start } else { s.end });
            result.push((self.cursor, new_anchor));
        }
        self.scatter(result);
    }

    /// `Ctrl+D`: select the next occurrence of the current selection (or the word
    /// at the cursor) and add it as a new caret, which becomes primary.
    pub fn add_next_occurrence(&mut self) {
        // Ensure the primary has a selection to search for.
        if self.selection.is_none_or(|s| s.is_empty()) {
            if let Some((s, e, _)) = self.word_at(self.cursor) {
                self.selection = Some(Selection::new(s, e));
                self.cursor = e;
            } else {
                return;
            }
        }
        let Some(sel) = self.selection else {
            return;
        };
        let (s, e) = (sel.start, sel.end);
        let needle: Vec<char> = self.get_content_slice(s, e).chars().collect();
        if needle.is_empty() {
            return;
        }
        let hay: Vec<char> = self.get_content().chars().collect();
        // Furthest caret end, to search after.
        let from = self
            .caret_selections()
            .iter()
            .map(|&(_, end)| end)
            .chain(self.caret_positions())
            .max()
            .unwrap_or(e);
        let found = find_from(&hay, &needle, from).or_else(|| find_from(&hay, &needle, 0));
        if let Some(start) = found {
            let end = start + needle.len();
            // Skip if this exact range is already a caret.
            if self.caret_selections().contains(&(start, end)) {
                return;
            }
            // Demote the current primary to an extra caret; promote the match.
            let cur_anchor =
                self.selection.map(|s| if s.end == self.cursor { s.start } else { s.end });
            self.carets.push(Caret { pos: self.cursor, anchor: cur_anchor });
            self.cursor = end;
            self.selection = Some(Selection::new(start, end));
        }
    }

    /// Alt+click: add an extra caret at character offset `pos`.
    pub fn add_caret_at(&mut self, pos: usize) {
        let pos = pos.min(self.code_ref().len_chars());
        if pos != self.cursor && !self.carets.iter().any(|c| c.pos == pos) {
            self.carets.push(Caret { pos, anchor: None });
        }
    }

    /// Add a caret on the line above the main cursor, at the same column (clamped
    /// to that line's length). No-op on the first line.
    pub fn add_caret_above(&mut self) {
        self.add_caret_vertical(false);
    }

    /// Add a caret on the line below the main cursor, at the same column (clamped
    /// to that line's length). No-op on the last line.
    pub fn add_caret_below(&mut self) {
        self.add_caret_vertical(true);
    }

    fn add_caret_vertical(&mut self, down: bool) {
        let pos = {
            let code = self.code_ref();
            let line = code.char_to_line(self.cursor);
            let col = self.cursor - code.line_to_char(line);
            let n = code.len_lines();
            if down && line + 1 >= n {
                return;
            }
            if !down && line == 0 {
                return;
            }
            let target = if down { line + 1 } else { line - 1 };
            code.line_to_char(target) + col.min(code.line_len(target))
        };
        self.add_caret_at(pos);
    }
}

/// First index `>= from` where `needle` occurs in `hay` (by character).
fn find_from(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

#[cfg(test)]
mod caret_tests {
    use crate::editor_core::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("text", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    #[test]
    fn add_caret_below_keeps_column() {
        let mut e = ed("abcd\nefgh\nij", 2); // line 0, col 2
        e.add_caret_below();
        let mut all = e.caret_positions();
        all.sort_unstable();
        // main caret at 2, new caret on line 1 col 2 = index 5+2 = 7.
        assert!(all.contains(&7), "carets: {all:?}");
    }

    #[test]
    fn add_caret_below_clamps_to_short_line() {
        let mut e = ed("abcd\nef", 3); // col 3; next line "ef" len 2
        e.add_caret_below();
        let mut all = e.caret_positions();
        all.sort_unstable();
        assert!(all.contains(&7), "clamped to end of 'ef' (5+2): {all:?}");
    }

    #[test]
    fn add_caret_above_on_first_line_is_noop() {
        let mut e = ed("abc\ndef", 1);
        e.add_caret_above();
        assert!(!e.has_multi_carets(), "no caret added above the first line");
    }
}
