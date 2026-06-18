//! The first-run welcome screen's scroll state.
//!
//! Vix shows this overlay the first time it runs (and on demand from **Help →
//! Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
//! locale key) so it is translatable; this crate is pure state — it holds the
//! lines the host hands it and tracks the scroll offset. The host renders the
//! visible window with a scrollbar and forwards scroll keys.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Scroll state for the welcome overlay.
///
/// The text is stored as source *paragraphs* (one entry per source line — each a
/// whole paragraph, a heading, a blank line, or a bullet). [`Panel::wrap_to`]
/// word-wraps them to the render width into display lines; scrolling is over
/// those display lines, so paragraphs **soft-wrap** and never leave orphan lines.
pub struct Panel {
    /// The source paragraphs (one per source line).
    paragraphs: Vec<String>,
    /// The paragraphs word-wrapped to [`Self::wrap_width`].
    wrapped: Vec<String>,
    /// Width the `wrapped` lines were computed for (so re-wrapping is skipped when
    /// the width is unchanged).
    wrap_width: usize,
    /// Index of the first visible display line.
    pub scroll: usize,
}

/// Greedily word-wrap one source `line` to `width` columns. A blank line yields a
/// single empty string; a word longer than `width` is left to overflow. `width`
/// `0` returns the line unchanged.
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for word in line.split_whitespace() {
        let wlen = word.chars().count();
        if cur_len == 0 {
            cur.push_str(word);
            cur_len = wlen;
        } else if cur_len + 1 + wlen <= width {
            cur.push(' ');
            cur.push_str(word);
            cur_len += 1 + wlen;
        } else {
            out.push(std::mem::take(&mut cur));
            cur.push_str(word);
            cur_len = wlen;
        }
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

impl Panel {
    /// Open the panel at the top over `paragraphs` (the `welcome.body` locale
    /// string split into lines — one paragraph/heading/bullet per line).
    #[must_use]
    pub fn open(paragraphs: Vec<String>) -> Self {
        let wrapped = paragraphs.clone();
        Panel { paragraphs, wrapped, wrap_width: usize::MAX, scroll: 0 }
    }

    /// Re-wrap the paragraphs to `width` columns (no-op if unchanged), clamping
    /// the scroll to the new line count. Call before reading [`Self::lines`].
    pub fn wrap_to(&mut self, width: usize) {
        if width == self.wrap_width {
            return;
        }
        self.wrapped = self.paragraphs.iter().flat_map(|l| wrap_line(l, width)).collect();
        self.wrap_width = width;
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

    /// The greatest valid scroll offset for a `viewport` of visible rows.
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
        assert!(lines.iter().any(std::string::String::is_empty), "blank line preserved");
        assert!(lines.iter().all(|l| l.chars().count() <= 9), "no line exceeds width: {lines:?}");
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
        assert_eq!(p.scroll, 30 - viewport, "down clamps to the last full window");
        p.page_up(1000);
        assert_eq!(p.scroll, 0);
    }
}
