//! Version control: Git status/diff/staging/branches/log/blame and the
//! Git panel, plus a sibling Jujutsu (`jj`) dispatcher for repos that use
//! it instead.
//!
//! Moved out of `app.rs` verbatim (T141, slice 2); unlike the keymap-
//! dispatch slice, these methods were scattered across the file rather
//! than one contiguous block, interleaved with the debugger, spellcheck,
//! and context-menu code that stayed behind. `run_git_action` also
//! dispatches the `run.*` debugger actions (a pre-existing quirk, not
//! introduced by this move) -- those call back into `app.rs`'s own
//! debugger methods, which is fine: an inherent `impl App` block can
//! live in any module of the crate.

#![warn(clippy::pedantic)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{
    App, BranchChooser, DiffViewState, GitPanel, Prompt, PromptKind, gutter_hex, rect_contains,
};
use crate::editor::Tab;

impl App {
    /// Dispatch a `git.*` action. Returns `true` if `action` was handled.
    /// Extracted from [`App::run_action`] to keep that function within the line
    /// limit.
    pub(super) fn run_git_action(&mut self, action: &str) -> bool {
        match action {
            "git.changes" => self.open_git_panel(),
            "git.push" => self.git_remote_command("git push"),
            "git.pull_ff" => self.git_remote_command("git pull --ff-only"),
            "git.pull_rebase" => self.git_remote_command("git pull --rebase"),
            "git.pull_merge" => self.git_remote_command("git pull --no-rebase"),
            "git.pull_squash" => self.git_remote_command("git pull --squash"),
            "git.fetch" => self.git_remote_command("git fetch"),
            "git.switch_branch" => self.open_branch_chooser(),
            "git.merge_branch" => self.open_branch_chooser_mode(true),
            "git.init" => self.git_init(),
            "git.new_branch" => self.git_begin_new_branch(),
            "git.log" => self.git_log(),
            "git.log_graph" => self.git_log_graph(),
            "git.log_since_1_day_ago" => self.git_log_since(Some("1-day-ago")),
            "git.log_since_1_week_ago" => self.git_log_since(Some("1-week-ago")),
            "git.log_since_1_month_ago" => self.git_log_since(Some("1-month-ago")),
            "git.status" => self.git_status_to_dock(),
            "git.clone" => self.git_begin_clone(),
            "git.edit_description" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GitEditDescription,
                    t!("prompt.git_edit_description").to_string(),
                ));
            }
            "git.delete_branch" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GitDeleteBranch,
                    t!("prompt.git_delete_branch").to_string(),
                ));
            }
            "git.grep" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GitGrep,
                    t!("prompt.git_grep").to_string(),
                ));
            }
            "git.blame" => self.git_blame_line(),
            "git.blame_inline" => self.toggle_inline_blame(),
            "run.start" => self.start_debugger(),
            "run.stop" => self.stop_debugger(),
            "run.toggle_breakpoint" => self.toggle_breakpoint(),
            "run.continue" => self.dap.continue_(),
            "run.step_over" => self.dap.step_over(),
            "run.step_into" => self.dap.step_into(),
            "run.step_out" => self.dap.step_out(),
            "run.pause" => self.dap.pause(),
            "run.panel" => self.show_debug_panel = !self.show_debug_panel,
            "run.repl" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::DebugRepl,
                    t!("prompt.debug_repl").to_string(),
                ));
            }
            "run.watch" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::DebugWatch,
                    t!("prompt.debug_watch").to_string(),
                ));
            }
            "git.revert_hunk" => self.revert_hunk(),
            "git.stage_hunk" => self.stage_hunk(),
            "git.unstage_hunk" => self.unstage_hunk(),
            "git.conflict_ours" => self.resolve_conflict(crate::conflict_tool::Resolution::Ours),
            "git.conflict_theirs" => {
                self.resolve_conflict(crate::conflict_tool::Resolution::Theirs);
            }
            "git.conflict_both" => self.resolve_conflict(crate::conflict_tool::Resolution::Both),
            "git.conflict_next" => self.conflict_next(),
            "git.stash" => self.git_op(crate::git::stash_push, "status.git_stashed"),
            "git.stash_pop" => self.git_op(crate::git::stash_pop, "status.git_stash_popped"),
            "git.amend" => self.git_op(crate::git::commit_amend, "status.git_amended"),
            "git.diff_next" => self.diff_goto(true),
            "git.diff_prev" => self.diff_goto(false),
            // Jujutsu (`jj.*`) is a sibling VCS dispatcher; chain to it here so
            // `run_action` needs only the one Git delegation arm.
            other => return self.run_jj_action(other),
        }
        true
    }

    /// Dispatch a Jujutsu (`jj.*`) version-control action, streaming command
    /// output into the bottom dock. Returns `true` if `action` was handled.
    /// Mirrors [`App::run_git_action`] for the jj VCS.
    fn run_jj_action(&mut self, action: &str) -> bool {
        match action {
            "jj.init" => self.jj_init(),
            "jj.clone" => self.jj_begin_clone(),
            "jj.status" => self.jj_command("jj --no-pager status"),
            "jj.diff" => self.jj_command("jj --no-pager diff"),
            "jj.show" => self.jj_command("jj --no-pager show"),
            "jj.log" => self.jj_command("jj --no-pager log"),
            "jj.log_all" => self.jj_command("jj --no-pager log -r ::"),
            "jj.op_log" => self.jj_command("jj --no-pager op log"),
            "jj.new" => self.jj_command("jj new"),
            "jj.describe" => self.jj_begin_prompt(PromptKind::JjDescribe, "prompt.jj_describe"),
            "jj.commit" => self.jj_begin_prompt(PromptKind::JjCommit, "prompt.jj_commit"),
            "jj.edit" => self.jj_begin_prompt(PromptKind::JjEdit, "prompt.jj_edit"),
            "jj.squash" => self.jj_command("jj squash"),
            "jj.abandon" => self.jj_command("jj abandon"),
            "jj.restore" => self.jj_command("jj restore"),
            "jj.rebase" => self.jj_begin_prompt(PromptKind::JjRebase, "prompt.jj_rebase"),
            "jj.undo" => self.jj_command("jj undo"),
            "jj.bookmark_create" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkCreate, "prompt.jj_bookmark_create");
            }
            "jj.bookmark_set" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkSet, "prompt.jj_bookmark_set");
            }
            "jj.bookmark_delete" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkDelete, "prompt.jj_bookmark_delete");
            }
            "jj.bookmark_list" => self.jj_command("jj --no-pager bookmark list"),
            "jj.git_push" => self.jj_command("jj git push"),
            "jj.git_fetch" => self.jj_command("jj git fetch"),
            _ => return false,
        }
        true
    }

    /// Whether the workspace root holds a Jujutsu repo (a `.jj` directory).
    fn jj_is_repo(&self) -> bool {
        self.root.join(".jj").exists()
    }

    /// Run a `jj` command in the workspace, streaming its output into the bottom
    /// dock, but only when the workspace is a jj repo.
    fn jj_command(&mut self, cmd: &str) {
        if !self.jj_is_repo() {
            self.status = t!("status.jj_not_repo").into();
            return;
        }
        self.run_command(cmd);
    }

    /// Open a single-line prompt (of `kind`, titled by i18n key `title`) for a jj
    /// command, but only when the workspace is a jj repo.
    fn jj_begin_prompt(&mut self, kind: PromptKind, title: &str) {
        if !self.jj_is_repo() {
            self.status = t!("status.jj_not_repo").into();
            return;
        }
        self.prompt = Some(Prompt::new(kind, t!(title).to_string()));
    }

    /// Initialize a Jujutsu repo in the workspace, refusing if one already exists.
    /// Colocates with an existing Git repo (so both tools share the working copy)
    /// when a `.git` is present; otherwise creates a plain jj-backed repo.
    fn jj_init(&mut self) {
        if self.jj_is_repo() {
            self.status = t!("status.jj_already_init").into();
            return;
        }
        let colocate = self.root.join(".git").exists() || crate::git::is_repo(&self.root);
        self.run_command(if colocate {
            "jj git init --colocate"
        } else {
            "jj git init"
        });
    }

    /// Begin cloning a repository with jj: prompt for its URL (works outside a
    /// repo too, so it is not guarded by [`App::jj_is_repo`]).
    fn jj_begin_clone(&mut self) {
        self.prompt = Some(Prompt::new(
            PromptKind::JjClone,
            t!("prompt.jj_clone").to_string(),
        ));
    }

    /// Clone `url` into the workspace with `jj git clone`.
    fn jj_clone(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            self.status = t!("status.jj_empty_url").into();
            return;
        }
        self.run_command(&format!("jj git clone {}", Self::shell_single_quote(url)));
    }

    /// Set the working-copy change description (`jj describe -m`).
    fn jj_describe(&mut self, message: &str) {
        self.jj_message_command("jj describe -m", message);
    }

    /// Describe the working-copy change and start a new one (`jj commit -m`).
    fn jj_commit(&mut self, message: &str) {
        self.jj_message_command("jj commit -m", message);
    }

    /// Run a `jj` command that takes a required `-m` message, rejecting an empty
    /// one and shell-quoting it.
    fn jj_message_command(&mut self, prefix: &str, message: &str) {
        let message = message.trim();
        if message.is_empty() {
            self.status = t!("status.jj_empty_message").into();
            return;
        }
        self.run_command(&format!("{prefix} {}", Self::shell_single_quote(message)));
    }

    /// Set the working-copy revision to `rev` (`jj edit`).
    fn jj_edit(&mut self, rev: &str) {
        let rev = rev.trim();
        if rev.is_empty() {
            self.status = t!("status.jj_empty_revision").into();
            return;
        }
        self.run_command(&format!("jj edit {}", Self::shell_single_quote(rev)));
    }

    /// Rebase the working-copy change onto destination `dest` (`jj rebase -d`).
    fn jj_rebase(&mut self, dest: &str) {
        let dest = dest.trim();
        if dest.is_empty() {
            self.status = t!("status.jj_empty_revision").into();
            return;
        }
        self.run_command(&format!("jj rebase -d {}", Self::shell_single_quote(dest)));
    }

    /// Run a `jj bookmark <sub> <name>` command, rejecting an empty name.
    fn jj_bookmark(&mut self, sub: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.status = t!("status.jj_empty_bookmark").into();
            return;
        }
        self.run_command(&format!(
            "jj bookmark {sub} {}",
            Self::shell_single_quote(name)
        ));
    }

    /// Handle a completed Jujutsu prompt: run the jj command for the entered
    /// `input`. Grouped out of [`App::accept_prompt`] to keep it within the line
    /// limit, mirroring [`App::accept_roam_prompt`].
    pub(super) fn accept_jj_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::JjClone => self.jj_clone(input),
            PromptKind::JjDescribe => self.jj_describe(input),
            PromptKind::JjCommit => self.jj_commit(input),
            PromptKind::JjEdit => self.jj_edit(input),
            PromptKind::JjRebase => self.jj_rebase(input),
            PromptKind::JjBookmarkCreate => self.jj_bookmark("create", input),
            PromptKind::JjBookmarkSet => self.jj_bookmark("set", input),
            PromptKind::JjBookmarkDelete => self.jj_bookmark("delete", input),
            _ => {}
        }
    }

    /// Refresh the cached git state (repo?, branch, changed files) for the workspace
    /// root. Cheap enough to call after saves and git actions; not per-frame.
    pub fn refresh_git(&mut self) {
        self.git_repo = crate::git::is_repo(&self.root);
        // HEAD may have moved (commit/checkout) or the working tree changed; drop
        // the cached HEAD blobs so the diff gutter refetches.
        self.git_head_cache.clear();
        if self.git_repo {
            self.git_branch = crate::git::branch(&self.root);
            self.git_status = crate::git::status(&self.root);
        } else {
            self.git_branch = None;
            self.git_status.clear();
        }
    }

    /// Recompute the editor diff gutter for the active tab: a colored bar on each
    /// line that differs from its committed (HEAD) version. The HEAD blob is
    /// fetched once per path and cached.
    pub fn refresh_git_gutter(&mut self) {
        if !self.git_repo {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_gutter_marks();
            }
            return;
        }
        let Some((path, current)) = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            t.path.clone().map(|p| (p, t.text()))
        }) else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_gutter_marks();
            }
            return;
        };
        if !self.git_head_cache.contains_key(&path) {
            let head = path
                .strip_prefix(&self.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .and_then(|rel| crate::git::head_blob(&self.root, &rel))
                .unwrap_or_default();
            self.git_head_cache.insert(path.clone(), head);
        }
        let marks = crate::git::diff_marks(&self.git_head_cache[&path], &current);
        let styled: Vec<(usize, &str)> = marks
            .iter()
            .map(|&(line, m)| (line, gutter_hex(m)))
            .collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_gutter_marks(styled);
        }
    }

    /// The diff hunks for the active tab (committed HEAD vs the current buffer),
    /// populating the HEAD blob cache on demand. Empty outside a repo or for
    /// images / unsaved buffers.
    fn active_hunks(&mut self) -> Vec<crate::git::Hunk> {
        if !self.git_repo {
            return Vec::new();
        }
        let Some((path, current)) = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            t.path.clone().map(|p| (p, t.text()))
        }) else {
            return Vec::new();
        };
        if !self.git_head_cache.contains_key(&path) {
            let head = path
                .strip_prefix(&self.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .and_then(|rel| crate::git::head_blob(&self.root, &rel))
                .unwrap_or_default();
            self.git_head_cache.insert(path.clone(), head);
        }
        crate::git::hunks(&self.git_head_cache[&path], &current)
    }

    /// Move the cursor to the next (or previous) changed hunk, wrapping around.
    pub(super) fn diff_goto(&mut self, forward: bool) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let target = if forward {
            hunks.iter().map(|h| h.current_start).find(|&s| s > line)
        } else {
            hunks
                .iter()
                .rev()
                .map(|h| h.current_start)
                .find(|&s| s < line)
        };
        // Wrap to the first/last hunk when there is none beyond the cursor.
        let target = target.unwrap_or_else(|| {
            if forward {
                hunks[0].current_start
            } else {
                hunks[hunks.len() - 1].current_start
            }
        });
        let area = self.editor_view();
        self.editor.goto(target + 1, None, area);
    }

    /// Restore the committed (HEAD) version of the hunk under the cursor in the
    /// working buffer. No-op (with a status) when there is no hunk there.
    fn revert_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some(content) = self.editor.active_tab().map(Tab::text) else {
            return;
        };
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let start = hunk.current_start.min(lines.len());
        let end = hunk.current_end.min(lines.len()).max(start);
        let mut rebuilt = String::new();
        rebuilt.push_str(&lines[..start].concat());
        rebuilt.push_str(&hunk.head_text);
        rebuilt.push_str(&lines[end..].concat());
        let caret: usize = lines[..start].iter().map(|l| l.chars().count()).sum();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_content(&rebuilt);
            t.editor.set_cursor(caret);
            t.editor.set_selection(None);
            t.dirty = true;
            t.preview = false;
        }
        self.refresh_git_gutter();
        self.status = t!("status.hunk_reverted").to_string();
    }

    /// Resolve the merge conflict at (or after) the cursor by keeping `how`.
    fn resolve_conflict(&mut self, how: crate::conflict_tool::Resolution) {
        let Some((content, line)) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (t.text(), t.editor.cursor_line()))
        else {
            return;
        };
        let Some(conflict) = crate::conflict_tool::find(&content, line) else {
            self.status = t!("status.no_conflict").to_string();
            return;
        };
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let mut rebuilt = lines[..conflict.start].concat();
        rebuilt.push_str(&conflict.resolved(how));
        rebuilt.push_str(&lines[conflict.end.min(lines.len())..].concat());
        let caret: usize = lines[..conflict.start]
            .iter()
            .map(|l| l.chars().count())
            .sum();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_content(&rebuilt);
            t.editor.set_cursor(caret);
            t.editor.set_selection(None);
            t.dirty = true;
            t.preview = false;
        }
        self.refresh_git_gutter();
        self.status = t!("status.conflict_resolved").to_string();
    }

    /// Move the cursor to the next merge conflict at or after it.
    fn conflict_next(&mut self) {
        let Some((content, line)) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (t.text(), t.editor.cursor_line()))
        else {
            return;
        };
        // Search from the line after the cursor so repeated calls advance.
        match crate::conflict_tool::find(&content, line + 1)
            .or_else(|| crate::conflict_tool::find(&content, 0))
        {
            Some(c) => {
                let area = self.editor_view();
                self.editor.goto(c.start + 1, None, area);
            }
            None => self.status = t!("status.no_conflict").to_string(),
        }
    }

    /// Run a workspace-level git op (stash/amend), then refresh state and report
    /// success with `ok_key` or the error in the status line.
    fn git_op(&mut self, op: fn(&Path) -> Result<(), String>, ok_key: &str) {
        if !self.git_repo {
            self.status = t!("status.git_not_repo").to_string();
            return;
        }
        match op(&self.root) {
            Ok(()) => {
                self.refresh_git();
                self.refresh_git_gutter();
                self.status = t!(ok_key).to_string();
            }
            Err(e) => self.status = t!("msg.git_failed_reason", error = e).to_string(),
        }
    }

    /// Stage just the hunk under the cursor into the git index, leaving the rest
    /// of the file's changes unstaged and the working tree untouched. Safe: it
    /// only stages when the index still matches HEAD for the hunk's region.
    fn stage_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some((path, current)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.text())))
        else {
            return;
        };
        let Ok(rel) = path.strip_prefix(&self.root) else {
            self.status = t!("status.stage_hunk_failed", error = "outside workspace").to_string();
            return;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        // Base the new index content on the currently-staged version (== HEAD
        // when nothing is staged yet), so other hunks stay unstaged.
        let base = crate::git::index_blob(&self.root, &rel)
            .or_else(|| self.git_head_cache.get(&path).cloned())
            .unwrap_or_default();
        let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
        let head_count = hunk.head_text.split_inclusive('\n').count();
        // The hunk's committed lines must still be present in the index region,
        // or staging this hunk could corrupt other staged changes.
        let region = base_lines.get(hunk.head_start..hunk.head_start + head_count);
        if region.map(<[&str]>::concat).as_deref() != Some(hunk.head_text.as_str()) {
            self.status = t!("status.stage_hunk_failed", error = "index diverged").to_string();
            return;
        }
        let cur_lines: Vec<&str> = current.split_inclusive('\n').collect();
        let added = cur_lines
            .get(hunk.current_start..hunk.current_end)
            .map(<[&str]>::concat)
            .unwrap_or_default();
        let mut new_index = base_lines[..hunk.head_start].concat();
        new_index.push_str(&added);
        new_index.push_str(&base_lines[hunk.head_start + head_count..].concat());
        match crate::git::stage_content(&self.root, &rel, &new_index) {
            Ok(()) => {
                self.refresh_git();
                self.status = t!("status.hunk_staged").to_string();
            }
            Err(e) => self.status = t!("status.stage_hunk_failed", error = e).to_string(),
        }
    }

    /// Unstage just the hunk under the cursor from the git index, leaving the
    /// rest of the file's staged changes and the working tree untouched. The
    /// mirror of [`App::stage_hunk`]: safe — it only unstages when the hunk's
    /// working-tree lines are present in the index at the expected position.
    fn unstage_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some((path, current)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.text())))
        else {
            return;
        };
        let Ok(rel) = path.strip_prefix(&self.root) else {
            self.status = t!("status.unstage_hunk_failed", error = "outside workspace").to_string();
            return;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        // The index must currently carry this hunk's working-tree lines (i.e. it
        // is staged); replacing them with the committed text removes it.
        let Some(base) = crate::git::index_blob(&self.root, &rel) else {
            self.status = t!("status.unstage_hunk_failed", error = "nothing staged").to_string();
            return;
        };
        let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
        let cur_lines: Vec<&str> = current.split_inclusive('\n').collect();
        let added = cur_lines
            .get(hunk.current_start..hunk.current_end)
            .map(<[&str]>::concat)
            .unwrap_or_default();
        let added_count = hunk.current_end - hunk.current_start;
        let region = base_lines.get(hunk.head_start..hunk.head_start + added_count);
        if region.map(<[&str]>::concat).as_deref() != Some(added.as_str()) {
            self.status = t!("status.unstage_hunk_failed", error = "hunk not staged").to_string();
            return;
        }
        let mut new_index = base_lines[..hunk.head_start].concat();
        new_index.push_str(&hunk.head_text);
        new_index.push_str(&base_lines[hunk.head_start + added_count..].concat());
        match crate::git::stage_content(&self.root, &rel, &new_index) {
            Ok(()) => {
                self.refresh_git();
                self.status = t!("status.hunk_unstaged").to_string();
            }
            Err(e) => self.status = t!("status.unstage_hunk_failed", error = e).to_string(),
        }
    }

    /// Whether the working tree has uncommitted changes (derived from the cached
    /// `git status`).
    #[must_use]
    pub fn git_dirty(&self) -> bool {
        !self.git_status.is_empty()
    }

    /// The git change for a file `path` (absolute, under the workspace root), from
    /// the cached status — `None` when not in a repo or the file is unchanged.
    #[must_use]
    pub fn git_change_for(&self, path: &Path) -> Option<crate::git::Change> {
        if !self.git_repo {
            return None;
        }
        let rel = path
            .strip_prefix(&self.root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        self.git_status
            .iter()
            .find(|s| s.path == rel)
            .and_then(crate::git::FileStatus::primary)
    }

    /// Open the git changes panel (refreshing status first). Reports a status when
    /// the workspace root is not a git repository.
    fn open_git_panel(&mut self) {
        self.refresh_git();
        if !self.git_repo {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = Some(GitPanel { selected: 0 });
    }

    pub(super) fn git_panel_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.git_panel.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.git_panel.as_mut()
                    && p.selected + 1 < self.git_status.len()
                {
                    p.selected += 1;
                }
            }
            // Space / s / u toggle or set the staged state of the selected file.
            KeyCode::Char(' ') => self.git_toggle_stage(),
            KeyCode::Char('s' | 'S') => self.git_stage_selected(true),
            KeyCode::Char('u' | 'U') => self.git_stage_selected(false),
            KeyCode::Char('c' | 'C') => self.git_begin_commit(),
            KeyCode::Char('r' | 'R') => {
                self.refresh_git();
                self.clamp_git_selection();
            }
            KeyCode::Esc => self.git_panel = None,
            _ => {}
        }
    }

    pub(super) fn git_panel_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.git_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        if row < self.git_status.len() {
            if let Some(p) = self.git_panel.as_mut() {
                p.selected = row;
            }
            self.git_toggle_stage();
        }
    }

    /// The repo-relative path of the selected changed file.
    fn git_selected_path(&self) -> Option<String> {
        let idx = self.git_panel.as_ref()?.selected;
        self.git_status.get(idx).map(|s| s.path.clone())
    }

    fn clamp_git_selection(&mut self) {
        if let Some(p) = self.git_panel.as_mut() {
            p.selected = p.selected.min(self.git_status.len().saturating_sub(1));
        }
    }

    /// Stage (or unstage) the selected file, then refresh.
    fn git_stage_selected(&mut self, stage: bool) {
        let Some(path) = self.git_selected_path() else {
            return;
        };
        let ok = if stage {
            crate::git::stage(&self.root, &path)
        } else {
            crate::git::unstage(&self.root, &path)
        };
        if !ok {
            self.messages.error(t!("msg.git_failed").to_string());
        }
        self.refresh_git();
        self.clamp_git_selection();
    }

    /// Toggle the selected file between staged and unstaged based on its current
    /// state (staged → unstage; otherwise stage).
    fn git_toggle_stage(&mut self) {
        let staged = self
            .git_panel
            .as_ref()
            .and_then(|p| self.git_status.get(p.selected))
            .is_some_and(crate::git::FileStatus::is_staged);
        self.git_stage_selected(!staged);
    }

    /// Begin a commit: prompt for a message (only when something is staged).
    fn git_begin_commit(&mut self) {
        let any_staged = self
            .git_status
            .iter()
            .any(crate::git::FileStatus::is_staged);
        if !any_staged {
            self.status = t!("status.git_nothing_staged").into();
            return;
        }
        self.git_panel = None;
        self.prompt = Some(Prompt::new(
            PromptKind::GitCommit,
            t!("prompt.git_commit").to_string(),
        ));
    }

    /// Run `git commit -m <message>` and report the outcome.
    pub(super) fn git_commit(&mut self, message: &str) {
        let message = message.trim();
        if message.is_empty() {
            self.status = t!("status.git_empty_message").into();
            return;
        }
        match crate::git::commit(&self.root, message) {
            Ok(()) => self.status = t!("status.git_committed").into(),
            Err(e) => self
                .messages
                .error(t!("msg.git_commit_failed", error = e).to_string()),
        }
        self.refresh_git();
    }

    /// Begin creating a topic branch: prompt for its name (only in a repo).
    fn git_begin_new_branch(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.prompt = Some(Prompt::new(
            PromptKind::GitNewBranch,
            t!("prompt.git_new_branch").to_string(),
        ));
    }

    /// Create a new topic branch named `name` and switch to it.
    pub(super) fn git_create_branch(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.status = t!("status.git_empty_branch").into();
            return;
        }
        match crate::git::create_branch(&self.root, name) {
            Ok(()) => {
                self.status = t!("status.git_switched", branch = name).to_string();
                self.refresh_git();
                // Files on disk may now differ; refresh the explorer tree.
                self.explorer.rebuild();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_branch_failed", error = e).to_string()),
        }
    }

    /// Show the commit history, streaming `git log` into the bottom dock.
    fn git_log(&mut self) {
        self.git_log_since(None);
    }

    /// Show the commit log, optionally limited to commits newer than `since`
    /// (a git date spec like `1-day-ago`), streaming it into the bottom dock.
    fn git_log_since(&mut self, since: Option<&str>) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        let filter = since.map(|s| format!(" --since={s}")).unwrap_or_default();
        self.run_command(&format!("git --no-pager log{filter}"));
    }

    /// Show a decorated commit graph across all refs, streaming it into the
    /// bottom dock.
    fn git_log_graph(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(
            "git --no-pager log --graph --topo-order --date=iso8601-strict \
             --no-abbrev-commit --decorate --all --boundary \
             --pretty=format:'%ad %h -%d %s [%aN <%aE>] %G?'",
        );
    }

    /// Show the working-tree status, streaming `git status` into the bottom dock.
    fn git_status_to_dock(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command("git --no-pager status");
    }

    /// Initialize a git repository in the workspace, refusing (for safety) if one
    /// already exists (a `.git` directory or a detected repo).
    fn git_init(&mut self) {
        if self.root.join(".git").exists() || crate::git::is_repo(&self.root) {
            self.status = t!("status.git_already_init").to_string();
            return;
        }
        self.run_command("git init");
    }

    /// Begin cloning a repository: prompt for its URL (works outside a repo too).
    fn git_begin_clone(&mut self) {
        self.git_panel = None;
        self.prompt = Some(Prompt::new(
            PromptKind::GitClone,
            t!("prompt.git_clone").to_string(),
        ));
    }

    /// Clone `url` into the workspace, streaming `git clone` into the bottom dock.
    pub(super) fn git_clone(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            self.status = t!("status.git_empty_url").into();
            return;
        }
        self.run_command(&format!("git clone {url}"));
    }

    /// Set the current branch's description (`git branch --edit-description`),
    /// feeding the prompted text via a throwaway `GIT_EDITOR` that copies it into
    /// the description file (so no interactive editor opens).
    pub(super) fn git_edit_description(&mut self, desc: &str) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let Ok(tmp) = crate::fileops::write_private_temp("vix-branchdesc", desc.as_bytes()) else {
            self.status = t!("status.git_not_repo").into();
            return;
        };
        let path = tmp.display();
        self.git_panel = None;
        self.run_command(&format!(
            "GIT_EDITOR='cp \"{path}\"' git branch --edit-description"
        ));
    }

    /// Delete the named branch (`git branch --delete`), streaming the result to
    /// the bottom dock.
    pub(super) fn git_delete_branch(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(&format!("git branch --delete {name}"));
    }

    /// Search the repository for `pattern` (`git grep`), streaming matches with
    /// line numbers into the bottom dock.
    pub(super) fn git_grep(&mut self, pattern: &str) {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return;
        }
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        // Single-quote the pattern so shell metacharacters in the regex are safe.
        let quoted = format!("'{}'", pattern.replace('\'', "'\\''"));
        self.run_command(&format!("git --no-pager grep -n -e {quoted}"));
    }

    /// Annotate the cursor's current line with its `git blame` attribution
    /// (short hash, author, date, and commit summary) in the status bar.
    fn git_blame_line(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let Some((path, line)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line() + 1)))
        else {
            self.status = t!("status.blame_no_file").into();
            return;
        };
        // Blame from the file's own directory so git resolves the repo itself —
        // robust to symlinked roots (e.g. macOS `/var` → `/private/var`).
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            self.status = t!("status.blame_no_file").into();
            return;
        };
        let rel = name.to_string_lossy();
        match crate::git::blame_line(dir, &rel, line) {
            Some(b) if b.is_uncommitted() => {
                self.status = t!("status.blame_uncommitted", line = line).to_string();
            }
            Some(b) => {
                self.status = t!(
                    "status.blame",
                    line = line,
                    hash = b.hash,
                    author = b.author,
                    date = b.date,
                    summary = b.summary
                )
                .to_string();
            }
            None => self.status = t!("status.blame_none").into(),
        }
    }

    /// Toggle the inline (end-of-line) git blame for the cursor's line, persisting
    /// the preference. Clears the annotation immediately when turned off.
    fn toggle_inline_blame(&mut self) {
        self.settings.inline_blame = !self.settings.inline_blame;
        let on = self.settings.inline_blame;
        if !on {
            self.blame_cache = None;
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_eol_note(None);
            }
        }
        self.status = t!(if on {
            "status.inline_blame_on"
        } else {
            "status.inline_blame_off"
        })
        .to_string();
    }

    /// Refresh the inline blame for the cursor's line when enabled. Cheap: blames
    /// only when the cursor moves to a different line (cached in `blame_cache`).
    /// Called once per event-loop iteration.
    pub fn refresh_inline_blame(&mut self) {
        if !self.settings.inline_blame {
            if self.blame_cache.take().is_some()
                && let Some(t) = self.editor.active_tab_mut()
            {
                t.editor.set_eol_note(None);
            }
            return;
        }
        let here = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line())));
        let Some((path, line0)) = here else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_eol_note(None);
            }
            return;
        };
        let line = line0 + 1;
        if self.blame_cache.as_ref() == Some(&(path.clone(), line)) {
            return;
        }
        self.blame_cache = Some((path.clone(), line));
        let note = match (path.parent(), path.file_name()) {
            (Some(dir), Some(name)) => {
                match crate::git::blame_line(dir, &name.to_string_lossy(), line) {
                    Some(b) if b.is_uncommitted() => t!("blame.uncommitted").to_string(),
                    Some(b) => t!(
                        "blame.inline",
                        author = b.author,
                        date = b.date,
                        summary = b.summary
                    )
                    .to_string(),
                    None => String::new(),
                }
            }
            _ => String::new(),
        };
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_eol_note(if note.is_empty() {
                None
            } else {
                Some((line0, note))
            });
        }
    }

    /// Run a remote git command (push/pull/fetch) asynchronously, streaming its
    /// output to the bottom dock. Git state refreshes when it completes.
    fn git_remote_command(&mut self, cmd: &str) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(cmd);
    }

    fn open_branch_chooser(&mut self) {
        self.open_branch_chooser_mode(false);
    }

    /// Open the branch chooser; `merge` picks merge-into-current rather than
    /// checkout.
    fn open_branch_chooser_mode(&mut self, merge: bool) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let branches = crate::git::local_branches(&self.root);
        if branches.is_empty() {
            self.status = t!("status.git_no_branches").into();
            return;
        }
        self.branch_chooser = Some(BranchChooser {
            branches,
            selected: 0,
            merge,
        });
    }

    pub(super) fn branch_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.branch_chooser.as_mut() {
                    let n = c.branches.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.branch_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.branches.len();
                }
            }
            KeyCode::Enter => self.checkout_selected_branch(),
            KeyCode::Esc => self.branch_chooser = None,
            _ => {}
        }
    }

    pub(super) fn branch_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.branch_chooser.as_mut()
            && idx < c.branches.len()
        {
            c.selected = idx;
            self.checkout_selected_branch();
        }
    }

    /// Open a read-only unified-diff overlay comparing the active buffer with the
    /// file at `input` (resolved relative to the workspace root).
    pub(super) fn open_diff_with(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let other = self.resolve(input);
        let Ok(other_text) = std::fs::read_to_string(&other) else {
            self.messages
                .error(t!("msg.open_failed", error = other.display()).to_string());
            return;
        };
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let current = tab.editor.get_content();
        let here = tab.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || t!("ui.untitled").to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let there = other
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lines = crate::diff_view::build(&other_text, &current);
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.diff_view = Some(DiffViewState {
            title: format!("{there} ↔ {here}"),
            lines,
            scroll: 0,
        });
    }

    pub(super) fn diff_view_key(&mut self, key: KeyEvent) {
        let page = self.layout.editor.height.max(1) as usize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.diff_view = None,
            KeyCode::Up => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + 1).min(d.lines.len().saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + page).min(d.lines.len().saturating_sub(1));
                }
            }
            _ => {}
        }
    }

    /// Apply the highlighted branch and close the chooser: merge it into the
    /// current branch (merge mode) or check it out.
    fn checkout_selected_branch(&mut self) {
        let Some(c) = self.branch_chooser.take() else {
            return;
        };
        let Some(branch) = c.branches.get(c.selected).cloned() else {
            return;
        };
        if c.merge {
            // Merge streams its output (and any conflicts) to the bottom dock;
            // git state and the tree refresh when it finishes.
            self.run_command(&format!("git merge {branch}"));
            return;
        }
        match crate::git::checkout(&self.root, &branch) {
            Ok(()) => {
                self.status = t!("status.git_switched", branch = branch).to_string();
                self.refresh_git();
                // Files on disk may now differ; refresh the explorer tree and
                // reload any open clean buffers so they reflect the new branch.
                self.explorer.rebuild();
                let n = self.editor.reload_clean_from_disk();
                if n > 0 {
                    self.messages
                        .info(t!("status.git_reloaded", count = n).to_string());
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_checkout_failed", error = e).to_string()),
        }
    }
}
