//! When to ask the language server for semantic tokens (T615).
//!
//! Tokens are requested for the active document once it is synced, and again
//! after it changes or, for a server that answers `semanticTokens/range`, after
//! the view scrolls out of the window last asked about. A burst of keystrokes
//! or scroll steps sends one request: a change waits [`SEMANTIC_DEBOUNCE`]
//! first. Switching to a document (or reopening one after a server restart) is
//! requested at once. Which request is sent (range, full/delta or full) is the
//! LSP client's choice, `Lsp::request_semantic_tokens`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{App, char_to_lsp_pos};

/// How long a change waits before its semantic tokens are requested.
pub(crate) const SEMANTIC_DEBOUNCE: Duration = Duration::from_millis(150);

/// What the last semantic-tokens request was for: the document, its revision,
/// and (for a ranged server) the window of lines `[first, end)` asked about.
pub(crate) type SemanticKey = (PathBuf, u64, Option<(usize, usize)>);

/// The window of lines to ask a ranged server about, for a view `height`
/// lines tall whose first visible line is `top`, in a `len_lines` document.
///
/// Windows are aligned to multiples of the view height and reach one view
/// above and below the visible lines, so scrolling within a view's height
/// keeps the same window (and sends no request).
pub(crate) fn token_window(top: usize, height: usize, len_lines: usize) -> (usize, usize) {
    let chunk = height.max(1);
    let base = top / chunk * chunk;
    let first = base.saturating_sub(chunk).min(len_lines);
    let end = base.saturating_add(2 * chunk).min(len_lines);
    (first, end.max(first))
}

impl App {
    /// Send a semantic-tokens request for the active document when it is due
    /// (called every event-loop tick from `poll_lsp`, after the sync).
    pub(super) fn poll_semantic_tokens(&mut self) {
        let Some(path) = self.active_path() else {
            self.semantic_due = None;
            return;
        };
        // `None` until a server for `path` has said it offers tokens.
        let Some(ranged) = self.lsp.semantic_tokens_ranged(&path) else {
            self.semantic_due = None;
            return;
        };
        let height = usize::from(self.editor_view().height);
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab() else {
            return;
        };
        let rev = t.editor.revision();
        if self.lsp_synced.get(&path) != Some(&rev) {
            return; // the next sync comes first
        }
        let code = t.editor.code_ref();
        let window = ranged.then(|| token_window(t.editor.top_line(), height, code.len_lines()));
        let range = window.map(|(first, end)| {
            let end = if end < code.len_lines() {
                code.line_to_char(end)
            } else {
                code.len()
            };
            let (sl, sc) = char_to_lsp_pos(code, code.line_to_char(first), enc);
            let (el, ec) = char_to_lsp_pos(code, end, enc);
            crate::lsp_core::Range {
                start: crate::lsp_core::Position {
                    line: sl,
                    character: sc,
                },
                end: crate::lsp_core::Position {
                    line: el,
                    character: ec,
                },
            }
        });
        let key: SemanticKey = (path.clone(), rev, window);
        if self.semantic_requested.as_ref() == Some(&key) {
            self.semantic_due = None;
            return;
        }
        let same_document = self
            .semantic_requested
            .as_ref()
            .is_some_and(|(p, _, _)| *p == path);
        if same_document {
            let now = Instant::now();
            match self.semantic_due {
                None => {
                    self.semantic_due = Some(now + SEMANTIC_DEBOUNCE);
                    return;
                }
                Some(due) if now < due => return,
                Some(_) => {}
            }
        }
        self.semantic_due = None;
        self.lsp.request_semantic_tokens(&path, range);
        self.semantic_requested = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::token_window;

    #[test]
    fn the_window_covers_the_view_and_a_view_either_side() {
        assert_eq!(token_window(0, 10, 100), (0, 20));
        assert_eq!(token_window(25, 10, 100), (10, 40));
        assert_eq!(token_window(95, 10, 100), (80, 100));
    }

    #[test]
    fn scrolling_within_a_view_keeps_the_window() {
        let w = token_window(20, 10, 1000);
        for top in 20..30 {
            assert_eq!(token_window(top, 10, 1000), w);
            let (first, end) = w;
            assert!(first <= top && top + 10 <= end, "view {top} inside {w:?}");
        }
    }

    #[test]
    fn a_short_document_is_one_window() {
        assert_eq!(token_window(0, 40, 3), (0, 3));
        assert_eq!(token_window(0, 0, 0), (0, 0));
    }
}
