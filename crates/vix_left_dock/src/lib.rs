//! State for the left dock: the file explorer's lazily-expanded directory tree,
//! its selection, multi-selection, and scroll offset.
//!
//! Pure logic over `std::fs` — the host (the `vix` app) renders the tree and
//! routes keys/clicks/file operations; this crate owns the tree state.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

/// A visible row in the flattened tree.
pub struct Node {
    /// Absolute path of this entry.
    pub path: PathBuf,
    /// File or directory name (no parent path).
    pub name: String,
    /// Indentation depth from the root.
    pub depth: usize,
    /// Whether this entry is a directory (following symlinks).
    pub is_dir: bool,
    /// Whether this entry is a symbolic link (the link itself, not its target).
    pub is_symlink: bool,
    /// Whether this directory is currently expanded.
    pub expanded: bool,
}

/// The file-explorer tree and its selection/scroll state.
pub struct Explorer {
    /// Workspace root the tree is rooted at.
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
    /// Original "include" filter text (kept so the prompt can show/reseed it).
    pub include_filter: String,
    /// Original "exclude" filter text.
    pub exclude_filter: String,
    /// Compiled include regex: when set, only files whose relative path matches
    /// are shown.
    include: Option<regex::Regex>,
    /// Compiled exclude regex: files whose relative path matches are hidden.
    exclude: Option<regex::Regex>,
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
            include_filter: String::new(),
            exclude_filter: String::new(),
            include: None,
            exclude: None,
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

    /// Set the include/exclude path-regex filters and rebuild. An empty string
    /// clears that side; an invalid regex is treated as no filter (so a
    /// half-typed pattern never hides everything).
    pub fn set_filter(&mut self, include: &str, exclude: &str) {
        self.include_filter = include.to_string();
        self.exclude_filter = exclude.to_string();
        let compile = |p: &str| (!p.is_empty()).then(|| regex::Regex::new(p).ok()).flatten();
        self.include = compile(include);
        self.exclude = compile(exclude);
        self.rebuild();
    }

    /// Whether any include/exclude filter is active.
    #[must_use]
    pub fn has_filter(&self) -> bool {
        self.include.is_some() || self.exclude.is_some()
    }

    /// Whether a file at `path` passes the include/exclude filters, tested against
    /// its forward-slashed path relative to the root. Directories are never
    /// filtered (so the tree stays navigable).
    fn allows(&self, path: &Path) -> bool {
        if self.include.is_none() && self.exclude.is_none() {
            return true;
        }
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(inc) = &self.include
            && !inc.is_match(&rel) {
                return false;
            }
        if let Some(exc) = &self.exclude
            && exc.is_match(&rel) {
                return false;
            }
        true
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
            // The link itself, regardless of what it points at.
            let is_symlink = std::fs::symlink_metadata(&path)
                .is_ok_and(|m| m.file_type().is_symlink());
            // Files must pass the include/exclude filters; directories are kept so
            // the tree stays navigable.
            if !is_dir && !self.allows(&path) {
                continue;
            }
            let expanded = is_dir && self.expanded.contains(&path);
            out.push(Node {
                path: path.clone(),
                name,
                depth,
                is_dir,
                is_symlink,
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

    /// Left-arrow behavior: collapse the selected directory if it is expanded;
    /// otherwise move the selection to its parent directory. Never expands a
    /// directory. Returns true if it changed anything.
    pub fn collapse_or_parent(&mut self) -> bool {
        let Some(node) = self.nodes.get(self.selected) else {
            return false;
        };
        let path = node.path.clone();
        if node.is_dir && self.expanded.contains(&path) {
            self.expanded.remove(&path);
            self.rebuild();
            return true;
        }
        if let Some(parent) = path.parent()
            && let Some(i) = self.nodes.iter().position(|n| n.path == parent) {
                self.selected = i;
                return true;
            }
        false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_exclude_filters_hide_files_but_keep_dirs() {
        let dir = std::env::temp_dir().join(format!("vix-ld-filter-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        std::fs::write(dir.join("b.txt"), "").unwrap();

        let mut e = Explorer::new(dir.clone());
        let files = |e: &Explorer| e.nodes.iter().filter(|n| !n.is_dir).count();
        let dirs = |e: &Explorer| e.nodes.iter().filter(|n| n.is_dir).count();
        assert_eq!(files(&e), 2);
        assert_eq!(dirs(&e), 1, "the sub directory is shown");

        // Include only .rs files; the directory is still shown.
        e.set_filter(r"\.rs$", "");
        assert!(e.has_filter());
        assert_eq!(files(&e), 1);
        assert_eq!(dirs(&e), 1);

        // Exclude .txt instead.
        e.set_filter("", r"\.txt$");
        let names: Vec<_> = e.nodes.iter().filter(|n| !n.is_dir).map(|n| n.name.clone()).collect();
        assert_eq!(names, vec!["a.rs"]);

        // Clearing both restores everything.
        e.set_filter("", "");
        assert!(!e.has_filter());
        assert_eq!(files(&e), 2);

        std::fs::remove_dir_all(&dir).ok();
    }
}
