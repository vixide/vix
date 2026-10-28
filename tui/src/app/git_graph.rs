//! **Git → Log → Graph…** (T681): the interactive commit graph. The panel
//! state, lane algorithm and git calls live in the `vix-git-graph` crate; this
//! module is the App side -- opening the overlay, loading pages as the
//! highlight nears the end, and the keys and clicks.
//!
//! `Enter` opens the highlighted commit's diff as a read-only `git show` tab
//! (the T207 machinery); `Ctrl+O`, `Ctrl+B` and `Ctrl+P` check out, branch
//! from and cherry-pick the commit (the first and last behind a yes/no);
//! `Ctrl+Y` copies its hash; `Tab` steps the branch/tag filter; typing
//! searches.

use vix_git_graph::{Action, GraphError, Mode, PAGE_SIZE, Panel};
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::{App, rect_contains};

/// The user-facing text of a graph error, with the install hint for a missing
/// `git`.
fn error_text(e: &GraphError) -> String {
    match e {
        GraphError::NotInstalled => t!("msg.git_not_installed").to_string(),
        other => other.to_string(),
    }
}

impl App {
    /// Open the graph over the whole repository, first page only.
    pub(super) fn open_git_graph(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        let commits = match vix_git_graph::fetch_page(&self.root, 0, PAGE_SIZE, None) {
            Ok(c) => c,
            Err(e) => {
                self.messages
                    .error(t!("msg.git_graph_failed", error = error_text(&e)).to_string());
                return;
            }
        };
        if commits.is_empty() {
            self.status = t!("status.git_no_commits").into();
            return;
        }
        let refs = vix_git_graph::list_refs(&self.root).unwrap_or_default();
        let exhausted = commits.len() < PAGE_SIZE;
        self.git_graph = Some(Panel::new(commits, refs, exhausted));
    }

    /// Load the next page when the highlight is near the end of what's loaded.
    fn git_graph_load_more(&mut self) {
        let Some(p) = self.git_graph.as_ref() else {
            return;
        };
        if !p.wants_more() {
            return;
        }
        let (skip, reference) = (p.commits.len(), p.reference.clone());
        match vix_git_graph::fetch_page(&self.root, skip, PAGE_SIZE, reference.as_deref()) {
            Ok(page) => {
                if let Some(p) = self.git_graph.as_mut() {
                    p.append_page(page, PAGE_SIZE);
                }
            }
            Err(e) => {
                if let Some(p) = self.git_graph.as_mut() {
                    p.exhausted = true;
                }
                self.messages
                    .error(t!("msg.git_graph_failed", error = error_text(&e)).to_string());
            }
        }
    }

