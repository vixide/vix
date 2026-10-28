//! Format Document and format-on-save through the user's own Prettier
//! (T683). Pure process plumbing lives in `vix_format_tool::prettier`; this
//! file decides *when* it applies and writes the result back to the buffer.
//!
//! A project-local `node_modules/.bin/prettier` is project-controlled code,
//! so it is used only in a workspace the user trusted (the same persisted
//! decision that gates project scripts, T132). Otherwise only a `PATH`
//! Prettier is considered.

use std::path::{Path, PathBuf};

use vix_format_tool::prettier::{self, PrettierError};

use super::App;

/// What [`App::prettier_format_active`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrettierOutcome {
    /// Prettier is not configured for this file; the caller should fall back
    /// to the language server.
    NotApplicable,
    /// Prettier ran and the buffer now holds its output.
    Formatted,
    /// Prettier was wanted but could not run; the status line says why.
    Failed,
}

impl App {
    /// Whether this workspace's project-local tools are trusted (the
    /// persisted script-trust decision, T132).
    fn workspace_trusted(&self) -> bool {
        self.load_session()
            .workspace(&self.session_key())
            .is_some_and(|w| w.scripts_trusted == Some(true))
    }

    /// The configured Prettier path for `path`, or why there is none.
    fn prettier_binary(&self, path: &Path) -> Result<PathBuf, PrettierError> {
        let trusted = self.workspace_trusted();
        prettier::resolve_binary(path, std::env::var_os("PATH").as_deref(), trusted)
            .ok_or(PrettierError::NotFound)
    }

    /// Format the active buffer with Prettier when `save.prettier_languages`
    /// lists its language. Sets the status line on every outcome.
    pub(super) fn prettier_format_active(&mut self) -> PrettierOutcome {
        let Some(path) = self.active_path() else {
            return PrettierOutcome::NotApplicable;
        };
        if !prettier::is_enabled_for(&path, &self.settings.save.prettier_languages) {
            return PrettierOutcome::NotApplicable;
        }
        let text = self
            .editor
            .active_tab()
            .map(crate::editor::Tab::text)
            .unwrap_or_default();
        let result: Result<String, PrettierError> = self.prettier_binary(&path).and_then(|bin| {
            #[cfg(feature = "tasks")]
            {
                prettier::format(&bin, &path, &text, prettier::TIMEOUT)
            }
            #[cfg(not(feature = "tasks"))]
            {
                let _ = (bin, &text);
                Err(PrettierError::NotFound)
            }
        });
        match result {
            Ok(out) => {
                self.apply_prettier_output(&out);
                self.status = t!("status.formatted").to_string();
                PrettierOutcome::Formatted
            }
            Err(e) => {
                self.status = match e {
                    PrettierError::NotFound if !self.workspace_trusted() => {
                        t!("status.prettier_missing_untrusted").to_string()
                    }
                    PrettierError::NotFound => t!("status.prettier_missing").to_string(),
                    PrettierError::TimedOut => t!("status.prettier_timeout").to_string(),
                    other => t!("status.prettier_failed", error = other.to_string()).to_string(),
                };
                PrettierOutcome::Failed
            }
        }
    }

    /// Replace the active buffer with `out`, keeping the caret in range.
    fn apply_prettier_output(&mut self, out: &str) {
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        if t.text() == out {
            return;
        }
        let caret = t.editor.get_cursor().min(out.chars().count());
        t.editor.set_content(out);
        t.editor.set_cursor(caret);
        t.dirty = true;
        t.preview = false;
    }

    /// Format-on-save through Prettier. Returns `true` when Prettier owns
    /// this file (success or failure), so the language-server path is
    /// skipped; the plain save already wrote the file, so on success the
    /// formatted buffer is written once more.
    pub(super) fn prettier_format_on_save(&mut self) -> bool {
        match self.prettier_format_active() {
            PrettierOutcome::NotApplicable => false,
            PrettierOutcome::Failed => true,
            PrettierOutcome::Formatted => {
                if self.editor.active_tab().is_some_and(|t| t.dirty) {
                    self.write_active_to_disk();
                    self.status = t!("status.formatted").to_string();
                }
                true
            }
        }
    }
}
