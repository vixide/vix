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

use crate::app::info_panel::InfoSlot;
use ratatui::layout::Rect;
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::{App, AppFlags, AsciiPanel, NerdPalette, Prompt, PromptKind, X11Panel, rect_contains};
use crate::settings::Settings;

/// The navigation surface the ASCII, X11 color, media type, and theme
/// editor panels all share -- extracted (Run H, T529) after their key/mouse
/// dispatch turned up near-identical apart from field name and the action
/// taken on select. A local trait over each panel's own (foreign, per-crate)
/// type; each impl below just forwards to the panel's identically-named
/// inherent methods.
pub(super) trait ListPanel {
    fn up(&mut self);
    fn down(&mut self);
    fn page_up(&mut self, page: usize);
    fn page_down(&mut self, page: usize);
    fn scroll(&self) -> usize;
}

macro_rules! impl_list_panel {
    ($ty:ty) => {
        impl ListPanel for $ty {
            fn up(&mut self) {
                Self::up(self);
            }
            fn down(&mut self) {
                Self::down(self);
            }
            fn page_up(&mut self, page: usize) {
                Self::page_up(self, page);
            }
            fn page_down(&mut self, page: usize) {
                Self::page_down(self, page);
            }
            fn scroll(&self) -> usize {
                self.scroll
            }
        }
    };
}

impl_list_panel!(AsciiPanel);
impl_list_panel!(X11Panel);
impl_list_panel!(crate::media_type::Panel);
impl_list_panel!(crate::todo_tree::Panel);
impl_list_panel!(vix_theme_editor_panel::Panel);
impl_list_panel!(crate::fhir::BundlePanel);

/// Try the Up/Down/PageUp/PageDown navigation keys against `panel`,
/// returning whether one handled `code`. Each caller still handles its own
/// remaining keys (Home/End, Enter, Esc, ...) since those differ per panel.
pub(super) fn list_panel_nav_key<P: ListPanel>(
    panel: Option<&mut P>,
    code: KeyCode,
    page: usize,
) -> bool {
    let Some(p) = panel else {
        return false;
    };
    match code {
        KeyCode::Up => p.up(),
        KeyCode::Down => p.down(),
        KeyCode::PageUp => p.page_up(page),
        KeyCode::PageDown => p.page_down(page),
        _ => return false,
    }
    true
}

/// Resolve a left-click inside a list-style picker panel's `rect` to the
/// item index under the cursor (the panel's `scroll` offset plus the
/// clicked row), or `None` when the click isn't a left-button-down inside
/// `rect`, or no panel is open. Each caller still decides what to do when
/// `select_index` reports the click landed on a real row -- that differs
/// per panel (insert-and-keep-open vs. the theme editor's pick-and-open-
/// x11-panel).
pub(super) fn list_panel_click_index<P: ListPanel>(
    panel: Option<&P>,
    rect: Rect,
    mouse: MouseEvent,
) -> Option<usize> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return None;
    }
    if !rect_contains(rect, mouse.column, mouse.row) {
        return None;
    }
    let p = panel?;
    let row_in_view = (mouse.row - rect.y) as usize;
    Some(p.scroll() + row_in_view)
}

impl App {
    pub(super) fn open_nerd_palette(&mut self) {
        self.info.open_nerd_palette(NerdPalette::open());
    }

