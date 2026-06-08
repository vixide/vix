//! Colors and Nerd Font icons used across the UI.

use ratatui::style::{Color, Modifier, Style};

// Nerd Font glyphs (monospace). Render best with a patched "Nerd Font".
// They degrade to a placeholder box in fonts without the glyphs.
pub mod icon {
    pub const FOLDER: &str = "\u{f07b}"; //
    pub const FOLDER_OPEN: &str = "\u{f07c}"; //
    pub const FILE: &str = "\u{f15b}"; //
    pub const FILE_DIRTY: &str = "\u{f111}"; //
    pub const RUST: &str = "\u{e7a8}"; //
    pub const MARKDOWN: &str = "\u{f48a}"; //
    pub const CALENDAR: &str = "\u{f073}"; //
    pub const CLOCK: &str = "\u{f017}"; //
    pub const SEARCH: &str = "\u{f002}"; //
    pub const BELL: &str = "\u{f0f3}"; //
    pub const CLOSE: &str = "\u{f00d}"; //
    pub const INFO: &str = "\u{f05a}"; //
    pub const PALETTE: &str = "\u{f120}"; //
}

/// Pick a file icon from a path's extension.
pub fn file_icon(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "rs" => icon::RUST,
        "md" | "markdown" => icon::MARKDOWN,
        _ => icon::FILE,
    }
}

pub const ACCENT: Color = Color::Cyan;
pub const ACCENT_DIM: Color = Color::DarkGray;
pub const WARN: Color = Color::Yellow;
pub const ERR: Color = Color::Red;

pub fn title(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT_DIM)
    }
}

pub fn selected() -> Style {
    Style::default()
        .bg(ACCENT)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(ACCENT_DIM)
}
