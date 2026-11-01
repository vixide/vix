//! Branch review (T665): **Git → Review Range…** opens a revision range (a
//! branch comparison, a commit, a stash) as one read-only multi-file diff
//! buffer. Line comments are saved per repository in
//! `.vix/review/<range>.json` and can be exported as Markdown. The model, the
//! rendering and the export live in `vix-review`; the diff comes from
//! `vix-git`; this file only wires them to the editor.

use std::path::PathBuf;

use vix_review::{Rendered, Review, Side, Target};

use super::{App, Prompt, PromptKind};

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
}
