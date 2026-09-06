//! Tools → the small glyph/color/type picker panels: Nerd Font
//! character picker, ASCII-art character picker, X11 color picker, media
//! type catalog, and the QR code generator -- each an "open, arrow/click
//! to select, insert" overlay.
//!
//! Moved out of `app.rs` verbatim (T141, slice 11 -- a sub-slice of
//! "tools"). `active_media_type` sits right next to this cluster by
//! line number but stayed in `app.rs`: it's the snippet library's own
//! media-type detection, not part of the media-type *panel*, despite the
//! similar name.

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{App, AsciiPanel, NerdPalette, X11Panel, rect_contains};

impl App {
    pub(super) fn open_nerd_palette(&mut self) {
        self.nerd_palette = Some(NerdPalette::open());
    }

    pub(super) fn nerd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.down();
                }
            }
            KeyCode::Left => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.left();
                }
            }
            KeyCode::Right => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.right();
                }
            }
            // Enter inserts and keeps the palette open so several glyphs can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_glyph(),
            KeyCode::Esc => self.nerd_palette = None,
            _ => {}
        }
    }

    pub(super) fn nerd_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.nerd_palette;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let col = ((mouse.column - r.x) / crate::ui::NERD_CELL_W) as usize;
        let row = (mouse.row - r.y) as usize;
        if let Some(p) = self.nerd_palette.as_mut()
            && p.select_at(row, col)
        {
            self.insert_selected_glyph();
        }
    }

    /// Insert the highlighted glyph into the active editor (leaving the palette
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_glyph(&mut self) {
        let Some(p) = self.nerd_palette.as_ref() else {
            return;
        };
        let glyph = p.selected_glyph();
        let name = p.selected_name();
        let area = self.layout.editor;
        if self.editor.insert_str(&glyph.to_string(), area) {
            self.status = t!("status.glyph_inserted", name = name).to_string();
        }
    }

    pub(super) fn open_ascii_panel(&mut self) {
        self.ascii_panel = Some(AsciiPanel::open());
    }

    /// Generate a QR code from the current selection (or the cursor's line) and
    /// show it in a read-only overlay. Warns when there is nothing to encode.
    pub(super) fn open_qrcode(&mut self) {
        let text = self.editor.active_tab_mut().map(|tab| {
            tab.editor
                .get_selection_text()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| tab.lines().get(tab.editor.cursor_line()).cloned())
                .unwrap_or_default()
                .trim()
                .to_string()
        });
        match text.as_deref().and_then(crate::qr_tool::render) {
            Some(art) => self.qrcode = Some(art),
            None => self.messages.warn(t!("msg.qrcode_empty").to_string()),
        }
    }

    pub(super) fn ascii_key(&mut self, key: KeyEvent) {
        let page = (self.layout.ascii_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several characters can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_ascii(),
            KeyCode::Esc => self.ascii_panel = None,
            _ => {}
        }
    }

    pub(super) fn ascii_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.ascii_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.ascii_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_ascii();
            }
        }
    }

    /// Insert the highlighted character into the active editor (leaving the panel
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_ascii(&mut self) {
        let Some(p) = self.ascii_panel.as_ref() else {
            return;
        };
        let ch = p.selected_char();
        let name = p.selected_label();
        let area = self.layout.editor;
        if self.editor.insert_str(&ch.to_string(), area) {
            self.status = t!("status.ascii_inserted", name = name).to_string();
        }
    }

    pub(super) fn open_x11_panel(&mut self) {
        self.x11_panel = Some(X11Panel::open());
    }

    pub(super) fn x11_key(&mut self, key: KeyEvent) {
        let page = (self.layout.x11_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several colors can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_x11(),
            KeyCode::Esc => self.x11_panel = None,
            _ => {}
        }
    }

    pub(super) fn x11_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.x11_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.x11_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_x11();
            }
        }
    }

    /// Insert the highlighted color's hex (e.g. `#F0F8FF`) into the active editor
    /// (leaving the panel open). No-op when there is no editable buffer.
    fn insert_selected_x11(&mut self) {
        let Some(p) = self.x11_panel.as_ref() else {
            return;
        };
        let hex = p.selected_hex().to_string();
        let name = p.selected_name().to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&hex, area) {
            self.status = t!("status.x11_inserted", name = name).to_string();
        }
    }

    /// Open the media-type picker, pre-selected to the active file's type when
    /// its extension is recognized.
    pub(super) fn open_media_type_panel(&mut self) {
        let ext = self
            .editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.media_type_panel = Some(crate::media_type::Panel::open_for_extension(&ext));
    }

    pub(super) fn media_type_key(&mut self, key: KeyEvent) {
        let page = (self.layout.media_type_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.push(c);
                }
            }
            // Enter inserts and keeps the panel open; Esc closes it.
            KeyCode::Enter => self.insert_selected_media_type(),
            KeyCode::Esc => self.media_type_panel = None,
            _ => {}
        }
    }

    pub(super) fn media_type_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.media_type_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.media_type_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_media_type();
            }
        }
    }

    /// Insert the highlighted media type (e.g. `image/png`) into the active editor
    /// (leaving the panel open). No-op when there is no editable buffer.
    fn insert_selected_media_type(&mut self) {
        let Some(entry) = self
            .media_type_panel
            .as_ref()
            .and_then(crate::media_type::Panel::selected_entry)
        else {
            return;
        };
        let media_type = entry.media_type.to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&media_type, area) {
            self.status = t!("status.media_type_inserted", media_type = media_type).to_string();
        }
    }
}
