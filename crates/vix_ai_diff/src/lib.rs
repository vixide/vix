//! Reviewable diff for AI text transforms (Annotate / Improve).
//!
//! Rather than overwrite the buffer the moment the assistant replies, the host
//! builds a [`Review`] of the proposed change and lets the user accept or reject
//! it hunk by hunk. Each [`Seg`]ment is either unchanged context or a change the
//! user can toggle; [`Review::result`] reconstructs the final text from the
//! accepted choices. Pure data over `split_inclusive('\n')` lines, so the result
//! is an exact reconstruction (no line-ending guessing) and unit-testable.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One segment of the proposed diff.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Seg {
    /// Lines unchanged between the old and new text (kept verbatim).
    Equal(Vec<String>),
    /// A changed region: the old lines, the proposed new lines, and whether the
    /// user currently accepts the new version.
    Change {
        /// Old lines (empty for a pure addition).
        old: Vec<String>,
        /// Proposed new lines (empty for a pure deletion).
        new: Vec<String>,
        /// Whether the new version is accepted (default `true`).
        accepted: bool,
    },
}

/// A proposed AI edit, segmented into context and toggleable changes.
pub struct Review {
    /// Segments in document order.
    pub segs: Vec<Seg>,
    /// Index into [`Self::change_positions`] of the highlighted change.
    pub selected: usize,
}

impl Review {
    /// Build a review of the change from `old` to `new`, or `None` when they are
    /// identical (nothing to review).
    #[must_use]
    pub fn from_texts(old: &str, new: &str) -> Option<Review> {
        use similar::{Algorithm, DiffOp, TextDiff};
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Myers)
            .diff_lines(old, new);
        let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
        let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
        let take = |src: &[&str], i: usize, n: usize| -> Vec<String> {
            src.get(i..i + n)
                .unwrap_or_default()
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        };
        let mut segs: Vec<Seg> = Vec::new();
        let mut any = false;
        for op in diff.ops() {
            match *op {
                DiffOp::Equal { old_index, len, .. } => {
                    segs.push(Seg::Equal(take(&old_lines, old_index, len)));
                }
                DiffOp::Insert {
                    old_index: _,
                    new_index,
                    new_len,
                } => {
                    any = true;
                    push_change(&mut segs, Vec::new(), take(&new_lines, new_index, new_len));
                }
                DiffOp::Delete {
                    old_index,
                    old_len,
                    new_index: _,
                } => {
                    any = true;
                    push_change(&mut segs, take(&old_lines, old_index, old_len), Vec::new());
                }
                DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    any = true;
                    push_change(
                        &mut segs,
                        take(&old_lines, old_index, old_len),
                        take(&new_lines, new_index, new_len),
                    );
                }
            }
        }
        if !any {
            return None;
        }
        Some(Review { segs, selected: 0 })
    }

    /// Document-order indices of the change segments.
    #[must_use]
    pub fn change_positions(&self) -> Vec<usize> {
        self.segs
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s, Seg::Change { .. }))
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of change hunks.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.segs
            .iter()
            .filter(|s| matches!(s, Seg::Change { .. }))
            .count()
    }

    /// Move the highlight to the next change (saturating).
    pub fn next(&mut self) {
        let n = self.change_count();
        if n > 0 && self.selected + 1 < n {
            self.selected += 1;
        }
    }

    /// Move the highlight to the previous change (saturating).
    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Toggle acceptance of the highlighted change.
    pub fn toggle(&mut self) {
        let positions = self.change_positions();
        if let Some(&idx) = positions.get(self.selected)
            && let Some(Seg::Change { accepted, .. }) = self.segs.get_mut(idx)
        {
            *accepted = !*accepted;
        }
    }

    /// Accept (`true`) or reject (`false`) every change.
    pub fn set_all(&mut self, accept: bool) {
        for seg in &mut self.segs {
            if let Seg::Change { accepted, .. } = seg {
                *accepted = accept;
            }
        }
    }

    /// How many changes are currently accepted.
    #[must_use]
    pub fn accepted_count(&self) -> usize {
        self.segs
            .iter()
            .filter(|s| matches!(s, Seg::Change { accepted: true, .. }))
            .count()
    }

    /// The final text: context kept, each change applied if accepted else reverted.
    #[must_use]
    pub fn result(&self) -> String {
        let mut out = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Equal(lines) => out.extend(lines.iter().map(String::as_str)),
                Seg::Change { old, new, accepted } => {
                    let pick = if *accepted { new } else { old };
                    out.extend(pick.iter().map(String::as_str));
                }
            }
        }
        out
    }
}

/// Append a change segment, merging it into a trailing change so an adjacent
/// delete+insert reads as one reviewable hunk.
fn push_change(segs: &mut Vec<Seg>, mut old: Vec<String>, mut new: Vec<String>) {
    if let Some(Seg::Change {
        old: po, new: pn, ..
    }) = segs.last_mut()
    {
        po.append(&mut old);
        pn.append(&mut new);
    } else {
        segs.push(Seg::Change {
            old,
            new,
            accepted: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_no_review() {
        assert!(Review::from_texts("a\nb\n", "a\nb\n").is_none());
    }

    #[test]
    fn accept_keeps_new_reject_keeps_old() {
        let mut r = Review::from_texts("one\ntwo\nthree\n", "one\nTWO\nthree\n").unwrap();
        assert_eq!(r.change_count(), 1);
        assert_eq!(r.result(), "one\nTWO\nthree\n");
        r.toggle();
        assert_eq!(r.result(), "one\ntwo\nthree\n");
        assert_eq!(r.accepted_count(), 0);
    }

    #[test]
    fn per_hunk_selection() {
        let mut r = Review::from_texts("a\nb\nc\nd\n", "A\nb\nC\nd\n").unwrap();
        assert_eq!(r.change_count(), 2);
        // Reject only the second hunk.
        r.next();
        r.toggle();
        assert_eq!(r.result(), "A\nb\nc\nd\n");
    }

    #[test]
    fn set_all_toggles_everything() {
        let mut r = Review::from_texts("a\nb\n", "A\nB\n").unwrap();
        r.set_all(false);
        assert_eq!(r.result(), "a\nb\n");
        r.set_all(true);
        assert_eq!(r.result(), "A\nB\n");
    }
}
