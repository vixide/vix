//! Interactive rebase and cherry-pick (T633): **Git → Rebase** and Browse
//! Log's `c`. The todo list is edited in an overlay and handed to
//! `git rebase -i` by `vix-git`'s runner through `GIT_SEQUENCE_EDITOR`, set
//! on that one process (see `crates/vix-git/src/rebase.rs`), so no editor
//! ever opens and Vix's own environment is never touched.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
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
            let idx = t.scroll.plus(row);
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

#[cfg(test)]
mod tests {
    use super::super::git::tests::{app_at, git, repo, temp};
    use super::*;
    use crate::app::cov4::{code, key};

    /// A repo on `main` whose history after the seed commit is `c1`, `c2`, `c3`
    /// (each adding its own file), with an app rooted in it.
    fn three_commits(tag: &str) -> (std::path::PathBuf, App) {
        let d = repo(tag, &[("seed.txt", "seed\n")]);
        for n in ["c1", "c2", "c3"] {
            std::fs::write(d.join(format!("{n}.txt")), format!("{n}\n")).unwrap();
            git(&d, &["add", "-A"]);
            git(&d, &["commit", "-q", "-m", n]);
        }
        let app = app_at(&d);
        (d, app)
    }

    fn subjects(dir: &std::path::Path) -> Vec<String> {
        git(dir, &["log", "--format=%s"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn rebase_rows(app: &App) -> Vec<(String, crate::git::RebaseAction)> {
        app.git_rebase
            .as_ref()
            .unwrap()
            .commits
            .iter()
            .map(|c| (c.subject.clone(), c.action))
            .collect()
    }

    #[test]
    fn unknown_rebase_actions_are_not_handled() {
        let d = temp("rebase-unknown");
        let mut app = app_at(&d);
        assert!(!app.run_git_rebase_action("git.rebase_nope"));
    }

    #[test]
    fn rebase_actions_outside_a_repository_say_so_and_open_nothing() {
        let d = temp("rebase-norepo");
        let mut app = app_at(&d);
        for action in [
            "git.rebase_interactive",
            "git.rebase_continue",
            "git.rebase_abort",
            "git.cherry_pick",
        ] {
            app.status.clear();
            assert!(app.run_git_rebase_action(action));
            assert_eq!(
                app.status,
                t!("status.git_not_repo").to_string(),
                "{action}"
            );
        }
        assert!(app.prompt.is_none() && app.git_rebase.is_none());
    }

    #[test]
    fn the_rebase_prompt_asks_for_a_base_and_continue_abort_need_a_stopped_rebase() {
        let (_d, mut app) = three_commits("rebase-prompt");
        assert!(app.run_git_rebase_action("git.rebase_interactive"));
        let prompt = app.prompt.take().unwrap();
        assert!(matches!(prompt.kind, PromptKind::GitRebaseBase));
        for action in ["git.rebase_continue", "git.rebase_abort"] {
            app.status.clear();
            app.run_git_rebase_action(action);
            assert_eq!(app.status, t!("status.git_rebase_none").to_string());
        }
    }

    #[test]
    fn beginning_a_rebase_lists_the_commits_after_the_base() {
        let (_d, mut app) = three_commits("rebase-begin");
        let status = app.status.clone();
        app.git_rebase_begin("");
        assert!(
            app.git_rebase.is_none() && app.status == status,
            "empty input is a no-op"
        );
        app.git_rebase_begin("HEAD");
        assert!(app.git_rebase.is_none());
        assert_eq!(
            app.status,
            t!("status.git_rebase_nothing", base = "HEAD").to_string()
        );
        app.git_rebase_begin("no-such-base");
        assert!(app.git_rebase.is_none());
        assert!(
            app.messages
                .items
                .last()
                .unwrap()
                .text
                .starts_with(t!("msg.git_failed_reason", error = "").trim_end())
        );
        app.git_rebase_begin("HEAD~3");
        let todo = app.git_rebase.as_ref().unwrap();
        assert_eq!(todo.base_label, "HEAD~3");
        assert_eq!(
            rebase_rows(&app)
                .iter()
                .map(|(s, a)| (s.as_str(), *a))
                .collect::<Vec<_>>(),
            [
                ("c1", crate::git::RebaseAction::Pick),
                ("c2", crate::git::RebaseAction::Pick),
                ("c3", crate::git::RebaseAction::Pick)
            ],
            "oldest first, all picks"
        );
    }

    #[test]
    fn overlay_keys_move_reorder_set_actions_and_cancel() {
        let (_d, mut app) = three_commits("rebase-keys");
        app.git_rebase_begin("HEAD~3");
        app.git_rebase_key(code(KeyCode::Down));
        assert_eq!(app.git_rebase.as_ref().unwrap().selected, 1);
        app.git_rebase_key(code(KeyCode::Up));
        assert_eq!(app.git_rebase.as_ref().unwrap().selected, 0);
        app.git_rebase_key(code(KeyCode::Down));
        // Alt+Up moves c2 above c1.
        app.git_rebase_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        let order: Vec<String> = rebase_rows(&app).into_iter().map(|(s, _)| s).collect();
        assert_eq!(order, ["c2", "c1", "c3"]);
        app.git_rebase_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        let order: Vec<String> = rebase_rows(&app).into_iter().map(|(s, _)| s).collect();
        assert_eq!(order, ["c1", "c2", "c3"]);
        // `d` drops the highlighted commit; an unbound letter changes nothing.
        app.git_rebase_key(key('d'));
        app.git_rebase_key(key('z'));
        app.git_rebase_key(code(KeyCode::Tab));
        assert_eq!(
            rebase_rows(&app)[2],
            ("c3".to_string(), crate::git::RebaseAction::Pick)
        );
        assert_eq!(rebase_rows(&app)[1].1, crate::git::RebaseAction::Drop);
        app.git_rebase_key(code(KeyCode::Esc));
        assert!(app.git_rebase.is_none());
        assert_eq!(app.status, t!("status.git_rebase_cancelled").to_string());
        // Keys with no overlay open do nothing.
        app.git_rebase_key(key('d'));
        app.git_rebase_key(code(KeyCode::Enter));
    }

    #[test]
    fn reword_prompts_with_the_subject_and_a_typed_message_is_remembered() {
        let (_d, mut app) = three_commits("rebase-reword");
        app.git_rebase_begin("HEAD~3");
        app.git_rebase_key(key('r'));
        let prompt = app.prompt.take().unwrap();
        assert!(matches!(prompt.kind, PromptKind::GitRebaseReword));
        assert_eq!(prompt.input, "c1", "seeded with the subject");
        assert_eq!(rebase_rows(&app)[0].1, crate::git::RebaseAction::Reword);
        app.git_rebase_reword("");
        assert_eq!(
            app.git_rebase.as_ref().unwrap().commits[0].message,
            None,
            "an empty message keeps the original"
        );
        app.git_rebase_reword("renamed one");
        assert_eq!(
            app.git_rebase.as_ref().unwrap().commits[0]
                .message
                .as_deref(),
            Some("renamed one")
        );
        app.git_rebase_key(key('r'));
        assert_eq!(
            app.prompt.take().unwrap().input,
            "renamed one",
            "reopens with it"
        );
        // With no overlay a reword has nothing to prompt about.
        app.git_rebase = None;
        app.git_rebase_key(key('r'));
        assert!(app.prompt.is_none());
        app.git_rebase_reword("ignored");
    }

    #[test]
    fn running_a_reordered_dropped_and_reworded_list_rewrites_history() {
        let (d, mut app) = three_commits("rebase-run");
        app.git_rebase_begin("HEAD~3");
        // c1 -> reworded, c2 -> dropped.
        app.git_rebase_key(key('r'));
        app.prompt = None;
        app.git_rebase_reword("first, renamed");
        app.git_rebase_key(code(KeyCode::Down));
        app.git_rebase_key(key('d'));
        app.git_rebase_key(code(KeyCode::Enter));
        assert!(app.git_rebase.is_none(), "the overlay closes");
        assert_eq!(app.status, t!("status.git_rebase_done").to_string());
        assert_eq!(subjects(&d), ["c3", "first, renamed", "seed"]);
        assert!(
            !d.join("c2.txt").exists(),
            "the dropped commit's file is gone"
        );
    }

    #[test]
    fn a_list_that_melds_the_first_commit_is_refused_before_anything_runs() {
        let (d, mut app) = three_commits("rebase-melds");
        app.git_rebase_begin("HEAD~3");
        app.git_rebase_key(key('s'));
        app.git_rebase_key(code(KeyCode::Enter));
        assert_eq!(app.status, t!("status.git_rebase_melds_first").to_string());
        assert!(app.git_rebase.is_some(), "the overlay stays for fixing");
        assert_eq!(subjects(&d), ["c3", "c2", "c1", "seed"]);
        // Fixing the first commit lets the squash of the second through.
        app.git_rebase_key(key('p'));
        app.git_rebase_key(code(KeyCode::Down));
        app.git_rebase_key(key('f'));
        app.git_rebase_key(code(KeyCode::Enter));
        assert_eq!(app.status, t!("status.git_rebase_done").to_string());
        assert_eq!(subjects(&d), ["c3", "c1", "seed"], "c2 melded into c1");
        assert!(d.join("c2.txt").exists(), "a fixup keeps the change");
    }

    #[test]
    fn an_edit_stop_can_be_continued_or_aborted() {
        let (d, mut app) = three_commits("rebase-edit");
        app.git_rebase_begin("HEAD~3");
        app.git_rebase_key(code(KeyCode::Down));
        app.git_rebase_key(key('e'));
        app.git_rebase_key(code(KeyCode::Enter));
        assert_eq!(app.status, t!("status.git_rebase_stopped").to_string());
        assert!(crate::git::rebase_in_progress(&d));
        // While stopped, starting another rebase explains how to finish.
        app.git_rebase_prompt();
        assert_eq!(app.status, t!("status.git_rebase_in_progress").to_string());
        assert!(app.prompt.is_none());
        app.git_rebase_continue();
        assert_eq!(app.status, t!("status.git_rebase_done").to_string());
        assert!(!crate::git::rebase_in_progress(&d));
        assert_eq!(subjects(&d), ["c3", "c2", "c1", "seed"]);

        // A second stop, this time abandoned.
        app.git_rebase_begin("HEAD~3");
        app.git_rebase_key(key('e'));
        app.git_rebase_key(code(KeyCode::Enter));
        assert!(crate::git::rebase_in_progress(&d));
        app.git_rebase_abort();
        assert_eq!(app.status, t!("status.git_rebase_aborted").to_string());
        assert!(!crate::git::rebase_in_progress(&d));
        assert_eq!(subjects(&d), ["c3", "c2", "c1", "seed"]);
    }

    #[test]
    fn clicking_a_row_selects_it_and_clicks_past_the_list_do_nothing() {
        let (_d, mut app) = three_commits("rebase-mouse");
        app.git_rebase_begin("HEAD~3");
        app.layout.chooser = ratatui::layout::Rect::new(0, 0, 40, 10);
        let before = app.git_rebase.as_ref().unwrap().selected;
        app.git_rebase_mouse(crate::app::cov4::click(5, 5));
        assert_eq!(
            app.git_rebase.as_ref().unwrap().selected,
            before,
            "row 5 is past three commits"
        );
        app.git_rebase = None;
        app.git_rebase_mouse(crate::app::cov4::click(5, 1));
    }

    #[test]
    fn cherry_picking_lists_the_missing_commits_and_applies_the_chosen_one() {
        let (d, mut app) = three_commits("cherry");
        git(&d, &["checkout", "-q", "-b", "feature", "HEAD~3"]);
        std::fs::write(d.join("f1.txt"), "f1\n").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-q", "-m", "feature one"]);
        git(&d, &["checkout", "-q", "main"]);
        app.refresh_git();

        assert!(app.run_git_rebase_action("git.cherry_pick"));
        assert!(matches!(
            app.prompt.take().unwrap().kind,
            PromptKind::GitCherryPickFrom
        ));
        app.git_cherry_pick_from("");
        assert!(app.git_log.is_none(), "empty input is a no-op");
        app.git_cherry_pick_from("main");
        assert!(app.git_log.is_none());
        assert_eq!(
            app.status,
            t!("status.git_cherry_pick_nothing", rev = "main").to_string()
        );
        app.git_cherry_pick_from("no-such-rev");
        assert!(app.git_log.is_none());
        assert!(
            app.messages
                .items
                .last()
                .unwrap()
                .text
                .starts_with(t!("msg.git_failed_reason", error = "").trim_end())
        );

        app.git_cherry_pick_from("feature");
        let panel = app.git_log.as_ref().unwrap();
        assert!(matches!(panel.scope, crate::git::LogScope::Pick(ref r) if r == "feature"));
        let abbrev = panel.selected_abbrev().unwrap().to_string();
        app.cherry_pick_selected_commit();
        assert!(app.git_log.is_none(), "the log closes");
        assert_eq!(
            app.status,
            t!("status.git_cherry_picked", sha = abbrev).to_string()
        );
        assert_eq!(subjects(&d)[0], "feature one");
        assert!(d.join("f1.txt").exists());
        // With no log open there is nothing to pick.
        app.cherry_pick_selected_commit();
    }

    #[test]
    fn a_failing_cherry_pick_reports_the_git_error() {
        let (d, mut app) = three_commits("cherry-conflict");
        git(&d, &["checkout", "-q", "-b", "other", "HEAD~3"]);
        std::fs::write(d.join("c3.txt"), "conflicting\n").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-q", "-m", "other c3"]);
        git(&d, &["checkout", "-q", "main"]);
        app.refresh_git();
        app.git_cherry_pick_from("other");
        app.cherry_pick_selected_commit();
        assert!(app.git_log.is_none());
        assert!(
            app.messages
                .items
                .last()
                .unwrap()
                .text
                .starts_with(t!("msg.git_failed_reason", error = "").trim_end())
        );
        git(&d, &["cherry-pick", "--abort"]);
    }
}
