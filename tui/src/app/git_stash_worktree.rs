//! The stash list and worktrees (T634): **Git → Stash List…** and the
//! **Git → Worktree** submenu. Every git call goes through `vix-git`'s
//! runners, which build their process through the workspace authority, so a
//! remote workspace stashes and lists worktrees on its own host.

use crate::app::chooser::ChooserSlot;
use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent, MouseEvent};

use super::git::git_error_text;
use super::{App, Confirm, ConfirmKind, Prompt, PromptKind, StashChooser, WorktreeChooser};

impl App {
    /// Dispatch a T634 stash/worktree action. `true` when `action` is one.
    pub(super) fn run_git_stash_worktree_action(&mut self, action: &str) -> bool {
        match action {
            "git.stash_list" => self.open_stash_chooser(),
            "git.worktree_add" => self.begin_worktree_add(),
            "git.worktree_list" => self.open_worktree_chooser(false),
            "git.worktree_remove" => self.open_worktree_chooser(true),
            _ => return false,
        }
        true
    }

    // ----- Stash list -----------------------------------------------------

    /// **Git → Stash List…**: list every stash, or say there are none.
    fn open_stash_chooser(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        if !self.load_stash_chooser(0) {
            self.status = t!("status.git_no_stashes").into();
        }
    }

    /// (Re)load the stash chooser, highlighting `selected` (clamped). Closes
    /// it and returns `false` when there are no stashes.
    fn load_stash_chooser(&mut self, selected: usize) -> bool {
        let entries = crate::git::stash_list(&self.root);
        if entries.is_empty() {
            self.chooser = None;
            return false;
        }
        let selected = selected.min(entries.len() - 1);
        self.chooser.open_stash(StashChooser { entries, selected });
        true
    }

    /// The highlighted stash's `(index, message)`.
    fn selected_stash(&self) -> Option<(usize, String)> {
        let c = self.chooser.stash()?;
        let e = c.entries.get(c.selected)?;
        Some((e.index, e.message.clone()))
    }

    pub(super) fn stash_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.stash_mut() {
                    c.selected = vix_list_state::up(c.selected);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.stash_mut() {
                    c.selected = vix_list_state::down(c.selected, c.entries.len());
                }
            }
            KeyCode::Enter => self.show_selected_stash(),
            KeyCode::Char('r') => self.review_selected_stash(),
            KeyCode::Char('a') => self.apply_selected_stash(false),
            KeyCode::Char('p') => self.apply_selected_stash(true),
            KeyCode::Char('d') | KeyCode::Delete => self.confirm_drop_selected_stash(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    pub(super) fn stash_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.chooser.stash_mut()
            && idx < c.entries.len()
        {
            c.selected = idx;
            self.show_selected_stash();
        }
    }

    /// `Enter`: the highlighted stash's diff in a read-only tab titled
    /// `stash@{N}`, the same as Browse Log's commit diffs.
    fn show_selected_stash(&mut self) {
        let Some((index, _)) = self.selected_stash() else {
            return;
        };
        let name = crate::git::stash_ref(index);
        let Some(patch) = crate::git::stash_show(&self.root, index) else {
            self.messages
                .error(t!("msg.git_stash_show_failed", stash = name).to_string());
            return;
        };
        self.chooser = None;
        self.open_readonly_text_tab(&name, &patch);
    }

    /// `r`: open the highlighted stash in Review (T665), as `stash@{N}`.
    fn review_selected_stash(&mut self) {
        let Some((index, _)) = self.selected_stash() else {
            return;
        };
        self.chooser = None;
        self.open_review(&crate::git::stash_ref(index));
    }

