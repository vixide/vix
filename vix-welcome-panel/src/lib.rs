//! The first-run welcome screen's scroll state.
//!
//! Vix shows this overlay the first time it runs (and on demand from **Help →
//! Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
//! locale key) so it is translatable; this crate is pure state — it holds the
//! lines the host hands it and tracks the scroll offset. The host renders the
//! visible window with a scrollbar and forwards scroll keys.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Scroll state for the welcome overlay, over the lines the host supplies.
pub struct Panel {
    /// The welcome text, one entry per line.
    lines: Vec<String>,
    /// Index of the first visible line.
    pub scroll: usize,
}

impl Panel {
    /// Open the panel at the top over `lines` (typically the `welcome.body`
    /// locale string split into lines).
    #[must_use]
    pub fn open(lines: Vec<String>) -> Self {
        Panel { lines, scroll: 0 }
    }

    /// The welcome text lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Total number of lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether there is no text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The greatest valid scroll offset for a `viewport` of visible rows.
    fn max_scroll(&self, viewport: usize) -> usize {
        self.lines.len().saturating_sub(viewport.max(1))
    }

    /// Scroll up one line.
    pub fn up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down one line, stopping when the last line is in view.
    pub fn down(&mut self, viewport: usize) {
        self.scroll = (self.scroll + 1).min(self.max_scroll(viewport));
    }

    /// Scroll up one page (`viewport` rows).
    pub fn page_up(&mut self, viewport: usize) {
        self.scroll = self.scroll.saturating_sub(viewport.max(1));
    }

    /// Scroll down one page (`viewport` rows), clamped to the end.
    pub fn page_down(&mut self, viewport: usize) {
        self.scroll = (self.scroll + viewport.max(1)).min(self.max_scroll(viewport));
    }

    /// Clamp the scroll offset to the current `viewport` (e.g. after a resize).
    pub fn clamp(&mut self, viewport: usize) {
        self.scroll = self.scroll.min(self.max_scroll(viewport));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<String> {
        (0..30).map(|i| format!("line {i}")).collect()
    }

    #[test]
    fn holds_lines_and_reports_length() {
        let p = Panel::open(sample());
        assert_eq!(p.len(), 30);
        assert_eq!(p.lines()[0], "line 0");
        assert!(!p.is_empty());
    }

    #[test]
    fn scrolling_clamps_to_the_window() {
        let mut p = Panel::open(sample());
        assert_eq!(p.scroll, 0);
        p.up();
        assert_eq!(p.scroll, 0, "up at the top stays put");
        let viewport = 5;
        for _ in 0..100 {
            p.down(viewport);
        }
        assert_eq!(p.scroll, 30 - viewport, "down clamps to the last full window");
        p.page_up(1000);
        assert_eq!(p.scroll, 0);
    }
}
