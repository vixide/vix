//! State for the right dock: a message drawer of advice and notifications, each
//! individually dismissable, plus the current selection.
//!
//! Pure data — the host (the `vix` app) renders the drawer and routes keys and
//! clicks; this crate owns only the message list and selection logic.
//!
//! Invariant: `Messages::selected` is a valid index whenever `items` is
//! non-empty (the navigation and dismiss methods clamp it); when `items` is
//! empty it is `0` and points at nothing, so renderers must check emptiness
//! before indexing. The list is unbounded; unlike the bottom dock there is no
//! scrollback cap because messages are dismissed individually by the user.
//!
//! Spec: `crates/vix-right-dock/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// Severity of a message, which selects its icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Neutral information.
    Info,
    /// A helpful tip.
    Advice,
    /// A non-fatal warning.
    Warn,
    /// An error.
    Error,
}

/// A single message row.
pub struct Message {
    /// Severity level.
    pub level: Level,
    /// Display text.
    pub text: String,
}

/// The message drawer: a list of messages plus the current selection.
#[derive(Default)]
pub struct Messages {
    /// Messages, oldest first.
    pub items: Vec<Message>,
    /// Index of the highlighted message.
    pub selected: usize,
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

impl Messages {
    /// Append a message with the given level.
    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        self.items.push(Message {
            level,
            text: text.into(),
        });
    }

    /// Append an [`Level::Info`] message.
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(Level::Info, text);
    }

    /// Append an [`Level::Advice`] message.
    pub fn advice(&mut self, text: impl Into<String>) {
        self.push(Level::Advice, text);
    }

    /// Append a [`Level::Warn`] message.
    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(Level::Warn, text);
    }

    /// Append a [`Level::Error`] message.
    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Level::Error, text);
    }

    /// Move the selection up one row.
    pub fn up(&mut self) {
        self.selected = sub_or(self.selected, 1, 0); // DOCK-F1: stop at the first message
    }

    /// Move the selection down one row, stopping at the last message.
    pub fn down(&mut self) {
        if let Some(next) = self.selected.checked_add(1)
            && next < self.items.len()
        {
            self.selected = next;
        }
    }

    /// Dismiss the selected message (the "close x"). A no-op when `items` is
    /// empty. Afterwards the selection stays on the same index (now the next
    /// message), or moves to the new last message if the final one was closed.
    pub fn close_selected(&mut self) {
        if self.selected < self.items.len() {
            self.items.remove(self.selected);
            // Removing the last row leaves `selected` one past the end; pull it
            // back so the invariant (valid index or 0 when empty) holds.
            if self.selected >= self.items.len() {
                self.selected = sub_or(self.items.len(), 1, 0); // DOCK-F1: empty list, row 0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_levels_and_select() {
        let mut m = Messages::default();
        m.info("a");
        m.advice("b");
        m.error("c");
        assert_eq!(m.items.len(), 3);
        m.down();
        m.down();
        assert_eq!(m.selected, 2);
        m.down(); // clamps at the last
        assert_eq!(m.selected, 2);
    }

    #[test]
    fn close_selected_keeps_selection_in_range() {
        let mut m = Messages::default();
        m.info("a");
        m.info("b");
        m.selected = 1;
        m.close_selected();
        assert_eq!(m.items.len(), 1);
        assert_eq!(m.selected, 0);
        m.close_selected();
        assert!(m.items.is_empty());
        assert_eq!(m.selected, 0);
    }
}
