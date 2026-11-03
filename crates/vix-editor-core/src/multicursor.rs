//! Multiple cursors ("carets").
//!
//! The editor keeps one *primary* cursor (`cursor` + `selection`); this module
//! adds zero or more *extra* carets in `Editor::carets` (private). Editing and movement
//! are applied at every caret at once, and `Ctrl+D` grows the set by selecting
//! the next occurrence of the current selection/word.
//!
//! Carets keep their **add order**: the primary is the newest caret, and
//! `carets` holds the older ones oldest first. Moves and edits never reorder
//! them, so [`Editor::remove_last_caret`](crate::editor::Editor::remove_last_caret) always drops the caret added last
//! and [`Editor::skip_occurrence`](crate::editor::Editor::skip_occurrence) always moves the newest match.
//!
//! Edits run directly on the rope in one undo transaction, processing carets in
//! ascending position order with a running offset so each edit stays valid.
//! A caret that falls inside text another caret already replaced is merged
//! into that caret instead of being applied at a stale offset (T575).
//!
//! The rest of the editor sees carets through `gather`/`scatter`: `gather`
//! flattens primary + extras into `(pos, anchor)` pairs, and `scatter` writes
//! them back (clamping to the buffer, merging carets that land on the same
//! position). Named single-caret motions in [`crate::named`] first offer
//! themselves to `Editor::multi_motion`. See `crates/vix-editor-core/spec/index.md`.
//!
//! # Examples
//!
//! ```
//! use vix_editor_core::editor::Editor;
//!
//! let mut ed = Editor::new("text", "foo foo", Vec::new()).unwrap();
//! ed.add_next_occurrence(); // selects the word under the cursor...
//! ed.add_next_occurrence(); // ...and adds the next match as a second caret
//! assert_eq!(ed.caret_count(), 2);
//! ed.multi_insert("x"); // one undo step, edits every caret
//! assert_eq!(ed.get_content(), "x x");
//! ```

use std::ops::Range;

use crate::actions::{MoveDown, MoveLeft, MoveRight, MoveUp};
use crate::editor::Editor;
use crate::selection::Selection;

/// Apply a running signed shift to a `usize` buffer offset (T150): earlier
/// carets' edits can move a later caret's position forward (an insert) or
/// backward (a delete), tracked as one `isize` accumulator per multi-caret
/// operation. `usize::checked_add_signed` is the exact primitive for this —
/// no `as isize`/`as usize` casts needed at all, so there is nothing here for
/// `clippy::cast_possible_wrap`/`cast_sign_loss` to flag. The `expect` holds
/// because `shift` only ever reflects edits already applied to this same
/// buffer: a caret can't be shifted further out of bounds than the buffer
/// actually grew or shrank.
#[expect(
    clippy::expect_used,
    reason = "the value is bounded well below the target type"
)]
fn shifted(pos: usize, shift: isize) -> usize {
    pos.checked_add_signed(shift)
        .expect("caret offset stays within the buffer after a prior edit's shift")
}

/// A character count as a signed shift.
#[expect(
    clippy::expect_used,
    reason = "a buffer length is bounded well below isize::MAX"
)]
fn signed(n: usize) -> isize {
    isize::try_from(n).expect("a buffer length fits in isize")
}

/// An extra caret beyond the primary cursor. The primary lives in the
/// [`Editor`]'s own `cursor`/`selection`; extras are stored oldest first.
#[derive(Clone, Copy, Debug)]
pub struct Caret {
    /// Cursor offset (character index).
    pub pos: usize,
    /// Selection anchor, if this caret has a selection.
    pub anchor: Option<usize>,
}

/// Which movement to apply to every caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaretMove {
    /// Move every caret one character to the left.
    Left,
    /// Move every caret one character to the right.
    Right,
    /// Move every caret one line up.
    Up,
    /// Move every caret one line down.
    Down,
    /// Move every caret to the start of the previous word.
    WordLeft,
    /// Move every caret to the end of the next word.
    WordRight,
    /// Move every caret to column 0 of its line.
    LineStart,
    /// Move every caret to the end of its line.
    LineEnd,
    /// Move every caret to the first non-blank character of its line.
    TextStart,
    /// Smart Home per caret: to the first non-blank character, or to column 0
    /// when already there (a blank line counts as column 0).
    Home,
    /// Move every caret to the start of the buffer (they merge into one).
    BufferStart,
    /// Move every caret to the end of the buffer (they merge into one).
    BufferEnd,
}

/// The start of a caret's replaced range: its selection's start, or its
/// position when it has no selection.
fn range_start((pos, anchor): (usize, Option<usize>)) -> usize {
    anchor.map_or(pos, |a| a.min(pos))
}

impl Editor {
    /// Whether more than one caret is active.
    #[must_use]
    pub fn has_multi_carets(&self) -> bool {
        !self.carets.is_empty()
    }

