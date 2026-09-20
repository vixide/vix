//! Push an item onto a `Vec`, evicting the oldest entry first if that would
//! grow it past a cap.
//!
//! Extracted (Run H, T525) after this exact 4-line "push, then trim from the
//! front if over the cap" idiom (with an identical `HISTORY_CAP` constant)
//! was found hand-rolled independently in the undo-stack of every
//! `vix-edit-*` crate: `vix-edit-bytes`, `vix-edit-sql`, `vix-edit-table`,
//! `vix-edit-outline`, `vix-edit-value`. Named for the general operation it
//! performs, not for undo specifically — nothing here is undo-history-aware
//! (that's each crate's own `Snapshot`/`restore` logic) — so it doesn't
//! collide in name or purpose with `vix-undo-store` (persistent, per-file
//! undo history saved to disk, a genuinely different feature).

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Push `item` onto `stack`, evicting the oldest entry (index `0`) first if
/// that would grow `stack` past `cap` entries. `cap == 0` degenerates to "the
/// stack never holds anything" rather than panicking or growing unbounded.
pub fn push_capped<T>(stack: &mut Vec<T>, item: T, cap: usize) {
    stack.push(item);
    if stack.len() > cap {
        stack.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_within_cap_evicting_the_oldest_first() {
        let mut stack = Vec::new();
        for i in 0..5 {
            push_capped(&mut stack, i, 3);
        }
        // The three most recent survive, oldest-first order preserved.
        assert_eq!(stack, vec![2, 3, 4]);
    }

    #[test]
    fn evicts_at_most_one_entry_per_call() {
        // Matches every original call site's own `if` (not `while`) exactly:
        // one push evicts at most the single oldest entry, on the assumption
        // (true for every current caller, which always calls this once per
        // push already at-or-under cap beforehand) that the stack was never
        // more than one entry over cap to begin with. A stack that started
        // further over cap than that stays over cap after one call -- this
        // is not a general "enforce the invariant no matter what" clamp.
        let mut stack = vec![1, 2, 3, 4, 5];
        push_capped(&mut stack, 6, 3);
        assert_eq!(stack, vec![2, 3, 4, 5, 6]);
    }

    #[test]
    fn cap_of_zero_never_retains_anything() {
        let mut stack = Vec::new();
        push_capped(&mut stack, "x", 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn under_cap_just_appends() {
        let mut stack = vec!["a"];
        push_capped(&mut stack, "b", 5);
        assert_eq!(stack, vec!["a", "b"]);
    }
}