    pub(super) fn nerd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.info.nerd_palette_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.info.nerd_palette_mut() {
                    p.down();
                }
            }
            KeyCode::Left => {
                if let Some(p) = self.info.nerd_palette_mut() {
                    p.left();
                }
            }
            KeyCode::Right => {
                if let Some(p) = self.info.nerd_palette_mut() {
                    p.right();
                }
            }
            // Enter inserts and keeps the palette open so several glyphs can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_glyph(),
            KeyCode::Esc => self.info.close_nerd_palette(),
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
        if let Some(p) = self.info.nerd_palette_mut()
            && p.select_at(row, col)
        {
            self.insert_selected_glyph();
        }
    }

    /// Insert the highlighted glyph into the active editor (leaving the palette
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_glyph(&mut self) {
        let Some(p) = self.info.nerd_palette() else {
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
        self.info.open_ascii_panel(AsciiPanel::open());
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
        if list_panel_nav_key(self.info.ascii_panel_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Home => {
                if let Some(p) = self.info.ascii_panel_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.info.ascii_panel_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several characters can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_ascii(),
            KeyCode::Esc => self.info.close_ascii_panel(),
            _ => {}
        }
    }

    pub(super) fn ascii_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) =
            list_panel_click_index(self.info.ascii_panel(), self.layout.ascii_panel, mouse)
        else {
            return;
        };
        if let Some(p) = self.info.ascii_panel_mut()
            && p.select_index(idx)
        {
            self.insert_selected_ascii();
        }
    }

    /// Insert the highlighted character into the active editor (leaving the panel
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_ascii(&mut self) {
        let Some(p) = self.info.ascii_panel() else {
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
        if list_panel_nav_key(self.x11_panel.as_mut(), key.code, page) {
            return;
        }
        match key.code {
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
            // Enter inserts (or, opened from the theme editor, picks) and
            // keeps the panel open so several colors can be picked in a row;
            // Esc closes it.
            KeyCode::Enter => self.use_selected_x11(),
            KeyCode::Esc => {
                self.x11_panel = None;
                self.flags.remove(AppFlags::THEME_EDITOR_PICKING);
            }
            _ => {}
        }
    }

    pub(super) fn x11_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) =
            list_panel_click_index(self.x11_panel.as_ref(), self.layout.x11_panel, mouse)
        else {
            return;
        };
        if let Some(p) = self.x11_panel.as_mut()
            && p.select_index(idx)
        {
            self.use_selected_x11();
        }
    }

    /// Act on the X11 panel's highlighted color: insert its hex (e.g.
    /// `#F0F8FF`) into the active editor and leave the panel open (Tools →
    /// X11 Colors — the panel's original purpose), or, when it was opened
    /// from the theme editor to pick a slot's color (T202), apply that color
    /// to the highlighted slot instead and close the panel, returning to the
    /// editor. No-op when there is no editable buffer (the first case) or no
    /// panel open at all.
    fn use_selected_x11(&mut self) {
        let Some(p) = self.x11_panel.as_ref() else {
            return;
        };
        if self.flags.contains(AppFlags::THEME_EDITOR_PICKING) {
            let c = p.selected_color();
            if let Some(editor) = self.theme_editor.as_mut() {
                editor.apply_color([c.r, c.g, c.b]);
                crate::theme_model::apply(&editor.theme);
                self.editor.refresh_theme();
            }
            self.x11_panel = None;
            self.flags.remove(AppFlags::THEME_EDITOR_PICKING);
            return;
        }
        let hex = p.selected_hex().to_string();
        let name = p.selected_name().to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&hex, area) {
            self.status = t!("status.x11_inserted", name = name).to_string();
        }
    }

    // ----- theme editor (T202) --------------------------------------------

    /// Open the theme editor over a copy of the active theme. Edits apply
    /// live; `Esc` without saving reverts to `settings.theme`.
    pub(super) fn open_theme_editor(&mut self) {
        let base = Self::available_custom_themes()
            .into_iter()
            .find(|t| {
                t.name
                    .eq_ignore_ascii_case(&Self::effective_theme(&self.settings))
            })
            .or_else(|| {
                Self::available_custom_themes()
                    .into_iter()
                    .find(|t| t.name.eq_ignore_ascii_case("dark"))
            });
        let Some(base) = base else {
            return;
        };
        self.theme_editor_baseline = Some(self.settings.theme.clone());
        self.theme_editor = Some(vix_theme_editor_panel::Panel::open(base));
    }

    pub(super) fn theme_editor_key(&mut self, key: KeyEvent) {
        let page = (self.layout.theme_editor.height as usize).max(1);
        if list_panel_nav_key(self.theme_editor.as_mut(), key.code, page) {
            return;
        }
        match key.code {
            // Enter opens the X11 color picker to choose the highlighted
            // slot's new color (see `use_selected_x11`'s theme-editor branch).
            KeyCode::Enter => {
                if self.theme_editor.is_some() {
                    self.flags.insert(AppFlags::THEME_EDITOR_PICKING);
                    self.x11_panel = Some(X11Panel::open());
                }
            }
            KeyCode::Char('s' | 'S') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.prompt = Some(Prompt::new(
                    PromptKind::ThemeSaveAs,
                    t!("prompt.theme_save_as").to_string(),
                ));
            }
            KeyCode::Esc => self.close_theme_editor(),
            _ => {}
        }
    }

    pub(super) fn theme_editor_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) =
            list_panel_click_index(self.theme_editor.as_ref(), self.layout.theme_editor, mouse)
        else {
            return;
        };
        if let Some(p) = self.theme_editor.as_mut()
            && p.select_index(idx)
        {
            self.flags.insert(AppFlags::THEME_EDITOR_PICKING);
            self.x11_panel = Some(X11Panel::open());
        }
    }

    /// Close the theme editor, reverting the live preview to the committed
    /// theme when nothing was saved.
    pub(super) fn close_theme_editor(&mut self) {
        self.theme_editor = None;
        self.x11_panel = None;
        self.flags.remove(AppFlags::THEME_EDITOR_PICKING);
        if let Some(name) = self.theme_editor_baseline.take() {
            Self::apply_saved_theme(&name);
            self.editor.refresh_theme();
        }
    }

    /// `PromptKind::ThemeSaveAs`'s accept handler: write the draft to
    /// `~/.config/vix/themes/<name>.json` and make it the active theme.
    /// A blank name, or no themes directory available, cancels back to the
    /// editor rather than losing the draft.
    pub(super) fn save_theme_as(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        editor.theme.name = name.to_string();
        let Some(dir) = Settings::themes_dir() else {
            let e = std::io::Error::other("no themes directory");
            self.messages
                .error(t!("msg.theme_save_failed", error = e).to_string());
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.messages
                .error(t!("msg.theme_save_failed", error = e).to_string());
            return;
        }
        let path = dir.join(format!("{name}.json"));
        let json = crate::theme_model::to_json(&editor.theme);
        if let Err(e) = std::fs::write(&path, json) {
            self.messages
                .error(t!("msg.theme_save_failed", error = e).to_string());
            return;
        }
        // Saved: the draft is now the committed theme, no revert on close.
        self.theme_editor_baseline = None;
        self.theme_editor = None;
        self.x11_panel = None;
        self.settings.theme = name.to_string();
        self.store_settings_or_warn();
        Self::apply_saved_theme(name);
        self.editor.refresh_theme();
        self.status = t!("status.theme_saved", name = name).to_string();
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
        self.info
            .open_media_type_panel(crate::media_type::Panel::open_for_extension(&ext));
    }

    pub(super) fn media_type_key(&mut self, key: KeyEvent) {
        let page = (self.layout.media_type_panel.height as usize).max(1);
        if list_panel_nav_key(self.info.media_type_panel_mut(), key.code, page) {
            return;
        }
        match key.code {
            KeyCode::Backspace => {
                if let Some(p) = self.info.media_type_panel_mut() {
                    p.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.info.media_type_panel_mut() {
                    p.push(c);
                }
            }
            // Enter inserts and keeps the panel open; Esc closes it.
            KeyCode::Enter => self.insert_selected_media_type(),
            KeyCode::Esc => self.info.close_media_type_panel(),
            _ => {}
        }
    }

    pub(super) fn media_type_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) = list_panel_click_index(
            self.info.media_type_panel(),
            self.layout.media_type_panel,
            mouse,
        ) else {
            return;
        };
        if let Some(p) = self.info.media_type_panel_mut()
            && p.select_index(idx)
        {
            self.insert_selected_media_type();
        }
    }

    /// Insert the highlighted media type (e.g. `image/png`) into the active editor
    /// (leaving the panel open). No-op when there is no editable buffer.
    fn insert_selected_media_type(&mut self) {
        let Some(entry) = self
            .info
            .media_type_panel()
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

    /// Open the Todo Tree over the project's comment tags.
    pub(super) fn open_todo_tree(&mut self) {
        self.build_file_index();
        let items = self.collect_todos();
        if items.is_empty() {
            self.status = t!("status.no_todos").to_string();
            return;
        }
        self.status = t!("status.todos_n", n = items.len()).to_string();
        self.todo_tree = Some(crate::todo_tree::Panel::new(items));
    }

    pub(super) fn todo_tree_key(&mut self, key: KeyEvent) {
        let page = (self.layout.todo_tree.height as usize).max(1);
        if list_panel_nav_key(self.todo_tree.as_mut(), key.code, page) {
            return;
        }
        let Some(p) = self.todo_tree.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(' ') => p.toggle_fold(),
            KeyCode::Char('g') => p.toggle_group_by(),
            KeyCode::Char('[') => p.collapse_all(),
            KeyCode::Char(']') => p.expand_all(),
            KeyCode::Enter => {
                if let Some(item) = p.activate() {
                    self.jump_to_todo(&item);
                }
            }
            KeyCode::Esc => self.todo_tree = None,
            _ => {}
        }
    }

    pub(super) fn todo_tree_mouse(&mut self, mouse: MouseEvent) {
        let Some(idx) =
            list_panel_click_index(self.todo_tree.as_ref(), self.layout.todo_tree, mouse)
        else {
            return;
        };
        let Some(p) = self.todo_tree.as_mut() else {
            return;
        };
        if p.select_index(idx)
            && let Some(item) = p.activate()
        {
            self.jump_to_todo(&item);
        }
    }

    /// Close the tree and open the item's file at its line.
    fn jump_to_todo(&mut self, item: &crate::todo_tree::Item) {
        let path = self.root.join(&item.file);
        let (line, col) = (item.line, item.col);
        self.todo_tree = None;
        self.with_jump(|s| {
            s.open_path(&path, false);
            let area = s.editor_view();
            s.editor.goto(line, Some(col), area);
            s.focus = super::Focus::Editor;
        });
    }
}
