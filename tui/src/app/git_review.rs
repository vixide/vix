//! Branch review (T665): **Git → Review Range…** opens a revision range (a
//! branch comparison, a commit, a stash) as one read-only multi-file diff
//! buffer. Line comments are saved per repository in
//! `.vix/review/<range>.json` and can be exported as Markdown. The model, the
//! rendering and the export live in `vix-review`; the diff comes from
//! `vix-git`; this file only wires them to the editor. Stage and discard act
//! on the hunk under the cursor by handing its patch to `git apply` (T665
//! follow-up); the stash chooser opens a stash here with `r`.

use std::path::PathBuf;

use vix_review::{Rendered, Review, Side, SideBySide, Target};

use super::git::git_error_text;
use super::{App, Confirm, ConfirmKind, Prompt, PromptKind};

/// The review shown in the editor, if any.
pub struct ReviewState {
    /// The range, its diff and its comments.
    pub review: Review,
    /// The buffer as last rendered, mapping each row to what it shows.
    pub rendered: Rendered,
    /// The path the review tab carries (a title, not a file).
    pub tab_path: PathBuf,
    /// The diff line a comment prompt is open for: file index, side, line.
    pub pending: Option<(usize, Side, usize)>,
    /// The hunk a discard confirmation is open for: file index, hunk index.
    pub pending_hunk: Option<(usize, usize)>,
    /// Whether the two-pane (side-by-side) view is wanted. It is drawn only
    /// when the editor is wide enough ([`vix_review::MIN_SIDE_BY_SIDE_WIDTH`]);
    /// otherwise the inline buffer shows.
    pub side_by_side: bool,
    /// The review laid out for the two-pane view, rebuilt with `rendered`.
    pub panes: SideBySide,
}

impl App {
    /// Dispatch a `git.review*` action. `true` when `action` is one.
    pub(super) fn run_git_review_action(&mut self, action: &str) -> bool {
        match action {
            "git.review" => self.begin_review(),
            "git.review_comment" => self.begin_review_comment(),
            "git.review_export" => self.export_review(),
            "git.review_next" => self.review_goto_hunk(true),
            "git.review_prev" => self.review_goto_hunk(false),
            "git.review_stage_hunk" => self.review_stage_hunk(),
            "git.review_stage_all" => self.review_stage_all(),
            "git.review_discard_hunk" => self.begin_review_discard_hunk(),
            "git.review_discard_all" => self.begin_review_discard_all(),
            "git.review_toggle_view" => self.review_toggle_view(),
            _ => return false,
        }
        true
    }

