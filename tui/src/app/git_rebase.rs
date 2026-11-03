//! Interactive rebase and cherry-pick (T633): **Git → Rebase** and Browse
//! Log's `c`. The todo list is edited in an overlay and handed to
//! `git rebase -i` by `vix-git`'s runner through `GIT_SEQUENCE_EDITOR`, set
//! on that one process (see `crates/vix-git/src/rebase.rs`), so no editor
//! ever opens and Vix's own environment is never touched.

use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use super::git::git_error_text;
use super::{App, Prompt, PromptKind};

impl App {
    /// Dispatch a T633 rebase/cherry-pick action. `true` when `action` is one.
    pub(super) fn run_git_rebase_action(&mut self, action: &str) -> bool {
        match action {
            "git.rebase_interactive" => self.git_rebase_prompt(),
            "git.rebase_continue" => self.git_rebase_continue(),
            "git.rebase_abort" => self.git_rebase_abort(),
            "git.cherry_pick" => self.git_cherry_pick_open(),
            _ => return false,
        }
        true
    }

    /// **Git → Rebase → Interactive Rebase…**: prompt for the base, unless a
    /// rebase is already stopped (then say how to finish it).
    fn git_rebase_prompt(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        if crate::git::rebase_in_progress(&self.root) {
            self.status = t!("status.git_rebase_in_progress").into();
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::GitRebaseBase,
            t!("prompt.git_rebase_base").to_string(),
        ));
    }

    /// `PromptKind::GitRebaseBase`'s accept handler: list the commits after
    /// `base` and open the todo overlay over them. Empty input is a no-op.
    pub(super) fn git_rebase_begin(&mut self, base: &str) {
        if base.is_empty() {
            return;
        }
        match crate::git::rebase_commits(&self.root, base) {
            Ok((_, commits)) if commits.is_empty() => {
                self.status = t!("status.git_rebase_nothing", base = base).to_string();
            }
            Ok((sha, commits)) => {
                self.git_rebase = Some(crate::git::RebaseTodo::new(sha, base.to_string(), commits));
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
    }

    /// Handle a key in the interactive-rebase todo editor: Up/Down move, Alt+Up/Alt+Down
    /// reorder the entry, `r` prompts for a new message (reword), another verb key sets that
    /// action, Enter runs the rebase, Esc cancels it.
    pub(super) fn git_rebase_key(&mut self, key: KeyEvent) {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Up if alt => self.with_rebase(crate::git::RebaseTodo::move_up),
            KeyCode::Down if alt => self.with_rebase(crate::git::RebaseTodo::move_down),
            KeyCode::Up => self.with_rebase(crate::git::RebaseTodo::up),
            KeyCode::Down => self.with_rebase(crate::git::RebaseTodo::down),
            KeyCode::Char('r') => self.git_rebase_reword_prompt(),
            KeyCode::Char(c) => {
                if let Some(action) = crate::git::RebaseAction::from_key(c) {
                    self.with_rebase(|t| t.set_action(action));
                }
            }
            KeyCode::Enter => self.git_rebase_run(),
            KeyCode::Esc => {
                self.git_rebase = None;
                self.status = t!("status.git_rebase_cancelled").into();
            }
            _ => {}
        }
    }

    /// A click selects the row under the pointer.
    pub(super) fn git_rebase_mouse(&mut self, mouse: MouseEvent) {
        if let Some(row) = self.chooser_row(mouse)
            && let Some(t) = self.git_rebase.as_mut()
        {
            let idx = t.scroll + row;
            if idx < t.commits.len() {
                t.selected = idx;
            }
        }
    }

    /// Apply `f` to the open overlay.
    fn with_rebase(&mut self, f: impl FnOnce(&mut crate::git::RebaseTodo)) {
        if let Some(t) = self.git_rebase.as_mut() {
            f(t);
        }
    }

    /// `r`: mark the highlighted commit `reword` and prompt for its new
    /// message, starting from the one typed before or the commit's subject.
    fn git_rebase_reword_prompt(&mut self) {
        let Some(c) = self.git_rebase.as_ref().and_then(|t| t.selected_commit()) else {
            return;
        };
        let start = c.message.clone().unwrap_or_else(|| c.subject.clone());
        self.with_rebase(|t| t.set_action(crate::git::RebaseAction::Reword));
        self.prompt = Some(
            Prompt::new(
                PromptKind::GitRebaseReword,
                t!("prompt.git_rebase_reword").to_string(),
            )
            .with_input(start),
        );
    }

    /// `PromptKind::GitRebaseReword`'s accept handler. An empty message
    /// leaves a plain `reword`, which keeps the commit's own message.
    pub(super) fn git_rebase_reword(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        let message = message.to_string();
        self.with_rebase(|t| t.set_message(message));
    }

    /// `Enter`: check the list, then run the rebase.
    fn git_rebase_run(&mut self) {
        let Some(todo) = self.git_rebase.as_ref() else {
            return;
        };
        if let Err(problem) = crate::git::check_todo(&todo.commits) {
            self.status = if problem == crate::git::TodoProblem::MeldsFirst {
                t!("status.git_rebase_melds_first")
            } else {
                t!("status.git_rebase_cancelled")
            }
            .to_string();
            return;
        }
        if self.commit_signing_blocked() {
            return;
        }
        let Some(todo) = self.git_rebase.take() else {
            return;
        };
        let result = crate::git::rebase_interactive(&self.root, &todo.base, &todo.commits);
        self.git_rebase_report(result);
    }

    /// **Git → Rebase → Continue**: `git rebase --continue` after an `edit`
    /// stop or a resolved conflict.
    fn git_rebase_continue(&mut self) {
        if !self.require_rebase_in_progress() || self.commit_signing_blocked() {
            return;
        }
        let result = crate::git::rebase_continue(&self.root);
        self.git_rebase_report(result);
    }

    /// **Git → Rebase → Abort**: `git rebase --abort`.
    fn git_rebase_abort(&mut self) {
        if !self.require_rebase_in_progress() {
            return;
        }
        match crate::git::rebase_abort(&self.root) {
            Ok(()) => self.status = t!("status.git_rebase_aborted").into(),
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
        self.after_tree_change();
    }

    /// Bail with a status message unless a rebase is stopped in the repo.
    fn require_rebase_in_progress(&mut self) -> bool {
        if !self.require_git_repo() {
            return false;
        }
        if crate::git::rebase_in_progress(&self.root) {
            return true;
        }
        self.status = t!("status.git_rebase_none").into();
        false
    }

    /// Report a rebase runner's result and refresh the tree.
    fn git_rebase_report(&mut self, result: Result<crate::git::RebaseState, crate::git::GitError>) {
        match result {
            Ok(crate::git::RebaseState::Finished) => {
                self.status = t!("status.git_rebase_done").into();
            }
            Ok(crate::git::RebaseState::Stopped) => {
                self.status = t!("status.git_rebase_stopped").into();
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string());
                if crate::git::rebase_in_progress(&self.root) {
                    self.status = t!("status.git_rebase_stopped").into();
                }
            }
        }
        self.after_tree_change();
    }

    /// **Git → Rebase → Cherry-Pick…**: prompt for the branch (or any
    /// revision) to pick from.
    fn git_cherry_pick_open(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::GitCherryPickFrom,
            t!("prompt.git_cherry_pick_from").to_string(),
        ));
    }

    /// `PromptKind::GitCherryPickFrom`'s accept handler: open the commit
    /// list (the Browse Log panel) over the commits on `rev` that `HEAD`
    /// lacks, where `c` picks one. Empty input is a no-op.
    pub(super) fn git_cherry_pick_from(&mut self, rev: &str) {
        if rev.is_empty() {
            return;
        }
        match crate::git::cherry_pick_candidates(&self.root, rev, 200) {
            Ok(entries) if entries.is_empty() => {
                self.status = t!("status.git_cherry_pick_nothing", rev = rev).to_string();
            }
            Ok(entries) => {
                self.git_log = Some(crate::git::LogPanel::new(
                    entries,
                    crate::git::LogScope::Pick(rev.to_string()),
                ));
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
    }

    /// Browse Log's `c`: cherry-pick the highlighted commit onto `HEAD` and
    /// close the log.
    pub(super) fn cherry_pick_selected_commit(&mut self) {
        let Some(panel) = self.git_log.as_ref() else {
            return;
        };
        let Some(sha) = panel.selected_sha().map(str::to_string) else {
            return;
        };
        let abbrev = panel.selected_abbrev().unwrap_or(&sha).to_string();
        if self.commit_signing_blocked() {
            return;
        }
        self.git_log = None;
        match crate::git::cherry_pick(&self.root, &sha) {
            Ok(()) => {
                self.status = t!("status.git_cherry_picked", sha = abbrev).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_failed_reason", error = git_error_text(&e)).to_string()),
        }
        self.after_tree_change();
    }
}
