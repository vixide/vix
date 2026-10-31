//! The undo tree panel (T638): a buffer's undo tree drawn as an ASCII graph,
//! one row per state, oldest first, each branch in its own column ("lane").
//!
//! Host-agnostic: the host turns its history into [`TreeNode`]s, calls
//! [`Panel::open`], previews [`Panel::selected_seq`] whenever the selection
//! moves, and commits or restores on close. See `spec/index.md`.

use std::collections::HashMap;

/// One state of the undo tree, as the host's history reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// The state's stable id (creation order); the root has the lowest.
    pub seq: usize,
    /// The parent state's `seq` (`None` for the root).
    pub parent: Option<usize>,
    /// The child states' `seq`s, in creation order.
    pub children: Vec<usize>,
    /// When the state was recorded, in Unix seconds (`0` when unknown).
    pub timestamp: u64,
}

/// One drawn row: a state, or the connector after a state that branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The graph cells, trailing spaces trimmed.
    pub graph: String,
    /// The state on this row, or `None` for a connector row.
    pub seq: Option<usize>,
    /// The state's timestamp in Unix seconds (`0` when unknown or a
    /// connector row).
    pub timestamp: u64,
}

/// The glyph for a state.
const NODE: char = 'o';
/// The glyph for the state marked by [`layout`]'s `mark` (where the panel
/// opened).
const MARK: char = '@';

/// Lay `nodes` out as graph rows, oldest state first; `mark` is drawn as `@`.
///
/// A node's first child continues its lane; every later child opens a new
/// lane (the lowest free one) on a connector row right after the parent. A
/// lane is freed after the last state of its branch.
#[must_use]
pub fn layout(nodes: &[TreeNode], mark: usize) -> Vec<Row> {
    let mut sorted: Vec<&TreeNode> = nodes.iter().collect();
    sorted.sort_by_key(|n| n.seq);
    let by_seq: HashMap<usize, &TreeNode> = sorted.iter().map(|n| (n.seq, *n)).collect();
    // The last state of the branch starting at `seq`: follow first children.
    let chain_end = |mut seq: usize| {
        while let Some(&next) = by_seq.get(&seq).and_then(|n| n.children.first()) {
            if !by_seq.contains_key(&next) {
                break;
            }
            seq = next;
        }
        seq
    };
    // Lane -> the last state of the branch occupying it (`None` = free).
    let mut lanes: Vec<Option<usize>> = Vec::new();
    let mut lane_of: HashMap<usize, usize> = HashMap::new();
    let mut rows = Vec::with_capacity(sorted.len() * 2);
    for node in &sorted {
        let lane = if let Some(&l) = lane_of.get(&node.seq) {
            l
        } else {
            // A branch with no known parent lane (the root, or an orphan).
            let l = claim(&mut lanes, chain_end(node.seq));
            lane_of.insert(node.seq, l);
            l
        };
        let mut cells: Vec<char> = cells_for(&lanes);
        if let Some(c) = cells.get_mut(lane * 2) {
            *c = if node.seq == mark { MARK } else { NODE };
        }
        rows.push(Row {
            graph: trimmed(&cells),
            seq: Some(node.seq),
            timestamp: node.timestamp,
        });
        let live: Vec<usize> = node
            .children
            .iter()
            .copied()
            .filter(|c| by_seq.contains_key(c))
            .collect();
        let Some((&first, rest)) = live.split_first() else {
            // The branch ends here: its lane is free for the next one.
            if let Some(slot) = lanes.get_mut(lane) {
                *slot = None;
            }
            continue;
        };
        lane_of.insert(first, lane);
        if rest.is_empty() {
            continue;
        }
        let opened: Vec<usize> = rest
            .iter()
            .map(|&c| {
                let l = claim(&mut lanes, chain_end(c));
                lane_of.insert(c, l);
                l
            })
            .collect();
        rows.push(Row {
            graph: connector(&lanes, lane, &opened),
            seq: None,
            timestamp: 0,
        });
    }
    rows
}

