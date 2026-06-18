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
            let sep = (start + 1..lines.len()).find(|&j| lines[j].trim_end().starts_with("======="));
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
}
