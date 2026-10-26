//! Side-by-side Compare Files (T660): two real, editable tabs in a vertical
//! split, kept aligned by `vix_diff_view::compare`. The row map is recomputed
//! whenever either buffer changes; the unfocused pane follows the focused one's
//! scroll; the gutter marks each side's changes. See
//! `crates/vix-diff-view/spec/index.md`.

use std::path::{Path, PathBuf};

use crate::diff_view::compare::{self, Hunk, Mark, Side};

use super::{App, DiffViewState, Focus};

/// An open side-by-side comparison. Tabs are tracked by path (tab indices move
/// as tabs open and close); the comparison ends when either tab closes.
pub(super) struct CompareState {
    /// The left (old) file.
    left: PathBuf,
    /// The right (new) file.
    right: PathBuf,
    /// The changed regions, as of `revisions`.
    hunks: Vec<Hunk>,
    /// The two buffers' edit revisions `hunks` was computed from.
    revisions: Option<(u64, u64)>,
}

/// Gutter colors for each [`Mark`]: the git gutter's modified/added/removed.
fn mark_hex(mark: Mark) -> &'static str {
    match mark {
        Mark::Changed => "#e5c07b",
        Mark::Only => "#98c379",
        Mark::Gap => "#e06c75",
    }
}

impl App {
    /// Compare `left` and `right` side by side: open both as tabs in a
    /// vertical split, left focused. Reports an open error and does nothing
    /// else if either can't be opened.
    pub fn open_compare_files(&mut self, left: &Path, right: &Path) {
        let Some(left) = self.open_compare_tab(left) else {
            return;
        };
        let Some(right) = self.open_compare_tab(right) else {
            return;
        };
        let (Some(li), Some(ri)) = (self.tab_index_of(&left), self.tab_index_of(&right)) else {
            return;
        };
        if li == ri {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.editor.split_root = Some(crate::pane_tree::Pane::Split {
            dir: crate::editor::SplitDir::Vertical,
            ratio: 50,
            first: Box::new(crate::pane_tree::Pane::Leaf(li)),
            second: Box::new(crate::pane_tree::Pane::Leaf(ri)),
        });
        self.editor.focused_leaf = 0;
        self.editor.active = li;
        self.focus = Focus::Editor;
        self.compare = Some(CompareState {
            left,
            right,
            hunks: Vec::new(),
            revisions: None,
        });
        self.refresh_compare();
        let identical = self.compare.as_ref().is_some_and(|c| c.hunks.is_empty());
        self.status = if identical {
            t!("status.diff_identical").to_string()
        } else {
            t!("status.compare_opened").to_string()
        };
    }

    /// Open `path` as a regular tab and return its canonical path, or report
    /// the failure and return `None`.
    fn open_compare_tab(&mut self, path: &Path) -> Option<PathBuf> {
        if let Err(e) = self.editor.open(path, false) {
            self.messages
                .error(t!("msg.open_failed", error = e).to_string());
            return None;
        }
        self.editor.active_tab().and_then(|t| t.path.clone())
    }

    fn tab_index_of(&self, path: &Path) -> Option<usize> {
        self.editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(path))
    }

    /// The `(left, right)` tab indices of the open comparison, ending it when
    /// either tab has closed.
    fn compare_tabs(&mut self) -> Option<(usize, usize)> {
        let c = self.compare.as_ref()?;
        if let (Some(l), Some(r)) = (self.tab_index_of(&c.left), self.tab_index_of(&c.right)) {
            return Some((l, r));
        }
        self.compare = None;
        None
    }

    /// Which compare side the active tab is, if a comparison is open and the
    /// active tab is in it.
    fn compare_side(&mut self) -> Option<(Side, usize, usize)> {
        let (l, r) = self.compare_tabs()?;
        match self.editor.active {
            a if a == l => Some((Side::Left, l, r)),
            a if a == r => Some((Side::Right, l, r)),
            _ => None,
        }
    }

    /// Per-frame upkeep of an open comparison: recompute the row map after an
    /// edit to either side, re-mark both gutters, and scroll the other pane to
    /// the line matching the focused pane's top line. Returns `true` when the
    /// active tab is one of the compared tabs, so the caller skips the git and
    /// coverage gutters for it.
    pub fn refresh_compare(&mut self) -> bool {
        let Some((l, r)) = self.compare_tabs() else {
            return false;
        };
        let revisions = (
            self.editor.tabs[l].editor.revision(),
            self.editor.tabs[r].editor.revision(),
        );
        if let Some(c) = self.compare.as_mut()
            && c.revisions != Some(revisions)
        {
            c.hunks = compare::hunks(
                &self.editor.tabs[l].editor.get_content(),
                &self.editor.tabs[r].editor.get_content(),
            );
            c.revisions = Some(revisions);
            for (side, i) in [(Side::Left, l), (Side::Right, r)] {
                let marks: Vec<(usize, &str)> = compare::marks(&c.hunks, side)
                    .into_iter()
                    .map(|(line, m)| (line, mark_hex(m)))
                    .collect();
                self.editor.tabs[i].editor.set_gutter_marks(marks);
            }
        }
        let Some((side, l, r)) = self.compare_side() else {
            return false;
        };
        let (here, there) = match side {
            Side::Left => (l, r),
            Side::Right => (r, l),
        };
        let top = self.editor.tabs[here].editor.get_offset_y();
        if let Some(c) = self.compare.as_ref() {
            let mapped = compare::map_line(&c.hunks, side, top);
            self.editor.tabs[there].editor.set_offset_y(mapped);
        }
        true
    }