    /// Ask for the range to review.
    fn begin_review(&mut self) {
        if !self.require_git_repo() {
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::GitReview,
            t!("prompt.git_review").to_string(),
        ));
    }

    /// `PromptKind::GitReview`'s accept handler: diff `range` and open it.
    pub(super) fn open_review(&mut self, range: &str) {
        if range.is_empty() || !self.require_git_repo() {
            return;
        }
        let Some(diff) = crate::git::review_diff(&self.root, range) else {
            self.messages
                .error(t!("msg.git_review_failed", range = range).to_string());
            return;
        };
        self.open_review_from_diff(range, &diff);
    }

    /// Open `diff` (the unified diff of `range`) as a review tab, loading any
    /// comments already saved for the range.
    pub fn open_review_from_diff(&mut self, range: &str, diff: &str) {
        let mut review = Review::new(range, diff);
        if review.files.is_empty() {
            self.status = t!("status.git_review_empty", range = range).to_string();
            return;
        }
        let saved =
            crate::authority::current().read_to_string_guarded(&self.review_file(range, "json"));
        if let Ok(json) = saved {
            review.load_json(&json);
        }
        let tab_path = PathBuf::from(format!("review {range}"));
        let rendered = review.render();
        let panes = review.side_by_side();
        let side_by_side = self.settings.review_view == "side_by_side";
        self.status = t!(
            "status.git_review_opened",
            range = range,
            files = review.files.len(),
            comments = review.comments.len()
        )
        .to_string();
        self.open_readonly_text_tab(&tab_path.to_string_lossy(), &rendered.text);
        self.review = Some(ReviewState {
            review,
            rendered,
            tab_path,
            pending: None,
            pending_hunk: None,
            side_by_side,
            panes,
        });
    }

    /// `.vix/review/<range>.<ext>` under the workspace root.
    fn review_file(&self, range: &str, ext: &str) -> PathBuf {
        self.root
            .join(".vix")
            .join("review")
            .join(format!("{}.{ext}", vix_review::store_name(range)))
    }

    /// The review state when the active tab is its tab.
    fn active_review(&self) -> Option<&ReviewState> {
        let state = self.review.as_ref()?;
        (self.active_path().as_ref() == Some(&state.tab_path)).then_some(state)
    }

    /// **Git → Comment on Review Line…**: prompt for the comment on the diff
    /// line (or its existing comment row) under the cursor.
    fn begin_review_comment(&mut self) {
        let row = self.editor.cursor_line();
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return;
        };
        let target = match state.rendered.targets.get(row) {
            Some(Target::Line { file, side, line }) => Some((*file, *side, *line)),
            // On a comment row: edit that comment.
            Some(Target::Comment(i)) => state.review.comments.get(*i).and_then(|c| {
                let file = state.review.files.iter().position(|f| f.path == c.file)?;
                Some((file, c.side, c.line))
            }),
            _ => None,
        };
        let Some((file, side, line)) = target else {
            self.status = t!("status.git_review_no_line").to_string();
            return;
        };
        let existing = state
            .review
            .files
            .get(file)
            .and_then(|f| state.review.comment_text(&f.path, side, line))
            .unwrap_or_default()
            .to_string();
        if let Some(s) = self.review.as_mut() {
            s.pending = Some((file, side, line));
        }
        self.prompt = Some(
            Prompt::new(
                PromptKind::GitReviewComment,
                t!("prompt.git_review_comment").to_string(),
            )
            .with_input(existing),
        );
    }

    /// `PromptKind::GitReviewComment`'s accept handler: store (or, when
    /// empty, remove) the comment, save the file, and redraw the review.
    pub(super) fn accept_review_comment(&mut self, text: &str) {
        let Some(state) = self.review.as_mut() else {
            return;
        };
        let Some((file, side, line)) = state.pending.take() else {
            return;
        };
        let Some(path) = state.review.files.get(file).map(|f| f.path.clone()) else {
            return;
        };
        let code = state
            .review
            .line_text(file, side, line)
            .unwrap_or_default()
            .to_string();
        state.review.set_comment(&path, side, line, &code, text);
        let range = state.review.range.clone();
        self.save_review_comments(&range);
        self.redraw_review();
    }

    /// Write the comments of the open review to `.vix/review/<range>.json`.
    fn save_review_comments(&mut self, range: &str) {
        let Some(json) = self.review.as_ref().map(|s| s.review.to_json()) else {
            return;
        };
        if let Err(e) = self.write_review_file(range, "json", &json) {
            self.messages
                .error(t!("msg.git_review_save_failed", error = e).to_string());
        }
    }

    /// Write `contents` to `.vix/review/<range>.<ext>`, creating the folder.
    fn write_review_file(&self, range: &str, ext: &str, contents: &str) -> std::io::Result<()> {
        let path = self.review_file(range, ext);
        let auth = crate::authority::current();
        if let Some(dir) = path.parent() {
            auth.create_dir_all(dir)?;
        }
        auth.write(&path, contents.as_bytes())
    }

    /// Re-render the review into its tab, keeping the cursor row.
    fn redraw_review(&mut self) {
        let row = self.editor.cursor_line();
        let Some(state) = self.review.as_mut() else {
            return;
        };
        state.rendered = state.review.render();
        state.panes = state.review.side_by_side();
        let (title, text) = (
            state.tab_path.to_string_lossy().into_owned(),
            state.rendered.text.clone(),
        );
        self.editor.close_active();
        self.open_readonly_text_tab(&title, &text);
        self.editor.goto(row + 1, None, self.layout.editor);
    }

    /// **Git → Export Review Comments**: write `.vix/review/<range>.md` and
    /// `.json` for the open review.
    fn export_review(&mut self) {
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return;
        };
        let range = state.review.range.clone();
        let count = state.review.comments.len();
        let (md, json) = (state.review.to_markdown(), state.review.to_json());
        let result = self
            .write_review_file(&range, "md", &md)
            .and_then(|()| self.write_review_file(&range, "json", &json));
        match result {
            Ok(()) => {
                self.status = t!(
                    "status.git_review_exported",
                    count = count,
                    path = self.review_file(&range, "md").display().to_string()
                )
                .to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_review_save_failed", error = e).to_string()),
        }
    }

    /// **Git → Next/Previous Review Hunk**: move to the next or previous hunk
    /// header of the review buffer, wrapping around.
    fn review_goto_hunk(&mut self, forward: bool) {
        let row = self.editor.cursor_line();
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return;
        };
        let target = if forward {
            state.rendered.next_hunk(row)
        } else {
            state.rendered.prev_hunk(row)
        };
        if let Some(r) = target {
            self.editor.goto(r + 1, None, self.layout.editor);
        }
    }

    /// The hunk under the cursor of the active review: `(file, hunk, file_name,
    /// diff_text)`. Says why in the status line when there is none.
    fn review_hunk_under_cursor(&mut self) -> Option<(usize, usize, String, String)> {
        let row = self.editor.cursor_line();
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return None;
        };
        let Some((file, hunk)) = state.rendered.hunk_at(row) else {
            self.status = t!("status.git_review_no_hunk").to_string();
            return None;
        };
        let diff_text = state.review.hunk_patch(file, hunk)?;
        let file_name = state.review.files.get(file)?.path.clone();
        Some((file, hunk, file_name, diff_text))
    }

    /// **Git → Stage Review Hunk**: stage the hunk under the cursor.
    fn review_stage_hunk(&mut self) {
        let Some((_, _, file_name, diff_text)) = self.review_hunk_under_cursor() else {
            return;
        };
        match crate::git::apply_patch(&self.root, &diff_text, crate::git::ApplyTarget::Index) {
            Ok(()) => {
                self.status = t!("status.git_review_staged", path = file_name).to_string();
                self.refresh_git();
                self.refresh_git_gutter();
                self.rediff_review();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_review_apply_failed", error = git_error_text(&e)).to_string()),
        }
    }

    /// **Git → Stage All Review Hunks**: stage every text hunk of the open
    /// review at once (for a stash, the whole stash).
    fn review_stage_all(&mut self) {
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return;
        };
        let range = state.review.range.clone();
        let Some(diff_text) = state.review.full_patch() else {
            self.status = t!("status.git_review_no_hunk").to_string();
            return;
        };
        match crate::git::apply_patch(&self.root, &diff_text, crate::git::ApplyTarget::Index) {
            Ok(()) => {
                self.status = t!("status.git_review_staged_all", range = range).to_string();
                self.refresh_git();
                self.refresh_git_gutter();
                self.rediff_review();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_review_apply_failed", error = git_error_text(&e)).to_string()),
        }
    }

    /// After a stage or discard, diff the range again and redraw the review
    /// with the new diff, keeping its comments. When git cannot diff the range
    /// any more the buffer is left as it was.
    fn rediff_review(&mut self) {
        let Some(range) = self.review.as_ref().map(|s| s.review.range.clone()) else {
            return;
        };
        let Some(diff) = crate::git::review_diff(&self.root, &range) else {
            return;
        };
        if let Some(s) = self.review.as_mut() {
            s.review.replace_diff(&diff);
        }
        self.redraw_review();
    }

    /// **Git → Discard Review Hunk…**: ask before undoing the hunk under the
    /// cursor in the working tree.
    fn begin_review_discard_hunk(&mut self) {
        let Some((file, hunk, file_name, _)) = self.review_hunk_under_cursor() else {
            return;
        };
        if let Some(s) = self.review.as_mut() {
            s.pending_hunk = Some((file, hunk));
        }
        self.confirm = Some(Confirm {
            message: t!("confirm.git_review_discard", path = file_name.as_str()).to_string(),
            paths: vec![PathBuf::from(file_name)],
            kind: ConfirmKind::GitReviewDiscard,
        });
    }

    /// The answer to [`ConfirmKind::GitReviewDiscard`]: undo the pending hunk
    /// on yes (git refuses when it no longer applies).
    pub(super) fn review_discard_answered(&mut self, yes: bool) {
        let pending = self.review.as_mut().and_then(|s| s.pending_hunk.take());
        let (true, Some((file, hunk))) = (yes, pending) else {
            return;
        };
        let Some((diff_text, file_name)) = self.review.as_ref().and_then(|s| {
            Some((
                s.review.hunk_patch(file, hunk)?,
                s.review.files.get(file)?.path.clone(),
            ))
        }) else {
            return;
        };
        match crate::git::apply_patch(
            &self.root,
            &diff_text,
            crate::git::ApplyTarget::RevertWorktree,
        ) {
            Ok(()) => {
                self.status = t!("status.git_review_discarded", path = file_name).to_string();
                self.reload_clean_buffers();
                self.refresh_git();
                self.refresh_git_gutter();
                self.rediff_review();
            }
            Err(e) => self
                .messages
                .error(t!("msg.git_review_apply_failed", error = git_error_text(&e)).to_string()),
        }
    }

    /// **Git → Discard All Review Hunks…**: ask, naming how many hunks and
    /// files, before undoing every text hunk of the open review.
    fn begin_review_discard_all(&mut self) {
        let Some(state) = self.active_review() else {
            self.status = t!("status.git_review_none").to_string();
            return;
        };
        let (hunks, files) = state.review.patch_counts();
        if hunks == 0 {
            self.status = t!("status.git_review_no_hunk").to_string();
            return;
        }
        self.confirm = Some(Confirm {
            message: t!(
                "confirm.git_review_discard_all",
                hunks = hunks,
                files = files
            )
            .to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::GitReviewDiscardAll,
        });
    }

    /// The answer to [`ConfirmKind::GitReviewDiscardAll`]: on yes, reverse the
    /// whole review patch in one `git apply`, which is all or nothing, so a
    /// hunk that no longer fits leaves the working tree untouched and git's
    /// reason is reported.
    pub(super) fn review_discard_all_answered(&mut self, yes: bool) {
        if !yes {
            return;
        }
        let Some((patch, range, counts)) = self.review.as_ref().and_then(|s| {
            Some((
                s.review.full_patch()?,
                s.review.range.clone(),
                s.review.patch_counts(),
            ))
        }) else {
            return;
        };
        match crate::git::apply_patch(&self.root, &patch, crate::git::ApplyTarget::RevertWorktree) {
            Ok(()) => {
                self.status = t!(
                    "status.git_review_discarded_all",
                    hunks = counts.0,
                    files = counts.1,
                    range = range
                )
                .to_string();
                self.reload_clean_buffers();
                self.refresh_git();
                self.refresh_git_gutter();
                self.rediff_review();
            }
            Err(e) => self.messages.error(
                t!(
                    "msg.git_review_discard_all_failed",
                    error = git_error_text(&e)
                )
                .to_string(),
            ),
        }
    }

    /// **Git → Review Two-Pane View** (and `v` in the review): switch the
    /// open review between the inline buffer and the side-by-side view.
    fn review_toggle_view(&mut self) {
        if self.active_review().is_none() {
            self.status = t!("status.git_review_none").to_string();
            return;
        }
        let Some(state) = self.review.as_mut() else {
            return;
        };
        state.side_by_side = !state.side_by_side;
        self.status = if state.side_by_side {
            t!("status.git_review_view_side_by_side")
        } else {
            t!("status.git_review_view_inline")
        }
        .to_string();
    }

    /// `v` in the review tab toggles the view. `true` when the key was taken.
    pub(super) fn review_view_key(&mut self, key: vix_input::KeyEvent) -> bool {
        use vix_input::{KeyCode, KeyModifiers};
        if key.code != KeyCode::Char('v')
            || !(key.modifiers - KeyModifiers::SHIFT).is_empty()
            || self.active_review().is_none()
        {
            return false;
        }
        self.review_toggle_view();
        true
    }

    /// The two-pane layout to draw into a text area `width` columns wide, or
    /// `None` when the inline buffer shows instead: the view is off, `width`
    /// is under [`vix_review::MIN_SIDE_BY_SIDE_WIDTH`], or the active tab is
    /// not the review.
    #[must_use]
    pub fn review_panes_for(&self, width: u16) -> Option<&SideBySide> {
        let state = self.active_review()?;
        (state.side_by_side && width >= vix_review::MIN_SIDE_BY_SIDE_WIDTH).then_some(&state.panes)
    }
}
