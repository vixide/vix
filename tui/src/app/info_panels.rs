//! Tools → the small info/reference panels: the Contacts vCard view,
//! the file-info and text-info panels (Tools → File Info / Text Info),
//! Markdown preview, the Snippets picker (with its tabstop-expansion
//! session and the active buffer's media-type-scoped library), and the
//! System Info panel.
//!
//! Moved out of `app.rs` verbatim (T141, slice 12 -- the last sub-slice
//! of "tools", another large genuinely contiguous block like org core
//! was, no cherry-picking needed).

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

use crate::app::info_panel::InfoSlot;
use crate::app::picker_panels::list_panel_full_nav_key;
use crate::geo::Geo;
use vix_input::{KeyCode, KeyEvent, MouseEvent};

use super::{
    App, ContactPanel, FileInfoPanel, MarkdownPreview, Outline, Prompt, PromptKind, SnippetSession,
    SystemInfoPanel, TextInfoPanel, VcardPanel,
};
use crate::editor::Tab;

impl App {
    /// Open the highlighted contact's vCard in the single-vCard view.
    fn open_selected_vcard(&mut self) {
        let Some(path) = self.contacts.as_ref().and_then(ContactPanel::selected_path) else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => self.vcard = Some(VcardPanel::open(crate::vcard_parser::parse(&text))),
            Err(e) => self
                .messages
                .error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    /// Handle a key in the contacts browser: Up/Down/PageUp/PageDown/Home/End move, Enter opens
    /// the selected contact's vCard, Esc closes.
    pub(super) fn contacts_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.contacts.height)).max(1);
        if list_panel_full_nav_key(self.contacts.as_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Enter => self.open_selected_vcard(),
            KeyCode::Esc => self.contacts = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the contacts browser: a left click on a row opens that contact's
    /// vCard.
    pub(super) fn contacts_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.contacts;
        let Some(row_in_view) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        let hit = self
            .contacts
            .as_mut()
            .is_some_and(|p| p.select_index(p.scroll.plus(row_in_view)));
        if hit {
            self.open_selected_vcard();
        }
    }

    /// Handle a key in the vCard detail panel: movement keys scroll the selection, Enter
    /// inserts the highlighted value into the editor, Esc closes the panel.
    pub(super) fn vcard_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.vcard.height)).max(1);
        if list_panel_full_nav_key(self.vcard.as_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Enter => self.insert_selected_vcard_value(),
            // Esc returns to the contact browser (or closes if opened directly).
            KeyCode::Esc => self.vcard = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the vCard panel: a left click on a row inserts that value into
    /// the editor.
    pub(super) fn vcard_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.vcard;
        let Some(row_in_view) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        let hit = self
            .vcard
            .as_mut()
            .is_some_and(|p| p.select_index(p.scroll.plus(row_in_view)));
        if hit {
            self.insert_selected_vcard_value();
        }
    }

    /// Insert the highlighted vCard field's value into the active editor.
    fn insert_selected_vcard_value(&mut self) {
        let Some(p) = self.vcard.as_ref() else { return };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    /// Open the file-information panel for the active tab (name, path, size, timestamps and
    /// similar).
    pub(super) fn open_file_info(&mut self) {
        let info = self.gather_file_info();
        self.info.open_file_info(FileInfoPanel::open(&info));
    }

    /// Collect facts about the active file: counts from the buffer, and size /
    /// permissions / modified-time from the filesystem when it is saved.
    fn gather_file_info(&self) -> crate::file_information_panel::FileInfo {
        use crate::file_information_panel::FileInfo;
        let mut info = FileInfo::default();
        let Some(t) = self.editor.active_tab() else {
            return info;
        };
        let content = t.editor.get_content();
        info.language = t.editor.language().to_string();
        info.chars = content.chars().count();
        info.words = content.split_whitespace().count();
        info.lines = t.editor.code_ref().len_lines();
        info.dirty = t.dirty;
        if let Some(path) = &t.path {
            info.name = path
                .file_name()
                .map_or_default(|n| n.to_string_lossy().into_owned());
            info.path = path.display().to_string();
            if let Ok(meta) = crate::authority::current().metadata(path) {
                info.bytes = Some(meta.len);
                if cfg!(unix) {
                    info.mode = meta.mode;
                }
                if let Some(modified) = meta.modified
                    && let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH)
                {
                    info.modified_secs = i64::try_from(d.as_secs()).ok();
                }
            }
        }
        info
    }

    /// Handle a key in the file-information panel: movement keys select, Enter inserts the
    /// highlighted value into the editor (panel stays open), Esc closes.
    pub(super) fn file_info_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.file_info.height)).max(1);
        if list_panel_full_nav_key(self.info.file_info_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Enter => self.insert_selected_file_info(),
            KeyCode::Esc => self.info.close_file_info(),
            _ => {}
        }
    }

    /// Handle a mouse event in the file-information panel: a left click on a row inserts its
    /// value.
    pub(super) fn file_info_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.file_info;
        let Some(row_in_view) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        if let Some(p) = self.info.file_info_mut() {
            let idx = p.scroll.plus(row_in_view);
            if p.select_index(idx) {
                self.insert_selected_file_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_file_info(&mut self) {
        let Some(p) = self.info.file_info() else {
            return;
        };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    /// The selected text, or the whole active buffer when nothing is selected.
    pub(super) fn selected_or_all_text(&mut self) -> String {
        let selection = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(s) if !s.trim().is_empty() => s,
            _ => self
                .editor
                .active_tab()
                .map_or_default(|t| t.editor.get_content()),
        }
    }

    /// Open the Text Information panel over the selection, or the whole buffer
    /// when nothing is selected.
    pub(super) fn open_text_info(&mut self) {
        let text = self.selected_or_all_text();
        let stats = crate::text_information_panel::analyze(&text);
        self.info.open_text_info(TextInfoPanel::open(&stats));
    }

    /// Handle a key in the text-statistics panel: Up/Down select, Enter inserts the highlighted
    /// value, Esc closes.
    pub(super) fn text_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.info.text_info_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.info.text_info_mut() {
                    p.down();
                }
            }
            KeyCode::Enter => self.insert_selected_text_info(),
            KeyCode::Esc => self.info.close_text_info(),
            _ => {}
        }
    }

    /// Handle a mouse event in the text-statistics panel: a left click on a row inserts its
    /// value. The panel is short, so rows are not scrolled.
    pub(super) fn text_info_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.text_info;
        let Some(row_in_view) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        if let Some(p) = self.info.text_info_mut()
            && p.select_index(row_in_view)
        {
            self.insert_selected_text_info();
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_text_info(&mut self) {
        let Some(p) = self.info.text_info() else {
            return;
        };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    /// Open a read-only Markdown preview of the active buffer, scroll-synced
    /// (T206) to whichever preview line came from the cursor's source line.
    pub(super) fn open_markdown_preview(&mut self) {
        let cursor_line = self.editor.cursor_1based().0;
        let Some(text) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(Tab::text)
        else {
            return;
        };
        let mut panel = MarkdownPreview::open(&text);
        panel.sync_to_source_line(cursor_line);
        self.markdown_preview = Some(panel);
    }

    /// Handle a key in the Markdown preview: arrows and paging scroll, `t` opens the table of
    /// contents (T206), Esc/`q` closes the preview.
    pub(super) fn markdown_preview_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.editor.height)).max(1).minus(2);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.up(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.down(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.down(page);
                }
            }
            // T206: table of contents, an overlay-over-the-overlay like the
            // theme editor's own Enter-opens-the-X11-picker.
            KeyCode::Char('t' | 'T') => self.open_markdown_toc(),
            KeyCode::Esc | KeyCode::Char('q') => self.markdown_preview = None,
            _ => {}
        }
    }

    /// Open the Markdown preview's table of contents (T206): every heading,
    /// jump-to on Enter. No-op with a status message when the document has
    /// no headings.
    pub(super) fn open_markdown_toc(&mut self) {
        let Some(entries) = self.markdown_preview.as_ref().map(|p| p.toc.clone()) else {
            return;
        };
        if entries.is_empty() {
            self.status = t!("status.markdown_toc_empty").into();
            return;
        }
        let mut toc = Outline::new(entries);
        if let Some(scroll) = self.markdown_preview.as_ref().map(|p| p.scroll) {
            // 0-based scroll -> 1-based "current line" for select_nearest.
            toc.select_nearest(scroll.plus(1));
        }
        self.markdown_toc = Some(toc);
    }

    /// Handle a key in the Markdown table of contents: movement keys select a heading, Enter
    /// scrolls the preview to it, Esc closes the list.
    pub(super) fn markdown_toc_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.editor.height)).max(1).minus(2);
        match key.code {
            KeyCode::Up => {
                if let Some(o) = self.markdown_toc.as_mut() {
                    o.up();
                }
            }
            KeyCode::Down => {
                if let Some(o) = self.markdown_toc.as_mut() {
                    o.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(o) = self.markdown_toc.as_mut() {
                    o.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(o) = self.markdown_toc.as_mut() {
                    o.page_down(page);
                }
            }
            KeyCode::Enter => self.jump_to_markdown_toc(),
            KeyCode::Esc => self.markdown_toc = None,
            _ => {}
        }
    }

    /// Scroll the preview to the highlighted TOC heading and close the TOC,
    /// back to the preview (not the source buffer -- unlike the source-file
    /// outline's `jump_to_outline`, this stays inside the preview overlay).
    fn jump_to_markdown_toc(&mut self) {
        let Some(line) = self.markdown_toc.as_ref().and_then(Outline::selected_line) else {
            return;
        };
        self.markdown_toc = None;
        if let Some(p) = self.markdown_preview.as_mut() {
            p.scroll_to_line(line);
        }
    }

    /// The active buffer's media type (from its file extension), if recognized.
    fn active_media_type(&self) -> Option<String> {
        let ext = self
            .editor
            .active_tab()?
            .path
            .as_ref()?
            .extension()?
            .to_string_lossy()
            .into_owned();
        crate::media_type::for_extension(&ext).map(|m| m.media_type.to_string())
    }

    /// Rebuild the in-scope snippet library (bundled + file scopes) for the active
    /// buffer's media type. Cached by media type; `force` rebuilds regardless.
    pub(super) fn refresh_snippet_library(&mut self, force: bool) {
        let media = self.active_media_type();
        let key = media.clone().unwrap_or_default();
        if !force && self.snippet_library_key.as_deref() == Some(key.as_str()) {
            return;
        }
        let files = crate::snippets::load_scoped(
            media.as_deref(),
            &self.root,
            &self.settings.project_snippets,
        );
        self.snippet_library = crate::snippets::merge(crate::snippets::bundled(), files);
        self.snippet_library_key = Some(key);
    }

    /// Open the Snippets picker (rebuilding the library for the active buffer).
    pub(super) fn open_snippets(&mut self) {
        self.refresh_snippet_library(true);
        self.snippets = Some(crate::snippets::Picker::new());
    }

    /// Tools → New Snippet from Selection (T205): capture the active
    /// selection's text and prompt for an expansion prefix, which also
    /// becomes the saved snippet's name. No-op with a status message when
    /// there's no selection.
    pub(super) fn new_snippet_from_selection(&mut self) {
        let selection = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(text) if !text.trim().is_empty() => {
                self.pending_snippet_body = Some(text);
                self.prompt = Some(Prompt::new(
                    PromptKind::SnippetPrefixFromSelection,
                    t!("prompt.snippet_prefix").to_string(),
                ));
            }
            _ => self.status = t!("status.no_selection").to_string(),
        }
    }

    /// `PromptKind::SnippetPrefixFromSelection`'s accept handler: save the
    /// captured selection to the global snippets file under `prefix`, used as
    /// both the snippet's name and its expansion prefix. An empty prefix is a
    /// no-op (matches `save_theme_as`'s empty-name precedent); a prefix that
    /// collides with an existing global snippet overwrites it.
    pub(super) fn save_snippet_from_selection(&mut self, prefix: &str) {
        let Some(body) = self.pending_snippet_body.take() else {
            return;
        };
        if prefix.is_empty() {
            return;
        }
        let Some(path) = crate::snippets::global_dir().map(|d| d.join("snippets.json")) else {
            self.messages
                .error(t!("msg.snippet_save_failed", error = "no config directory").to_string());
            return;
        };
        let mut snippets = crate::snippets::load_file(&path, &crate::snippets::Scope::Global);
        snippets.retain(|s| s.name != prefix);
        snippets.push(crate::snippets::Snippet {
            name: prefix.to_string(),
            prefixes: vec![prefix.to_string()],
            body,
            description: String::new(),
            scope: crate::snippets::Scope::Global,
        });
        if let Err(e) = crate::snippets::save_file(&path, &snippets) {
            self.messages
                .error(t!("msg.snippet_save_failed", error = e).to_string());
            return;
        }
        self.refresh_snippet_library(true);
        self.status = t!("status.snippet_saved", name = prefix).to_string();
    }

    /// Handle a key in the snippet picker: arrows and paging move, typing filters by prefix
    /// (Backspace edits), Enter inserts the snippet, Esc closes.
    pub(super) fn snippets_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.snippets.height)).max(1);
        let lib = &self.snippet_library;
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.snippets.as_mut() {
                    p.up(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.snippets.as_mut() {
                    p.down(1, lib);
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.snippets.as_mut() {
                    p.up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.snippets.as_mut() {
                    p.down(page, lib);
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self.snippets.as_mut() {
                    p.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.snippets.as_mut() {
                    p.push(c);
                }
            }
            KeyCode::Enter => self.insert_selected_snippet(),
            KeyCode::Esc => self.snippets = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the snippet picker: a left click on a row inserts that snippet.
    pub(super) fn snippets_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.snippets;
        let Some(row) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        let idx = self.snippets.as_ref().map_or(0, |p| p.scroll).plus(row);
        let hit = self
            .snippets
            .as_mut()
            .is_some_and(|p| p.select_index(idx, &self.snippet_library));
        if hit {
            self.insert_selected_snippet();
        }
    }

    /// Insert the highlighted snippet's body at the cursor and close the picker,
    /// starting a tabstop session if the snippet has fields.
    fn insert_selected_snippet(&mut self) {
        let body = self
            .snippets
            .as_ref()
            .and_then(|p| p.selected_library_index(&self.snippet_library))
            .and_then(|i| self.snippet_library.get(i))
            .map(|s| s.body.clone());
        let Some(body) = body else { return };
        self.snippets = None;
        self.insert_snippet_body(&body);
    }

    /// If the word just before the cursor is a snippet prefix, replace it with the
    /// snippet body and arm a tabstop session. Returns `true` if it expanded.
    pub(super) fn expand_snippet_prefix(&mut self) -> bool {
        // Snippets are single-caret (T622): with extra carets Tab is just
        // the plain Tab key.
        if self.multi_caret_active() {
            return false;
        }
        // `expr.if` / `items.for`: postfix templates win over a prefix snippet.
        if self.expand_postfix() {
            return true;
        }
        self.refresh_snippet_library(false);
        let Some(tab) = self.editor.active_tab() else {
            return false;
        };
        if tab.is_image() {
            return false;
        }
        let cursor = tab.editor.get_cursor();
        let chars: Vec<char> = tab.editor.get_content().chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut start = cursor;
        while start > 0 && chars.get(start.minus(1)).copied().is_some_and(is_word) {
            start = start.minus(1);
        }
        if start == cursor {
            return false;
        }
        let word: String = chars.get(start..cursor).unwrap_or(&[]).iter().collect();
        let Some(snippet) = crate::snippets::find_by_prefix(&self.snippet_library, &word) else {
            return false;
        };
        let body = snippet.body.clone();
        // Select the prefix word so the snippet insertion replaces it.
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, cursor);
            t.editor.focus(&area);
        }
        self.insert_snippet_body(&body);
        true
    }

    /// Insert a snippet `body` (with `$1`/`${1:…}`/`$0` tabstops) at the cursor.
    /// Places the cursor at the first tabstop and arms a [`SnippetSession`] so Tab
    /// walks the rest; with no tabstops it is a plain insert.
    pub(super) fn insert_snippet_body(&mut self, body: &str) {
        let parsed = crate::snippet_tool::parse(body);
        let area = self.layout.editor;
        // Insertion replaces any active selection, so the body begins at the
        // selection start (used by prefix expansion); otherwise at the cursor.
        let base = self
            .editor
            .active_tab_mut()
            .map_or(0, |t| match t.editor.get_selection() {
                Some(sel) if !sel.is_empty() => sel.sorted().0,
                _ => t.editor.get_cursor(),
            });
        if !self.editor.insert_str(&parsed.text, area) {
            return;
        }
        self.status = t!("status.snippet_inserted").to_string();
        self.start_snippet_session(base, &parsed.stops);
    }

    /// Arm a tabstop session over `stops` (offsets relative to `base`, the
    /// char offset where the snippet's text now begins) and select the
    /// first; nothing when there are no stops. Shared by the snippet picker,
    /// prefix/postfix expansion, and LSP snippet completions (T613).
    pub(super) fn start_snippet_session(
        &mut self,
        base: usize,
        stops: &[crate::snippet_tool::Stop],
    ) {
        if stops.is_empty() {
            return;
        }
        let stops: Vec<(usize, usize)> = stops
            .iter()
            .map(|s| (base.plus(s.start), base.plus(s.end)))
            .collect();
        let single = stops.len() == 1;
        self.snippet_session = Some(SnippetSession { stops, index: 0 });
        self.snippet_goto(0);
        // A lone `$0` is just a caret placement — no session to navigate.
        if single {
            self.snippet_session = None;
        }
    }

    /// Move to tabstop `index`: select its placeholder (or place a bare caret).
    fn snippet_goto(&mut self, index: usize) {
        let Some((start, end)) = self
            .snippet_session
            .as_ref()
            .and_then(|s| s.stops.get(index).copied())
        else {
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, end);
            t.editor.focus(&area);
        }
    }

    /// Advance to the next snippet tabstop, shifting later stops by the net length
    /// change the user made at the current one. Ends the session after the last.
    pub(super) fn snippet_tab(&mut self) {
        let cursor = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.get_cursor());
        let next = {
            let Some(sess) = self.snippet_session.as_mut() else {
                return;
            };
            let Some(&(cur_start, cur_end)) = sess.stops.get(sess.index) else {
                return;
            };
            // A later stop nested inside the current field (`${1:a ${2:b}}`)
            // was typed over once the user edited that field: drop it.
            if cursor != cur_end {
                let current = sess.index;
                let mut i = 0;
                sess.stops.retain(|&(s, e)| {
                    let nested = s >= cur_start && e <= cur_end && s < cur_end;
                    let keep = i <= current || !nested;
                    i = i.plus(1);
                    keep
                });
            }
            // Shift every later stop by the net length change the user made at the
            // current field (cursor vs. the field's original end).
            for (s, e) in &mut sess.stops {
                if *s >= cur_end {
                    if cursor >= cur_end {
                        let add = cursor.minus(cur_end);
                        *s = s.plus(add);
                        *e = e.plus(add);
                    } else {
                        let sub = cur_end.minus(cursor);
                        *s = s.minus(sub);
                        *e = e.minus(sub);
                    }
                }
            }
            let next = sess.index.plus(1);
            if next >= sess.stops.len() {
                None
            } else {
                sess.index = next;
                Some(next)
            }
        };
        match next {
            Some(i) => self.snippet_goto(i),
            None => self.snippet_session = None,
        }
    }

    /// Whether a snippet tabstop session is active (so Tab navigates fields).
    pub(super) fn snippet_active(&self) -> bool {
        self.snippet_session.is_some()
    }

    /// Open the system-information panel (OS, terminal, Vix version and similar).
    pub(super) fn open_system_info(&mut self) {
        self.info.open_system_info(SystemInfoPanel::open());
    }

    /// Handle a key in the system-information panel: movement keys select, Enter inserts the
    /// highlighted value (panel stays open), Esc closes.
    pub(super) fn system_info_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.system_info.height)).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.info.system_info_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.info.system_info_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.info.system_info_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.info.system_info_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.info.system_info_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.info.system_info_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts the highlighted value and keeps the panel open; Esc closes.
            KeyCode::Enter => self.insert_selected_system_info(),
            KeyCode::Esc => self.info.close_system_info(),
            _ => {}
        }
    }

    /// Handle a mouse event in the system-information panel: a left click on a row inserts its
    /// value.
    pub(super) fn system_info_mouse(&mut self, mouse: MouseEvent) {
        let r = self.layout.system_info;
        let Some(row_in_view) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        if let Some(p) = self.info.system_info_mut() {
            let idx = p.scroll.plus(row_in_view);
            if p.select_index(idx) {
                self.insert_selected_system_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel
    /// open). Section-heading rows have no value, so they insert nothing.
    fn insert_selected_system_info(&mut self) {
        let Some(p) = self.info.system_info() else {
            return;
        };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    use std::path::PathBuf;
    use vix_input::{KeyModifiers, MouseButton, MouseEventKind};

    fn app() -> App {
        let mut a = App::new(std::env::temp_dir(), Settings::default());
        a.layout.editor = Rect::new(0, 0, 80, 24);
        a
    }

    fn press(a: &mut App, code: KeyCode) {
        a.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// Draw a full frame so the panels record their hit areas in `layout`.
    fn draw(a: &mut App) {
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
        terminal.draw(|f| crate::ui::draw(a, f)).unwrap();
    }

    fn click(a: &mut App, rect: Rect, row: u16) {
        a.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x.plus(1),
            row: rect.y.plus(row),
            modifiers: KeyModifiers::NONE,
        });
    }

    fn tab_text(a: &App) -> String {
        a.editor.active_tab().unwrap().text()
    }

    fn vcf_dir(tag: &str) -> (PathBuf, Vec<crate::contact_panel::Contact>) {
        let dir = std::env::temp_dir().join(format!("vix-r2c-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut list = Vec::new();
        for (i, name) in ["Ada Lovelace", "Grace Hopper"].iter().enumerate() {
            let path = dir.join(format!("c{i}.vcf"));
            std::fs::write(
                &path,
                format!("BEGIN:VCARD\nVERSION:3.0\nFN:{name}\nEMAIL:c{i}@example.org\nEND:VCARD\n"),
            )
            .unwrap();
            list.push(crate::contact_panel::Contact {
                name: (*name).to_string(),
                path,
            });
        }
        (dir, list)
    }

    #[test]
    fn contacts_keys_navigate_open_a_card_and_close() {
        let (dir, list) = vcf_dir("contacts-keys");
        let mut a = app();
        a.contacts = Some(ContactPanel::open(list));
        draw(&mut a);
        press(&mut a, KeyCode::Down);
        assert_eq!(a.contacts.as_ref().unwrap().selected, 1);
        press(&mut a, KeyCode::Home);
        assert_eq!(a.contacts.as_ref().unwrap().selected, 0);
        press(&mut a, KeyCode::End);
        assert_eq!(a.contacts.as_ref().unwrap().selected, 1);
        press(&mut a, KeyCode::Enter);
        let card = a.vcard.as_ref().expect("Enter opens the selected card");
        assert!(card.title().contains("Grace Hopper"));
        press(&mut a, KeyCode::Esc);
        assert!(a.vcard.is_none());
        press(&mut a, KeyCode::Esc);
        assert!(a.contacts.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn contacts_click_opens_that_contact_and_a_missing_file_reports() {
        let (dir, mut list) = vcf_dir("contacts-click");
        list.push(crate::contact_panel::Contact {
            name: "Ghost".to_string(),
            path: dir.join("missing.vcf"),
        });
        let mut a = app();
        a.contacts = Some(ContactPanel::open(list));
        draw(&mut a);
        let rect = a.layout.contacts;
        click(&mut a, rect, 1);
        assert!(a.vcard.as_ref().unwrap().title().contains("Grace Hopper"));
        a.vcard = None;
        // A click outside the rows does nothing; the unreadable card reports.
        a.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert!(a.vcard.is_none());
        click(&mut a, rect, 2);
        assert!(a.vcard.is_none(), "an unreadable card is not opened");
        assert!(!a.messages.items.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn vcard_enter_and_click_insert_the_highlighted_value() {
        let (dir, list) = vcf_dir("vcard");
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.contacts = Some(ContactPanel::open(list));
        draw(&mut a);
        press(&mut a, KeyCode::Enter);
        draw(&mut a);
        let value = a.vcard.as_ref().unwrap().selected_value();
        assert!(!value.is_empty());
        press(&mut a, KeyCode::Enter);
        assert_eq!(tab_text(&a), value);
        assert_eq!(
            a.status,
            t!("status.ascii_inserted", name = value.clone()).to_string()
        );
        // Navigation keys move the selection; a click inserts that row.
        press(&mut a, KeyCode::Down);
        let second = a.vcard.as_ref().unwrap().selected_value();
        let rect = a.layout.vcard;
        click(&mut a, rect, 1);
        assert!(tab_text(&a).contains(&second));
        press(&mut a, KeyCode::Esc);
        assert!(a.vcard.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn vcard_with_an_empty_value_inserts_nothing() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        let mut p = VcardPanel::open(crate::vcard_parser::parse("BEGIN:VCARD\nEND:VCARD\n"));
        p.selected = p.len();
        a.vcard = Some(p);
        press(&mut a, KeyCode::Enter);
        assert_eq!(tab_text(&a), "");
    }

    #[test]
    fn file_info_keys_and_clicks_insert_values_and_esc_closes() {
        let dir = std::env::temp_dir().join(format!("vix-r2c-fi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.txt");
        std::fs::write(&file, "alpha beta\ngamma\n").unwrap();
        let mut a = App::new(dir.clone(), Settings::default());
        a.layout.editor = Rect::new(0, 0, 80, 24);
        a.open_initial(&file);
        a.run_action("tools.file_info");
        draw(&mut a);
        let panel = a.info.file_info().unwrap();
        let rows: Vec<(String, String)> = panel
            .rows
            .iter()
            .map(|r| (r.label.clone(), r.value.clone()))
            .collect();
        assert!(rows.iter().any(|(_, v)| v.contains("note.txt")));
        // Keys move the selection and Enter inserts the highlighted value.
        press(&mut a, KeyCode::Down);
        let value = a.info.file_info().unwrap().selected_value();
        press(&mut a, KeyCode::Enter);
        if !value.is_empty() {
            assert!(tab_text(&a).contains(&value));
        }
        press(&mut a, KeyCode::PageDown);
        press(&mut a, KeyCode::PageUp);
        // A click on the first row inserts it too, and the panel stays open.
        let rect = a.layout.file_info;
        let before = tab_text(&a);
        click(&mut a, rect, 0);
        assert!(a.info.file_info().is_some());
        let first = a.info.file_info().unwrap().selected_value();
        if !first.is_empty() {
            assert!(tab_text(&a).len() > before.len());
        }
        press(&mut a, KeyCode::Esc);
        assert!(a.info.file_info().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_info_of_an_unsaved_buffer_still_counts_the_text() {
        let mut a = app();
        a.editor.new_tab_with_content("one two\nthree\n");
        a.open_file_info();
        let rows = &a.info.file_info().unwrap().rows;
        assert!(rows.iter().any(|r| r.value == "3" || r.value == "2"));
    }

    #[test]
    fn text_info_uses_the_selection_and_keys_insert_values() {
        let mut a = app();
        a.editor.new_tab_with_content("alpha beta gamma\nsecond line\n");
        a.editor.active_tab_mut().unwrap().editor.set_selection_range(0, 10);
        assert_eq!(a.selected_or_all_text(), "alpha beta");
        a.run_action("tools.text_info");
        draw(&mut a);
        // Down then Enter inserts the second statistic and keeps the panel.
        press(&mut a, KeyCode::Down);
        let value = a.info.text_info().unwrap().selected_value();
        assert!(!value.is_empty());
        press(&mut a, KeyCode::Enter);
        assert!(tab_text(&a).contains(&value));
        press(&mut a, KeyCode::Up);
        assert_eq!(a.info.text_info().unwrap().selected, 0);
        // Clicking a row inserts that row's value.
        let rect = a.layout.text_info;
        click(&mut a, rect, 1);
        assert_eq!(a.info.text_info().unwrap().selected, 1);
        press(&mut a, KeyCode::Esc);
        assert!(a.info.text_info().is_none());
    }

    #[test]
    fn markdown_preview_keys_scroll_open_the_toc_and_close() {
        let mut a = app();
        let body: String = (0..30)
            .map(|i| format!("# Heading {i}\n\ntext {i}\n\n"))
            .collect();
        a.editor.new_tab_with_content(&body);
        a.run_action("tools.markdown_preview");
        draw(&mut a);
        press(&mut a, KeyCode::Down);
        assert_eq!(a.markdown_preview.as_ref().unwrap().scroll, 1);
        press(&mut a, KeyCode::Up);
        assert_eq!(a.markdown_preview.as_ref().unwrap().scroll, 0);
        press(&mut a, KeyCode::PageDown);
        let after_page = a.markdown_preview.as_ref().unwrap().scroll;
        assert!(after_page > 1);
        press(&mut a, KeyCode::PageUp);
        assert!(a.markdown_preview.as_ref().unwrap().scroll < after_page);

        // `t` opens the table of contents; its keys walk the headings and
        // Enter scrolls the preview to the chosen one.
        press(&mut a, KeyCode::Char('t'));
        assert!(a.markdown_toc.is_some());
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::PageDown);
        press(&mut a, KeyCode::PageUp);
        press(&mut a, KeyCode::Up);
        let sel = a.markdown_toc.as_ref().unwrap().selected;
        assert!(sel > 0);
        let line = a.markdown_toc.as_ref().unwrap().selected_line().unwrap();
        press(&mut a, KeyCode::Enter);
        assert!(a.markdown_toc.is_none());
        assert!(a.markdown_preview.is_some(), "back in the preview");
        let _ = line;
        press(&mut a, KeyCode::Char('t'));
        press(&mut a, KeyCode::Esc);
        assert!(a.markdown_toc.is_none());
        press(&mut a, KeyCode::Char('q'));
        assert!(a.markdown_preview.is_none());
    }

    #[test]
    fn markdown_toc_without_headings_says_so() {
        let mut a = app();
        a.editor.new_tab_with_content("just a paragraph\n");
        a.run_action("tools.markdown_preview");
        press(&mut a, KeyCode::Char('T'));
        assert!(a.markdown_toc.is_none());
        assert_eq!(a.status, t!("status.markdown_toc_empty").to_string());
        press(&mut a, KeyCode::Esc);
        assert!(a.markdown_preview.is_none());
    }

    #[test]
    fn markdown_preview_of_no_tab_opens_nothing() {
        let mut a = app();
        a.open_markdown_preview();
        // The scratch tab App::new opens is text, so a preview exists...
        assert!(a.markdown_preview.is_some());
        a.markdown_preview = None;
        // ...but a key with no panel open is simply ignored.
        a.markdown_preview_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        a.markdown_toc_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(a.markdown_preview.is_none() && a.markdown_toc.is_none());
    }

    fn snippet(name: &str, prefix: &str, body: &str) -> crate::snippets::Snippet {
        crate::snippets::Snippet {
            name: name.to_string(),
            prefixes: vec![prefix.to_string()],
            body: body.to_string(),
            description: String::new(),
            scope: crate::snippets::Scope::Global,
        }
    }

    #[test]
    fn snippets_picker_filters_and_enter_inserts_the_body() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.run_action("tools.snippets");
        a.snippet_library = vec![
            snippet("alpha", "al", "ALPHA-BODY"),
            snippet("beta", "be", "BETA-BODY"),
        ];
        draw(&mut a);
        for c in "bet".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        press(&mut a, KeyCode::Backspace);
        press(&mut a, KeyCode::Char('t'));
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::PageDown);
        press(&mut a, KeyCode::Up);
        press(&mut a, KeyCode::PageUp);
        press(&mut a, KeyCode::Enter);
        assert!(a.snippets.is_none());
        assert_eq!(tab_text(&a), "BETA-BODY");
        assert_eq!(a.status, t!("status.snippet_inserted").to_string());
        a.run_action("tools.snippets");
        press(&mut a, KeyCode::Esc);
        assert!(a.snippets.is_none());
    }

    #[test]
    fn snippets_click_inserts_the_clicked_row() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.run_action("tools.snippets");
        a.snippet_library = vec![
            snippet("alpha", "al", "ALPHA-BODY"),
            snippet("beta", "be", "BETA-BODY"),
        ];
        draw(&mut a);
        let rect = a.layout.snippets;
        click(&mut a, rect, 1);
        assert_eq!(tab_text(&a), "BETA-BODY");
        assert!(a.snippets.is_none());
    }

    #[test]
    fn a_snippet_prefix_expands_and_tab_walks_its_stops() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.snippet_library = vec![snippet("loop", "lp", "for ${1:i} in ${2:xs} { $0 }")];
        a.snippet_library_key = Some(String::new());
        for c in "lp".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        assert!(a.expand_snippet_prefix());
        assert_eq!(tab_text(&a), "for i in xs {  }");
        assert!(a.snippet_active());
        // The first field is selected; typing replaces it, Tab moves on.
        let sel = a.editor.active_tab_mut().unwrap().editor.get_selection_text();
        assert_eq!(sel.as_deref(), Some("i"));
        press(&mut a, KeyCode::Char('n'));
        a.snippet_tab();
        let sel = a.editor.active_tab_mut().unwrap().editor.get_selection_text();
        assert_eq!(sel.as_deref(), Some("xs"));
        a.snippet_tab();
        a.snippet_tab();
        assert!(!a.snippet_active(), "the session ends after the last stop");
        assert!(tab_text(&a).starts_with("for n in xs"));
    }

    #[test]
    fn a_word_that_is_no_snippet_prefix_does_not_expand() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.snippet_library_key = Some(String::new());
        assert!(!a.expand_snippet_prefix(), "an empty line has no word");
        for c in "zzz".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        assert!(!a.expand_snippet_prefix());
        assert_eq!(tab_text(&a), "zzz");
        a.snippet_tab();
        assert!(!a.snippet_active());
    }

    #[test]
    fn new_snippet_from_selection_needs_a_selection() {
        let mut a = app();
        a.editor.new_tab_with_content("some text");
        a.new_snippet_from_selection();
        assert_eq!(a.status, t!("status.no_selection").to_string());
        assert!(a.prompt.is_none());
        a.editor.active_tab_mut().unwrap().editor.set_selection_range(0, 4);
        a.new_snippet_from_selection();
        assert!(a.prompt.is_some());
        assert_eq!(a.pending_snippet_body.as_deref(), Some("some"));
        // An empty prefix saves nothing and an absent capture is ignored.
        a.save_snippet_from_selection("");
        assert!(a.pending_snippet_body.is_none());
        a.save_snippet_from_selection("anything");
    }

    #[test]
    fn media_type_comes_from_the_file_extension() {
        let mut a = app();
        assert_eq!(a.active_media_type(), None);
        a.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("main.rs"));
        assert!(a.active_media_type().is_some());
        a.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("noext"));
        assert_eq!(a.active_media_type(), None);
    }

    #[test]
    fn system_info_keys_and_clicks_walk_and_insert() {
        let mut a = app();
        a.editor.new_tab_with_content("");
        a.run_action("tools.system_info");
        draw(&mut a);
        let n = a.info.system_info().unwrap().len();
        press(&mut a, KeyCode::End);
        assert!(a.info.system_info().unwrap().selected > 0);
        press(&mut a, KeyCode::Home);
        assert_eq!(a.info.system_info().unwrap().selected, 0);
        press(&mut a, KeyCode::PageDown);
        assert!(a.info.system_info().unwrap().selected > 0);
        press(&mut a, KeyCode::PageUp);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Up);
        assert!(n > 3);
        // Walk to a row that has a value and insert it with Enter.
        let mut value = String::new();
        for _ in 0..n {
            press(&mut a, KeyCode::Down);
            value = a.info.system_info().unwrap().selected_value();
            if !value.is_empty() {
                break;
            }
        }
        assert!(!value.is_empty());
        press(&mut a, KeyCode::Enter);
        assert!(tab_text(&a).contains(&value));
        // A click on a heading row inserts nothing.
        let before = tab_text(&a);
        a.info.system_info_mut().unwrap().scroll = 0;
        let rect = a.layout.system_info;
        click(&mut a, rect, 0);
        assert_eq!(tab_text(&a), before);
        press(&mut a, KeyCode::Esc);
        assert!(a.info.system_info().is_none());
    }
}
