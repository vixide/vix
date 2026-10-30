//! The completion popup (T612): the full candidate list plus a fuzzy-filtered
//! view that narrows as you type, accepting an item (its `textEdit` range and
//! `additionalTextEdits`), and the debounced auto-trigger after an identifier
//! or server trigger character. Shared by LSP completion and the two non-LSP
//! completers (Org-roam node titles, Org-contacts) in `lsp_dap.rs`. See
//! `crates/vix-lsp/spec/index.md` § Completion.

use std::time::{Duration, Instant};

use vix_input::{KeyCode, KeyEvent};

use super::{App, apply_edits_to_text, char_to_lsp_pos, lsp_pos_to_char};
use crate::lsp_core::{CompletionItem, CompletionList, Encoding, Range};

/// How long typing must pause before an auto-triggered completion request is
/// sent, so a burst of keystrokes sends one request rather than one each.
pub(crate) const COMPLETION_DEBOUNCE: Duration = Duration::from_millis(100);

/// Whether `c` continues a word for completion purposes (letter, digit, `_`)
/// — the same set `App::word_prefix_before_cursor` uses.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The completion overlay: the full candidate list, the fuzzy-filtered view
/// of it currently shown, and the highlighted row.
pub struct CompletionPopup {
    /// Every candidate, in server order (the unfiltered list).
    pub items: Vec<CompletionItem>,
    /// The filtered, ranked view: indices into `items`, best match first.
    pub visible: Vec<usize>,
    /// Index into `visible` of the highlighted candidate.
    pub selected: usize,
    /// The server marked the list `isIncomplete`: further typing re-requests
    /// instead of only filtering.
    pub is_incomplete: bool,
    /// Char offset where the typed word began when the list arrived; the
    /// filter query is the text from here to the cursor.
    pub anchor: usize,
}

impl CompletionPopup {
    /// A popup over `items`, anchored at char offset `anchor` and filtered by
    /// `query` (the text already typed from `anchor` to the cursor).
    #[must_use]
    pub fn new(
        items: Vec<CompletionItem>,
        is_incomplete: bool,
        anchor: usize,
        query: &str,
    ) -> Self {
        let mut popup = CompletionPopup {
            items,
            visible: Vec::new(),
            selected: 0,
            is_incomplete,
            anchor,
        };
        popup.refilter(query);
        popup
    }

    /// Recompute the visible view for `query`: every item whose filter text
    /// fuzzy-matches it, best score first, ties (and an empty query) in
    /// `sortText`/label order, then server order. Resets the highlight to the
    /// top row.
    pub fn refilter(&mut self, query: &str) {
        let items = &self.items;
        let mut scored: Vec<(i32, usize)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| {
                if query.is_empty() {
                    Some((0, i))
                } else {
                    crate::palette::fuzzy_score(it.filter_key(), query).map(|s| (s, i))
                }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| items[a.1].sort_key().cmp(items[b.1].sort_key()))
                .then(a.1.cmp(&b.1))
        });
        self.visible = scored.into_iter().map(|(_, i)| i).collect();
        self.selected = 0;
    }

    /// The candidates currently shown, in display order.
    pub fn visible_items(&self) -> impl Iterator<Item = &CompletionItem> {
        self.visible.iter().filter_map(|&i| self.items.get(i))
    }

    /// The highlighted candidate, if any row is shown.
    #[must_use]
    pub fn selected_item(&self) -> Option<&CompletionItem> {
        self.visible
            .get(self.selected)
            .and_then(|&i| self.items.get(i))
    }

    /// The highlighted candidate, mutably (to fill in resolved detail).
    pub fn selected_item_mut(&mut self) -> Option<&mut CompletionItem> {
        let i = *self.visible.get(self.selected)?;
        self.items.get_mut(i)
    }
}

