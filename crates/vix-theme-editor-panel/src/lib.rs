//! The theme editor overlay (T202): list a theme's color slots and edit
//! each one, live-previewed on the real UI as you go.
//!
//! This crate holds the being-edited [`CustomTheme`] draft plus the panel's
//! row selection; the host applies the draft live (`vix_theme_model::apply`)
//! after each edit, reverts to the committed theme if the editor is
//! cancelled, and drives the actual color-picking UI (the existing
//! `vix-x11-color-picker`, reused rather than duplicated — see
//! `spec/index.md`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

use vix_theme_model::{CustomTheme, Rgb};

/// One editable color slot. Named directly rather than modeled generically
/// over `CustomTheme`'s field structure — 15 fields, each a one-line
/// get/set, is simpler than a lens/path abstraction would be here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// Menu bar foreground.
    MenuBarFg,
    /// Menu bar background.
    MenuBarBg,
    /// Status bar foreground.
    StatusBarFg,
    /// Status bar background.
    StatusBarBg,
    /// Left dock (file explorer) foreground.
    LeftDockFg,
    /// Left dock (file explorer) background.
    LeftDockBg,
    /// Right dock (message drawer) foreground.
    RightDockFg,
    /// Right dock (message drawer) background.
    RightDockBg,
    /// Editor text foreground.
    EditorFg,
    /// Editor background.
    EditorBg,
    /// Editor cursor color.
    EditorCursor,
    /// Syntax: keywords.
    SyntaxKeyword,
    /// Syntax: string literals.
    SyntaxString,
    /// Syntax: comments.
    SyntaxComment,
    /// Syntax: numeric literals.
    SyntaxNumber,
}

impl Slot {
    /// Every slot, in the order the panel lists them.
    pub const ALL: [Slot; 15] = [
        Slot::MenuBarFg,
        Slot::MenuBarBg,
        Slot::StatusBarFg,
        Slot::StatusBarBg,
        Slot::LeftDockFg,
        Slot::LeftDockBg,
        Slot::RightDockFg,
        Slot::RightDockBg,
        Slot::EditorFg,
        Slot::EditorBg,
        Slot::EditorCursor,
        Slot::SyntaxKeyword,
        Slot::SyntaxString,
        Slot::SyntaxComment,
        Slot::SyntaxNumber,
    ];

    /// The i18n key naming this slot, for the row label; the host translates.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Slot::MenuBarFg => "ui.theme_slot_menu_bar_fg",
            Slot::MenuBarBg => "ui.theme_slot_menu_bar_bg",
            Slot::StatusBarFg => "ui.theme_slot_status_bar_fg",
            Slot::StatusBarBg => "ui.theme_slot_status_bar_bg",
            Slot::LeftDockFg => "ui.theme_slot_left_dock_fg",
            Slot::LeftDockBg => "ui.theme_slot_left_dock_bg",
            Slot::RightDockFg => "ui.theme_slot_right_dock_fg",
            Slot::RightDockBg => "ui.theme_slot_right_dock_bg",
            Slot::EditorFg => "ui.theme_slot_editor_fg",
            Slot::EditorBg => "ui.theme_slot_editor_bg",
            Slot::EditorCursor => "ui.theme_slot_editor_cursor",
            Slot::SyntaxKeyword => "ui.theme_slot_syntax_keyword",
            Slot::SyntaxString => "ui.theme_slot_syntax_string",
            Slot::SyntaxComment => "ui.theme_slot_syntax_comment",
            Slot::SyntaxNumber => "ui.theme_slot_syntax_number",
        }
    }

    /// This slot's current color in `theme` (`None` if unset — it then falls
    /// back to the primary editor color, same as everywhere else the theme
    /// model reads a region color).
    #[must_use]
    pub fn get(self, theme: &CustomTheme) -> Option<Rgb> {
        match self {
            Slot::MenuBarFg => theme.menu_bar.foreground,
            Slot::MenuBarBg => theme.menu_bar.background,
            Slot::StatusBarFg => theme.status_bar.foreground,
            Slot::StatusBarBg => theme.status_bar.background,
            Slot::LeftDockFg => theme.left_dock.foreground,
            Slot::LeftDockBg => theme.left_dock.background,
            Slot::RightDockFg => theme.right_dock.foreground,
            Slot::RightDockBg => theme.right_dock.background,
            Slot::EditorFg => theme.editor.foreground,
            Slot::EditorBg => theme.editor.background,
            Slot::EditorCursor => theme.editor.cursor,
            Slot::SyntaxKeyword => theme.syntax.keyword,
            Slot::SyntaxString => theme.syntax.string,
            Slot::SyntaxComment => theme.syntax.comment,
            Slot::SyntaxNumber => theme.syntax.number,
        }
    }

    /// Set this slot's color in `theme`.
    pub fn set(self, theme: &mut CustomTheme, rgb: Rgb) {
        let slot = match self {
            Slot::MenuBarFg => &mut theme.menu_bar.foreground,
            Slot::MenuBarBg => &mut theme.menu_bar.background,
            Slot::StatusBarFg => &mut theme.status_bar.foreground,
            Slot::StatusBarBg => &mut theme.status_bar.background,
            Slot::LeftDockFg => &mut theme.left_dock.foreground,
            Slot::LeftDockBg => &mut theme.left_dock.background,
            Slot::RightDockFg => &mut theme.right_dock.foreground,
            Slot::RightDockBg => &mut theme.right_dock.background,
            Slot::EditorFg => &mut theme.editor.foreground,
            Slot::EditorBg => &mut theme.editor.background,
            Slot::EditorCursor => &mut theme.editor.cursor,
            Slot::SyntaxKeyword => &mut theme.syntax.keyword,
            Slot::SyntaxString => &mut theme.syntax.string,
            Slot::SyntaxComment => &mut theme.syntax.comment,
            Slot::SyntaxNumber => &mut theme.syntax.number,
        };
        *slot = Some(rgb);
    }
}

