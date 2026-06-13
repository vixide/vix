//! The first-run welcome screen: friendly, novice-oriented text plus the panel's
//! scroll state.
//!
//! Vix shows this overlay the first time it runs (and on demand from **Help →
//! Welcome…**). It explains what Vix is, how to get going, what it can do, and
//! how to send feedback. This crate is pure data — it owns the text ([`LINES`])
//! and the scroll offset ([`Panel`]); the host renders the visible window with a
//! scrollbar and forwards scroll keys.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// The welcome text, one entry per line (blank strings are blank lines). Kept as
/// plain text so the host can render it in any theme.
pub const LINES: &[&str] = &[
    "Welcome to Vix!",
    "",
    "Vix is a simple, keyboard-friendly text editor and IDE that runs right in",
    "your terminal. It is fast, mouse-aware, and works the same on macOS, Linux,",
    "and Windows. This screen is here to help you get started — press Esc to",
    "close it at any time, and reopen it later from Help → Welcome.",
    "",
    "Getting started",
    "───────────────",
    "• Open the menus with F10, or click a menu name. Arrow keys move, Enter runs,",
    "  Esc backs out. Each menu letter also has an Alt shortcut (Alt+F for File).",
    "• Open a file with Ctrl+O, save with Ctrl+S, and close a tab with Ctrl+W.",
    "• Toggle the file explorer (left dock) with Ctrl+B, and switch focus between",
    "  the explorer and the editor with Ctrl+E.",
    "• Press Ctrl+P for the Command Palette — a searchable list of every command.",
    "  If you ever forget a shortcut, start there.",
    "• Press F1 for the full keyboard-shortcut reference.",
    "",
    "What Vix can do",
    "───────────────",
    "• Edit code with syntax highlighting, soft wrap, undo/redo, and multiple tabs.",
    "• Find and replace in the current file or across the whole workspace.",
    "• Browse and edit files in the explorer, with cut/copy/paste and multi-select.",
    "• Use Git: view status and log, stage and commit, switch and create branches,",
    "  pull/push/fetch, and clone — all from the Git menu.",
    "• Pick characters and colors from the Tools menu: Nerd Font glyphs, ASCII,",
    "  HTML entities, and X11 colors.",
    "• Turn on Language Server Protocol features (diagnostics, hover, go-to-",
    "  definition, completion) by configuring a server for your language.",
    "• Choose a color theme, a language, and a keymap (Apple, VS Code, Emacs, or",
    "  Vim) from the View menu — Vix adapts to how you like to work.",
    "",
    "Make it yours",
    "─────────────",
    "Vix remembers your choices in a small settings file, so the editor looks and",
    "behaves the way you set it up the next time you open it.",
    "",
    "Feedback is welcome",
    "───────────────────",
    "Vix is open source and we would love to hear from you — bug reports, ideas,",
    "and questions are all appreciated.",
    "",
    "• Website: https://github.com/joelparkerhenderson/vix",
    "• Email:   joel@joelparkerhenderson.com",
    "",
    "Happy editing!",
];

/// Scroll state for the welcome overlay: the first visible line.
#[derive(Default)]
pub struct Panel {
    /// Index of the first visible line.
    pub scroll: usize,
}

impl Panel {
    /// Open the panel scrolled to the top.
    #[must_use]
    pub fn open() -> Self {
        Panel { scroll: 0 }
    }

    /// Total number of lines.
    #[must_use]
    pub fn len(&self) -> usize {
        LINES.len()
    }

    /// Whether there is no text (there always is; clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        LINES.is_empty()
    }

    /// The greatest valid scroll offset for a `viewport` of visible rows.
    fn max_scroll(viewport: usize) -> usize {
        LINES.len().saturating_sub(viewport.max(1))
    }

    /// Scroll up one line.
    pub fn up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down one line, stopping when the last line is in view.
    pub fn down(&mut self, viewport: usize) {
        self.scroll = (self.scroll + 1).min(Self::max_scroll(viewport));
    }

    /// Scroll up one page (`viewport` rows).
    pub fn page_up(&mut self, viewport: usize) {
        self.scroll = self.scroll.saturating_sub(viewport.max(1));
    }

    /// Scroll down one page (`viewport` rows), clamped to the end.
    pub fn page_down(&mut self, viewport: usize) {
        self.scroll = (self.scroll + viewport.max(1)).min(Self::max_scroll(viewport));
    }

    /// Clamp the scroll offset to the current `viewport` (e.g. after a resize).
    pub fn clamp(&mut self, viewport: usize) {
        self.scroll = self.scroll.min(Self::max_scroll(viewport));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_is_present_and_mentions_key_topics() {
        let text = LINES.join("\n");
        assert!(text.contains("Welcome to Vix"));
        assert!(text.contains("Command Palette"));
        assert!(text.contains("joelparkerhenderson.com"), "feedback contact present");
        assert!(LINES.len() > 20, "substantial welcome content");
    }

    #[test]
    fn scrolling_clamps_to_the_window() {
        let mut p = Panel::open();
        assert_eq!(p.scroll, 0);
        p.up();
        assert_eq!(p.scroll, 0, "up at the top stays put");
        let viewport = 5;
        p.page_down(viewport);
        assert!(p.scroll <= LINES.len().saturating_sub(viewport));
        // Paging down repeatedly never scrolls past the last full window.
        for _ in 0..100 {
            p.down(viewport);
        }
        assert_eq!(p.scroll, LINES.len() - viewport);
        p.page_up(1000);
        assert_eq!(p.scroll, 0);
    }
}
