//! AI chat panel: a persistent conversation surface for the configured assistant.
//!
//! The host (see `app.rs`) drives the actual CLI via the shared `spawn_ai`
//! machinery and the `ai_command` setting; this module is pure data. It holds the
//! conversation [`Turn`]s, the in-progress input line, a busy flag (a request is
//! in flight), and a scroll offset measured in wrapped lines up from the bottom.
//!
//! Keeping it data-only makes the transcript layout (word wrapping, the visible
//! window) unit-testable without a terminal.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Who authored a turn in the transcript.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// A message the user typed and sent.
    User,
    /// A reply captured from the assistant CLI.
    Assistant,
    /// An error notice (spawn/exec failure or empty output).
    Error,
}

/// One message in the conversation.
#[derive(Clone, Debug)]
pub struct Turn {
    /// Who authored this turn.
    pub role: Role,
    /// The message text (may contain newlines).
    pub text: String,
}

/// Chat-panel state: the transcript, the input line, scroll, and busy flag.
pub struct Panel {
    /// Conversation so far, oldest first.
    pub turns: Vec<Turn>,
    /// The line being composed (sent on Enter).
    pub input: String,
    /// Scroll position as wrapped lines up from the bottom (0 = newest visible).
    pub scroll: usize,
    /// Whether a request is in flight (input is held until it returns).
    pub busy: bool,
}

impl Default for Panel {
    fn default() -> Self {
        Self::open()
    }
}

impl Panel {
    /// Open an empty chat panel.
    #[must_use]
    pub fn open() -> Self {
        Panel { turns: Vec::new(), input: String::new(), scroll: 0, busy: false }
    }

    /// Append a turn and jump the view back to the newest content.
    pub fn push(&mut self, role: Role, text: impl Into<String>) {
        self.turns.push(Turn { role, text: text.into() });
        self.scroll = 0;
    }

    /// The most recent assistant reply, if any (for "open in tab" / "copy").
    #[must_use]
    pub fn last_assistant(&self) -> Option<&str> {
        self.turns.iter().rev().find(|t| t.role == Role::Assistant).map(|t| t.text.as_str())
    }

    /// Prior conversation formatted as plain text, fed to the CLI on stdin so the
    /// assistant has the running context. Empty when there are no turns yet.
    #[must_use]
    pub fn context(&self) -> String {
        let mut out = String::new();
        for turn in &self.turns {
            let tag = match turn.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Error => continue,
            };
            out.push_str(tag);
            out.push_str(": ");
            out.push_str(&turn.text);
            out.push_str("\n\n");
        }
        out
    }

    /// Scroll one line toward older messages.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll one line toward newer messages.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Every transcript line, word-wrapped to `width` columns, tagged by role.
    /// A blank separator line is inserted between turns.
    #[must_use]
    pub fn wrapped(&self, width: usize) -> Vec<(Role, String)> {
        let width = width.max(1);
        let mut lines = Vec::new();
        for (i, turn) in self.turns.iter().enumerate() {
            if i > 0 {
                lines.push((turn.role, String::new()));
            }
            for para in turn.text.split('\n') {
                if para.is_empty() {
                    lines.push((turn.role, String::new()));
                    continue;
                }
                for chunk in wrap_line(para, width) {
                    lines.push((turn.role, chunk));
                }
            }
        }
        lines
    }

    /// The window of wrapped lines visible in a `width` × `height` viewport, with
    /// `scroll` clamped so the view never runs past either end.
    #[must_use]
    pub fn visible(&mut self, width: usize, height: usize) -> Vec<(Role, String)> {
        let all = self.wrapped(width);
        let height = height.max(1);
        let max_scroll = all.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        let end = all.len().saturating_sub(self.scroll);
        let start = end.saturating_sub(height);
        all[start..end].to_vec()
    }
}

/// Greedily word-wrap one paragraph (no embedded newlines) to `width` columns,
/// breaking over-long words. Returns at least one (possibly empty) line.
fn wrap_line(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        if word.chars().count() > width {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let mut chunk = String::new();
            for ch in word.chars() {
                if chunk.chars().count() == width {
                    lines.push(std::mem::take(&mut chunk));
                }
                chunk.push(ch);
            }
            cur = chunk;
            continue;
        }
        let extra = usize::from(!cur.is_empty());
        if cur.chars().count() + extra + word.chars().count() > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    lines.push(cur);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_skips_errors_and_formats_turns() {
        let mut p = Panel::open();
        p.push(Role::User, "hello");
        p.push(Role::Assistant, "hi there");
        p.push(Role::Error, "boom");
        assert_eq!(p.context(), "User: hello\n\nAssistant: hi there\n\n");
        assert_eq!(p.last_assistant(), Some("hi there"));
    }

    #[test]
    fn wrap_breaks_long_words_and_wraps_on_spaces() {
        assert_eq!(wrap_line("the quick brown", 9), vec!["the quick", "brown"]);
        assert_eq!(wrap_line("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn visible_window_is_bottom_anchored_and_clamps_scroll() {
        let mut p = Panel::open();
        p.push(Role::User, "a\nb\nc\nd");
        // 4 lines; a 2-row viewport shows the last two by default.
        assert_eq!(p.visible(10, 2), vec![(Role::User, "c".into()), (Role::User, "d".into())]);
        p.scroll = 100; // over-scroll is clamped to the top
        assert_eq!(p.visible(10, 2), vec![(Role::User, "a".into()), (Role::User, "b".into())]);
    }

    #[test]
    fn push_resets_scroll_to_bottom() {
        let mut p = Panel::open();
        p.push(Role::User, "x");
        p.scroll = 5;
        p.push(Role::Assistant, "y");
        assert_eq!(p.scroll, 0);
    }
}
