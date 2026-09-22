//! Parse Git merge-conflict markers and resolve the conflict under the cursor.
//!
//! A conflict block looks like:
//! ```text
//! <<<<<<< HEAD
//! our lines
//! =======
//! their lines
//! >>>>>>> other-branch
//! ```
//! [`find`] locates the block containing a given line; the host then replaces
//! that line range with the chosen side (ours / theirs / both) via
//! [`Resolution`].

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Which side of a conflict to keep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// Keep the lines above `=======` (HEAD / current).
    Ours,
    /// Keep the lines below `=======` (incoming).
    Theirs,
    /// Keep both, ours first.
    Both,
}

/// A located conflict block: the inclusive-exclusive line range it spans and the
/// two sides' text (with line endings).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Conflict {
    /// First line (0-based) — the `<<<<<<<` marker.
    pub start: usize,
    /// One past the last line — after the `>>>>>>>` marker.
    pub end: usize,
    /// Lines between `<<<<<<<` and `=======` (ours), with endings.
    pub ours: String,
    /// Lines between `=======` and `>>>>>>>` (theirs), with endings.
    pub theirs: String,
}

impl Conflict {
    /// The replacement text for a given resolution.
    #[must_use]
    pub fn resolved(&self, how: Resolution) -> String {
        match how {
            Resolution::Ours => self.ours.clone(),
            Resolution::Theirs => self.theirs.clone(),
            Resolution::Both => format!("{}{}", self.ours, self.theirs),
        }
    }
}

/// Find every conflict block in `text`, in source order (T556, the Conflict
/// overlay). [`find`]'s single-block scan, generalized.
#[must_use]
pub fn find_all(text: &str) -> Vec<Conflict> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end().starts_with("<<<<<<<") {
            let start = i;
            let sep =
                (start + 1..lines.len()).find(|&j| lines[j].trim_end().starts_with("======="));
            let Some(sep) = sep else { break };
            let endm = (sep + 1..lines.len()).find(|&j| lines[j].trim_end().starts_with(">>>>>>>"));
            let Some(endm) = endm else { break };
            out.push(Conflict {
                start,
                end: endm + 1,
                ours: lines[start + 1..sep].concat(),
                theirs: lines[sep + 1..endm].concat(),
            });
            i = endm + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Every conflict in a buffer, plus selection/scroll state for the Conflict
/// overlay (T556) — the same shape `vix-outline-panel`'s `Outline` uses, on
/// the shared `vix-list-state` navigation helpers.
pub struct List {
    /// Conflict blocks in source order.
    pub entries: Vec<Conflict>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`List::ensure_visible`].
    pub scroll: usize,
}

impl List {
    /// Build a list from `entries` (typically [`find_all`]'s result),
    /// selecting the first row.
    #[must_use]
    pub fn new(entries: Vec<Conflict>) -> Self {
        List {
            entries,
            selected: 0,
            scroll: 0,
        }
    }

    /// Number of conflicts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no conflicts left.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.entries.len());
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.entries.len());
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.entries.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.entries.len());
    }

    /// Re-scan `text` for conflicts and replace `entries` with the fresh
    /// result, clamping `selected` to stay in range (T556: after a resolve,
    /// the just-resolved block's line range no longer exists, so the list
    /// can't be patched in place -- it has to be rebuilt from the buffer's
    /// new content).
    pub fn refresh(&mut self, text: &str) {
        self.entries = find_all(text);
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }
}

/// Find the conflict block containing line `at` (0-based). When `at` is not
/// inside a block, returns the first block at or after it (so a single action
/// resolves the next conflict).
#[must_use]
pub fn find(text: &str, at: usize) -> Option<Conflict> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut best: Option<Conflict> = None;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end().starts_with("<<<<<<<") {
            let start = i;
            let sep =
                (start + 1..lines.len()).find(|&j| lines[j].trim_end().starts_with("======="));
            let Some(sep) = sep else { break };
            let endm = (sep + 1..lines.len()).find(|&j| lines[j].trim_end().starts_with(">>>>>>>"));
            let Some(endm) = endm else { break };
            let conflict = Conflict {
                start,
                end: endm + 1,
                ours: lines[start + 1..sep].concat(),
                theirs: lines[sep + 1..endm].concat(),
            };
            // The block containing `at` wins; otherwise remember the first block
            // beginning at/after `at` as the fallback ("next conflict").
            if at >= conflict.start && at < conflict.end {
                return Some(conflict);
            }
            if best.is_none() && conflict.start >= at {
                best = Some(conflict);
            }
            i = endm + 1;
        } else {
            i += 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "a\n<<<<<<< HEAD\nours1\nours2\n=======\ntheirs1\n>>>>>>> branch\nz\n";

    #[test]
    fn finds_block_under_cursor() {
        let c = find(SAMPLE, 2).unwrap();
        assert_eq!((c.start, c.end), (1, 7));
        assert_eq!(c.ours, "ours1\nours2\n");
        assert_eq!(c.theirs, "theirs1\n");
    }

    #[test]
    fn resolutions() {
        let c = find(SAMPLE, 2).unwrap();
        assert_eq!(c.resolved(Resolution::Ours), "ours1\nours2\n");
        assert_eq!(c.resolved(Resolution::Theirs), "theirs1\n");
        assert_eq!(c.resolved(Resolution::Both), "ours1\nours2\ntheirs1\n");
    }

    #[test]
    fn falls_back_to_next_conflict() {
        // Cursor on line 0 (before the block) still finds it.
        assert!(find(SAMPLE, 0).is_some());
    }

    #[test]
    fn none_when_no_conflict() {
        assert!(find("plain\ntext\n", 0).is_none());
    }

    const TWO_CONFLICTS: &str = "a\n<<<<<<< HEAD\nours1\n=======\ntheirs1\n>>>>>>> branch\nmid\n\
        <<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> branch\nz\n";

    #[test]
    fn find_all_collects_every_block_in_order() {
        let all = find_all(TWO_CONFLICTS);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].ours, "ours1\n");
        assert_eq!(all[1].ours, "ours2\n");
    }

    #[test]
    fn find_all_is_empty_with_no_conflicts() {
        assert!(find_all("plain\ntext\n").is_empty());
    }

    #[test]
    fn list_navigation_stops_at_the_ends() {
        let mut list = List::new(find_all(TWO_CONFLICTS));
        assert_eq!(list.selected, 0);
        list.up(); // already at top
        assert_eq!(list.selected, 0);
        list.down();
        assert_eq!(list.selected, 1);
        list.down(); // already at bottom
        assert_eq!(list.selected, 1);
    }

    #[test]
    fn list_refresh_rebuilds_and_clamps_selection() {
        let mut list = List::new(find_all(TWO_CONFLICTS));
        list.selected = 1; // the second (now-about-to-vanish) conflict
        // Resolve the second conflict away, leaving only one.
        list.refresh("a\n<<<<<<< HEAD\nours1\n=======\ntheirs1\n>>>>>>> branch\nmid\nours2\nz\n");
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.selected, 0, "clamped back into range");
    }
}
