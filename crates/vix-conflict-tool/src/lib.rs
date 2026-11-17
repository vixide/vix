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
//!
//! Two scanners share one marker grammar: [`find`] returns a single block
//! (the one under, or next after, the cursor) and [`find_all`] returns every
//! block, feeding the [`List`] state of the Conflict overlay (T556).
//!
//! # Marker grammar and limits
//!
//! A marker is recognised by its line *starting with* seven `<`, `=` or `>`
//! (trailing whitespace trimmed first), so labels after the marker are
//! ignored. Nesting is not supported. An unterminated block (missing `=======`
//! or `>>>>>>>`) ends the scan: that block and anything after it are not
//! reported. `diff3`-style `|||||||` base sections are not recognised; the
//! base lines end up inside [`Conflict::ours`].
//!
//! Spec: `crates/vix-conflict-tool/spec/index.md`.

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

/// A located conflict block: the half-open line range `start..end` it spans
/// (both marker lines included) and the two sides' text (with line endings).
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
        // Each side keeps its own line endings, so `Both` is a plain concat: a
        // final line lacking a newline would run into the next side's first line.
        match how {
            Resolution::Ours => self.ours.clone(),
            Resolution::Theirs => self.theirs.clone(),
            Resolution::Both => format!("{}{}", self.ours, self.theirs),
        }
    }
}

/// Find every conflict block in `text`, in source order (T556, the Conflict
/// overlay). [`find`]'s single-block scan, generalized.
///
/// Scanning stops at the first unterminated block (see the module docs).
///
/// # Examples
///
/// ```
/// let text = "a\n<<<<<<< HEAD\nmine\n=======\nyours\n>>>>>>> b\nz\n";
/// let all = vix_conflict_tool::find_all(text);
/// assert_eq!(all.len(), 1);
/// assert_eq!((all[0].start, all[0].end), (1, 6));
/// assert_eq!(all[0].ours, "mine\n");
/// ```
#[must_use]
pub fn find_all(text: &str) -> Vec<Conflict> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    blocks(&lines).collect()
}

/// Whether `line` is a conflict marker beginning with `marker`.
fn is_marker(line: &str, marker: &str) -> bool {
    line.trim_end().starts_with(marker)
}

/// The first complete conflict block starting at or after line `from`, or
/// `None` when there is none (or the next block is unterminated).
fn next_block(lines: &[&str], from: usize) -> Option<Conflict> {
    let start = (from..lines.len()).find(|&j| is_marker(lines[j], "<<<<<<<"))?;
    // Scanning from the marker line itself is the same as from the line after
    // it: a `<<<<<<<` line can't also start with `=======` (nor `=======` with
    // `>>>>>>>`).
    let sep = (start..lines.len()).find(|&j| is_marker(lines[j], "======="))?;
    let endm = (sep..lines.len()).find(|&j| is_marker(lines[j], ">>>>>>>"))?;
    Some(Conflict {
        start,
        end: endm + 1,
        ours: lines[start + 1..sep].concat(),
        theirs: lines[sep + 1..endm].concat(),
    })
}