    /// Dispatch a `diff.*` action. Returns `true` if handled.
    pub(super) fn run_compare_action(&mut self, action: &str) -> bool {
        match action {
            "diff.next" => self.compare_jump(true),
            "diff.prev" => self.compare_jump(false),
            "diff.copy_to_left" => self.compare_copy(Side::Right),
            "diff.copy_to_right" => self.compare_copy(Side::Left),
            "diff.unified" => self.compare_unified(),
            "diff.close" => self.close_compare(),
            _ => return false,
        }
        true
    }

    /// The active compare side and its cursor line, or a status saying no
    /// comparison is active here.
    fn compare_cursor(&mut self) -> Option<(Side, usize, usize, usize)> {
        self.refresh_compare();
        let Some((side, l, r)) = self.compare_side() else {
            self.status = t!("status.compare_inactive").to_string();
            return None;
        };
        let line = self.editor.active_tab()?.editor.cursor_line();
        Some((side, l, r, line))
    }

    /// Move the cursor to the next (`forward`) or previous difference.
    fn compare_jump(&mut self, forward: bool) {
        let Some((side, _, _, line)) = self.compare_cursor() else {
            return;
        };
        let Some(c) = self.compare.as_ref() else {
            return;
        };
        let found = if forward {
            compare::next_hunk(&c.hunks, side, line)
        } else {
            compare::prev_hunk(&c.hunks, side, line)
        };
        let Some(start) = found.map(|i| c.hunks[i].on(side).start) else {
            self.status = t!("status.compare_no_more").to_string();
            return;
        };
        let area = self.editor_view();
        let off = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.code_ref().line_to_char(start));
        self.editor.goto_offset(off, area);
        self.refresh_compare();
    }

    /// Copy the change at the cursor from `from` to the other side, as one
    /// undo step there.
    fn compare_copy(&mut self, from: Side) {
        let Some((_, l, r, line)) = self.compare_cursor() else {
            return;
        };
        let side = if self.editor.active == l {
            Side::Left
        } else {
            Side::Right
        };
        let target = match from {
            Side::Left => r,
            Side::Right => l,
        };
        if self.editor.tabs[target].read_only {
            self.status = t!("status.read_only_blocked").to_string();
            return;
        }
        let Some(c) = self.compare.as_ref() else {
            return;
        };
        let Some(index) = compare::hunk_at(&c.hunks, side, line) else {
            self.status = t!("status.compare_no_change_here").to_string();
            return;
        };
        let left = self.editor.tabs[l].editor.get_content();
        let right = self.editor.tabs[r].editor.get_content();
        let Some(new) = compare::copy_hunk(&c.hunks, index, from, &left, &right) else {
            return;
        };
        let tab = &mut self.editor.tabs[target];
        let cursor = tab.editor.get_cursor();
        tab.editor.set_content(&new);
        tab.editor.set_selection(None);
        tab.editor.set_cursor(cursor.min(new.chars().count()));
        tab.dirty = true;
        tab.preview = false;
        self.refresh_compare();
    }

    /// Show the open comparison as the read-only unified diff overlay.
    fn compare_unified(&mut self) {
        let Some((l, r)) = self.compare_tabs() else {
            self.status = t!("status.compare_inactive").to_string();
            return;
        };
        let name = |i: usize| {
            self.editor.tabs[i]
                .path
                .as_deref()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let title = format!("{} ↔ {}", name(l), name(r));
        let lines = crate::diff_view::build(
            &self.editor.tabs[l].editor.get_content(),
            &self.editor.tabs[r].editor.get_content(),
        );
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.diff_view = Some(DiffViewState {
            title,
            lines,
            scroll: 0,
        });
    }

    /// End the comparison: clear both gutters and remove the split. The tabs
    /// stay open.
    fn close_compare(&mut self) {
        let Some((l, r)) = self.compare_tabs() else {
            self.status = t!("status.compare_inactive").to_string();
            return;
        };
        for i in [l, r] {
            self.editor.tabs[i].editor.clear_gutter_marks();
        }
        self.compare = None;
        self.editor.split_root = None;
        self.editor.focused_leaf = 0;
        self.status = t!("status.compare_closed").to_string();
    }
}