    /// How many carets are active, the primary included (always at least 1).
    #[must_use]
    pub fn caret_count(&self) -> usize {
        self.carets.len() + 1
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

    /// The selected text of every caret that has a selection, in buffer
    /// order. Multi-caret copy joins these with newlines.
    #[must_use]
    pub fn caret_selection_texts(&self) -> Vec<String> {
        let mut ranges = self.caret_selections();
        ranges.sort_unstable();
        ranges
            .into_iter()
            .map(|(s, e)| self.get_content_slice(s, e))
            .collect()
    }

    /// The primary caret's selection anchor, when it has a non-empty selection.
    fn primary_anchor(&self) -> Option<usize> {
        // The selection stores (start, end); the anchor is whichever end the
        // cursor is *not* on.
        self.selection
            .filter(|s| !s.is_empty())
            .map(|s| if s.end == self.cursor { s.start } else { s.end })
    }

    /// All carets (primary first, then the extras in add order) as
    /// `(pos, anchor)` pairs.
    fn gather(&self) -> Vec<(usize, Option<usize>)> {
        let mut v = vec![(self.cursor, self.primary_anchor())];
        v.extend(self.carets.iter().map(|c| (c.pos, c.anchor)));
        v
    }

    /// Install carets from `(pos, anchor)` pairs, keeping their order: the
    /// first becomes the primary cursor and the rest the extras. A caret on
    /// the same position as an earlier one is merged into it.
    fn scatter(&mut self, carets: Vec<(usize, Option<usize>)>) {
        let len = self.code_ref().len_chars();
        // Later carets on an already-seen position are dropped, so when carets
        // collide the earliest one (and its anchor) wins.
        let mut seen = std::collections::HashSet::new();
        let mut kept = carets
            .into_iter()
            .map(|(p, a)| (p.min(len), a.map(|a| a.min(len))))
            .filter(|&(p, _)| seen.insert(p));
        // `carets` is never empty in practice; the fallback only keeps the
        // primary cursor valid if a caller hands in nothing.
        let (pos, anchor) = kept.next().unwrap_or((self.cursor.min(len), None));
        self.cursor = pos;
        self.goal_col = None;
        self.selection = anchor
            .filter(|&a| a != pos)
            .map(|a| Selection::from_anchor_and_cursor(a, pos));
        self.carets = kept.map(|(pos, anchor)| Caret { pos, anchor }).collect();
        self.reset_highlight_cache();
    }

    /// Insert `text` at every caret (replacing each selection), as one undo step.
    pub fn multi_insert(&mut self, text: &str) {
        let texts = vec![text; self.caret_count()];
        self.multi_replace(&texts);
    }

    /// Replace every caret's selection (or insert at a bare caret) with its own
    /// text — `texts[0]` for the primary, then one per extra caret in add
    /// order — as one undo step. Carets are processed in ascending order with a
    /// running offset `shift`, so each edit's coordinates stay valid in the
    /// live buffer.
    fn multi_replace(&mut self, texts: &[&str]) {
        let carets = self.gather();
        // By the start of each caret's replaced range, so an enclosing selection
        // is applied before a caret inside it.
        let mut order: Vec<usize> = (0..carets.len()).collect();
        order.sort_by_key(|&i| range_start(carets[i]));
        let primary = (self.cursor, self.selection);
        let code = self.code_mut();
        // One transaction for all carets so a single Undo reverts the lot, and
        // the undo state remembers the primary's cursor/selection.
        code.tx();
        code.set_state_before(primary.0, primary.1);
        let mut shift: isize = 0;
        // Indexed by the caret's original slot so add order survives the
        // position-sorted processing; merged-away carets stay `None`.
        let mut result: Vec<Option<(usize, Option<usize>)>> = vec![None; carets.len()];
        // End of the span the previous caret already replaced, in the original
        // text. A caret that starts inside it overlaps that edit: its text is
        // already gone, so it is merged into the earlier caret rather than
        // applied at a shifted offset that no longer exists (T575).
        let mut prev_end = 0;
        let len = code.len_chars();
        for i in order {
            let (pos, anchor) = carets[i];
            let (pos, anchor) = (pos.min(len), anchor.map(|a| a.min(len)));
            let (rstart, rend) = match anchor.filter(|&a| a != pos) {
                Some(a) => (a.min(pos), a.max(pos)),
                None => (pos, pos),
            };
            if rstart < prev_end {
                continue;
            }
            prev_end = rend;
            let text = texts.get(i).copied().unwrap_or("");
            let added = text.chars().count();
            let at = shifted(rstart, shift);
            if rend > rstart {
                code.remove(at, shifted(rend, shift));
            }
            code.insert(at, text);
            shift += signed(added) - signed(rend - rstart);
            result[i] = Some((at + added, None));
        }
        let result: Vec<_> = result.into_iter().flatten().collect();
        if let Some(&(p, _)) = result.first() {
            code.set_state_after(p, None);
        }
        code.commit();
        self.scatter(result);
    }

    /// Delete at every caret (each selection, else one char), as one undo step.
    /// `forward` chooses Delete (the char at the caret) over Backspace (the one
    /// before it) for carets with no selection.
    pub fn multi_delete(&mut self, forward: bool) {
        let len = self.code_ref().len_chars();
        let carets = self.gather();
        let mut order: Vec<usize> = (0..carets.len()).collect();
        order.sort_by_key(|&i| range_start(carets[i]));
        let primary = (self.cursor, self.selection);
        let code = self.code_mut();
        code.tx();
        code.set_state_before(primary.0, primary.1);
        let mut shift: isize = 0;
        let mut result: Vec<Option<(usize, Option<usize>)>> = vec![None; carets.len()];
        // Like `multi_replace`: a caret whose span starts inside text an earlier
        // caret already removed is merged into that caret.
        let mut prev_end = 0;
        for i in order {
            let (pos, anchor) = carets[i];
            let (pos, anchor) = (pos.min(len), anchor.map(|a| a.min(len)));
            // Clamped at the buffer edges, so Backspace at 0 or Delete at the
            // end is an empty range (a no-op for that caret).
            let (s, e) = match anchor.filter(|&a| a != pos) {
                Some(a) => (a.min(pos), a.max(pos)),
                None if forward => (pos, (pos + 1).min(len)),
                None => (pos.saturating_sub(1), pos),
            };
            if s < prev_end {
                continue;
            }
            prev_end = e;
            let rs = shifted(s, shift);
            if e > s {
                code.remove(rs, shifted(e, shift));
                shift -= signed(e - s);
            }
            result[i] = Some((rs, None));
        }
        let result: Vec<_> = result.into_iter().flatten().collect();
        if let Some(&(p, _)) = result.first() {
            code.set_state_after(p, None);
        }
        code.commit();
        self.scatter(result);
    }

    /// Paste `text` at every caret, as one undo step. When `text` has exactly
    /// one line per caret (ignoring one trailing newline), the lines are
    /// distributed one per caret in buffer order — so copying N selections and
    /// pasting into N carets puts each back in its place. Otherwise every
    /// caret gets the whole text.
    pub fn multi_paste(&mut self, text: &str) {
        let n = self.caret_count();
        let body = text
            .strip_suffix('\n')
            .map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
        let lines: Vec<&str> = body
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .collect();
        if n > 1 && lines.len() == n {
            // Hand out lines by buffer order (not add order) so copy and paste
            // round-trip: `caret_selection_texts` is also in buffer order.
            let carets = self.gather();
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by_key(|&i| range_start(carets[i]));
            let mut texts = vec![""; n];
            for (line, i) in lines.into_iter().zip(order) {
                texts[i] = line;
            }
            self.multi_replace(&texts);
        } else {
            self.multi_insert(text);
        }
    }

    /// Cut every caret's selection: copy them (joined with newlines, in buffer
    /// order) and remove them, as one undo step. Carets without a selection
    /// stay put. No-op when no caret has a selection.
    pub fn multi_cut(&mut self) {
        let texts = self.caret_selection_texts();
        if texts.is_empty() {
            return;
        }
        // Clipboard first: if the removal below were to fail the text is not lost.
        self.copy_to_clipboard(&texts.join("\n"));
        let empty = vec![""; self.caret_count()];
        self.multi_replace(&empty);
    }

    /// Move every caret in `dir` (extending its selection when `shift`).
    pub fn multi_move(&mut self, dir: CaretMove, shift: bool) {
        let carets = self.gather();
        let mut result: Vec<(usize, Option<usize>)> = Vec::with_capacity(carets.len());
        for (pos, anchor) in carets {
            result.push(self.move_one(dir, shift, pos, anchor));
        }
        self.scatter(result);
    }

    /// One caret's position and anchor after moving it in `dir`.
    fn move_one(
        &mut self,
        dir: CaretMove,
        shift: bool,
        pos: usize,
        anchor: Option<usize>,
    ) -> (usize, Option<usize>) {
        if matches!(
            dir,
            CaretMove::Left | CaretMove::Right | CaretMove::Up | CaretMove::Down
        ) {
            // Reuse the single-cursor arrow logic (selection collapse, goal
            // column, line wrap) by temporarily making this caret the primary;
            // `scatter` restores the real primary afterwards. Drive the single-cursor arrow logic for this caret.
            self.cursor = pos;
            self.selection = anchor.map(|a| Selection::from_anchor_and_cursor(a, pos));
            match dir {
                CaretMove::Left => self.apply(MoveLeft { shift }),
                CaretMove::Right => self.apply(MoveRight { shift }),
                CaretMove::Up => self.apply(MoveUp { shift }),
                _ => self.apply(MoveDown { shift }),
            }
            return (self.cursor, self.primary_anchor());
        }
        let target = self.motion_target(dir, pos);
        if shift {
            let a = anchor.unwrap_or(pos);
            (target, (a != target).then_some(a))
        } else {
            (target, None)
        }
    }

    /// Where a non-arrow [`CaretMove`] takes a caret at `pos`.
    fn motion_target(&self, dir: CaretMove, pos: usize) -> usize {
        match dir {
            CaretMove::WordLeft => self.prev_word(pos),
            CaretMove::WordRight => self.next_word(pos),
            CaretMove::LineStart => self.line_start(pos),
            CaretMove::LineEnd => self.line_end(pos),
            CaretMove::TextStart => self.line_first_text(pos),
            CaretMove::Home => {
                // Smart Home: a line that is blank (or all indent) counts as
                // indent 0, so Home goes to column 0 rather than toggling.
                let start = self.line_start(pos);
                let len = self.line_end(pos) - start;
                let first = self.line_first_text(pos) - start;
                let indent = if first >= len { 0 } else { first };
                if pos - start == indent {
                    start
                } else {
                    start + indent
                }
            }
            CaretMove::BufferStart => 0,
            CaretMove::BufferEnd => self.code_ref().len_chars(),
            CaretMove::Left | CaretMove::Right | CaretMove::Up | CaretMove::Down => pos,
        }
    }

    /// Route a single-caret motion through [`Self::multi_move`] when extra
    /// carets exist. Returns whether it did (so the caller skips its own
    /// primary-only move).
    pub(crate) fn multi_motion(&mut self, dir: CaretMove, shift: bool) -> bool {
        if self.has_multi_carets() {
            self.multi_move(dir, shift);
            true
        } else {
            false
        }
    }

    /// Give the primary caret a selection to search for: keep a non-empty
    /// selection, else select the word at the cursor. Returns the selected
    /// range, or `None` when there is no word there.
    fn ensure_word_selection(&mut self) -> Option<(usize, usize)> {
        if self.selection.is_none_or(|s| s.is_empty()) {
            let (s, e, _) = self.word_at(self.cursor)?;
            self.selection = Some(Selection::new(s, e));
            self.cursor = e;
        }
        let sel = self.selection?;
        (sel.end > sel.start).then_some((sel.start, sel.end))
    }

    /// `Ctrl+D`: select the next occurrence of the current selection (or the word
    /// at the cursor) and add it as a new caret, which becomes primary.
    pub fn add_next_occurrence(&mut self) {
        let Some((s, e)) = self.ensure_word_selection() else {
            return;
        };
        // Compared as `char` vectors (not bytes) so offsets are char indices,
        // the unit every caret position uses. O(buffer) per call, fine for an
        // interactive command.
        let needle: Vec<char> = self.get_content_slice(s, e).chars().collect();
        let hay: Vec<char> = self.get_content().chars().collect();
        // Furthest caret end, to search after.
        let from = self
            .caret_selections()
            .iter()
            .map(|&(_, end)| end)
            .chain(self.caret_positions())
            .max()
            .unwrap_or(e);
        // Search forward from the newest caret, wrapping to the buffer start.
        let found = find_from(&hay, &needle, from).or_else(|| find_from(&hay, &needle, 0));
        if let Some(start) = found {
            let end = start + needle.len();
            // Wrapped all the way round onto an existing caret: nothing new.
            if self.caret_selections().contains(&(start, end)) {
                return;
            }
            // Demote the current primary to an extra caret; promote the match.
            self.carets.push(Caret {
                pos: self.cursor,
                anchor: self.primary_anchor(),
            });
            self.cursor = end;
            self.selection = Some(Selection::new(start, end));
        }
    }

    /// Skip the newest match: move the primary caret's selection to the next
    /// occurrence of its text (`forward`) or the previous one, wrapping around
    /// the buffer and passing over ranges another caret already holds. With
    /// no selection it first selects the word at the cursor. No-op when there
    /// is no other free occurrence.
    pub fn skip_occurrence(&mut self, forward: bool) {
        let Some((s, e)) = self.ensure_word_selection() else {
            return;
        };
        let needle: Vec<char> = self.get_content_slice(s, e).chars().collect();
        let hay: Vec<char> = self.get_content().chars().collect();
        let n = needle.len();
        let taken = self.caret_selections();
        let starts: Vec<usize> = occurrences(&hay, &needle)
            .into_iter()
            .filter(|&st| !taken.contains(&(st, st + n)))
            .collect();
        // `st >= e` / `st + n <= s`: strictly past the current match, so the
        // current selection is never re-picked; the `or_else` arms wrap.
        let pick = if forward {
            starts
                .iter()
                .copied()
                .find(|&st| st >= e)
                .or_else(|| starts.iter().copied().find(|&st| st < s))
        } else {
            starts
                .iter()
                .rev()
                .copied()
                .find(|&st| st + n <= s)
                .or_else(|| starts.iter().rev().copied().find(|&st| st > s))
        };
        if let Some(start) = pick {
            self.cursor = start + n;
            self.goal_col = None;
            self.selection = Some(Selection::new(start, start + n));
        }
    }

    /// Remove the caret added last (the primary), promoting the previous one
    /// to primary. Returns `false`, changing nothing, when there is only one
    /// caret.
    pub fn remove_last_caret(&mut self) -> bool {
        let Some(last) = self.carets.pop() else {
            return false;
        };
        self.cursor = last.pos;
        self.goal_col = None;
        self.selection = last
            .anchor
            .filter(|&a| a != last.pos)
            .map(|a| Selection::from_anchor_and_cursor(a, last.pos));
        self.reset_highlight_cache();
        true
    }

    /// Select every occurrence of the current selection (or the word at the
    /// cursor) in the buffer: replace any existing extra carets with one selected
    /// caret per match, the last of which becomes primary. No-op when there is
    /// only a single match.
    pub fn add_all_occurrences(&mut self) {
        let Some((s, e)) = self.ensure_word_selection() else {
            return;
        };
        let needle: Vec<char> = self.get_content_slice(s, e).chars().collect();
        let hay: Vec<char> = self.get_content().chars().collect();
        let matches: Vec<(usize, usize)> = occurrences(&hay, &needle)
            .into_iter()
            .map(|st| (st, st + needle.len()))
            .collect();
        if matches.len() <= 1 {
            return;
        }
        self.carets.clear();
        let (last_start, last_end) = matches[matches.len() - 1];
        for &(start, end) in &matches[..matches.len() - 1] {
            self.carets.push(Caret {
                pos: end,
                anchor: Some(start),
            });
        }
        self.cursor = last_end;
        self.selection = Some(Selection::new(last_start, last_end));
    }

    /// Split every caret's selection into one selection per line it covers
    /// (Helix `Alt-s`, Kakoune `Alt-s`): each piece runs from the selection's
    /// start (or the line's start) to the selection's end (or the line's end,
    /// without its newline). Empty pieces — a blank line, or the column-0 end
    /// of a selection that stops just after a newline — are dropped. Carets
    /// without a selection stay as they are. The last piece in the buffer
    /// becomes primary. Returns `false`, changing nothing, when no caret has a
    /// selection.
    pub fn split_selection_into_lines(&mut self) -> bool {
        let carets = self.gather();
        if !carets.iter().any(|&(p, a)| a.is_some_and(|a| a != p)) {
            return false;
        }
        let code = self.code_ref();
        let mut out = Vec::new();
        for (p, a) in carets {
            let Some(a) = a.filter(|&a| a != p) else {
                out.push((p, None));
                continue;
            };
            let (s, e) = (a.min(p), a.max(p));
            let before = out.len();
            for line in code.char_to_line(s)..=code.char_to_line(e) {
                let ls = code.line_to_char(line);
                let le = ls + code.line_len(line);
                let (ps, pe) = (s.max(ls), e.min(le));
                if pe > ps {
                    out.push((pe, Some(ps)));
                }
            }
            if out.len() == before {
                out.push((p, Some(a)));
            }
        }
        self.install_in_buffer_order(out);
        true
    }

    /// Replace the carets with one selection per match of `find` inside the
    /// current selections (Kakoune/Helix `s`). `find` gets each selection's
    /// text and returns the matches' **byte** ranges in it; empty matches and
    /// ranges off a char boundary are ignored. When no caret has a selection
    /// the whole buffer is searched. The last match becomes primary. Returns
    /// how many carets were installed; `0` (no match) changes nothing.
    pub fn select_matches_in_selection(
        &mut self,
        mut find: impl FnMut(&str) -> Vec<Range<usize>>,
    ) -> usize {
        let mut ranges = self.caret_selections();
        if ranges.is_empty() {
            ranges.push((0, self.code_ref().len_chars()));
        }
        let mut out = Vec::new();
        for (s, e) in ranges {
            let text = self.get_content_slice(s, e);
            for m in find(&text) {
                if m.end <= m.start
                    || m.end > text.len()
                    || !text.is_char_boundary(m.start)
                    || !text.is_char_boundary(m.end)
                {
                    continue;
                }
                let cs = s + text[..m.start].chars().count();
                let ce = cs + text[m.start..m.end].chars().count();
                out.push((ce, Some(cs)));
            }
        }
        if out.is_empty() {
            return 0;
        }
        self.install_in_buffer_order(out);
        self.caret_count()
    }

    /// Install carets sorted by position, the last in the buffer as primary.
    fn install_in_buffer_order(&mut self, mut carets: Vec<(usize, Option<usize>)>) {
        carets.sort_by_key(|&(p, a)| (p, a));
        carets.dedup_by_key(|&mut (p, _)| p);
        if let Some(last) = carets.pop() {
            carets.insert(0, last);
        }
        self.scatter(carets);
    }

    /// Alt+click: add a caret at character offset `pos`. It becomes the
    /// primary (the newest caret); the old primary joins the extras. No-op
    /// when a caret is already there.
    pub fn add_caret_at(&mut self, pos: usize) {
        let pos = pos.min(self.code_ref().len_chars());
        if pos != self.cursor && !self.carets.iter().any(|c| c.pos == pos) {
            self.push_primary(pos, None);
        }
    }

    /// Make a new caret at `pos` (with `anchor`) the primary, demoting the
    /// current primary to the newest extra caret.
    fn push_primary(&mut self, pos: usize, anchor: Option<usize>) {
        self.carets.push(Caret {
            pos: self.cursor,
            anchor: self.primary_anchor(),
        });
        self.cursor = pos;
        self.goal_col = None;
        self.selection = anchor
            .filter(|&a| a != pos)
            .map(|a| Selection::from_anchor_and_cursor(a, pos));
    }

    /// Add a caret on the line above the newest caret, at the same column
    /// (clamped to that line's length); it becomes primary, so repeating keeps
    /// going up. No-op on the first line.
    pub fn add_caret_above(&mut self) {
        self.add_caret_vertical(false);
    }

    /// Add a caret on the line below the newest caret, at the same column
    /// (clamped to that line's length); it becomes primary, so repeating keeps
    /// going down. No-op on the last line.
    pub fn add_caret_below(&mut self) {
        self.add_caret_vertical(true);
    }

    /// Extend a column (rectangular) selection by one line: add a caret on the
    /// line past the current block frontier, spanning the same columns as the
    /// block's first caret (its selection, or a bare caret when there is
    /// none). Columns clamp to each line's length. The new caret becomes
    /// primary. No-op at the buffer's top/bottom. The resulting carets edit
    /// together via [`Editor::multi_insert`] etc.
    pub fn column_select(&mut self, down: bool) {
        // The block's columns come from its oldest caret, so a clamped short
        // line in the middle doesn't narrow the lines after it.
        let (origin, origin_anchor) = self
            .carets
            .first()
            .map_or((self.cursor, self.primary_anchor()), |c| (c.pos, c.anchor));
        let code = self.code_ref();
        let col_of = |p: usize| p - code.line_to_char(code.char_to_line(p));
        let cur_col = col_of(origin);
        let anchor_col = origin_anchor.map_or(cur_col, col_of);
        // Frontier = furthest caret line in the direction of travel.
        let lines = self
            .caret_positions()
            .into_iter()
            .map(|p| code.char_to_line(p));
        let frontier = if down { lines.max() } else { lines.min() }.unwrap_or(0);
        if (down && frontier + 1 >= code.len_lines()) || (!down && frontier == 0) {
            return;
        }
        let target = if down { frontier + 1 } else { frontier - 1 };
        let base = code.line_to_char(target);
        let llen = code.line_len(target);
        let pos = base + cur_col.min(llen);
        let anchor = (anchor_col != cur_col).then_some(base + anchor_col.min(llen));
        if pos != self.cursor && !self.carets.iter().any(|c| c.pos == pos) {
            self.push_primary(pos, anchor);
        }
    }

    /// Shared body of [`Editor::add_caret_above`] / [`Editor::add_caret_below`]:
    /// the new caret keeps the newest caret's column, clamped to the target
    /// line's length. No-op at the first/last line.
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
    // Naive scan: needles here are a selected word or short snippet. The
    // `needle.len() > hay.len()` guard above keeps the range bound from
    // underflowing.
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// Every non-overlapping occurrence of `needle` in `hay`, as start indices.
fn occurrences(hay: &[char], needle: &[char]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = find_from(hay, needle, from) {
        out.push(start);
        from = start + needle.len();
    }
    out
}

#[cfg(test)]
mod caret_tests {
    use crate::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("text", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    /// T575: scrolling to the cursor in a view narrower than the scroll step, or
    /// with no rows, must not underflow (the chaos monkey found both).
    #[test]
    fn focus_in_a_tiny_view_does_not_underflow() {
        use ratatui_core::layout::Rect;
        let mut e = ed(&"x".repeat(200), 150);
        for (w, h) in [(0, 0), (1, 0), (3, 1), (9, 0), (12, 2)] {
            e.focus(&Rect::new(0, 0, w, h));
        }
    }

    /// T575: replacing the whole text with something shorter leaves no cursor,
    /// selection or extra caret pointing past the new end.
    #[test]
    fn set_content_keeps_cursor_selection_and_carets_inside_shorter_text() {
        let mut e = ed("hello world", 9);
        e.set_selection_range(6, 9);
        e.add_caret_at(10);
        e.set_content("hi");
        let len = e.code_ref().len_chars();
        assert!(e.get_cursor() <= len);
        assert!(e.caret_positions().iter().all(|&p| p <= len));
        assert!(
            e.caret_selections()
                .iter()
                .all(|&(a, b)| a <= len && b <= len)
        );
        // And typing afterwards is safe.
        e.multi_insert("!");
    }

    /// T575: a caret inside another caret's selection is merged into it rather
    /// than applied at an offset the first edit already removed.
    #[test]
    fn multi_insert_merges_a_caret_inside_another_carets_selection() {
        let mut e = ed("abcdefgh", 5);
        e.set_selection_range(1, 5);
        e.add_caret_at(3);
        e.multi_insert("X");
        assert_eq!(e.code_ref().len_chars(), "aXfgh".chars().count());
    }

    #[test]
    fn add_all_occurrences_selects_every_match_of_the_word() {
        let mut e = ed("foo bar foo baz foo", 1); // cursor inside the first "foo"
        e.add_all_occurrences();
        assert_eq!(
            e.caret_selections().len(),
            3,
            "all three 'foo' matches selected"
        );
        assert!(e.has_multi_carets());
    }

    #[test]
    fn add_all_occurrences_is_noop_for_a_unique_word() {
        let mut e = ed("alpha beta gamma", 0);
        e.add_all_occurrences();
        assert!(!e.has_multi_carets(), "single match adds no extra carets");
    }

    #[test]
    fn column_select_builds_a_vertical_block_of_carets() {
        let mut e = ed("abc\ndef\nghi", 1); // line 0, col 1
        e.column_select(true);
        e.column_select(true);
        let mut pos = e.caret_positions();
        pos.sort_unstable();
        // line starts 0/4/8, column 1 -> 1/5/9
        assert_eq!(pos, vec![1, 5, 9]);
    }

    #[test]
    fn column_select_clamps_to_short_lines_and_stops_at_edges() {
        let mut e = ed("longline\nx\nlongline", 6); // col 6 on line 0
        e.column_select(true); // line 1 "x" len 1 -> clamps to col 1
        e.column_select(true); // line 2 -> col 6
        let mut pos = e.caret_positions();
        pos.sort_unstable();
        // 6 ; line1 start 9 + min(6,1)=1 ->10 ; line2 start 11 + 6 ->17
        assert_eq!(pos, vec![6, 10, 17]);
        e.column_select(true); // already at last line -> no-op
        assert_eq!(e.caret_positions().len(), 3);
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

    // T150: exercises the `checked_add_signed`/`isize::try_from` arithmetic
    // `multi_insert`/`multi_delete` were rewritten to (replacing the file's
    // former blanket `#[allow(clippy::cast_possible_wrap,
    // clippy::cast_sign_loss)]`) — no prior test in this crate or
    // `tui/tests/integration.rs` actually drove either method; the existing
    // multi-caret tests only ever check that carets were *added*.

    #[test]
    fn multi_insert_types_at_every_bare_caret_with_correct_shift() {
        let mut e = ed("abcd\nefgh\nij", 2); // col 2, line 0
        e.add_caret_below(); // second caret at line 1 col 2 (offset 7)
        e.multi_insert("X");
        assert_eq!(e.code_ref().get_content(), "abXcd\nefXgh\nij");
    }

    #[test]
    fn multi_insert_replaces_every_selected_occurrence() {
        let mut e = ed("foo bar foo baz foo", 1); // cursor inside the first "foo"
        e.add_all_occurrences(); // selects all three "foo" occurrences
        e.multi_insert("X");
        assert_eq!(e.code_ref().get_content(), "X bar X baz X");
    }

    #[test]
    fn multi_delete_forward_removes_one_char_at_every_bare_caret() {
        let mut e = ed("abcd\nefgh\nij", 2);
        e.add_caret_below(); // second caret at offset 7
        e.multi_delete(true);
        assert_eq!(e.code_ref().get_content(), "abd\nefh\nij");
    }

    #[test]
    fn multi_delete_removes_every_selected_occurrence() {
        let mut e = ed("foo bar foo baz foo", 1);
        e.add_all_occurrences();
        e.multi_delete(true);
        assert_eq!(e.code_ref().get_content(), " bar  baz ");
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

    // ----- T622: multi-cursor depth -----------------------------------------

    use super::CaretMove;

    fn sorted_positions(e: &Editor) -> Vec<usize> {
        let mut v = e.caret_positions();
        v.sort_unstable();
        v
    }

    /// Two bare carets: the primary at `a`, an extra at `b`.
    fn two(text: &str, a: usize, b: usize) -> Editor {
        let mut e = ed(text, b);
        e.add_caret_at(a);
        e
    }

    #[test]
    fn word_motions_move_every_caret() {
        let mut e = two("foo bar\nbaz qux", 0, 8);
        e.multi_move(CaretMove::WordRight, false);
        assert_eq!(sorted_positions(&e), vec![3, 11]);
        e.multi_move(CaretMove::WordLeft, false);
        assert_eq!(sorted_positions(&e), vec![0, 8]);
        // The named catalog actions route through the same per-caret move.
        e.word_right();
        assert_eq!(sorted_positions(&e), vec![3, 11]);
    }

    #[test]
    fn line_start_and_end_move_every_caret() {
        let mut e = two("  ab\n  cd", 3, 8);
        e.multi_move(CaretMove::LineEnd, false);
        assert_eq!(sorted_positions(&e), vec![4, 9]);
        e.multi_move(CaretMove::LineStart, false);
        assert_eq!(sorted_positions(&e), vec![0, 5]);
        e.end_of_line();
        assert_eq!(sorted_positions(&e), vec![4, 9]);
        e.start_of_line();
        assert_eq!(sorted_positions(&e), vec![0, 5]);
    }

    #[test]
    fn home_toggles_per_caret_between_indent_and_column_zero() {
        let mut e = two("  ab\n  cd", 4, 9);
        e.multi_move(CaretMove::Home, false);
        assert_eq!(sorted_positions(&e), vec![2, 7], "first non-blank");
        e.multi_move(CaretMove::Home, false);
        assert_eq!(sorted_positions(&e), vec![0, 5], "then column 0");
        e.multi_move(CaretMove::TextStart, false);
        assert_eq!(sorted_positions(&e), vec![2, 7]);
    }

    #[test]
    fn buffer_start_and_end_merge_the_carets() {
        let mut e = two("ab\ncd", 1, 4);
        e.multi_move(CaretMove::BufferEnd, false);
        assert_eq!(e.caret_positions(), vec![5]);
        assert!(!e.has_multi_carets(), "carets on one spot merge");
        let mut e = two("ab\ncd", 1, 4);
        e.cursor_start();
        assert_eq!(e.caret_positions(), vec![0]);
    }

    #[test]
    fn shifted_motions_extend_each_carets_selection() {
        let mut e = two("foo bar\nbaz qux", 0, 8);
        e.multi_move(CaretMove::WordRight, true);
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(0, 3), (8, 11)]);
        e.select_to_end_of_line();
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(0, 7), (8, 15)]);
    }

    #[test]
    fn split_selection_into_lines_makes_one_selection_per_line() {
        let mut e = ed("ab\ncd\nef\n", 0);
        e.set_selection_range(1, 9); // "b\ncd\nef\n"
        assert!(e.split_selection_into_lines());
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(1, 2), (3, 5), (6, 8)], "no empty last piece");
        assert_eq!(e.get_cursor(), 8, "the last piece is primary");
        e.multi_insert("X");
        assert_eq!(e.get_content(), "aX\nX\nX\n");
    }

    #[test]
    fn split_selection_into_lines_needs_a_selection() {
        let mut e = ed("ab\ncd", 1);
        assert!(!e.split_selection_into_lines());
        assert_eq!(e.caret_positions(), vec![1]);
    }

    #[test]
    fn select_matches_in_selection_selects_each_match() {
        // Byte ranges, as a regex reports them: "é" is two bytes.
        let finder = |t: &str| t.match_indices("ab").map(|(i, m)| i..i + m.len()).collect();
        let mut e = ed("ab é ab ab", 0);
        e.set_selection_range(0, 7); // "ab é ab"
        assert_eq!(e.select_matches_in_selection(finder), 2);
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(
            sels,
            vec![(0, 2), (5, 7)],
            "char offsets, inside the selection only"
        );
    }

    #[test]
    fn select_matches_without_a_selection_searches_the_buffer() {
        let finder = |t: &str| t.match_indices('x').map(|(i, _)| i..i + 1).collect();
        let mut e = ed("x y x", 2);
        assert_eq!(e.select_matches_in_selection(finder), 2);
        let none = |_: &str| Vec::new();
        assert_eq!(e.select_matches_in_selection(none), 0);
        assert_eq!(e.caret_count(), 2, "no match leaves the carets alone");
    }

    #[test]
    fn skip_occurrence_moves_the_newest_match_on() {
        let mut e = ed("foo foo foo foo", 1);
        e.add_next_occurrence(); // carets on #1 (selected) and #2 (primary)
        e.skip_occurrence(true); // #2 skipped in favor of #3
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(0, 3), (8, 11)]);
        e.skip_occurrence(false); // back: #2 is free again, so it is picked
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(0, 3), (4, 7)]);
    }

    #[test]
    fn skip_occurrence_wraps_and_passes_over_held_matches() {
        let mut e = ed("ab ab ab", 7);
        e.skip_occurrence(true); // selects the last "ab", then wraps to the first
        assert_eq!(e.caret_selections(), vec![(0, 2)]);
        e.skip_occurrence(false); // back wraps to the last
        assert_eq!(e.caret_selections(), vec![(6, 8)]);
    }

    #[test]
    fn remove_last_caret_drops_the_newest_one() {
        let mut e = ed("foo foo foo", 1);
        e.add_next_occurrence();
        e.add_next_occurrence();
        assert_eq!(e.caret_count(), 3);
        assert!(e.remove_last_caret());
        let mut sels = e.caret_selections();
        sels.sort_unstable();
        assert_eq!(sels, vec![(0, 3), (4, 7)], "the third match went");
        assert!(e.remove_last_caret());
        assert!(!e.remove_last_caret(), "one caret is left");
        assert_eq!(e.caret_selections(), vec![(0, 3)]);
    }

    #[test]
    fn remove_last_caret_keeps_add_order_across_edits() {
        let mut e = ed("ab\ncd\nef", 0);
        e.add_caret_below();
        e.add_caret_below(); // the newest is on line 2
        e.multi_insert("X");
        assert!(e.remove_last_caret());
        assert_eq!(sorted_positions(&e), vec![1, 5], "line 2's caret went");
    }

    #[test]
    fn add_caret_below_repeats_downward() {
        let mut e = ed("ab\ncd\nef", 1);
        e.add_caret_below();
        e.add_caret_below();
        assert_eq!(sorted_positions(&e), vec![1, 4, 7]);
    }

    #[test]
    fn paste_distributes_one_line_per_caret() {
        let mut e = ed("a\nb\nc", 1);
        e.add_caret_below();
        e.add_caret_below();
        e.paste_text("1\n2\n3\n");
        assert_eq!(e.get_content(), "a1\nb2\nc3");
    }

    #[test]
    fn paste_puts_the_whole_text_at_each_caret_when_counts_differ() {
        let mut e = two("a\nb", 1, 3);
        e.paste_text("xy");
        assert_eq!(e.get_content(), "axy\nbxy");
    }

    #[test]
    fn copy_texts_come_in_buffer_order() {
        let mut e = ed("foo bar foo", 9);
        e.add_all_occurrences();
        assert_eq!(e.caret_selection_texts(), vec!["foo", "foo"]);
        e.multi_cut();
        assert_eq!(e.get_content(), " bar ");
    }

    #[test]
    fn undo_reverts_a_multi_caret_edit_in_one_step() {
        let mut e = two("ab\ncd", 1, 4);
        e.multi_insert("X");
        e.multi_insert("Y");
        assert_eq!(e.get_content(), "aXYb\ncXYd");
        e.undo();
        assert_eq!(e.get_content(), "aXb\ncXd", "one undo per multi-caret edit");
        assert!(!e.has_multi_carets(), "undo collapses to the primary caret");
        e.undo();
        assert_eq!(e.get_content(), "ab\ncd");
        e.redo();
        assert_eq!(e.get_content(), "aXb\ncXd");
    }

    #[test]
    fn remote_edits_keep_extra_carets_on_their_characters() {
        use crate::code::Splice;
        let mut e = two("ab\ncd", 1, 4);
        e.apply_remote_edits(&[Splice {
            offset: 0,
            delete: 0,
            insert: ">>".to_string(),
        }]);
        assert_eq!(sorted_positions(&e), vec![3, 6]);
        e.multi_insert("X");
        assert_eq!(e.get_content(), ">>aXb\ncXd");
    }

    #[test]
    fn a_multi_caret_edit_is_logged_so_a_peer_can_replay_it() {
        let mut e = two("ab\ncd", 1, 4);
        e.start_edit_log();
        e.multi_insert("X");
        let mut mirror: Vec<char> = "ab\ncd".chars().collect();
        for s in e.take_edit_log() {
            mirror.splice(s.offset..s.offset + s.delete, s.insert.chars());
        }
        assert_eq!(mirror.into_iter().collect::<String>(), e.get_content());
    }
}
