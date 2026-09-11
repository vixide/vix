//! Tools → the small info/reference panels: the Contacts vCard view,
//! the file-info and text-info panels (Tools → File Info / Text Info),
//! Markdown preview, the Snippets picker (with its tabstop-expansion
//! session and the active buffer's media-type-scoped library), and the
//! System Info panel.
//!
//! Moved out of `app.rs` verbatim (T141, slice 12 -- the last sub-slice
//! of "tools", another large genuinely contiguous block like org core
//! was, no cherry-picking needed).

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{
    App, ContactPanel, FileInfoPanel, MarkdownPreview, Prompt, PromptKind, SnippetSession,
    SystemInfoPanel, TextInfoPanel, VcardPanel, rect_contains,
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

    pub(super) fn contacts_key(&mut self, key: KeyEvent) {
        let page = (self.layout.contacts.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.contacts.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.contacts.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.open_selected_vcard(),
            KeyCode::Esc => self.contacts = None,
            _ => {}
        }
    }

    pub(super) fn contacts_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.contacts;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        let hit = self
            .contacts
            .as_mut()
            .is_some_and(|p| p.select_index(p.scroll + row_in_view));
        if hit {
            self.open_selected_vcard();
        }
    }

    pub(super) fn vcard_key(&mut self, key: KeyEvent) {
        let page = (self.layout.vcard.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.vcard.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.vcard.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.insert_selected_vcard_value(),
            // Esc returns to the contact browser (or closes if opened directly).
            KeyCode::Esc => self.vcard = None,
            _ => {}
        }
    }

    pub(super) fn vcard_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.vcard;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        let hit = self
            .vcard
            .as_mut()
            .is_some_and(|p| p.select_index(p.scroll + row_in_view));
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

    pub(super) fn open_file_info(&mut self) {
        let info = self.gather_file_info();
        self.file_info = Some(FileInfoPanel::open(&info));
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
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            info.path = path.display().to_string();
            if let Ok(meta) = std::fs::metadata(path) {
                info.bytes = Some(meta.len());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    info.mode = Some(meta.permissions().mode());
                }
                if let Ok(modified) = meta.modified()
                    && let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH)
                {
                    info.modified_secs = i64::try_from(d.as_secs()).ok();
                }
            }
        }
        info
    }

    pub(super) fn file_info_key(&mut self, key: KeyEvent) {
        let page = (self.layout.file_info.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.file_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.file_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.insert_selected_file_info(),
            KeyCode::Esc => self.file_info = None,
            _ => {}
        }
    }

    pub(super) fn file_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.file_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.file_info.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_file_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_file_info(&mut self) {
        let Some(p) = self.file_info.as_ref() else {
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

    /// Open the Text Information panel over the selection, or the whole buffer
    /// when nothing is selected.
    pub(super) fn open_text_info(&mut self) {
        let text = self.selected_or_all_text();
        let stats = crate::text_information_panel::analyze(&text);
        self.text_info = Some(TextInfoPanel::open(&stats));
    }

    pub(super) fn text_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.text_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.text_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::Enter => self.insert_selected_text_info(),
            KeyCode::Esc => self.text_info = None,
            _ => {}
        }
    }

    pub(super) fn text_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.text_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.text_info.as_mut()
            && p.select_index(row_in_view)
        {
            self.insert_selected_text_info();
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_text_info(&mut self) {
        let Some(p) = self.text_info.as_ref() else {
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

    /// Open a read-only Markdown preview of the active buffer.
    pub(super) fn open_markdown_preview(&mut self) {
        let Some(text) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(Tab::text)
        else {
            return;
        };
        self.markdown_preview = Some(MarkdownPreview::open(&text));
    }

    pub(super) fn markdown_preview_key(&mut self, key: KeyEvent) {
        let page = (self.layout.editor.height as usize)
            .max(1)
            .saturating_sub(2);
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
            KeyCode::Esc | KeyCode::Char('q') => self.markdown_preview = None,
            _ => {}
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

    pub(super) fn snippets_key(&mut self, key: KeyEvent) {
        let page = (self.layout.snippets.height as usize).max(1);
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

    pub(super) fn snippets_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.snippets;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        let idx = self.snippets.as_ref().map_or(0, |p| p.scroll) + row;
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
        while start > 0 && chars.get(start - 1).copied().is_some_and(is_word) {
            start -= 1;
        }
        if start == cursor {
            return false;
        }
        let word: String = chars[start..cursor].iter().collect();
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
    fn insert_snippet_body(&mut self, body: &str) {
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
        if parsed.stops.is_empty() {
            return;
        }
        let stops: Vec<(usize, usize)> = parsed
            .stops
            .iter()
            .map(|s| (base + s.start, base + s.end))
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
            let cur_end = sess.stops[sess.index].1;
            // Shift every later stop by the net length change the user made at the
            // current field (cursor vs. the field's original end).
            for (s, e) in &mut sess.stops {
                if *s >= cur_end {
                    if cursor >= cur_end {
                        let add = cursor - cur_end;
                        *s += add;
                        *e += add;
                    } else {
                        let sub = cur_end - cursor;
                        *s = s.saturating_sub(sub);
                        *e = e.saturating_sub(sub);
                    }
                }
            }
            let next = sess.index + 1;
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

    pub(super) fn open_system_info(&mut self) {
        self.system_info = Some(SystemInfoPanel::open());
    }

    pub(super) fn system_info_key(&mut self, key: KeyEvent) {
        let page = (self.layout.system_info.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.system_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.system_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts the highlighted value and keeps the panel open; Esc closes.
            KeyCode::Enter => self.insert_selected_system_info(),
            KeyCode::Esc => self.system_info = None,
            _ => {}
        }
    }

    pub(super) fn system_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.system_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.system_info.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_system_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel
    /// open). Section-heading rows have no value, so they insert nothing.
    fn insert_selected_system_info(&mut self) {
        let Some(p) = self.system_info.as_ref() else {
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
