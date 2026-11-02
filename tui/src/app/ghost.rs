//! Inline ghost-text completion (T618): a dimmed suggestion after the cursor,
//! from a language server (`textDocument/inlineCompletion`) or an AI
//! fill-in-the-middle prompt, accepted with Tab, one word at a time with
//! Ctrl+Right, or dismissed with Esc. The pure rules (setting parsing, word
//! splitting, staleness) are in [`crate::lsp::ghost`]; the prompt and reply
//! clean-up are in `vix_ai_core::fim`; the drawing is `tui/src/ui/ghost.rs`.
//! See `crates/vix-lsp/spec/index.md` § Inline (ghost-text) completion.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use vix_input::{KeyCode, KeyEvent};

use super::{App, Focus};
use crate::lsp::ghost::{Ghost, Source};

/// How long the buffer must be still after an edit before a suggestion is
/// requested on its own.
pub(crate) const GHOST_DEBOUNCE: Duration = Duration::from_millis(350);

/// An in-flight AI request and the buffer state it was made for.
struct AiRequest {
    rx: Receiver<Option<String>>,
    revision: u64,
    cursor: usize,
}

/// Everything ghost-text completion remembers between ticks.
#[derive(Default)]
pub(crate) struct GhostState {
    /// The suggestion on show, when there is one.
    pub(crate) ghost: Option<Ghost>,
    /// The document, revision and cursor the last tick saw.
    seen: Option<(Option<PathBuf>, u64, usize)>,
    /// When a request is due, and whether it is automatic (not user-invoked).
    due: Option<(Instant, bool)>,
    /// The `(revision, cursor)` of the language-server request in flight.
    lsp_pending: Option<(u64, usize)>,
    /// The AI request in flight.
    ai: Option<AiRequest>,
    /// An AI request could not start; no more are tried until re-enabled.
    ai_failed: bool,
}

/// Where the active buffer's cursor is, as far as ghost text cares.
struct CursorState {
    path: Option<PathBuf>,
    revision: u64,
    cursor: usize,
    /// The cursor line's text before the cursor.
    line_prefix: String,
    /// Whether only blanks follow the cursor on its line.
    rest_blank: bool,
}

impl App {
    /// The active buffer's cursor state, or `None` for an image or read-only
    /// buffer, one with a selection or extra carets.
    fn ghost_cursor(&self) -> Option<CursorState> {
        let tab = self.editor.active_tab()?;
        if tab.is_image() || tab.read_only || tab.editor.has_multi_carets() {
            return None;
        }
        let ed = &tab.editor;
        if ed.selection_span().is_some_and(|(a, b)| a != b) {
            return None;
        }
        let code = ed.code_ref();
        let cursor = ed.get_cursor();
        let line = code.char_to_line(cursor);
        let start = code.line_to_char(line);
        let end = start + code.line_len(line);
        Some(CursorState {
            path: tab.path.clone(),
            revision: ed.revision(),
            cursor,
            line_prefix: code.slice(start, cursor),
            rest_blank: code.slice(cursor, end).trim().is_empty(),
        })
    }

    /// Whether the editor is in a state where a suggestion makes sense: it has
    /// the focus and nothing else (a popup, overlay or snippet) is using keys.
    fn ghost_context_ok(&self) -> bool {
        self.focus == Focus::Editor
            && self.completion.is_none()
            && !self.overlay_capturing_keys()
            && !self.snippet_active()
    }

    /// Advance ghost-text completion by one tick: drop a suggestion the buffer
    /// moved away from, arm the idle debounce after an edit, collect a finished
    /// AI reply and send a request once the debounce has passed.
    pub(super) fn poll_ghost(&mut self) {
        let source = Source::parse(&self.settings.inline_completion);
        if source == Source::Off {
            self.ghost = GhostState::default();
            return;
        }
        let Some(c) = self.ghost_cursor() else {
            self.ghost.ghost = None;
            self.ghost.due = None;
            return;
        };
        let key = (c.path.clone(), c.revision, c.cursor);
        if self.ghost.seen.as_ref() != Some(&key) {
            if self
                .ghost
                .ghost
                .as_ref()
                .is_some_and(|g| !g.is_current(c.revision, c.cursor))
            {
                self.ghost.ghost = None;
            }
            // Only typing arms a request; opening a file, switching tabs and
            // moving the cursor do not.
            let edited = self
                .ghost
                .seen
                .as_ref()
                .is_some_and(|(p, rev, _)| *p == c.path && *rev != c.revision);
            self.ghost.due = edited.then(|| (Instant::now() + GHOST_DEBOUNCE, true));
            self.ghost.seen = Some(key);
        }
        self.collect_ghost_ai(&c);
        if let Some((due, automatic)) = self.ghost.due
            && Instant::now() >= due
        {
            self.ghost.due = None;
            self.request_ghost(source, &c, automatic);
        }
    }

