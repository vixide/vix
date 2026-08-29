//! Command run history: push-with-dedup policies.
//!
//! This models only the push/dedup *policy* — three ways to handle a push
//! that duplicates the history's current last entry. Out of scope: which
//! history a given project root maps to across worktrees/checkouts of the
//! "same" repository (a host/persistence-layer concern about history
//! *scope*, not the push operation), and any capacity limit — this crate
//! imposes none; a host that wants a bounded history can truncate the `Vec`
//! itself.

/// How [`push_history`] handles a `cmd` equal to the history's current last
/// entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    /// Always push, even if it duplicates the last entry.
    KeepAll,
    /// Skip the push if `cmd` equals the last entry.
    IgnoreConsecutive,
    /// If `cmd` equals the last entry, replace it in place
    /// (net effect: the entry moves to "most recent" without growing the
    /// list); otherwise push normally.
    EraseLast,
}

/// Push `cmd` onto `history` (most-recent-**last**) applying `policy`.
pub fn push_history(history: &mut Vec<String>, cmd: String, policy: DuplicatePolicy) {
    match policy {
        DuplicatePolicy::KeepAll => history.push(cmd),
        DuplicatePolicy::IgnoreConsecutive => {
            if history.last() != Some(&cmd) {
                history.push(cmd);
            }
        }
        DuplicatePolicy::EraseLast => {
            if let Some(last) = history.last_mut().filter(|last| **last == cmd) {
                *last = cmd;
            } else {
                history.push(cmd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn keep_all_always_pushes() {
        let mut h = vec!["a".to_string()];
        push_history(&mut h, "a".into(), DuplicatePolicy::KeepAll);
        assert_eq!(h, vec!["a", "a"]);
    }

    #[test]
    fn ignore_consecutive_skips_matching_last() {
        let mut h = vec!["a".to_string()];
        push_history(&mut h, "a".into(), DuplicatePolicy::IgnoreConsecutive);
        assert_eq!(h, vec!["a"]);
        push_history(&mut h, "b".into(), DuplicatePolicy::IgnoreConsecutive);
        assert_eq!(h, vec!["a", "b"]);
    }

    #[test]
    fn ignore_consecutive_allows_non_consecutive_repeat() {
        let mut h = vec!["a".to_string(), "b".to_string()];
        push_history(&mut h, "a".into(), DuplicatePolicy::IgnoreConsecutive);
        assert_eq!(h, vec!["a", "b", "a"]);
    }

    #[test]
    fn erase_last_replaces_matching_last_in_place() {
        let mut h = vec!["a".to_string(), "b".to_string()];
        push_history(&mut h, "b".into(), DuplicatePolicy::EraseLast);
        assert_eq!(h, vec!["a", "b"]);
    }

    #[test]
    fn erase_last_pushes_when_not_matching() {
        let mut h = vec!["a".to_string()];
        push_history(&mut h, "b".into(), DuplicatePolicy::EraseLast);
        assert_eq!(h, vec!["a", "b"]);
    }

    #[test]
    fn push_on_empty_history_always_pushes() {
        for policy in [
            DuplicatePolicy::KeepAll,
            DuplicatePolicy::IgnoreConsecutive,
            DuplicatePolicy::EraseLast,
        ] {
            let mut h: Vec<String> = Vec::new();
            push_history(&mut h, "a".into(), policy);
            assert_eq!(h, vec!["a"]);
        }
    }

    proptest! {
        #[test]
        fn ignore_consecutive_never_leaves_adjacent_duplicates(
            cmds in proptest::collection::vec("[a-c]", 0..20)
        ) {
            let mut h: Vec<String> = Vec::new();
            for c in cmds {
                push_history(&mut h, c, DuplicatePolicy::IgnoreConsecutive);
            }
            for w in h.windows(2) {
                prop_assert_ne!(&w[0], &w[1]);
            }
        }

        #[test]
        fn push_history_never_shrinks(
            cmds in proptest::collection::vec("[a-c]", 0..20),
            policy_idx in 0..3usize,
        ) {
            let policy = [
                DuplicatePolicy::KeepAll,
                DuplicatePolicy::IgnoreConsecutive,
                DuplicatePolicy::EraseLast,
            ][policy_idx];
            let mut h: Vec<String> = Vec::new();
            for c in cmds {
                let before = h.len();
                push_history(&mut h, c, policy);
                prop_assert!(h.len() >= before);
            }
        }
    }
}
