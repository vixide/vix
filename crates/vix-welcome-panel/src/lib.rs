//! The first-run welcome screen's scroll state.
//!
//! Vix shows this overlay the first time it runs (and on demand from **Help →
//! Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
//! locale key) so it is translatable; this crate is pure state — it holds the
//! lines the host hands it and tracks the scroll offset. The host renders the
//! visible window with a scrollbar and forwards scroll keys.
//!
//! # Usage order
//!
//! 1. [`Panel::open`] with the localized paragraphs.
//! 2. Each frame, [`Panel::wrap_to`] with the current inner width, then read
//!    [`Panel::lines`] and draw the window starting at [`Panel::scroll`].
//! 3. Forward scroll keys to [`Panel::up`] / [`Panel::down`] / the page
//!    variants, passing the visible row count as `viewport`; call
//!    [`Panel::clamp`] after a resize that shrinks or grows the viewport.
//!
//! Wrapping is delegated to `vix-greedy-wrap`; scrolling helpers here do not
//! use `vix-list-state` because there is no row selection, only a window.
//!
//! Spec: `crates/vix-welcome-panel/spec/index.md`.

/// Scroll state for the welcome overlay.
///
/// The text is stored as source *paragraphs* (one entry per source line — each a
/// whole paragraph, a heading, a blank line, or a bullet). [`Panel::wrap_to`]
/// word-wraps them to the render width into display lines; scrolling is over
/// those display lines, so paragraphs **soft-wrap** and never leave orphan lines.
///
/// # Examples
///
/// ```
/// use vix_welcome_panel::Panel;
///
/// let mut p = Panel::open(vec!["alpha beta gamma".to_string(), String::new()]);
/// p.wrap_to(10);
/// assert_eq!(p.lines(), ["alpha beta", "gamma", ""]);
/// p.down(2);
/// assert_eq!(p.scroll, 1);
/// ```
pub struct Panel {
    /// The source paragraphs (one per source line).
    paragraphs: Vec<String>,
    /// The paragraphs word-wrapped to [`Self::wrap_width`].
    wrapped: Vec<String>,
    /// Width the `wrapped` lines were computed for (so re-wrapping is skipped when
    /// the width is unchanged). `usize::MAX` means "not yet wrapped": `open`
    /// seeds `wrapped` with the raw paragraphs and no real width can equal it,
    /// so the first `wrap_to` always does the work.
    wrap_width: usize,
    /// Index of the first visible display line. Public so the host can read it
    /// when drawing; mutate it via the scroll methods so the clamping holds.
    pub scroll: usize,
}

/// Greedily word-wrap one source `line` to `width` columns. A blank line
/// yields a single empty string; a word longer than `width` is hard-broken
/// character-by-character; `width` `0` returns the line unchanged. Delegates
/// to `vix-greedy-wrap` (Run H, T526) -- this crate and `vix-ai-panel` used
/// to each hand-roll an independent copy, disagreeing on exactly the
/// over-long-word case; breaking is the product decision that won.
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    vix_greedy_wrap::wrap_line(line, width)
}

impl Panel {
    /// Open the panel at the top over `paragraphs` (the `welcome.body` locale
    /// string split into lines — one paragraph/heading/bullet per line).
    #[must_use]
    pub fn open(paragraphs: Vec<String>) -> Self {
        // Unwrapped until the host supplies a width via `wrap_to`.
        let wrapped = paragraphs.clone();
        Panel {
            paragraphs,
            wrapped,
            wrap_width: usize::MAX,
            scroll: 0,
        }
    }

    /// Re-wrap the paragraphs to `width` columns (no-op if unchanged), clamping
    /// the scroll to the new line count. Call before reading [`Self::lines`].
    pub fn wrap_to(&mut self, width: usize) {
        if width == self.wrap_width {
            return;
        }
        self.wrapped = self
            .paragraphs
            .iter()
            .flat_map(|l| wrap_line(l, width))
            .collect();
        self.wrap_width = width;
        // Fewer display lines after a widen: keep the offset on a real line.
        // (Unlike `clamp` this does not account for the viewport height.)
        self.scroll = self.scroll.min(self.wrapped.len().saturating_sub(1));
    }

    /// The wrapped display lines (call [`Self::wrap_to`] first).
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.wrapped
    }

    /// Total number of display lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.wrapped.len()
    }

    /// Whether there is no text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.wrapped.is_empty()
    }

    /// The greatest valid scroll offset for a `viewport` of visible rows: the
    /// offset at which the last line sits on the bottom row. `viewport` is
    /// floored at 1 so a zero-height pane cannot yield `len` (one past the end).
    fn max_scroll(&self, viewport: usize) -> usize {
        self.wrapped.len().saturating_sub(viewport.max(1))
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
    fn wraps_paragraphs_and_preserves_blank_lines() {
        let mut p = Panel::open(vec![
            "one two three four five".to_string(),
            String::new(),
            "short".to_string(),
        ]);
        p.wrap_to(9); // "one two" (7), "three" wraps, etc.
        let lines = p.lines();
        // The long paragraph wrapped into several lines, the blank stayed blank,
        // and no display line exceeds the width.
        assert!(lines.len() > 3, "paragraph wrapped: {lines:?}");
        assert!(
            lines.iter().any(std::string::String::is_empty),
            "blank line preserved"
        );
        assert!(
            lines.iter().all(|l| l.chars().count() <= 9),
            "no line exceeds width: {lines:?}"
        );
        assert_eq!(lines.last().unwrap(), "short");
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
        assert_eq!(
            p.scroll,
            30 - viewport,
            "down clamps to the last full window"
        );
        p.page_up(1000);
        assert_eq!(p.scroll, 0);
    }
}
