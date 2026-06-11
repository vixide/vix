//! State for the bottom dock: a scrollable line buffer for log messages,
//! terminal/command output, data views, and similar.
//!
//! Pure data — the host (the `vix` app) renders the dock and routes keys/clicks;
//! this crate owns the line buffer and its scroll offset.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

/// Maximum lines retained; the oldest are dropped past this.
const CAP: usize = 5_000;

/// The bottom dock's line buffer and scroll offset.
#[derive(Default)]
pub struct BottomDock {
    /// Lines, oldest first.
    pub lines: Vec<String>,
    /// Index of the first visible line (scroll offset).
    pub scroll: usize,
}

impl BottomDock {
    /// An empty dock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a line, capping the buffer and keeping the view pinned to the
    /// bottom (newest) content.
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.lines.len() > CAP {
            let drop = self.lines.len() - CAP;
            self.lines.drain(0..drop);
        }
        self.scroll = self.lines.len();
    }

    /// Remove all lines.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll = 0;
    }

    /// Whether the buffer has no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Scroll up by `n` lines.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Scroll down by `n` lines (clamped to the last line).
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = (self.scroll + n).min(self.lines.len());
    }

    /// The lines visible in a `height`-row viewport, given the current scroll,
    /// keeping the view within range.
    #[must_use]
    pub fn visible(&self, height: usize) -> &[String] {
        let height = height.max(1);
        let max_start = self.lines.len().saturating_sub(height);
        let start = self.scroll.min(max_start).min(self.lines.len());
        let end = (start + height).min(self.lines.len());
        &self.lines[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_appends_and_pins_to_bottom() {
        let mut d = BottomDock::new();
        d.push("a");
        d.push("b");
        assert_eq!(d.lines, ["a", "b"]);
        assert_eq!(d.scroll, 2);
    }

    #[test]
    fn visible_window_respects_scroll_and_height() {
        let mut d = BottomDock::new();
        for i in 0..10 {
            d.push(format!("line {i}"));
        }
        // Pinned to bottom: last 3 lines.
        assert_eq!(d.visible(3), ["line 7", "line 8", "line 9"]);
        d.scroll_up(100);
        assert_eq!(d.visible(3), ["line 0", "line 1", "line 2"]);
    }

    #[test]
    fn clear_empties_the_buffer() {
        let mut d = BottomDock::new();
        d.push("x");
        d.clear();
        assert!(d.is_empty());
        assert_eq!(d.scroll, 0);
    }
}
