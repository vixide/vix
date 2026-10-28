//! Property tests for the lane-assignment algorithm over random commit DAGs.

use proptest::prelude::*;
use vix_git_graph::{Commit, assign_lanes};

/// Build `n` commits `0..n` (newest first) where each commit's parents are
/// chosen among later commits, so the list is a valid topological order.
fn dag() -> impl Strategy<Value = Vec<Commit>> {
    (1usize..40)
        .prop_flat_map(|n| {
            let parents = (0..n)
                .map(|i| proptest::collection::vec(i + 1..n.max(i + 2), 0..3))
                .collect::<Vec<_>>();
            (Just(n), parents)
        })
        .prop_map(|(n, parents)| {
            (0..n)
                .map(|i| {
                    let mut ps: Vec<usize> =
                        parents[i].iter().copied().filter(|p| *p < n).collect();
                    ps.sort_unstable();
                    ps.dedup();
                    Commit {
                        sha: format!("{i:04x}"),
                        abbrev: format!("{i:04x}"),
                        parents: ps.iter().map(|p| format!("{p:04x}")).collect(),
                        author: String::new(),
                        date: String::new(),
                        subject: String::new(),
                        refs: Vec::new(),
                    }
                })
                .collect()
        })
}

proptest! {
    #[test]
    fn rows_are_consistent(commits in dag()) {
        let rows = assign_lanes(&commits);
        prop_assert_eq!(rows.len(), commits.len());
        // The lanes each hash is waiting in, simulated independently.
        let mut waiting: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::default();
        for (c, r) in commits.iter().zip(&rows) {
            let mut expect = waiting.remove(&c.sha).unwrap_or_default();
            expect.sort_unstable();
            prop_assert_eq!(&r.incoming, &expect);
            if let Some(first) = expect.first() {
                prop_assert_eq!(r.lane, *first);
            }
            for l in r.incoming.iter().chain(&r.through).chain(&r.outgoing) {
                prop_assert!(*l < r.width);
            }
            prop_assert!(r.lane < r.width);
            prop_assert!(!r.through.contains(&r.lane));
            prop_assert_eq!(r.outgoing.len(), c.parents.len());
            for (p, o) in c.parents.iter().zip(&r.outgoing) {
                let lanes = waiting.entry(p.clone()).or_default();
                if !lanes.contains(o) {
                    lanes.push(*o);
                }
            }
            prop_assert_eq!(r.glyphs(r.width).chars().count(), r.width * 2);
        }
    }

    #[test]
    fn a_prefix_gives_the_same_rows(commits in dag(), cut in 0usize..40) {
        let cut = cut.min(commits.len());
        let full = assign_lanes(&commits);
        let part = assign_lanes(&commits[..cut]);
        prop_assert_eq!(&full[..cut], &part[..]);
    }
}