    /// Reload the first page, keeping the current ref filter and search.
    fn git_graph_reload(&mut self) {
        let Some(reference) = self.git_graph.as_ref().map(|p| p.reference.clone()) else {
            return;
        };
        match vix_git_graph::fetch_page(&self.root, 0, PAGE_SIZE, reference.as_deref()) {
            Ok(page) => {
                if let Some(p) = self.git_graph.as_mut() {
                    p.replace(page, PAGE_SIZE);
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_graph_failed", error = error_text(&e)).to_string()),
        }
    }

    pub(super) fn git_graph_key(&mut self, key: KeyEvent) {
        let mode = self
            .git_graph
            .as_ref()
            .map_or(Mode::Browse, |p| p.mode.clone());
        match mode {
            Mode::Browse => self.git_graph_browse_key(key),
            Mode::Confirm { action, sha } => self.git_graph_confirm_key(key, action, &sha),
            Mode::NewBranch { sha, name } => self.git_graph_branch_key(key, &sha, name),
        }
        self.git_graph_load_more();
    }

    fn git_graph_browse_key(&mut self, key: KeyEvent) {
        let page = (self.layout.git_graph.height as usize).max(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Some(p) = self.git_graph.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => p.up(),
            KeyCode::Down => p.down(),
            KeyCode::PageUp => p.page_up(page),
            KeyCode::PageDown => p.page_down(page),
            KeyCode::Backspace => p.backspace(),
            KeyCode::Tab | KeyCode::BackTab => {
                p.cycle_reference(key.code == KeyCode::Tab);
                self.git_graph_reload();
            }
            KeyCode::Enter => self.open_git_graph_diff(),
            KeyCode::Esc => self.git_graph = None,
            KeyCode::Char('o' | 'O') if ctrl => self.git_graph_ask(Some(Action::Checkout)),
            KeyCode::Char('p' | 'P') if ctrl => self.git_graph_ask(Some(Action::CherryPick)),
            KeyCode::Char('b' | 'B') if ctrl => self.git_graph_ask(None),
            KeyCode::Char('y' | 'Y') if ctrl => self.git_graph_copy_hash(),
            KeyCode::Char(c) if !ctrl => p.push(c),
            _ => {}
        }
    }

    /// Start a mutating action on the highlighted commit: a yes/no for
    /// `Some(action)`, the branch-name input for `None`.
    fn git_graph_ask(&mut self, action: Option<Action>) {
        let Some(p) = self.git_graph.as_mut() else {
            return;
        };
        let Some(sha) = p.selected_commit().map(|c| c.sha.clone()) else {
            return;
        };
        p.mode = match action {
            Some(action) => Mode::Confirm { action, sha },
            None => Mode::NewBranch {
                sha,
                name: String::new(),
            },
        };
    }

    fn git_graph_confirm_key(&mut self, key: KeyEvent, action: Action, sha: &str) {
        let yes = matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter);
        if let Some(p) = self.git_graph.as_mut() {
            p.mode = Mode::Browse;
        }
        if !yes {
            return;
        }
        match action {
            Action::Checkout => self.git_graph_checkout(sha),
            Action::CherryPick => self.git_graph_cherry_pick(sha),
        }
    }

    fn git_graph_branch_key(&mut self, key: KeyEvent, sha: &str, mut name: String) {
        match key.code {
            KeyCode::Esc => {
                if let Some(p) = self.git_graph.as_mut() {
                    p.mode = Mode::Browse;
                }
            }
            KeyCode::Enter => {
                if let Some(p) = self.git_graph.as_mut() {
                    p.mode = Mode::Browse;
                }
                let name = name.trim();
                if name.is_empty() {
                    self.status = t!("status.git_empty_branch").into();
                    return;
                }
                match vix_git_graph::create_branch_at(&self.root, name, sha) {
                    Ok(()) => {
                        self.status = t!("status.git_switched", branch = name).to_string();
                        self.git_graph_after_change();
                    }
                    Err(e) => self
                        .messages
                        .error(t!("msg.git_branch_failed", error = error_text(&e)).to_string()),
                }
            }
            code => {
                match code {
                    KeyCode::Backspace => {
                        name.pop();
                    }
                    KeyCode::Char(c) => name.push(c),
                    _ => return,
                }
                if let Some(p) = self.git_graph.as_mut() {
                    p.mode = Mode::NewBranch {
                        sha: sha.to_string(),
                        name,
                    };
                }
            }
        }
    }

    fn git_graph_checkout(&mut self, sha: &str) {
        let Some(commit) = self
            .git_graph
            .as_ref()
            .and_then(|p| p.commits.iter().find(|c| c.sha == sha))
            .cloned()
        else {
            return;
        };
        match vix_git_graph::checkout(&self.root, &commit) {
            Ok(vix_git_graph::CheckoutTarget::Branch(b)) => {
                self.status = t!("status.git_switched", branch = b).to_string();
                self.git_graph_after_change();
            }
            Ok(vix_git_graph::CheckoutTarget::Detached(_)) => {
                self.status = t!("status.git_graph_detached", sha = commit.abbrev).to_string();
                self.git_graph_after_change();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_checkout_failed", error = error_text(&e)).to_string()),
        }
    }

    fn git_graph_cherry_pick(&mut self, sha: &str) {
        let abbrev = self
            .git_graph
            .as_ref()
            .and_then(|p| p.commits.iter().find(|c| c.sha == sha))
            .map_or_else(|| sha.to_string(), |c| c.abbrev.clone());
        match vix_git_graph::cherry_pick(&self.root, sha) {
            Ok(()) => {
                self.status = t!("status.git_graph_picked", sha = abbrev).to_string();
                self.git_graph_after_change();
            }
            Err(e) => self.messages.error(
                t!(
                    "msg.git_cherry_pick_failed",
                    sha = abbrev,
                    error = error_text(&e)
                )
                .to_string(),
            ),
        }
    }

    /// Refresh everything a checkout, new branch or cherry-pick may have
    /// changed: git state, the explorer, clean buffers, and the graph itself.
    fn git_graph_after_change(&mut self) {
        self.refresh_git();
        self.explorer.rebuild();
        let n = self.reload_clean_buffers();
        if n > 0 {
            self.messages
                .info(t!("status.git_reloaded", count = n).to_string());
        }
        if let Some(p) = self.git_graph.as_mut()
            && let Ok(refs) = vix_git_graph::list_refs(&self.root)
        {
            p.refs = refs;
        }
        self.git_graph_reload();
    }

    fn git_graph_copy_hash(&mut self) {
        let Some(sha) = self
            .git_graph
            .as_ref()
            .and_then(Panel::selected_commit)
            .map(|c| c.sha.clone())
        else {
            return;
        };
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.copy_to_clipboard(&sha);
        } else {
            vix_clipboard::set(&sha).ok();
        }
        self.status = t!("status.git_graph_copied", sha = sha).to_string();
    }

    /// Open the highlighted commit's diff in a read-only tab titled
    /// `<abbrev> graph`, and close the panel.
    fn open_git_graph_diff(&mut self) {
        let Some((sha, abbrev)) = self
            .git_graph
            .as_ref()
            .and_then(Panel::selected_commit)
            .map(|c| (c.sha.clone(), c.abbrev.clone()))
        else {
            return;
        };
        let Some(patch) = crate::git::show_commit(&self.root, &sha, None) else {
            self.messages
                .error(t!("msg.git_show_failed", sha = abbrev).to_string());
            return;
        };
        self.git_graph = None;
        self.open_readonly_text_tab(&format!("{abbrev} graph"), &patch);
    }

    /// A click selects a row; a click on the already-highlighted row opens
    /// its diff.
    pub(super) fn git_graph_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.git_graph;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        let Some(p) = self.git_graph.as_mut() else {
            return;
        };
        let idx = p.scroll + row;
        let again = p.selected == idx;
        if p.select_index(idx) && again {
            self.open_git_graph_diff();
        }
    }
}
