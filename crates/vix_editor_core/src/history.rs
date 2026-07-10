#![warn(clippy::pedantic)]
use std::collections::HashSet;

use crate::code::EditBatch;

/// One state in the edit tree: the batch that produced it from its parent, plus
/// the tree links. The root (index 0) has no batch and no parent.
#[derive(serde::Serialize, serde::Deserialize)]
struct Node {
    /// The edit leading from the parent to this node (`None` for the root).
    batch: Option<EditBatch>,
    /// Arena index of the parent node (`None` for the root).
    parent: Option<usize>,
    /// Arena indices of child states, in creation order.
    children: Vec<usize>,
    /// Which child `redo` advances into (the most recently visited/created).
    active_child: Option<usize>,
}

/// A **tree** of undo states (an "undo tree", like Vim's undotree / Emacs
/// undo-tree). A new edit after an undo adds a *branch* instead of discarding the
/// redo history, so no state is ever lost; [`History::switch_branch`] chooses
/// which branch `redo` follows.
///
/// `undo`/`redo` behave exactly like a linear history in the common case: `redo`
/// follows the most-recently-active branch, so immediately after a fresh edit
/// there is nothing to redo (the new edit is the active branch's tip).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct History {
    /// Arena of nodes; slots are tombstoned (`None`) when pruned. `nodes[0]` is
    /// the root (the buffer's initial state).
    nodes: Vec<Option<Node>>,
    /// Arena index of the current state.
    current: usize,
    /// Soft cap on the number of live nodes; the oldest off-path leaves are
    /// pruned beyond it.
    max_items: usize,
}

impl History {
    /// Create an empty history (just the root state) keeping at most `max_items`
    /// live nodes.
    #[must_use]
    pub fn new(max_items: usize) -> Self {
        let root = Node { batch: None, parent: None, children: Vec::new(), active_child: None };
        Self { nodes: vec![Some(root)], current: 0, max_items }
    }

    /// Record `batch` as a new state branching from the current one, and move to
    /// it. Existing sibling branches are kept (not discarded).
    pub fn push(&mut self, batch: EditBatch) {
        let new = self.nodes.len();
        self.nodes.push(Some(Node {
            batch: Some(batch),
            parent: Some(self.current),
            children: Vec::new(),
            active_child: None,
        }));
        if let Some(cur) = self.nodes[self.current].as_mut() {
            cur.children.push(new);
            cur.active_child = Some(new);
        }
        self.current = new;
        self.prune();
    }

    /// Step back to the parent state, returning the batch to invert (or `None` at
    /// the root). The branch just left becomes the parent's active redo branch.
    pub fn undo(&mut self) -> Option<EditBatch> {
        let node = self.nodes[self.current].as_ref()?;
        let parent = node.parent?;
        let batch = node.batch.clone()?;
        let left = self.current;
        if let Some(p) = self.nodes[parent].as_mut() {
            p.active_child = Some(left);
        }
        self.current = parent;
        Some(batch)
    }

    /// Step forward into the active child branch, returning the batch to apply (or
    /// `None` if the current state is a tip).
    pub fn redo(&mut self) -> Option<EditBatch> {
        let active = self.nodes[self.current].as_ref()?.active_child?;
        let batch = self.nodes[active].as_ref()?.batch.clone()?;
        self.current = active;
        Some(batch)
    }

    /// Cycle which child branch `redo` will follow from the current state. Returns
    /// `true` when the current state has more than one branch to choose between.
    pub fn switch_branch(&mut self) -> bool {
        let Some(node) = self.nodes[self.current].as_mut() else { return false };
        if node.children.len() < 2 {
            return false;
        }
        let active = node.active_child.unwrap_or(node.children[0]);
        let pos = node.children.iter().position(|&c| c == active).unwrap_or(0);
        let next = node.children[(pos + 1) % node.children.len()];
        node.active_child = Some(next);
        true
    }

    /// The set of nodes on the path from the root to the current state (never
    /// pruned).
    fn protected(&self) -> HashSet<usize> {
        let mut set = HashSet::new();
        let mut cur = Some(self.current);
        while let Some(i) = cur {
            set.insert(i);
            cur = self.nodes[i].as_ref().and_then(|n| n.parent);
        }
        set
    }

    /// Drop the oldest off-path leaf states until the live count is within
    /// `max_items` (a no-op when `max_items` is 0 or already within bounds).
    fn prune(&mut self) {
        if self.max_items == 0 {
            return;
        }
        loop {
            let alive = self.nodes.iter().filter(|n| n.is_some()).count();
            if alive <= self.max_items {
                return;
            }
            let protected = self.protected();
            let victim = self.nodes.iter().enumerate().find_map(|(i, n)| {
                n.as_ref().filter(|nd| nd.children.is_empty() && !protected.contains(&i)).map(|_| i)
            });
            let Some(v) = victim else { return };
            if let Some(parent) = self.nodes[v].as_ref().and_then(|n| n.parent)
                && let Some(p) = self.nodes[parent].as_mut()
            {
                p.children.retain(|&c| c != v);
                if p.active_child == Some(v) {
                    p.active_child = p.children.last().copied();
                }
            }
            self.nodes[v] = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::History;
    use crate::code::{Edit, EditBatch, EditKind};

    fn batch(text: &str) -> EditBatch {
        let mut b = EditBatch::new();
        b.edits.push(Edit { kind: EditKind::Insert { offset: 0, text: text.to_string() } });
        b
    }

    fn first_text(b: &EditBatch) -> String {
        match &b.edits[0].kind {
            EditKind::Insert { text, .. } | EditKind::Remove { text, .. } => text.clone(),
        }
    }

    #[test]
    fn linear_undo_redo_round_trips() {
        let mut h = History::new(100);
        h.push(batch("a"));
        h.push(batch("b"));
        assert_eq!(h.redo().map(|b| first_text(&b)), None, "at the tip, nothing to redo");
        assert_eq!(h.undo().map(|b| first_text(&b)), Some("b".into()));
        assert_eq!(h.undo().map(|b| first_text(&b)), Some("a".into()));
        assert_eq!(h.undo().map(|b| first_text(&b)), None, "at the root");
        assert_eq!(h.redo().map(|b| first_text(&b)), Some("a".into()));
        assert_eq!(h.redo().map(|b| first_text(&b)), Some("b".into()));
    }

    #[test]
    fn editing_after_undo_keeps_the_other_branch() {
        let mut h = History::new(100);
        h.push(batch("a")); // root -> a
        h.undo(); // back to root
        h.push(batch("b")); // root now has children [a, b], active = b
        // Right after the new edit, redo follows the active (b) branch: nothing.
        assert_eq!(h.redo().map(|b| first_text(&b)), None);
        // Undo back to the branch point and switch branches to reach "a".
        assert_eq!(h.undo().map(|b| first_text(&b)), Some("b".into()));
        assert!(h.switch_branch(), "two branches at the root");
        assert_eq!(h.redo().map(|b| first_text(&b)), Some("a".into()), "the old branch survived");
    }

    #[test]
    fn prune_keeps_the_current_path() {
        // Cap of 2 live nodes: a long linear chain keeps the most recent states.
        let mut h = History::new(2);
        for c in ["a", "b", "c", "d"] {
            h.push(batch(c));
        }
        // The current path is protected; undo still works and never panics.
        assert_eq!(h.undo().map(|b| first_text(&b)), Some("d".into()));
    }
}
