//! The Large File Viewer (T733): a read-only, chunked view of a file too big
//! to load into a text buffer. The viewer itself (reads, line index, search)
//! lives in `vix-large-file`; this wires it to the open flow, the keys, the
//! clipboard, and the tick loop. See `crates/vix-large-file/spec/index.md`.

use std::path::Path;

use super::{App, Prompt, PromptKind};
use crate::large_file::{Outcome, Viewer};
use vix_input::KeyEvent;

impl App {
    /// `file.open_large`: ask for a path to view, seeded with the active
    /// buffer's file.
    pub(super) fn open_large_file_prompt(&mut self) {
        let seed = self
            .editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.prompt = Some(
            Prompt::new(
                PromptKind::OpenLargeFile,
                t!("prompt.open_large_file").to_string(),
            )
            .with_input(seed),
        );
    }

    /// Open `path` in the Large File Viewer. The viewer reads local files
    /// only, so a remote workspace's files are refused with a message.
    pub(crate) fn open_large_file_viewer(&mut self, path: &Path) {
        if !crate::authority::current().is_local() {
            self.messages.warn(t!("msg.large_file_remote").to_string());
            return;
        }
        match Viewer::open(path) {
            Ok(viewer) => {
                self.large_file = Some(viewer);
                self.status = t!("status.large_file_opened", path = path.display()).to_string();
            }
            Err(e) => self.messages.error(
                t!(
                    "msg.large_file_open_failed",
                    path = path.display(),
                    error = e
                )
                .to_string(),
            ),
        }
    }

    /// Route a key to the open viewer and act on its outcome.
    pub(super) fn large_file_key(&mut self, key: KeyEvent) {
        let height = usize::from(self.layout.large_file.height).max(1);
        let Some(viewer) = self.large_file.as_mut() else {
            return;
        };
        match viewer.handle_key(key, height) {
            Outcome::Consumed => {}
            Outcome::Close => self.large_file = None,
            Outcome::Copy(text) => {
                // The editor's fallback-aware copy when a buffer is open, so
                // something is always stored; else the shared clipboard.
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.copy_to_clipboard(&text);
                } else if crate::clipboard::set(&text).is_err() {
                    self.messages
                        .warn(t!("msg.large_file_copy_failed").to_string());
                }
            }
        }
    }

    /// Collect the viewer's finished background work (a search, a pending
    /// goto). Called every tick.
    pub fn poll_large_file(&mut self) {
        if let Some(v) = self.large_file.as_mut() {
            v.poll();
        }
    }

    /// Whether the viewer is still indexing or searching, so the loop ticks
    /// often enough for its progress to show.
    #[must_use]
    pub fn large_file_busy(&self) -> bool {
        self.large_file.as_ref().is_some_and(Viewer::busy)
    }
}