    /// `a` applies the highlighted stash and keeps it; `p` pops it.
    fn apply_selected_stash(&mut self, pop: bool) {
        let Some((index, _)) = self.selected_stash() else {
            return;
        };
        let name = crate::git::stash_ref(index);
        let result = if pop {
            crate::git::stash_pop_at(&self.root, index)
        } else {
            crate::git::stash_apply(&self.root, index)
        };
        match result {
            Ok(()) => {
                self.after_tree_change();
                self.status = if pop {
                    t!("status.git_stash_popped_ref", stash = name)
                } else {
                    t!("status.git_stash_applied", stash = name)
                }
                .to_string();
                if pop {
                    self.load_stash_chooser(index);
                } else {
                    self.chooser = None;
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
    }

    /// `d`: ask before dropping the highlighted stash. The chooser closes
    /// under the confirmation and reopens after it.
    fn confirm_drop_selected_stash(&mut self) {
        let Some((index, message)) = self.selected_stash() else {
            return;
        };
        let name = crate::git::stash_ref(index);
        self.chooser = None;
        self.confirm = Some(Confirm {
            message: t!("confirm.git_stash_drop", stash = name, message = message).to_string(),
            paths: vec![PathBuf::from(name)],
            kind: ConfirmKind::GitStashDrop,
        });
    }

    /// The answer to [`ConfirmKind::GitStashDrop`]: drop `name` on yes, then
    /// reopen the chooser either way.
    pub(super) fn git_stash_drop_answered(&mut self, name: &Path, yes: bool) {
        let name = name.to_string_lossy().into_owned();
        let index = name
            .strip_prefix("stash@{")
            .and_then(|r| r.strip_suffix('}'))
            .and_then(|n| n.parse::<usize>().ok());
        let Some(index) = index else {
            return;
        };
        if yes {
            match crate::git::stash_drop(&self.root, index) {
                Ok(()) => self.status = t!("status.git_stash_dropped", stash = name).to_string(),
                Err(e) => self
                    .messages
                    .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
            }
        }
        self.load_stash_chooser(index);
    }

    /// Refresh git state and clean buffers after a stash changed the tree.
    pub(super) fn after_tree_change(&mut self) {
        self.reload_clean_buffers();
        self.refresh_git();
        self.refresh_git_gutter();
    }

    // ----- Worktrees ------------------------------------------------------

    /// **Git → Worktree → Add…**: prompt for the new worktree's path.
    fn begin_worktree_add(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        self.chooser = None;
        self.prompt = Some(Prompt::new(
            PromptKind::GitWorktreeAdd,
            t!("prompt.git_worktree_add").to_string(),
        ));
    }

    /// `PromptKind::GitWorktreeAdd`'s accept handler: `git worktree add
    /// <path>` (relative to the workspace root), on a new branch named after
    /// the path's last component. Empty input is a no-op.
    pub(super) fn git_worktree_add(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        match crate::git::worktree_add(&self.root, path) {
            Ok(()) => {
                self.refresh_git();
                self.status = t!("status.git_worktree_added", path = path).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
    }

    /// **Git → Worktree → List…** (`remove == false`) or **Remove…**.
    fn open_worktree_chooser(&mut self, remove: bool) {
        if !self.require_git_repo() {
            return;
        }
        if !self.load_worktree_chooser(remove, None) {
            self.status = t!("status.git_not_repo").into();
        }
    }

    /// (Re)load the worktree chooser. `selected` defaults to the current
    /// worktree. `false` (and closed) when git lists none.
    fn load_worktree_chooser(&mut self, remove: bool, selected: Option<usize>) -> bool {
        let worktrees = crate::git::worktree_list(&self.root);
        if worktrees.is_empty() {
            self.chooser = None;
            return false;
        }
        let selected = selected
            .or_else(|| WorktreeChooser::current_in(&worktrees, &self.root))
            .unwrap_or(0)
            .min(worktrees.len() - 1);
        self.chooser.open_worktree(WorktreeChooser {
            worktrees,
            selected,
            remove,
        });
        true
    }

    pub(super) fn worktree_chooser_key(&mut self, key: KeyEvent) {
        let remove = self.chooser.worktree().is_some_and(|c| c.remove);
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.worktree_mut() {
                    c.selected = vix_list_state::up(c.selected);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.worktree_mut() {
                    c.selected = vix_list_state::down(c.selected, c.worktrees.len());
                }
            }
            KeyCode::Enter => self.activate_selected_worktree(),
            KeyCode::Char('d') | KeyCode::Delete => self.confirm_remove_selected_worktree(),
            KeyCode::Char('a') if !remove => self.begin_worktree_add(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    pub(super) fn worktree_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.chooser.worktree_mut()
            && idx < c.worktrees.len()
        {
            c.selected = idx;
            self.activate_selected_worktree();
        }
    }

    /// `Enter` (or a click): remove the highlighted worktree in remove mode,
    /// switch to it otherwise.
    fn activate_selected_worktree(&mut self) {
        if self.chooser.worktree().is_some_and(|c| c.remove) {
            self.confirm_remove_selected_worktree();
        } else {
            self.switch_to_selected_worktree();
        }
    }

    /// The highlighted worktree, its index, and whether it holds the root.
    fn selected_worktree(&self) -> Option<(usize, crate::git::Worktree, bool)> {
        let c = self.chooser.worktree()?;
        let w = c.worktrees.get(c.selected)?.clone();
        let current = c.current(&self.root) == Some(c.selected);
        Some((c.selected, w, current))
    }

    /// `Enter` in list mode: re-root the workspace at the highlighted
    /// worktree through [`App::switch_workspace`] — the project-switch path —
    /// which saves this workspace's session and restores the worktree's own.
    fn switch_to_selected_worktree(&mut self) {
        let Some((_, w, current)) = self.selected_worktree() else {
            return;
        };
        if current {
            self.status = t!("status.git_worktree_current").into();
            return;
        }
        if w.checkout == crate::git::Checkout::Bare || w.prunable {
            self.status = t!("status.git_worktree_unavailable").into();
            return;
        }
        self.chooser = None;
        self.switch_workspace(&w.path, &[]);
    }

    /// `d` (or `Enter` in remove mode): ask before removing the highlighted
    /// worktree. The main worktree and the one holding the workspace can't
    /// be removed from here.
    fn confirm_remove_selected_worktree(&mut self) {
        let Some((idx, w, current)) = self.selected_worktree() else {
            return;
        };
        if idx == 0 {
            self.status = t!("status.git_worktree_main").into();
            return;
        }
        if current {
            self.status = t!("status.git_worktree_current").into();
            return;
        }
        self.chooser = None;
        self.confirm = Some(Confirm {
            message: t!("confirm.git_worktree_remove", path = w.path.display()).to_string(),
            paths: vec![w.path],
            kind: ConfirmKind::GitWorktreeRemove,
        });
    }

    /// The answer to [`ConfirmKind::GitWorktreeRemove`]: `git worktree
    /// remove` on yes (git itself refuses a dirty one), then reopen the
    /// chooser either way, in list mode so the result shows.
    pub(super) fn git_worktree_remove_answered(&mut self, path: &Path, yes: bool) {
        if yes {
            let text = path.to_string_lossy();
            match crate::git::worktree_remove(&self.root, &text) {
                Ok(()) => {
                    self.status =
                        t!("status.git_worktree_removed", path = path.display()).to_string();
                }
                Err(e) => self
                    .messages
                    .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
            }
        }
        self.load_worktree_chooser(false, None);
    }
}