/// Take the lowest free lane (or a new one) for a branch ending at `end`.
fn claim(lanes: &mut Vec<Option<usize>>, end: usize) -> usize {
    if let Some(i) = lanes.iter().position(Option::is_none) {
        lanes[i] = Some(end);
        i
    } else {
        lanes.push(Some(end));
        lanes.len() - 1
    }
}

/// The base cells for a row: `|` on every busy lane, spaces elsewhere.
fn cells_for(lanes: &[Option<usize>]) -> Vec<char> {
    let width = (lanes.len() * 2).saturating_sub(1);
    let mut cells = vec![' '; width];
    for (i, l) in lanes.iter().enumerate() {
        if l.is_some() {
            cells[i * 2] = '|';
        }
    }
    cells
}

/// The connector row from the parent's lane `from` to the newly `opened`
/// lanes.
fn connector(lanes: &[Option<usize>], from: usize, opened: &[usize]) -> String {
    let mut cells = cells_for(lanes);
    for &to in opened {
        cells[to * 2] = ' ';
    }
    for &to in opened {
        if to > from {
            for c in &mut cells[from * 2 + 1..to * 2 - 1] {
                if *c == ' ' {
                    *c = '-';
                }
            }
            cells[to * 2 - 1] = '\\';
        } else {
            cells[to * 2 + 1] = '/';
            for c in &mut cells[to * 2 + 2..from * 2] {
                if *c == ' ' {
                    *c = '-';
                }
            }
        }
    }
    trimmed(&cells)
}

/// `cells` as a string without trailing spaces.
fn trimmed(cells: &[char]) -> String {
    cells.iter().collect::<String>().trim_end().to_string()
}

/// The panel: the laid-out rows, the selection (over state rows only) and the
/// scroll offset.
#[derive(Debug, Clone)]
pub struct Panel {
    /// Every row, oldest state first.
    pub rows: Vec<Row>,
    /// Indices into `rows` of the state rows, in order.
    states: Vec<usize>,
    /// The selected entry of `states`.
    selected: usize,
    /// The first visible row.
    pub scroll: usize,
    /// The state the panel opened at.
    pub start: usize,
}

impl Panel {
    /// Lay out `nodes` and select `current`, the state the buffer is in.
    #[must_use]
    pub fn open(nodes: &[TreeNode], current: usize) -> Self {
        let rows = layout(nodes, current);
        let states: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.seq.map(|_| i))
            .collect();
        let selected = states
            .iter()
            .position(|&i| rows[i].seq == Some(current))
            .unwrap_or(0);
        Self {
            rows,
            states,
            selected,
            scroll: 0,
            start: current,
        }
    }

    /// The number of rows (states plus connectors).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there are no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The index into [`Panel::rows`] of the selected row.
    #[must_use]
    pub fn selected_row(&self) -> usize {
        self.states.get(self.selected).copied().unwrap_or(0)
    }

    /// The selected state's `seq`.
    #[must_use]
    pub fn selected_seq(&self) -> Option<usize> {
        self.rows.get(self.selected_row()).and_then(|r| r.seq)
    }

    /// The widest graph, for aligning the text after it.
    #[must_use]
    pub fn graph_width(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.graph.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// Select the previous state.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Select the next state.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.states.len());
    }

    /// Move `page` states up.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move `page` states down.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.states.len());
    }

    /// Select the first (original) state.
    pub fn home(&mut self) {
        self.selected = 0;
    }

    /// Select the last (newest) state.
    pub fn end(&mut self) {
        self.selected = self.states.len().saturating_sub(1);
    }

    /// Select the state on row `row` (a click). Returns `false` for a
    /// connector row or a row past the end.
    pub fn select_row(&mut self, row: usize) -> bool {
        match self.states.iter().position(|&i| i == row) {
            Some(p) => {
                self.selected = p;
                true
            }
            None => false,
        }
    }

    /// Scroll so the selected row is within a view `height` rows tall.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected_row(), self.scroll, height, self.len());
    }
}

#[cfg(test)]
mod tests {
    use super::{Panel, TreeNode, layout};

