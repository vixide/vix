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
use crate::geo::Geo;
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
    /// Move the selection up one row.
    fn up(&mut self);
    /// Move the selection down one row.
    fn down(&mut self);
    /// Move the selection up by `page` rows.
    fn page_up(&mut self, page: usize);
    /// Move the selection down by `page` rows.
    fn page_down(&mut self, page: usize);
    /// Index of the first visible row (used to map a mouse click to a row).
    fn scroll(&self) -> usize;
    /// Number of rows, so Home/End can jump to either end.
    fn len(&self) -> usize;
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
            fn len(&self) -> usize {
                Self::len(self)
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
impl_list_panel!(crate::contact_panel::Panel);
impl_list_panel!(crate::vcard_panel::Panel);
impl_list_panel!(crate::file_information_panel::Panel);
impl_list_panel!(crate::conflict_tool::List);
impl_list_panel!(crate::app::HtmlPanel);
impl_list_panel!(crate::outline_panel::Outline);

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

/// [`list_panel_nav_key`] plus Home and End (jump to the first or last row),
/// for the panels whose only other keys are Enter and Esc.
pub(super) fn list_panel_full_nav_key<P: ListPanel>(
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
        KeyCode::Home => p.page_up(p.len()),
        KeyCode::End => p.page_down(p.len()),
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
    let row = crate::app::hit::left_click_row(rect, mouse)?;
    Some((panel?.scroll()).plus(row))
}

impl App {
    /// Open the Nerd Font glyph palette.
    pub(super) fn open_nerd_palette(&mut self) {
        self.info.open_nerd_palette(NerdPalette::open());
    }

    /// Handle a key in the Nerd Font palette: arrows move across the glyph grid, Enter inserts
    /// the glyph (palette stays open for several picks), Esc closes.
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

    /// Handle a mouse event in the Nerd Font palette: a left click maps the pointer to a grid
    /// cell (cells are `NERD_CELL_W` columns wide) and inserts that glyph.
    pub(super) fn nerd_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.nerd_palette;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let col = usize::from(mouse.column.minus(r.x).quot(crate::ui::NERD_CELL_W));
        let row = usize::from(mouse.row.minus(r.y));
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
        let (Some(glyph), Some(name)) = (p.selected_glyph(), p.selected_name()) else {
            self.messages
                .error(t!("msg.command_failed", error = "selected: out of range").to_string());
            return;
        };
        let area = self.layout.editor;
        if self.editor.insert_str(&glyph.to_string(), area) {
            self.status = t!("status.glyph_inserted", name = name).to_string();
        }
    }

    /// Open the ASCII character table.
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

    /// Handle a key in the ASCII table: shared list navigation first, then Home/End jump to the
    /// ends, Enter inserts the character (panel stays open), Esc closes.
    pub(super) fn ascii_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.ascii_panel.height)).max(1);
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

    /// Handle a mouse event in the ASCII table: a left click on a row inserts that character.
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

    /// Open the X11 color-name picker.
    pub(super) fn open_x11_panel(&mut self) {
        self.x11_panel = Some(X11Panel::open());
    }

    /// Handle a key in the X11 color picker: shared list navigation, Home/End, Enter inserts
    /// the color (or, when opened from the theme editor, assigns it to the slot), Esc closes it
    /// and ends any theme-editor pick.
    pub(super) fn x11_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.x11_panel.height)).max(1);
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

    /// Handle a mouse event in the X11 color picker: a left click on a row acts on that color
    /// as Enter does.
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
            let Some(c) = p.selected_color() else {
                self.messages
                    .error(t!("msg.command_failed", error = "selected: out of range").to_string());
                return;
            };
            let mut applied = Ok(());
            if let Some(editor) = self.theme_editor.as_mut() {
                applied = editor.apply_color([c.r, c.g, c.b]);
                crate::theme_model::apply(&editor.theme);
                self.editor.refresh_theme();
            }
            if let Err(e) = applied {
                self.messages
                    .error(t!("msg.command_failed", error = e.to_string()).to_string());
            }
            self.x11_panel = None;
            self.flags.remove(AppFlags::THEME_EDITOR_PICKING);
            return;
        }
        let (Some(hex), Some(name)) = (p.selected_hex(), p.selected_name()) else {
            self.messages
                .error(t!("msg.command_failed", error = "selected: out of range").to_string());
            return;
        };
        let (hex, name) = (hex.to_string(), name.to_string());
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

    /// Handle a key in the theme editor: shared list navigation; Enter opens the X11 picker to
    /// choose the highlighted slot's color; Ctrl+S prompts for a name to save the theme as; Esc
    /// closes the editor (reverting the live preview).
    pub(super) fn theme_editor_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.theme_editor.height)).max(1);
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

    /// Handle a mouse event in the theme editor: a left click on a slot row selects it and
    /// opens the X11 picker for it.
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
        let json = match crate::theme_model::to_json(&editor.theme) {
            Ok(json) => json,
            Err(e) => {
                self.messages
                    .error(t!("msg.theme_save_failed", error = e).to_string());
                return;
            }
        };
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
            .map_or_default(|e| e.to_string_lossy().into_owned());
        self.info
            .open_media_type_panel(crate::media_type::Panel::open_for_extension(&ext));
    }

    /// Handle a key in the media-type picker: shared list navigation, typing filters (Backspace
    /// edits), Enter inserts the media type (panel stays open), Esc closes.
    pub(super) fn media_type_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.media_type_panel.height)).max(1);
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

    /// Handle a mouse event in the media-type picker: a left click on a row inserts that media
    /// type.
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

    /// Handle a key in the Todo Tree: shared list navigation; Space folds a group, `g` toggles
    /// the grouping, `[`/`]` collapse/expand all, Enter jumps to the item, Esc closes.
    pub(super) fn todo_tree_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.todo_tree.height)).max(1);
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

    /// Handle a mouse event in the Todo Tree: a left click selects a row and activates it (a
    /// group row folds, an item jumps to its location).
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

