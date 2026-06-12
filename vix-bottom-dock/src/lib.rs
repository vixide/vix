//! State for the bottom dock: a scrollable line buffer for log messages,
//! terminal/command output, data views, and similar.
//!
//! Pure data — the host (the `vix` app) renders the dock and routes keys/clicks;
//! this crate owns the line buffer and its scroll offset.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

/// Default scrollback: maximum lines retained before the oldest are dropped.
pub const DEFAULT_SCROLLBACK: usize = 1_000;

/// The bottom dock's line buffer and scroll offset.
pub struct BottomDock {
    /// Lines, oldest first.
    pub lines: Vec<String>,
    /// Index of the first visible line (scroll offset).
    pub scroll: usize,
    /// Maximum lines retained (scrollback); the oldest are dropped past this.
    cap: usize,
    /// Whether the view sticks to the newest line. While `true`, [`BottomDock::push`]
    /// keeps the bottom in view; scrolling up clears it, scrolling back to the
    /// bottom restores it.
    follow: bool,
}

impl Default for BottomDock {
    fn default() -> Self {
        BottomDock { lines: Vec::new(), scroll: 0, cap: DEFAULT_SCROLLBACK, follow: true }
    }
}

impl BottomDock {
    /// An empty dock (following the newest line) with the default scrollback.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty dock with a specific scrollback (minimum 1 line).
    #[must_use]
    pub fn with_scrollback(cap: usize) -> Self {
        BottomDock { cap: cap.max(1), ..Self::default() }
    }

    /// Change the scrollback (minimum 1 line), trimming the buffer if it now
    /// exceeds the new cap.
    pub fn set_scrollback(&mut self, cap: usize) {
        self.cap = cap.max(1);
        self.trim();
    }

    /// Drop the oldest lines past the cap, keeping `scroll` aligned.
    fn trim(&mut self) {
        if self.lines.len() > self.cap {
            let drop = self.lines.len() - self.cap;
            self.lines.drain(0..drop);
            self.scroll = self.scroll.saturating_sub(drop);
        }
    }

    /// Append a line, capping the buffer at the scrollback. The view is pinned to
    /// the newest line only while *following* (i.e. the user has not scrolled up)
    /// — so streamed output does not yank the view away from something being read.
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        self.trim();
        if self.follow {
            self.scroll = self.lines.len();
        }
    }

    /// Remove all lines.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll = 0;
        self.follow = true;
    }

    /// Whether the buffer has no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Whether the view is following the newest line.
    #[must_use]
    pub fn is_following(&self) -> bool {
        self.follow
    }

    /// Scroll up by `n` lines (stops following).
    pub fn scroll_up(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.follow = false;
        self.scroll = self.scroll.min(self.lines.len()).saturating_sub(n);
    }

    /// Scroll down by `n` lines within a `viewport`-row window. Reaching the
    /// bottom resumes following.
    pub fn scroll_down(&mut self, n: usize, viewport: usize) {
        let len = self.lines.len();
        let max_start = len.saturating_sub(viewport.max(1));
        self.scroll = (self.scroll.min(len) + n).min(len);
        if self.scroll >= max_start {
            self.scroll = len;
            self.follow = true;
        } else {
            self.follow = false;
        }
    }

    /// Jump to the top (stops following).
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.follow = false;
    }

    /// Jump to the bottom and resume following.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.lines.len();
        self.follow = true;
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
    fn scrollback_caps_the_buffer_and_can_change() {
        let mut d = BottomDock::with_scrollback(3);
        for i in 0..5 {
            d.push(format!("l{i}"));
        }
        assert_eq!(d.lines, ["l2", "l3", "l4"], "oldest lines drop past the cap");
        // Lowering the cap trims what is already there.
        d.set_scrollback(2);
        assert_eq!(d.lines, ["l3", "l4"]);
        // A cap of 0 is clamped to 1.
        d.set_scrollback(0);
        assert_eq!(d.lines, ["l4"]);
    }

    #[test]
    fn clear_empties_the_buffer() {
        let mut d = BottomDock::new();
        d.push("x");
        d.clear();
        assert!(d.is_empty());
        assert_eq!(d.scroll, 0);
    }

    #[test]
    fn does_not_follow_while_scrolled_up() {
        let mut d = BottomDock::new();
        for i in 0..20 {
            d.push(format!("l{i}"));
        }
        assert!(d.is_following());
        assert_eq!(d.visible(3), ["l17", "l18", "l19"]);

        // Scroll up to read older lines: the window stops following.
        d.scroll_up(5);
        assert!(!d.is_following());
        let window = d.visible(3).to_vec();

        // New streamed lines must NOT move the view.
        d.push("new-a");
        d.push("new-b");
        assert_eq!(d.visible(3), window, "the view stays put while scrolled up");

        // Scrolling back to the bottom resumes following.
        d.scroll_to_bottom();
        assert!(d.is_following());
        assert_eq!(d.visible(3), ["l19", "new-a", "new-b"]);
        d.push("new-c");
        assert_eq!(d.visible(3), ["new-a", "new-b", "new-c"], "follows again at the bottom");
    }
}