/// Every complete conflict block in `lines`, in source order, stopping at the
/// first unterminated one. Each block's `end` is past its `start`, so the scan
/// always advances.
fn blocks<'a>(lines: &'a [&'a str]) -> impl Iterator<Item = Conflict> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        let conflict = next_block(lines, from)?;
        from = conflict.end;
        Some(conflict)
    })
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
/// resolves the next conflict). Returns `None` when there is no such block
/// (including when every later block is unterminated).
///
/// # Examples
///
/// ```
/// let text = "<<<<<<< HEAD\nmine\n=======\nyours\n>>>>>>> b\n";
/// let c = vix_conflict_tool::find(text, 2).unwrap();
/// assert_eq!(c.resolved(vix_conflict_tool::Resolution::Theirs), "yours\n");
/// ```
#[must_use]
pub fn find(text: &str, at: usize) -> Option<Conflict> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    // Blocks are in order and disjoint, so the first one ending after `at`
    // either contains `at` or is the first block beginning after it (the
    // "next conflict" fallback).
    blocks(&lines).find(|c| at < c.end)
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
        assert_eq!(find_all("plain\ntext\n"), [] as [Conflict; 0]);
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

    #[test]
    fn find_all_reports_ranges_and_both_sides() {
        let all = find_all(TWO_CONFLICTS);
        assert_eq!((all[0].start, all[0].end), (1, 6));
        assert_eq!(all[0].theirs, "theirs1\n");
        assert_eq!((all[1].start, all[1].end), (7, 12));
        assert_eq!(all[1].theirs, "theirs2\n");
    }

    #[test]
    fn find_all_handles_a_block_on_the_first_line() {
        let all = find_all("<<<<<<< HEAD\nmine\n=======\nyours\n>>>>>>> b\n");
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].start, all[0].end), (0, 5));
        assert_eq!(all[0].ours, "mine\n");
        assert_eq!(all[0].theirs, "yours\n");
    }

    #[test]
    fn separator_before_the_block_is_ignored() {
        // A stray `=======` above `<<<<<<<` is not the block's separator.
        let all = find_all("=======\n<<<<<<< HEAD\nmine\n=======\nyours\n>>>>>>> b\n");
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].start, all[0].end), (1, 6));
        assert_eq!(all[0].ours, "mine\n");
    }

    #[test]
    fn end_marker_inside_ours_ends_nothing() {
        // A `>>>>>>>` line above `=======` is part of ours, not the block end.
        let all = find_all("<<<<<<< HEAD\n>>>>>>> fake\n=======\nyours\n>>>>>>> b\n");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].end, 5);
        assert_eq!(all[0].ours, ">>>>>>> fake\n");
        assert_eq!(all[0].theirs, "yours\n");
    }

    #[test]
    fn scanning_resumes_after_the_end_marker() {
        // The start marker inside theirs belongs to the first block; the
        // following `=======`/`>>>>>>>` lines are not a second block.
        let text = "<<<<<<< A\no\n=======\n<<<<<<< x\n>>>>>>> B\n=======\nt\n>>>>>>> C\n";
        let all = find_all(text);
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].start, all[0].end), (0, 5));
        assert_eq!(all[0].theirs, "<<<<<<< x\n");
        assert_eq!(find(text, 6), None);
    }

    #[test]
    fn unterminated_block_ends_the_scan() {
        assert_eq!(find_all("<<<<<<< HEAD\nmine\n"), [] as [Conflict; 0]);
        assert_eq!(
            find_all("<<<<<<< HEAD\nmine\n=======\nyours\n"),
            [] as [Conflict; 0]
        );
    }

    #[test]
    fn find_picks_the_block_containing_the_cursor() {
        // Cursor on the first block's last line (`>>>>>>>`) selects it.
        assert_eq!(find(TWO_CONFLICTS, 5).unwrap().start, 1);
        // Cursor on the second block's `<<<<<<<` line selects that one.
        assert_eq!(find(TWO_CONFLICTS, 7).unwrap().start, 7);
        // Inside the second block.
        assert_eq!(find(TWO_CONFLICTS, 9).unwrap().start, 7);
    }

    #[test]
    fn find_falls_back_to_the_next_block_only() {
        // Between the blocks: the next one, not the previous one.
        assert_eq!(find(TWO_CONFLICTS, 6).unwrap().start, 7);
        // Past the last block: nothing.
        assert_eq!(find(TWO_CONFLICTS, 12), None);
        assert_eq!(find(SAMPLE, 7), None);
    }

    #[test]
    fn list_len_and_is_empty() {
        let list = List::new(find_all(TWO_CONFLICTS));
        assert_eq!(list.len(), 2);
        assert!(!list.is_empty());
        let empty = List::new(Vec::new());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    const FIVE_CONFLICTS: &str = "<<<<<<<\na\n=======\nb\n>>>>>>>\n\
        <<<<<<<\na\n=======\nb\n>>>>>>>\n<<<<<<<\na\n=======\nb\n>>>>>>>\n\
        <<<<<<<\na\n=======\nb\n>>>>>>>\n<<<<<<<\na\n=======\nb\n>>>>>>>\n";

    #[test]
    fn list_up_moves_one_row() {
        let mut list = List::new(find_all(TWO_CONFLICTS));
        list.selected = 1;
        list.up();
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn list_paging_moves_by_a_page_and_clamps() {
        let mut list = List::new(find_all(FIVE_CONFLICTS));
        assert_eq!(list.len(), 5);
        list.page_down(2);
        assert_eq!(list.selected, 2);
        list.page_down(10);
        assert_eq!(list.selected, 4);
        list.page_up(3);
        assert_eq!(list.selected, 1);
        list.page_up(10);
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn list_select_index_accepts_only_real_rows() {
        let mut list = List::new(find_all(TWO_CONFLICTS));
        assert!(list.select_index(1));
        assert_eq!(list.selected, 1);
        assert!(!list.select_index(2));
        assert_eq!(list.selected, 1, "an out-of-range click changes nothing");
    }

    #[test]
    fn list_ensure_visible_scrolls_to_the_selection() {
        let mut list = List::new(find_all(FIVE_CONFLICTS));
        list.selected = 4;
        list.ensure_visible(2);
        assert_eq!(list.scroll, 3);
        list.selected = 0;
        list.ensure_visible(2);
        assert_eq!(list.scroll, 0);
    }
}
