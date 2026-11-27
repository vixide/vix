//! Property tests for the undo tree (T831): for any sequence of committed
//! insert/remove batches, undoing everything restores the original text,
//! redoing everything restores the final text, every intermediate state is
//! visited in order, and `History::squash_since` (T837) changes only how
//! many steps that takes, never the text any step lands on.

use proptest::prelude::*;
use vix_editor_core::code::Code;

/// One primitive edit: insert `text` at `at`, or remove from `at` for
/// `span` characters (both reduced modulo the buffer length when applied).
#[derive(Debug, Clone)]
enum Op {
    Insert { at: usize, text: String },
    Remove { at: usize, span: usize },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), "(?s).{0,4}").prop_map(|(at, text)| Op::Insert { at, text }),
        (any::<usize>(), 0usize..6).prop_map(|(at, span)| Op::Remove { at, span }),
    ]
}

/// A batch is one committed transaction: one to three primitive edits.
fn batches(max: usize) -> impl Strategy<Value = Vec<Vec<Op>>> {
    batches_from(0, max)
}

/// Between `min` and `max` (exclusive) batches.
fn batches_from(min: usize, max: usize) -> impl Strategy<Value = Vec<Vec<Op>>> {
    prop::collection::vec(prop::collection::vec(op(), 1..4), min..max)
}

/// Apply `ops` as one transaction and commit it.
fn commit(code: &mut Code, ops: &[Op]) {
    code.tx();
    for op in ops {
        let len = code.len_chars();
        match op {
            Op::Insert { at, text } => code.insert(at % (len + 1), text),
            Op::Remove { at, span } => {
                let from = at % (len + 1);
                let to = from + span % (len - from + 1);
                code.remove(from, to);
            }
        }
    }
    code.commit();
}

/// A fresh buffer over `initial` with no grammar.
fn buffer(initial: &str) -> Result<Code, TestCaseError> {
    Code::new(initial, "", None).map_err(|e| TestCaseError::fail(e.to_string()))
}

/// Commit every batch, returning the text after each one (index 0 is the
/// text before any) and the history state each one left (index 0 is the
/// state before any).
fn run(code: &mut Code, batches: &[Vec<Op>]) -> (Vec<String>, Vec<usize>) {
    let mut texts = vec![code.get_content()];
    let mut seqs = vec![code.history().current()];
    for b in batches {
        commit(code, b);
        texts.push(code.get_content());
        seqs.push(code.history().current());
    }
    (texts, seqs)
}

/// Undo until the root, returning the text after each step.
fn undo_all(code: &mut Code) -> Vec<String> {
    let mut out = Vec::new();
    while code.undo().is_some() {
        out.push(code.get_content());
    }
    out
}

/// Redo until the tip, returning the text after each step.
fn redo_all(code: &mut Code) -> Vec<String> {
    let mut out = Vec::new();
    while code.redo().is_some() {
        out.push(code.get_content());
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Undo walks back through every committed state to the original text
    /// and redo walks forward through them again to the final text.
    #[test]
    fn undo_and_redo_revisit_every_state(initial in "(?s).{0,12}", bs in batches(10)) {
        let mut code = buffer(&initial)?;
        let (texts, _) = run(&mut code, &bs);
        let back: Vec<String> = texts.iter().rev().skip(1).cloned().collect();
        prop_assert_eq!(undo_all(&mut code), back);
        prop_assert_eq!(code.get_content(), initial);
        let forward: Vec<String> = texts.iter().skip(1).cloned().collect();
        prop_assert_eq!(redo_all(&mut code), forward);
        prop_assert_eq!(code.get_content(), texts.last().cloned().unwrap_or_default());
    }

    /// Editing after some undos starts a branch: undoing everything still
    /// reaches the original text, and redoing follows the new branch to its
    /// final text.
    #[test]
    fn a_branch_after_undo_still_undoes_to_the_original(
        initial in "(?s).{0,12}",
        first in batches(6),
        undos in 0usize..6,
        second in batches_from(1, 6),
    ) {
        let mut code = buffer(&initial)?;
        run(&mut code, &first);
        for _ in 0..undos {
            code.undo();
        }
        let (texts, _) = run(&mut code, &second);
        let last = code.get_content();
        prop_assert_eq!(texts.last(), Some(&last));
        undo_all(&mut code);
        prop_assert_eq!(code.get_content(), initial);
        redo_all(&mut code);
        prop_assert_eq!(code.get_content(), last);
    }

    /// Squashing the states after one point into a single step (as the Vi
    /// keymap does for a change plus its Insert-mode text) skips the folded
    /// intermediate states and nothing else: every remaining step lands on
    /// the same text as without the squash, and undo-all / redo-all end on
    /// the same text.
    #[test]
    fn squash_since_keeps_undo_and_redo_results(
        initial in "(?s).{0,12}",
        bs in batches(10),
        cut in (0usize..=10, 0usize..=10),
    ) {
        let n = bs.len();
        let (k, m) = (cut.0.min(n), cut.1.min(n));
        let (k, m) = (k.min(m), k.max(m));
        // Unsquashed reference.
        let mut plain = buffer(&initial)?;
        let (texts, _) = run(&mut plain, &bs);
        // Squashed: commit up to m, fold the states after k, commit the rest.
        let mut squashed = buffer(&initial)?;
        let (_, seqs) = run(&mut squashed, &bs[..m]);
        let folded = squashed.squash_history_since(seqs[k]);
        prop_assert_eq!(folded, m - k >= 2, "squash refuses fewer than two states");
        run(&mut squashed, &bs[m..]);
        prop_assert_eq!(squashed.get_content(), plain.get_content());
        // The states undo visits: n down to m, then k (m's predecessors
        // after k are gone when folded), then on down to 0.
        let expect: Vec<String> = (0..n)
            .rev()
            .filter(|&i| !folded || i <= k || i >= m)
            .map(|i| texts[i].clone())
            .collect();
        prop_assert_eq!(undo_all(&mut squashed), expect);
        prop_assert_eq!(&squashed.get_content(), &initial);
        undo_all(&mut plain);
        prop_assert_eq!(plain.get_content(), initial);
        redo_all(&mut squashed);
        redo_all(&mut plain);
        prop_assert_eq!(squashed.get_content(), plain.get_content());
        prop_assert_eq!(squashed.get_content(), texts[n].clone());
    }
}