/// Resolve every edit of accepting `item` to char spans in the buffer, then
/// return them as LSP edits for [`apply_edits_to_text`] plus the cursor's
/// char offset afterwards. The main edit replaces the item's `textEdit`
/// range — its end stretched to the cursor when the user typed past it — or,
/// with no range, the typed word `[anchor, cursor)`. Additional edits that
/// overlap the main edit are dropped (the protocol forbids them).
fn completion_edits(
    code: &crate::editor_core::code::Code,
    enc: Encoding,
    item: &CompletionItem,
    anchor: usize,
    cursor: usize,
) -> (Vec<(Range, String)>, usize) {
    let (start, end) = match item.range {
        Some(r) => {
            let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
            let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
            let same_line = code.char_to_line(cursor) == code.char_to_line(s);
            (
                s,
                if same_line && cursor > e {
                    cursor
                } else {
                    e.max(s)
                },
            )
        }
        None => (anchor.min(cursor), cursor),
    };
    let to_range = |a: usize, b: usize| {
        let (sl, sc) = char_to_lsp_pos(code, a, enc);
        let (el, ec) = char_to_lsp_pos(code, b, enc);
        Range {
            start: crate::lsp_core::Position {
                line: sl,
                character: sc,
            },
            end: crate::lsp_core::Position {
                line: el,
                character: ec,
            },
        }
    };
    let mut edits = vec![(to_range(start, end), item.insert_text.clone())];
    let mut caret = start + item.insert_text.chars().count();
    for (r, text) in &item.additional_edits {
        let a = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
        let b = lsp_pos_to_char(code, r.end.line, r.end.character, enc).max(a);
        if a < end && b > start {
            continue; // overlaps the main edit
        }
        if b <= start {
            // An edit before the insertion (an import) shifts the caret.
            caret = caret + text.chars().count() - (b - a).min(caret);
        }
        edits.push((*r, text.clone()));
    }
    (edits, caret)
}

impl App {
    /// The completion popup, when one is open (read-only, for tests and
    /// front ends).
    #[must_use]
    pub fn completion_popup(&self) -> Option<&CompletionPopup> {
        self.completion.as_ref()
    }

