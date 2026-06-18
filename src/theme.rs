//! Nerd Font icons, plus a re-export of the [`crate::theme_model`] theme model.
//!
//! The JSON theme model and its ratatui styles live in the `vix-theme-model`
//! crate; this module re-exports them as `crate::theme::*` and keeps the icon set
//! (which is not part of the theme model).

#![warn(clippy::pedantic)]

pub use crate::theme_model::{
    base, bg, custom_name, dim, editor_cursor, fg, region_base, region_bg, region_fg,
    region_modifiers, region_title, selected, set_custom, syntax_theme, title, CustomTheme, Region,
};

/// Nerd Font glyphs (monospace). They render best with a patched "Nerd Font"
/// and degrade to a placeholder box in fonts without the glyphs.
pub mod icon {
    /// Closed folder.
    pub const FOLDER: &str = "\u{f07b}";
    /// Open folder.
    pub const FOLDER_OPEN: &str = "\u{f07c}";
    /// Generic file.
    pub const FILE: &str = "\u{f15b}";
    /// Unsaved-buffer dot.
    pub const FILE_DIRTY: &str = "\u{f111}";
    /// Rust source file.
    pub const RUST: &str = "\u{e7a8}";
    /// Markdown file.
    pub const MARKDOWN: &str = "\u{f48a}";
    /// Calendar.
    pub const CALENDAR: &str = "\u{f073}";
    /// Clock.
    pub const CLOCK: &str = "\u{f017}";
    /// Magnifying glass (search).
    pub const SEARCH: &str = "\u{f002}";
    /// Bell (messages).
    pub const BELL: &str = "\u{f0f3}";
    /// Close / error mark.
    pub const CLOSE: &str = "\u{f00d}";
    /// Information mark.
    pub const INFO: &str = "\u{f05a}";
    /// Palette / app logo glyph.
    pub const PALETTE: &str = "\u{f120}";
    /// Table grid (ASCII panel).
    pub const TABLE: &str = "\u{f0ce}";
    /// Git branch (Powerline).
    pub const BRANCH: &str = "\u{e0a0}";
    /// Code / outline glyph.
    pub const CODE: &str = "\u{f121}";
    /// Symbolic link (Octicons `nf-oct-link`).
    pub const LINK: &str = "\u{f44c}";
}

/// Pick a file icon from a path's extension.
#[must_use]
pub fn file_icon(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "rs" => icon::RUST,
        "md" | "markdown" => icon::MARKDOWN,
        _ => icon::FILE,
    }
}