    /// Build nodes from `(seq, parent)` pairs; children follow `seq` order.
    fn tree(edges: &[(usize, Option<usize>)]) -> Vec<TreeNode> {
        edges
            .iter()
            .map(|&(seq, parent)| TreeNode {
                seq,
                parent,
                children: edges
                    .iter()
                    .filter(|&&(_, p)| p == Some(seq))
                    .map(|&(c, _)| c)
                    .collect(),
                timestamp: 0,
            })
            .collect()
    }

    fn graphs(nodes: &[TreeNode], mark: usize) -> Vec<String> {
        layout(nodes, mark).into_iter().map(|r| r.graph).collect()
    }

    #[test]
    fn a_linear_history_is_one_column() {
        let nodes = tree(&[(0, None), (1, Some(0)), (2, Some(1))]);
        assert_eq!(graphs(&nodes, 2), ["o", "o", "@"]);
    }

    #[test]
    fn a_branch_opens_a_lane_on_a_connector_row() {
        // 0 -> 1 -> 2, then undo to 1 and edit: 3 is a sibling of 2.
        let nodes = tree(&[(0, None), (1, Some(0)), (2, Some(1)), (3, Some(1))]);
        assert_eq!(graphs(&nodes, 3), ["o", "o", "|\\", "o |", "  @"]);
        let rows = layout(&nodes, 3);
        let seqs: Vec<Option<usize>> = rows.iter().map(|r| r.seq).collect();
        assert_eq!(seqs, [Some(0), Some(1), None, Some(2), Some(3)]);
    }

    #[test]
    fn three_branches_at_one_state() {
        let nodes = tree(&[(0, None), (1, Some(0)), (2, Some(0)), (3, Some(0))]);
        assert_eq!(
            graphs(&nodes, 0),
            ["@", "|\\-\\", "o | |", "  o |", "    o"]
        );
    }

    #[test]
    fn a_freed_lane_is_reused_and_a_left_branch_draws_slash() {
        // 0 -> 1 (ends), 0 -> 2 -> 3, 2 -> 4: lane 0 frees after 1, so 4
        // opens in lane 0, left of 2's lane 1.
        let nodes = tree(&[
            (0, None),
            (1, Some(0)),
            (2, Some(0)),
            (3, Some(2)),
            (4, Some(2)),
        ]);
        assert_eq!(
            graphs(&nodes, 4),
            ["o", "|\\", "o |", "  o", " /|", "| o", "@"]
        );
    }

    #[test]
    fn a_deep_history_stays_narrow() {
        let mut edges = vec![(0, None)];
        edges.extend((1..500).map(|i| (i, Some(i - 1))));
        let nodes = tree(&edges);
        let rows = layout(&nodes, 499);
        assert_eq!(rows.len(), 500);
        assert!(rows.iter().all(|r| r.graph.chars().count() == 1));
        assert_eq!(rows[499].graph, "@");
    }

    #[test]
    fn navigation_skips_connector_rows() {
        let nodes = tree(&[(0, None), (1, Some(0)), (2, Some(1)), (3, Some(1))]);
        let mut p = Panel::open(&nodes, 1);
        assert_eq!(p.selected_seq(), Some(1));
        p.down();
        assert_eq!(p.selected_seq(), Some(2), "the connector is skipped");
        p.up();
        p.up();
        assert_eq!(p.selected_seq(), Some(0));
        p.end();
        assert_eq!(p.selected_seq(), Some(3));
        assert!(!p.select_row(2), "a connector row is not selectable");
        assert!(p.select_row(3));
        assert_eq!(p.selected_seq(), Some(2));
        assert_eq!(p.graph_width(), 3);
    }

    #[test]
    fn ensure_visible_follows_the_selection() {
        let mut edges = vec![(0, None)];
        edges.extend((1..50).map(|i| (i, Some(i - 1))));
        let mut p = Panel::open(&tree(&edges), 49);
        p.ensure_visible(10);
        assert_eq!(p.scroll, 40);
        p.home();
        p.ensure_visible(10);
        assert_eq!(p.scroll, 0);
    }
}
