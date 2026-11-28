//! Workspace edits bound to the version the user previewed (T864).
//!
//! **Replace in Files**, **Structural Replace in Workspace** and the
//! editable search results all plan a whole-file rewrite from a file's
//! current text, show it, and write it only when the user confirms. Between
//! the two the file can change: edited on disk by another program, deleted,
//! or (when an open buffer was the source) edited again in Vix. A
//! [`ReplaceReview`] records, for each planned target, a fingerprint of the
//! exact text the plan was computed from; [`App::apply_reviewed`] reads the
//! text the target would be written over again — the dirty buffer, or the
//! file through the authority — and writes only when it is still that text.
//! A changed target is skipped and offered again in a refreshed preview
//! ([`App::refresh_conflicted`]); a deleted one is reported and never
//! recreated. The summary counts what was really replaced, from the
//! per-target counts the preview showed.

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

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::geo::Geo;
use vix_structural_replace::Pattern;

use super::App;

/// A fingerprint of `text`: the version a target was previewed at. A
/// content hash rather than a modification time, so a rewrite within the
/// clock's resolution, or one that keeps the size, is still noticed.
pub(crate) fn fingerprint(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Which flow made a review, so its changed targets can be planned again.
pub(crate) enum ReviewOrigin {
    /// Replace in Files: the workspace search panel's pattern and replacement.
    Text,
    /// Structural Replace in Workspace, with its compiled pattern and template.
    Structural {
        /// The compiled `$X` pattern.
        pattern: Pattern,
        /// The replacement template.
        template: String,
    },
    /// The editable search-results buffer.
    EditedResults,
}

/// One planned target and the version of it the user saw.
pub(crate) struct ReviewedTarget {
    /// Replacements (or edited lines) the preview listed for it.
    pub(crate) count: usize,
    /// [`fingerprint`] of the text the plan was computed from.
    pub(crate) source: u64,
}

/// What a confirmation is bound to: one [`ReviewedTarget`] per row of the
/// `ReplaceConfirm` plan it accompanies, in the same order.
pub(crate) struct ReplaceReview {
    /// The targets, parallel to the plan.
    pub(crate) targets: Vec<ReviewedTarget>,
    /// The flow that planned them.
    pub(crate) origin: ReviewOrigin,
    /// The workspace file list was incomplete when this was planned (still
    /// indexing, or cut short at its cap): files not listed may also match,
    /// so `y`/Enter refuse and only `!` applies (PQ-04).
    pub(crate) partial: bool,
}

impl ReplaceReview {
    /// An empty review for `origin`.
    pub(crate) fn new(origin: ReviewOrigin, partial: bool) -> Self {
        Self {
            targets: Vec::new(),
            origin,
            partial,
        }
    }

    /// Record the next plan row's target, planned from `source_text` with
    /// `count` changes.
    pub(crate) fn push(&mut self, count: usize, source_text: &str) {
        self.targets.push(ReviewedTarget {
            count,
            source: fingerprint(source_text),
        });
    }
}

/// What applying a review did.
#[derive(Default)]
pub(crate) struct Applied {
    /// Targets changed (on disk, or in a dirty buffer), with their counts.
    pub(crate) files: Vec<(PathBuf, usize)>,
    /// Targets skipped because their text changed since the preview.
    pub(crate) conflicted: Vec<PathBuf>,
}

impl Applied {
    /// Total replacements actually made.
    pub(crate) fn replaced(&self) -> usize {
        self.files.iter().fold(0, |n, (_, c)| n.plus(*c))
    }

    /// Whether `path` was changed.
    pub(crate) fn changed(&self, path: &Path) -> bool {
        self.files.iter().any(|(p, _)| p == path)
    }
}

impl App {
    /// The canonical form of `path` as tabs store it; for a path that no
    /// longer exists, its canonical directory joined with its name.
    fn canonical_target(path: &Path) -> PathBuf {
        let auth = crate::authority::current();
        if let Ok(canon) = auth.canonicalize(path) {
            return canon;
        }
        match (path.parent(), path.file_name()) {
            (Some(dir), Some(name)) => auth
                .canonicalize(dir)
                .map_or_else(|_| path.to_path_buf(), |d| d.join(name)),
            _ => path.to_path_buf(),
        }
    }

    /// Apply a confirmed per-file `plan` bound to `review` (T653, T864).
    ///
    /// - A path open in a **read-only** tab (the View-only toggle, a lossy
    ///   or synthetic buffer) is refused and reported, never written.
    /// - A path open in a **dirty** tab is changed in that buffer only and
    ///   left dirty (writing it would also save the tab's unrelated unsaved
    ///   edits), and only while the buffer still holds the previewed text.
    /// - Everything else is written with `write_atomic`, only while the
    ///   file, read again through the authority, still holds the previewed
    ///   text; a clean open tab on it is refreshed to match.
    /// - A target whose text changed is skipped and listed in
    ///   [`Applied::conflicted`]; a file deleted since the preview is
    ///   reported and not recreated.
    ///
    /// The on-save normalizations (trim trailing whitespace, final newline)
    /// are deliberately not applied: the preview showed exactly these
    /// contents, and a targeted replace must not rewrite unrelated lines.
    pub(super) fn apply_reviewed(
        &mut self,
        plan: &[(PathBuf, String)],
        review: &ReplaceReview,
    ) -> Applied {
        let auth = crate::authority::current();
        let mut applied = Applied::default();
        let mut buffer_only = 0usize;
        for ((path, new), target) in plan.iter().zip(&review.targets) {
            let canon = Self::canonical_target(path);
            let open = self
                .editor
                .tabs
                .iter()
                .position(|t| t.path.as_deref() == Some(canon.as_path()));
            if let Some(tab) = open.and_then(|i| self.editor.tabs.get_mut(i)) {
                if tab.read_only || tab.is_image() {
                    self.messages.error(
                        t!("msg.replace_read_only_skipped", path = path.display()).to_string(),
                    );
                    continue;
                }
                if tab.dirty {
                    if fingerprint(&tab.text()) != target.source {
                        self.messages.warn(
                            t!("msg.replace_target_changed", path = path.display()).to_string(),
                        );
                        applied.conflicted.push(path.clone());
                        continue;
                    }
                    tab.editor.set_content(new);
                    buffer_only = buffer_only.plus(1);
                    applied.files.push((path.clone(), target.count));
                    continue;
                }
            }
            match auth.read_to_string_guarded(path) {
                Ok(disk) if fingerprint(&disk) == target.source => {}
                Err(_) if !auth.exists(path) => {
                    self.messages
                        .warn(t!("msg.replace_target_deleted", path = path.display()).to_string());
                    continue;
                }
                Ok(_) | Err(_) => {
                    self.messages
                        .warn(t!("msg.replace_target_changed", path = path.display()).to_string());
                    applied.conflicted.push(path.clone());
                    continue;
                }
            }
            if let Err(e) = auth.write_text_in_file_encoding(path, new) {
                self.messages
                    .error(t!("msg.write_failed", path = path.display(), error = e).to_string());
                continue;
            }
            if let Some(tab) = open.and_then(|i| self.editor.tabs.get_mut(i)) {
                tab.editor.set_content(new);
                tab.dirty = false;
            }
            applied.files.push((path.clone(), target.count));
        }
        if buffer_only > 0 {
            self.messages
                .warn(t!("msg.replace_buffer_only", count = buffer_only).to_string());
        }
        applied
    }

    /// Plan the `conflicted` targets of a just-applied review again, from
    /// their current text, and open the matching preview so the user can
    /// review them a second time. Returns whether a preview opened (a
    /// target that no longer matches drops out).
    pub(super) fn refresh_conflicted(
        &mut self,
        origin: ReviewOrigin,
        conflicted: &[PathBuf],
    ) -> bool {
        if conflicted.is_empty() {
            return false;
        }
        let opened = match origin {
            ReviewOrigin::Text => self.plan_text_replace(Some(conflicted)),
            ReviewOrigin::Structural { pattern, template } => {
                self.plan_structural_workspace(&pattern, &template, Some(conflicted))
            }
            ReviewOrigin::EditedResults => self.plan_wgrep_confirm(Some(conflicted)),
        };
        if opened {
            let note = t!("status.replace_conflicts_review", count = conflicted.len()).to_string();
            self.messages.warn(note.clone());
            self.status = note;
        }
        opened
    }

    /// Whether the workspace file list is incomplete, so a workspace-wide
    /// plan may miss files (PQ-04).
    pub(super) fn file_index_incomplete(&self) -> bool {
        self.file_index_state != super::FileIndexState::Complete
    }
}