    /// The char offset where the word before the cursor starts, and that
    /// word (the filter query for a freshly opened popup).
    pub(super) fn word_anchor(&self) -> (usize, String) {
        let prefix = self.word_prefix_before_cursor();
        let cursor = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.get_cursor());
        (cursor.saturating_sub(prefix.chars().count()), prefix)
    }

    /// Open a completion popup over `items` (a non-LSP completer's list),
    /// anchored at the word before the cursor and filtered by it.
    pub(super) fn open_completion(&mut self, items: Vec<CompletionItem>) {
        let (anchor, query) = self.word_anchor();
        self.completion = Some(CompletionPopup::new(items, false, anchor, &query));
    }

    /// A `textDocument/completion` reply arrived: open (or, for a re-request
    /// of an incomplete list, refresh) the popup, keeping an open popup's
    /// anchor. An empty reply only matters for an open incomplete popup —
    /// nothing matches any more, so it closes.
    pub(super) fn on_completion_list(&mut self, list: CompletionList) {
        if list.items.is_empty() {
            if self.completion.as_ref().is_some_and(|p| p.is_incomplete) {
                self.completion = None;
            }
            return;
        }
        let (word_anchor, _) = self.word_anchor();
        let anchor = self.completion.as_ref().map_or(word_anchor, |p| p.anchor);
        let Some(query) = self.completion_query(anchor) else {
            return; // the cursor already left the word
        };
        let popup = CompletionPopup::new(list.items, list.is_incomplete, anchor, &query);
        if popup.visible.is_empty() && !popup.is_incomplete {
            self.completion = None;
            return;
        }
        self.completion = Some(popup);
        self.resolve_selected_completion();
    }

    /// The text typed from `anchor` to the cursor, or `None` when the cursor
    /// is before `anchor`, on another line, or past a non-word character.
    fn completion_query(&self, anchor: usize) -> Option<String> {
        let t = self.editor.active_tab()?;
        let code = t.editor.code_ref();
        let cursor = t.editor.get_cursor();
        if cursor < anchor || anchor > code.len() {
            return None;
        }
        if code.char_to_line(cursor) != code.char_to_line(anchor) {
            return None;
        }
        let query: String = code.slice(anchor, cursor);
        query.chars().all(is_word_char).then_some(query)
    }

    /// Accept the highlighted candidate: replace its range (or the typed
    /// word) with its text and apply its additional edits, as one undo step.
    pub(super) fn accept_completion(&mut self) {
        let Some(popup) = self.completion.take() else {
            return;
        };
        let Some(item) = popup.selected_item().cloned() else {
            return;
        };
        let enc = self
            .active_path()
            .map_or(Encoding::Utf16, |p| self.lsp.encoding_for(&p));
        let area = self.layout.editor;
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() || tab.read_only {
            return;
        }
        let text = tab.editor.get_content();
        let cursor = tab.editor.get_cursor();
        let (edits, caret) =
            completion_edits(tab.editor.code_ref(), enc, &item, popup.anchor, cursor);
        let new = apply_edits_to_text(&text, enc, &edits);
        if new != text {
            tab.editor.set_content(&new);
            tab.dirty = true;
            tab.preview = false;
        }
        tab.editor.set_cursor(caret.min(new.chars().count()));
        tab.editor.focus(&area);
    }

    /// Handle a key while the completion popup is open. Returns true if consumed.
    pub(super) fn completion_key(&mut self, key: KeyEvent) -> bool {
        if self.completion.is_none() {
            return false;
        }
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.completion.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
                self.resolve_selected_completion();
                true
            }
            KeyCode::Down => {
                if let Some(p) = self.completion.as_mut()
                    && p.selected + 1 < p.visible.len()
                {
                    p.selected += 1;
                }
                self.resolve_selected_completion();
                true
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Esc => {
                self.completion = None;
                true
            }
            _ => {
                self.completion_typing_key(key);
                false
            }
        }
    }

    /// A key the popup doesn't consume, about to be handled normally: a word
    /// character or Backspace keeps the popup (re-filtered after the edit by
    /// [`App::completion_after_key`]); the selected item's commit character
    /// accepts it first; anything else dismisses it.
    fn completion_typing_key(&mut self, key: KeyEvent) {
        let plain = !Self::ctrl(&key) && !Self::alt(&key);
        match key.code {
            KeyCode::Char(c) if plain => {
                let commits = self
                    .completion
                    .as_ref()
                    .and_then(CompletionPopup::selected_item)
                    .is_some_and(|i| i.is_commit_char(c));
                if commits {
                    self.accept_completion();
                } else if !is_word_char(c) {
                    self.completion = None;
                }
            }
            KeyCode::Backspace if plain => {}
            _ => self.completion = None,
        }
    }

    /// After an editor key: re-filter an open popup (re-requesting an
    /// incomplete list), or — with `lsp_auto_complete` on and no popup open —
    /// schedule a debounced completion request after a word or trigger
    /// character. Any other key cancels a pending auto-request.
    pub(super) fn completion_after_key(&mut self, key: KeyEvent) {
        if self.completion.is_some() {
            self.refresh_completion_popup();
            return;
        }
        let typed = match key.code {
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => Some(c),
            _ => None,
        };
        self.completion_due = None;
        let Some(c) = typed else {
            return;
        };
        if !self.settings.completion.lsp_auto_complete {
            return;
        }
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            return;
        }
        let trigger = self.lsp.is_completion_trigger(&path, c);
        if trigger || is_word_char(c) {
            self.completion_due =
                Some((Instant::now() + COMPLETION_DEBOUNCE, trigger.then_some(c)));
        }
    }

    /// Re-filter the open popup against the text typed since its anchor,
    /// closing it once the cursor leaves the word or nothing matches a
    /// complete list; an incomplete list is re-requested instead.
    fn refresh_completion_popup(&mut self) {
        let Some(anchor) = self.completion.as_ref().map(|p| p.anchor) else {
            return;
        };
        let Some(query) = self.completion_query(anchor) else {
            self.completion = None;
            return;
        };
        let Some(popup) = self.completion.as_mut() else {
            return;
        };
        popup.refilter(&query);
        if popup.is_incomplete {
            if let Some(path) = self.active_path()
                && self.lsp.handles(&path)
            {
                // Sync first so the server sees what was just typed.
                self.lsp_sync_active();
                let (line, character) = self.cursor_lsp_position(&path);
                self.lsp
                    .request_completion_with(&path, line, character, None, true);
            }
        } else if popup.visible.is_empty() {
            self.completion = None;
        } else {
            self.resolve_selected_completion();
        }
    }

    /// Send a debounced auto-completion request once its delay has passed
    /// (called every event-loop tick from `poll_lsp`).
    pub(super) fn poll_completion_due(&mut self) {
        let Some((due, trigger)) = self.completion_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        self.completion_due = None;
        if self.completion.is_some() {
            return;
        }
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp
            .request_completion_with(&path, line, character, trigger, false);
    }

    /// Lazily fetch fuller detail for the highlighted completion item via
    /// `completionItem/resolve`, when it has no detail yet but carries resolve
    /// data.
    pub(super) fn resolve_selected_completion(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            return;
        }
        let Some(item) = self
            .completion
            .as_ref()
            .and_then(CompletionPopup::selected_item)
        else {
            return;
        };
        if item.detail.is_some() {
            return; // already has detail to show
        }
        let Some(data) = item.data.clone() else {
            return;
        };
        let label = item.label.clone();
        self.lsp
            .request_completion_resolve(&path, &label, Some(&data));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vix_input::{KeyCode, KeyEvent, KeyModifiers};

    use crate::app::App;
    use crate::lsp_core::{CompletionItem, CompletionList, Position, Range};
    use crate::settings::Settings;

    /// An `App` editing a fresh plain-text file (no language server handles
    /// it) holding `content`, with the cursor at the end of `content`.
    fn app_with(tag: &str, content: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-t612-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, content).unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.open_path(&file, false);
        let t = app.editor.active_tab_mut().unwrap();
        let end = t.editor.code_ref().len_chars();
        t.editor.set_cursor(end);
        (app, dir)
    }

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn shown(app: &App) -> Vec<String> {
        app.completion_popup()
            .map(|p| p.visible_items().map(|i| i.label.clone()).collect())
            .unwrap_or_default()
    }

    fn text(app: &App) -> String {
        app.editor.active_tab().unwrap().editor.get_content()
    }

    fn range(line: u32, start: u32, end: u32) -> Range {
        Range {
            start: Position {
                line,
                character: start,
            },
            end: Position {
                line,
                character: end,
            },
        }
    }

    #[test]
    fn typing_narrows_the_list_and_backspace_widens_it() {
        let (mut app, dir) = app_with("narrow", "");
        let items = ["apple", "apricot", "banana"]
            .into_iter()
            .map(|l| CompletionItem::new(l, l))
            .collect();
        app.on_completion_list(CompletionList::new(items, false));
        assert_eq!(shown(&app).len(), 3, "an empty query shows everything");
        key(&mut app, KeyCode::Char('a'));
        key(&mut app, KeyCode::Char('p'));
        assert_eq!(shown(&app), ["apple", "apricot"], "`ap` filters out banana");
        assert_eq!(
            app.completion_popup().unwrap().items.len(),
            3,
            "full list kept"
        );
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(shown(&app), ["apricot"]);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(shown(&app), ["apple", "apricot"], "Backspace re-widens");
        key(&mut app, KeyCode::Char('z'));
        assert!(
            app.completion.is_none(),
            "no match in a complete list closes it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sort_text_orders_ties_and_filter_text_is_matched() {
        let (mut app, dir) = app_with("sort", "");
        let mut zeta = CompletionItem::new("zeta", "zeta");
        zeta.sort_text = Some("0".into());
        let mut alpha = CompletionItem::new("alpha", "alpha");
        alpha.sort_text = Some("1".into());
        let mut hidden = CompletionItem::new("shown_label", "x");
        hidden.filter_text = Some("qqq".into());
        app.on_completion_list(CompletionList::new(vec![alpha, zeta, hidden], false));
        assert_eq!(shown(&app)[..2], ["zeta", "alpha"], "sortText wins ties");
        key(&mut app, KeyCode::Char('q'));
        assert_eq!(shown(&app), ["shown_label"], "matched on filterText");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_non_word_key_closes_the_popup() {
        let (mut app, dir) = app_with("close", "ab");
        app.on_completion_list(CompletionList::new(
            vec![CompletionItem::new("abc", "abc")],
            false,
        ));
        assert!(app.completion.is_some());
        key(&mut app, KeyCode::Char(' '));
        assert!(app.completion.is_none());
        assert_eq!(text(&app), "ab ", "the key is still typed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_replaces_the_range_then_applies_the_import_edit() {
        let (mut app, dir) = app_with("import", "fn main() {\n    let m = Has");
        let mut item = CompletionItem::new("HashMap", "HashMap");
        // The server's range covers `Has` (cols 12..15) as of the request.
        item.range = Some(range(1, 12, 15));
        item.additional_edits = vec![(range(0, 0, 0), "use std::collections::HashMap;\n".into())];
        app.on_completion_list(CompletionList::new(vec![item], false));
        // Typing past the range while the popup is open still replaces it all.
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(shown(&app), ["HashMap"]);
        key(&mut app, KeyCode::Enter);
        assert!(app.completion.is_none());
        let want = "use std::collections::HashMap;\nfn main() {\n    let m = HashMap";
        assert_eq!(text(&app), want);
        let cursor = app.editor.active_tab().unwrap().editor.get_cursor();
        assert_eq!(
            cursor,
            want.chars().count(),
            "caret after the inserted text"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_without_a_range_replaces_the_typed_word() {
        let (mut app, dir) = app_with("word", "x = pri");
        app.on_completion_list(CompletionList::new(
            vec![CompletionItem::new("println", "println!")],
            false,
        ));
        key(&mut app, KeyCode::Tab);
        assert_eq!(text(&app), "x = println!");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_commit_character_accepts_then_types() {
        let (mut app, dir) = app_with("commit", "pr");
        let mut item = CompletionItem::new("print", "print");
        item.commit_chars = vec!["(".into()];
        app.on_completion_list(CompletionList::new(vec![item], false));
        key(&mut app, KeyCode::Char('('));
        assert!(app.completion.is_none());
        assert!(text(&app).starts_with("print("), "got {:?}", text(&app));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_incomplete_list_stays_open_until_an_empty_reply() {
        let (mut app, dir) = app_with("incomplete", "");
        app.on_completion_list(CompletionList::new(
            vec![CompletionItem::new("abc", "abc")],
            true,
        ));
        key(&mut app, KeyCode::Char('z'));
        let popup = app
            .completion_popup()
            .expect("kept open awaiting a re-request");
        assert_eq!(popup.visible.len(), 0);
        // The re-request's reply replaces the full list and is re-filtered.
        app.on_completion_list(CompletionList::new(
            vec![
                CompletionItem::new("zed", "zed"),
                CompletionItem::new("abc", "abc"),
            ],
            true,
        ));
        assert_eq!(shown(&app), ["zed"]);
        assert_eq!(app.completion_popup().unwrap().anchor, 0, "anchor kept");
        app.on_completion_list(CompletionList::new(Vec::new(), true));
        assert!(app.completion.is_none(), "an empty reply closes it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_auto_request_without_a_server() {
        let (mut app, dir) = app_with("noserver", "");
        key(&mut app, KeyCode::Char('a'));
        assert!(app.completion_due.is_none(), "no server handles .txt");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