/// The theme editor overlay's state: the being-edited draft, and which slot
/// row is highlighted.
pub struct Panel {
    /// The theme draft being edited. Applied live by the host after every
    /// change; only persisted to disk on Save As.
    pub theme: CustomTheme,
    /// Index into [`Slot::ALL`] of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
    /// Whether any slot has been changed since [`Panel::open`].
    pub dirty: bool,
}

impl Panel {
    /// Open the editor over a copy of `theme` (the currently active theme,
    /// typically) — edits never touch the original until Save As.
    #[must_use]
    pub fn open(theme: CustomTheme) -> Self {
        Panel {
            theme,
            selected: 0,
            scroll: 0,
            dirty: false,
        }
    }

    /// The number of editable slots.
    #[must_use]
    pub fn len(&self) -> usize {
        Slot::ALL.len()
    }

    /// Whether the slot list is empty (it never is; clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The highlighted slot.
    #[must_use]
    pub fn selected_slot(&self) -> Slot {
        Slot::ALL[self.selected]
    }

    /// Apply `rgb` to the highlighted slot.
    pub fn apply_color(&mut self, rgb: Rgb) {
        self.selected_slot().set(&mut self.theme, rgb);
        self.dirty = true;
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a row directly (e.g. from a click); returns whether `idx`
    /// landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlighted row stays within a
    /// window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CustomTheme {
        vix_theme_model::parse_theme(
            r#"{
                "name": "Sample",
                "menu-bar": {"foreground": [1,1,1], "background": [2,2,2]},
                "editor": {"foreground": [3,3,3], "background": [4,4,4], "cursor": [5,5,5]},
                "syntax": {"keyword": [6,6,6]}
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn every_slot_round_trips_through_get_and_set() {
        let mut theme = sample();
        for slot in Slot::ALL {
            slot.set(&mut theme, [9, 9, 9]);
            assert_eq!(slot.get(&theme), Some([9, 9, 9]), "{slot:?}");
        }
    }

    #[test]
    fn open_reads_the_given_theme_unmodified() {
        let p = Panel::open(sample());
        assert_eq!(Slot::MenuBarFg.get(&p.theme), Some([1, 1, 1]));
        assert!(!p.dirty);
    }

    #[test]
    fn apply_color_writes_the_highlighted_slot_and_marks_dirty() {
        let mut p = Panel::open(sample());
        p.selected = 0; // MenuBarFg
        p.apply_color([200, 200, 200]);
        assert_eq!(Slot::MenuBarFg.get(&p.theme), Some([200, 200, 200]));
        assert!(p.dirty);
        // Only the selected slot changed.
        assert_eq!(Slot::MenuBarBg.get(&p.theme), Some([2, 2, 2]));
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut p = Panel::open(sample());
        assert_eq!(p.selected, 0);
        p.up();
        assert_eq!(p.selected, 0, "stops at the top");
        for _ in 0..p.len() {
            p.down();
        }
        assert_eq!(p.selected, p.len() - 1, "stops at the bottom");
    }

    #[test]
    fn page_navigation_moves_and_clamps() {
        let mut p = Panel::open(sample());
        p.page_down(3);
        assert_eq!(p.selected, 3);
        p.page_up(2);
        assert_eq!(p.selected, 1);
        p.page_up(100);
        assert_eq!(p.selected, 0, "stops at the top");
        p.page_down(100);
        assert_eq!(p.selected, p.len() - 1, "stops at the bottom");
    }

    #[test]
    fn select_index_hits_real_rows_only() {
        let mut p = Panel::open(sample());
        assert!(p.select_index(3));
        assert_eq!(p.selected, 3);
        assert!(!p.select_index(p.len()));
    }
}
