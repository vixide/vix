//! Left-drawer file explorer: a lazily-expanded directory tree.

use std::path::{Path, PathBuf};

/// A visible row in the flattened tree.
pub struct Node {
    /// Absolute path of this entry.
    pub path: PathBuf,
    /// File or directory name (no parent path).
    pub name: String,
    /// Indentation depth from the root.
    pub depth: usize,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Whether this directory is currently expanded.
    pub expanded: bool,
}

/// The file-explorer tree and its selection/scroll state.
pub struct Explorer {
    /// Project root the tree is rooted at.
    pub root: PathBuf,
    /// Flattened list of currently visible rows.
    pub nodes: Vec<Node>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync with the rendered viewport so mouse
    /// clicks map to the correct node.
    pub top: usize,
    /// Multi-selection (paths, so it survives a rebuild). Empty means "just the
    /// cursor row".
    pub marked: std::collections::HashSet<PathBuf>,
    /// Anchor row for Shift+Up/Down range selection.
    anchor: usize,
    /// Directories the user has expanded.
    expanded: std::collections::HashSet<PathBuf>,
}

impl Explorer {
    /// Build an explorer rooted at `root`, with the root expanded.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let mut e = Explorer {
            root: root.clone(),
            nodes: Vec::new(),
            selected: 0,
            top: 0,
            marked: std::collections::HashSet::new(),
            anchor: 0,
            expanded: std::collections::HashSet::new(),
        };
        e.expanded.insert(root);
        e.rebuild();
        e
    }

    /// Paths the next clipboard/delete operation acts on: the multi-selection
    /// if any, otherwise the single cursor row.
    #[must_use] 
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        if self.marked.is_empty() {
            self.selected_node().map(|n| vec![n.path.clone()]).unwrap_or_default()
        } else {
            self.nodes
                .iter()
                .filter(|n| self.marked.contains(&n.path))
                .map(|n| n.path.clone())
                .collect()
        }
    }

    /// Clear the multi-selection.
    pub fn clear_marks(&mut self) {
        self.marked.clear();
    }

    /// Extend the multi-selection one row up or down from the anchor.
    pub fn extend(&mut self, down: bool) {
        if self.marked.is_empty() {
            self.anchor = self.selected;
        }
        if down {
            self.down();
        } else {
            self.up();
        }
        let (lo, hi) = (self.anchor.min(self.selected), self.anchor.max(self.selected));
        self.marked.clear();
        for i in lo..=hi {
            if let Some(n) = self.nodes.get(i) {
                self.marked.insert(n.path.clone());
            }
        }
    }

    /// Adjust the scroll offset so the selection stays within a `height`-row
    /// viewport. Returns the first visible index.
    pub fn ensure_visible(&mut self, height: usize) -> usize {
        let height = height.max(1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + height {
            self.top = self.selected + 1 - height;
        }
        let max_top = self.nodes.len().saturating_sub(height);
        if self.top > max_top {
            self.top = max_top;
        }
        self.top
    }

    /// Rebuild the flattened, visible node list from disk.
    pub fn rebuild(&mut self) {
        let mut nodes = Vec::new();
        let root = self.root.clone();
        self.walk(&root, 0, &mut nodes);
        if self.selected >= nodes.len() {
            self.selected = nodes.len().saturating_sub(1);
        }
        self.nodes = nodes;
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<Node>) {
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd.flatten().collect(),
            Err(_) => return,
        };
        entries.sort_by_key(|e| {
            let p = e.path();
            // Directories first, then case-insensitive name.
            (!p.is_dir(), e.file_name().to_string_lossy().to_lowercase())
        });
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue; // hide dotfiles
            }
            let is_dir = path.is_dir();
            let expanded = is_dir && self.expanded.contains(&path);
            out.push(Node {
                path: path.clone(),
                name,
                depth,
                is_dir,
                expanded,
            });
            if expanded {
                self.walk(&path, depth + 1, out);
            }
        }
    }

    /// The highlighted node, if any.
    #[must_use]
    pub fn selected_node(&self) -> Option<&Node> {
        self.nodes.get(self.selected)
    }

    /// Move the selection up one row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the selection down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < self.nodes.len() {
            self.selected += 1;
        }
    }

    /// Move the selection up by `n` rows.
    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
    }

    /// Move the selection down by `n` rows.
    pub fn page_down(&mut self, n: usize) {
        self.selected = (self.selected + n).min(self.nodes.len().saturating_sub(1));
    }

    /// Select the first row.
    pub fn first(&mut self) {
        self.selected = 0;
    }

    /// Select the last row.
    pub fn last(&mut self) {
        self.selected = self.nodes.len().saturating_sub(1);
    }

    /// Toggle expansion of the selected directory. Returns true if it acted on
    /// a directory.
    pub fn toggle_selected(&mut self) -> bool {
        let Some(node) = self.nodes.get(self.selected) else {
            return false;
        };
        if !node.is_dir {
            return false;
        }
        let path = node.path.clone();
        if self.expanded.contains(&path) {
            self.expanded.remove(&path);
        } else {
            self.expanded.insert(path);
        }
        self.rebuild();
        true
    }

    /// Expand every ancestor directory of `path` and select that row, so a file
    /// opened elsewhere is revealed in the tree.
    pub fn reveal(&mut self, path: &Path) {
        let mut cur = path.parent();
        while let Some(dir) = cur {
            if dir.starts_with(&self.root) || dir == self.root {
                self.expanded.insert(dir.to_path_buf());
            }
            if dir == self.root {
                break;
            }
            cur = dir.parent();
        }
        self.rebuild();
        if let Some(i) = self.nodes.iter().position(|n| n.path == path) {
            self.selected = i;
        }
    }
}
