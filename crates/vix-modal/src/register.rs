//! Registers (`spec/index.md`, § Design: registers). The unnamed `"`
//! register mirrors the real OS clipboard exactly — that's the host's job
//! (`vix_clipboard`), not this pure crate's, so it isn't modeled here.
//! [`Registers`] holds only the named `a`-`z` registers: a small,
//! session-only in-memory map, deliberately never persisted (an explicit v1
//! simplification, not an oversight — see the spec's own cut list).

use std::collections::HashMap;

/// Whether a register's content is a whole-line block or an inline run of
/// characters — decides how `p`/`P` pastes it (spec's own distinction:
/// `yy`/`dd` write line-wise, `yw`/`dw` write character-wise, and real
/// Vim's `p` behaves differently — "paste as its own line" vs. "paste
/// inline at the cursor" — depending on which).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterKind {
    /// Paste inline at/after the cursor.
    Char,
    /// Paste as its own line, above or below the cursor's line.
    Line,
}

/// One register's content and how it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterValue {
    /// The text itself.
    pub text: String,
    /// How it was written — see [`RegisterKind`].
    pub kind: RegisterKind,
}

/// The named `a`-`z` registers (`"ayy`, `"ap`). `Default` starts empty —
/// every register is unset until something is written to it.
#[derive(Debug, Default)]
pub struct Registers(HashMap<char, RegisterValue>);

impl Registers {
    /// The named register `name`'s content, if anything has been written to
    /// it this session.
    #[must_use]
    pub fn get(&self, name: char) -> Option<&RegisterValue> {
        self.0.get(&name)
    }

    /// Write `value` into the named register `name`, replacing whatever was
    /// there before.
    pub fn set(&mut self, name: char, value: RegisterValue) {
        self.0.insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::{RegisterKind, RegisterValue, Registers};

    #[test]
    fn a_fresh_register_map_has_nothing_in_it() {
        assert!(Registers::default().get('a').is_none());
    }

    #[test]
    fn set_then_get_round_trips() {
        let mut regs = Registers::default();
        regs.set(
            'a',
            RegisterValue {
                text: "hi".to_string(),
                kind: RegisterKind::Char,
            },
        );
        let v = regs.get('a').unwrap();
        assert_eq!(v.text, "hi");
        assert_eq!(v.kind, RegisterKind::Char);
    }

    #[test]
    fn set_replaces_the_previous_value() {
        let mut regs = Registers::default();
        regs.set(
            'a',
            RegisterValue {
                text: "old".to_string(),
                kind: RegisterKind::Char,
            },
        );
        regs.set(
            'a',
            RegisterValue {
                text: "new".to_string(),
                kind: RegisterKind::Line,
            },
        );
        let v = regs.get('a').unwrap();
        assert_eq!(v.text, "new");
        assert_eq!(v.kind, RegisterKind::Line);
    }

    #[test]
    fn registers_are_independent_of_each_other() {
        let mut regs = Registers::default();
        regs.set(
            'a',
            RegisterValue {
                text: "a-text".to_string(),
                kind: RegisterKind::Char,
            },
        );
        assert!(regs.get('b').is_none());
    }
}