    /// Install a finished AI reply as the suggestion when it is still for this
    /// buffer state.
    fn collect_ghost_ai(&mut self, c: &CursorState) {
        let Some(req) = self.ghost.ai.take() else {
            return;
        };
        let reply = match req.rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => {
                self.ghost.ai = Some(req);
                return;
            }
            Err(TryRecvError::Disconnected) => None,
        };
        if req.revision == c.revision
            && req.cursor == c.cursor
            && c.rest_blank
            && self.ghost_context_ok()
        {
            self.ghost.ghost = reply.and_then(|t| Ghost::new(t, c.revision, c.cursor));
        }
    }

    /// Ask the `source` for a suggestion at the cursor `c`. Returns whether a
    /// request went out.
    fn request_ghost(&mut self, source: Source, c: &CursorState, automatic: bool) -> bool {
        if !c.rest_blank || !self.ghost_context_ok() {
            return false;
        }
        match source {
            Source::Lsp => {
                let Some(path) = c.path.as_ref() else {
                    return false;
                };
                if !self.lsp.supports_inline_completion(path)
                    || self.lsp_synced.get(path) != Some(&c.revision)
                {
                    return false;
                }
                let (line, character) = self.cursor_lsp_position(path);
                self.lsp
                    .request_inline_completion(path, line, character, automatic);
                self.ghost.lsp_pending = Some((c.revision, c.cursor));
                true
            }
            Source::Ai => {
                if self.ghost.ai_failed || self.ghost.ai.is_some() || !self.settings.ai_enabled() {
                    return false;
                }
                let Some(content) = self.editor.active_tab().map(|t| t.editor.get_content()) else {
                    return false;
                };
                let before: String = content.chars().take(c.cursor).collect();
                let after: String = content.chars().skip(c.cursor).collect();
                if let Some(rx) = self.spawn_ghost_ai(&before, &after, &c.line_prefix) {
                    self.ghost.ai = Some(AiRequest {
                        rx,
                        revision: c.revision,
                        cursor: c.cursor,
                    });
                    true
                } else {
                    self.ghost.ai_failed = true;
                    false
                }
            }
            Source::Off => false,
        }
    }

    /// A language server's suggestions for `path` arrived: show the first one if
    /// the buffer is still exactly as it was when they were asked for.
    pub(super) fn on_ghost_lsp(&mut self, path: &std::path::Path, items: Vec<String>) {
        let Some(c) = self.ghost_cursor() else {
            return;
        };
        if c.path.as_deref() != Some(path) || self.ghost.lsp_pending != Some((c.revision, c.cursor))
        {
            return;
        }
        self.ghost.lsp_pending = None;
        if Source::parse(&self.settings.inline_completion) == Source::Lsp
            && c.rest_blank
            && self.ghost_context_ok()
        {
            self.ghost.ghost = items
                .into_iter()
                .next()
                .and_then(|t| Ghost::new(t, c.revision, c.cursor));
        }
    }

    /// `inline_completion.trigger`: ask for a suggestion now.
    pub(super) fn ghost_trigger(&mut self) {
        if Source::parse(&self.settings.inline_completion) == Source::Off {
            self.status = t!("status.inline_completion_off").to_string();
            return;
        }
        self.ghost.ai_failed = false;
        self.ghost.ghost = None;
        self.ghost.due = Some((Instant::now(), false));
    }

    /// `inline_completion.dismiss`: drop the suggestion.
    pub(super) fn ghost_dismiss(&mut self) {
        self.ghost.ghost = None;
    }

    /// `inline_completion.accept` / `.accept_word`: insert the whole suggestion,
    /// or its next word and keep the rest as the new suggestion. Returns whether
    /// anything was inserted; a suggestion the buffer has moved away from is
    /// dropped instead.
    pub(super) fn ghost_accept(&mut self, word: bool) -> bool {
        let Some(c) = self.ghost_cursor() else {
            return false;
        };
        let Some(ghost) = self
            .ghost
            .ghost
            .clone()
            .filter(|g| g.is_current(c.revision, c.cursor))
        else {
            self.ghost.ghost = None;
            return false;
        };
        let taken = if word {
            ghost.next_word().to_string()
        } else {
            ghost.text().to_string()
        };
        let area = self.layout.editor;
        if !self.editor.insert_str(&taken, area) {
            return false;
        }
        self.ghost.lsp_pending = None;
        self.ghost.due = None;
        self.ghost.ghost = None;
        if let Some(after) = self.ghost_cursor() {
            self.ghost.ghost = ghost
                .remainder(&taken, after.revision, after.cursor)
                .filter(|_| after.rest_blank);
            self.ghost.seen = Some((after.path, after.revision, after.cursor));
        }
        true
    }

    /// Route a key to a visible suggestion: Tab accepts it, Ctrl+Right accepts
    /// one word, Esc dismisses. Any other key is left for normal handling.
    pub(super) fn ghost_key(&mut self, key: KeyEvent) -> bool {
        if self.ghost.ghost.is_none() || !self.ghost_context_ok() {
            return false;
        }
        let (ctrl, alt, shift) = (Self::ctrl(&key), Self::alt(&key), Self::shift(&key));
        match key.code {
            KeyCode::Tab if !ctrl && !alt && !shift => self.ghost_accept(false),
            KeyCode::Right if ctrl && !alt && !shift => self.ghost_accept(true),
            KeyCode::Esc if !ctrl && !alt && !shift => {
                self.ghost_dismiss();
                true
            }
            _ => false,
        }
    }

    /// What to draw after the cursor: the suggestion's first line and whether
    /// more lines follow, when it still applies to the active buffer.
    #[must_use]
    pub(crate) fn ghost_overlay(&self) -> Option<(&str, bool)> {
        let g = self.ghost.ghost.as_ref()?;
        let c = self.ghost_cursor()?;
        (g.is_current(c.revision, c.cursor) && c.rest_blank && self.ghost_context_ok())
            .then(|| (g.first_line(), g.has_more_lines()))
    }
}