#[cfg(test)]
mod nav_tests {
    use super::*;

    /// A ten-row panel that records the last move, standing in for any real
    /// list panel.
    #[derive(Default)]
    struct Fake {
        selected: usize,
    }

    impl ListPanel for Fake {
        fn up(&mut self) {
            self.selected = vix_list_state::up(self.selected);
        }
        fn down(&mut self) {
            self.selected = vix_list_state::down(self.selected, 10);
        }
        fn page_up(&mut self, page: usize) {
            self.selected = vix_list_state::page_up(self.selected, page);
        }
        fn page_down(&mut self, page: usize) {
            self.selected = vix_list_state::page_down(self.selected, page, 10);
        }
        fn scroll(&self) -> usize {
            0
        }
        fn len(&self) -> usize {
            10
        }
    }

    #[test]
    fn the_full_nav_keys_move_page_and_jump_to_either_end() {
        let mut p = Fake::default();
        let mut press = |code| list_panel_full_nav_key(Some(&mut p), code, 4);
        assert!(press(KeyCode::Down));
        assert!(press(KeyCode::PageDown));
        assert!(press(KeyCode::Up));
        assert!(press(KeyCode::PageUp));
        assert!(press(KeyCode::End));
        assert_eq!(p.selected, 9);
        let mut press = |code| list_panel_full_nav_key(Some(&mut p), code, 4);
        assert!(press(KeyCode::Home));
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn other_keys_and_a_closed_panel_are_not_handled() {
        let mut p = Fake::default();
        for code in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('x')] {
            assert!(!list_panel_full_nav_key(Some(&mut p), code, 4), "{code:?}");
        }
        assert!(!list_panel_full_nav_key::<Fake>(None, KeyCode::Down, 4));
        // The four-key variant leaves Home and End to its caller.
        assert!(!list_panel_nav_key(Some(&mut p), KeyCode::Home, 4));
        assert!(!list_panel_nav_key(Some(&mut p), KeyCode::End, 4));
        assert!(list_panel_nav_key(Some(&mut p), KeyCode::Down, 4));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Focus;
    use crate::app::cov4::*;

    fn fresh(tag: &str) -> App {
        let dir = temp(tag);
        let mut app = app_at(&dir);
        buffer_with(&mut app, "", 0);
        app
    }

    #[test]
    fn nerd_palette_moves_inserts_the_glyph_and_closes() {
        let mut app = fresh("pp-nerd");
        app.open_nerd_palette();
        assert!(app.info.nerd_palette().is_some());
        for k in [
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Left,
            KeyCode::Tab,
        ] {
            app.nerd_key(code(k));
        }
        let (glyph, name) = {
            let p = app.info.nerd_palette().unwrap();
            (
                p.selected_glyph().unwrap(),
                p.selected_name().unwrap().to_string(),
            )
        };
        app.nerd_key(code(KeyCode::Enter));
        assert_eq!(content(&mut app), glyph.to_string());
        assert_eq!(
            app.status,
            t!("status.glyph_inserted", name = name).to_string()
        );
        assert!(
            app.info.nerd_palette().is_some(),
            "stays open for more picks"
        );
        app.nerd_key(code(KeyCode::Esc));
        assert!(app.info.nerd_palette().is_none());
        // Enter with no palette is a no-op.
        app.nerd_key(code(KeyCode::Enter));
    }

    #[test]
    fn nerd_palette_click_inserts_only_for_a_left_press_inside_a_cell() {
        let mut app = fresh("pp-nerd-mouse");
        app.open_nerd_palette();
        app.layout.nerd_palette = Rect::new(2, 2, 40, 10);
        let moved = vix_input::MouseEvent {
            kind: vix_input::MouseEventKind::Moved,
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        };
        app.nerd_mouse(moved);
        app.nerd_mouse(click(0, 0)); // outside the palette
        assert_eq!(content(&mut app), "");
        app.nerd_mouse(click(2, 2));
        let glyph = app.info.nerd_palette().unwrap().selected_glyph().unwrap();
        assert_eq!(content(&mut app), glyph.to_string());
    }

    #[test]
    fn ascii_table_navigates_jumps_inserts_and_closes() {
        let mut app = fresh("pp-ascii");
        app.layout.ascii_panel = Rect::new(0, 0, 30, 8);
        app.open_ascii_panel();
        app.ascii_key(code(KeyCode::Down));
        app.ascii_key(code(KeyCode::PageDown));
        app.ascii_key(code(KeyCode::Up));
        app.ascii_key(code(KeyCode::PageUp));
        app.ascii_key(code(KeyCode::End));
        let last = app.info.ascii_panel().unwrap().selected_char();
        app.ascii_key(code(KeyCode::Enter));
        assert_eq!(content(&mut app), last.to_string());
        app.ascii_key(code(KeyCode::Home));
        let first = app.info.ascii_panel().unwrap().selected_char();
        assert_ne!(first, last);
        app.ascii_key(code(KeyCode::Tab));
        app.ascii_key(code(KeyCode::Esc));
        assert!(app.info.ascii_panel().is_none());
    }

    #[test]
    fn ascii_table_click_inserts_the_character_on_that_row() {
        let mut app = fresh("pp-ascii-mouse");
        app.layout.ascii_panel = Rect::new(0, 0, 30, 8);
        app.open_ascii_panel();
        app.ascii_mouse(click(40, 40)); // outside
        assert_eq!(content(&mut app), "");
        app.ascii_mouse(click(1, 3));
        let ch = app.info.ascii_panel().unwrap().selected_char();
        assert_eq!(content(&mut app), ch.to_string());
        // With the panel closed a click is ignored.
        app.info.close_ascii_panel();
        app.ascii_mouse(click(1, 3));
    }

    #[test]
    fn x11_picker_inserts_hex_navigates_and_closes() {
        let mut app = fresh("pp-x11");
        app.layout.x11_panel = Rect::new(0, 0, 30, 8);
        app.open_x11_panel();
        app.x11_key(code(KeyCode::Down));
        app.x11_key(code(KeyCode::End));
        app.x11_key(code(KeyCode::Home));
        app.x11_key(code(KeyCode::Tab));
        let (hex, name) = {
            let p = app.x11_panel.as_ref().unwrap();
            (
                p.selected_hex().unwrap().to_string(),
                p.selected_name().unwrap().to_string(),
            )
        };
        app.x11_key(code(KeyCode::Enter));
        assert_eq!(content(&mut app), hex);
        assert_eq!(
            app.status,
            t!("status.x11_inserted", name = name).to_string()
        );
        app.flags.insert(AppFlags::THEME_EDITOR_PICKING);
        app.x11_key(code(KeyCode::Esc));
        assert!(app.x11_panel.is_none());
        assert!(!app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
        app.x11_key(code(KeyCode::Enter)); // no panel: nothing
    }

    #[test]
    fn x11_click_acts_on_the_clicked_row_and_a_stray_pick_without_a_theme_editor_closes() {
        let mut app = fresh("pp-x11-mouse");
        app.layout.x11_panel = Rect::new(0, 0, 30, 8);
        app.open_x11_panel();
        app.x11_mouse(click(1, 2));
        let hex = app
            .x11_panel
            .as_ref()
            .unwrap()
            .selected_hex()
            .unwrap()
            .to_string();
        assert_eq!(content(&mut app), hex);
        // Opened for a theme-editor pick that no longer exists: it just closes.
        app.flags.insert(AppFlags::THEME_EDITOR_PICKING);
        let before = content(&mut app);
        app.x11_mouse(click(1, 2));
        assert!(app.x11_panel.is_none());
        assert!(!app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
        assert_eq!(content(&mut app), before);
    }

    #[test]
    fn media_type_picker_filters_inserts_and_closes() {
        let mut app = fresh("pp-media");
        let json = app.root.join("data.json");
        std::fs::write(&json, "{}").unwrap();
        app.open_path(&json, false);
        buffer_with(&mut app, "", 0);
        app.layout.media_type_panel = Rect::new(0, 0, 40, 8);
        app.open_media_type_panel();
        let selected = app
            .info
            .media_type_panel()
            .unwrap()
            .selected_entry()
            .unwrap()
            .media_type
            .to_string();
        assert_eq!(
            selected, "application/json",
            "pre-selected from the extension"
        );
        app.media_type_key(code(KeyCode::Enter));
        assert_eq!(content(&mut app), "application/json");
        assert_eq!(
            app.status,
            t!(
                "status.media_type_inserted",
                media_type = "application/json"
            )
            .to_string()
        );
        for c in "png".chars() {
            app.media_type_key(key(c));
        }
        app.media_type_key(code(KeyCode::Backspace));
        app.media_type_key(key('g'));
        app.media_type_key(code(KeyCode::Down));
        app.media_type_key(code(KeyCode::Tab));
        let filtered = app
            .info
            .media_type_panel()
            .unwrap()
            .selected_entry()
            .unwrap()
            .media_type
            .to_string();
        assert!(filtered.contains("png"), "{filtered}");
        app.media_type_mouse(click(1, 0));
        let clicked = app
            .info
            .media_type_panel()
            .unwrap()
            .selected_entry()
            .unwrap()
            .media_type
            .to_string();
        assert!(content(&mut app).ends_with(&clicked));
        app.media_type_key(code(KeyCode::Esc));
        assert!(app.info.media_type_panel().is_none());
        app.media_type_mouse(click(1, 0)); // closed: ignored
        app.media_type_key(code(KeyCode::Enter));
    }

    #[test]
    fn the_qr_overlay_encodes_the_selection_or_line_and_warns_on_nothing() {
        let mut app = fresh("pp-qr");
        app.open_qrcode();
        assert!(app.qrcode.is_none());
        assert_eq!(last_message(&app), Some(t!("msg.qrcode_empty").to_string()));
        buffer_with(&mut app, "https://example.invalid", 0);
        app.open_qrcode();
        assert!(app.qrcode.as_ref().is_some_and(|a| !a.is_empty()));
        app.qrcode = None;
        buffer_with(&mut app, "ignored line\nfoo bar", 0);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(13, 20);
        app.open_qrcode();
        assert!(app.qrcode.is_some());
    }

    #[test]
    fn todo_tree_lists_comment_tags_navigates_folds_and_jumps() {
        let mut app = fresh("pp-todo");
        std::fs::write(
            app.root.join("work.rs"),
            "fn a() {}\n// TODO: first thing\nfn b() {}\n// FIXME: second thing\n",
        )
        .unwrap();
        app.layout.todo_tree = Rect::new(0, 0, 40, 10);
        app.open_todo_tree();
        assert!(app.todo_tree.as_ref().expect("todos found").len() >= 2);
        let n = 2;
        assert_eq!(app.status, t!("status.todos_n", n = n).to_string());
        for k in [
            KeyCode::Down,
            KeyCode::PageDown,
            KeyCode::Up,
            KeyCode::PageUp,
        ] {
            app.todo_tree_key(code(k));
        }
        let rows = app.todo_tree.as_ref().unwrap().len();
        app.todo_tree_key(key('['));
        assert!(app.todo_tree.as_ref().unwrap().len() <= rows);
        app.todo_tree_key(key(']'));
        assert_eq!(app.todo_tree.as_ref().unwrap().len(), rows);
        app.todo_tree_key(key('g'));
        app.todo_tree_key(key('g'));
        app.todo_tree_key(key(' '));
        app.todo_tree_key(key('x')); // ignored
        assert!(app.todo_tree.is_some());
        app.todo_tree_key(code(KeyCode::Esc));
        assert!(app.todo_tree.is_none());
        app.todo_tree_key(code(KeyCode::Enter)); // closed: nothing
        app.todo_tree_mouse(click(1, 1));

        // Enter on an item opens its file at the line.
        app.open_todo_tree();
        app.todo_tree_key(key(']'));
        for _ in 0..rows {
            app.todo_tree_key(code(KeyCode::Down));
            app.todo_tree_key(code(KeyCode::Enter));
            if app.todo_tree.is_none() {
                break;
            }
        }
        assert!(app.todo_tree.is_none(), "an item was activated");
        assert!(
            app.editor
                .active_tab_mut()
                .unwrap()
                .path
                .as_ref()
                .is_some_and(|p| p.ends_with("work.rs"))
        );
        assert_eq!(app.focus, Focus::Editor);
    }

    #[test]
    fn todo_tree_click_activates_an_item_and_an_empty_project_says_so() {
        let mut app = fresh("pp-todo-click");
        app.layout.todo_tree = Rect::new(0, 0, 40, 10);
        app.open_todo_tree();
        assert!(app.todo_tree.is_none());
        assert_eq!(app.status, t!("status.no_todos").to_string());
        std::fs::write(app.root.join("w.rs"), "// TODO: x\n").unwrap();
        app.open_todo_tree();
        app.todo_tree_mouse(click(1, 0)); // the group header folds
        assert!(app.todo_tree.is_some());
        app.todo_tree_mouse(click(1, 0)); // and unfolds again
        app.todo_tree_mouse(click(1, 1)); // the item under it jumps
        assert!(app.todo_tree.is_none(), "clicking the item jumped to it");
    }

    #[test]
    fn theme_editor_opens_navigates_picks_a_color_and_closes() {
        let mut app = fresh("pp-theme");
        app.layout.theme_editor = Rect::new(0, 0, 40, 10);
        app.open_theme_editor();
        assert!(app.theme_editor.is_some());
        assert_eq!(
            app.theme_editor_baseline.as_deref(),
            Some(app.settings.theme.as_str())
        );
        for k in [
            KeyCode::Down,
            KeyCode::PageDown,
            KeyCode::Up,
            KeyCode::PageUp,
            KeyCode::Tab,
        ] {
            app.theme_editor_key(code(k));
        }
        app.theme_editor_key(code(KeyCode::Enter));
        assert!(app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
        assert!(app.x11_panel.is_some());
        app.x11_key(code(KeyCode::Esc));
        assert!(app.x11_panel.is_none());
        assert!(!app.flags.contains(AppFlags::THEME_EDITOR_PICKING));

        app.theme_editor_key(ctrl('s'));
        assert!(matches!(
            app.prompt.as_ref().unwrap().kind,
            PromptKind::ThemeSaveAs
        ));
        app.prompt = None;
        app.theme_editor_mouse(click(40, 40)); // outside
        assert!(app.x11_panel.is_none());
        app.theme_editor_mouse(click(1, 1));
        assert!(app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
        assert!(app.x11_panel.is_some());
        app.theme_editor_key(code(KeyCode::Esc));
        assert!(app.theme_editor.is_none());
        assert!(app.x11_panel.is_none());
        assert!(app.theme_editor_baseline.is_none());
        assert!(!app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
        // With no editor open these do nothing.
        app.theme_editor_key(code(KeyCode::Enter));
        assert!(app.x11_panel.is_none());
        app.theme_editor_mouse(click(1, 1));
    }

    #[test]
    fn saving_a_theme_needs_a_name_and_an_open_editor() {
        let mut app = fresh("pp-theme-save");
        app.save_theme_as(""); // blank name: cancelled
        app.save_theme_as("whatever"); // no editor: nothing written
        assert!(app.theme_editor.is_none());
        app.open_theme_editor();
        app.save_theme_as("");
        assert!(app.theme_editor.is_some(), "a blank name keeps the draft");
    }
}

#[cfg(test)]
#[path = "tests_r3_picker_panels.rs"]
mod r3_tests;
