//! Backend-neutral keyboard and mouse input types.
//!
//! Every Vix crate that reacts to input — the `App` shell, the keymap, keyboard
//! macros, modal recording, the editor widget, the edit surfaces — takes these
//! types rather than a particular backend's. Each front end converts its own
//! events at the edge: the terminal build converts crossterm's (behind the
//! `crossterm` feature, see [`From`] impls below), and the desktop GUI converts
//! GPUI keystrokes.
//!
//! The shapes deliberately mirror crossterm 0.29's `event` module — the same
//! type names, variants, fields, constructors and modifier flags — so code
//! written against crossterm reads the same against `vix-input`, and the
//! terminal conversion is a lossless field-by-field copy for every key Vix
//! handles. Keys Vix has no use for (media keys, bare modifier presses) map
//! to [`KeyCode::Null`].

use std::hash::{Hash, Hasher};

bitflags::bitflags! {
    /// Modifier keys held during a key or mouse event.
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Default)]
    pub struct KeyModifiers: u8 {
        /// Shift.
        const SHIFT = 0b0000_0001;
        /// Control.
        const CONTROL = 0b0000_0010;
        /// Alt (Option on macOS).
        const ALT = 0b0000_0100;
        /// Super (Command on macOS, Windows key on Windows).
        const SUPER = 0b0000_1000;
        /// Hyper.
        const HYPER = 0b0001_0000;
        /// Meta.
        const META = 0b0010_0000;
        /// No modifiers.
        const NONE = 0b0000_0000;
    }
}

bitflags::bitflags! {
    /// Extra lock/keypad state attached to a key event, where the backend reports it.
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Default)]
    pub struct KeyEventState: u8 {
        /// The key came from the keypad.
        const KEYPAD = 0b0000_0001;
        /// Caps Lock was on.
        const CAPS_LOCK = 0b0000_0010;
        /// Num Lock was on.
        const NUM_LOCK = 0b0000_0100;
        /// No extra state.
        const NONE = 0b0000_0000;
    }
}

/// Whether a key event is a press, an auto-repeat, or a release.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash, Default)]
pub enum KeyEventKind {
    /// The key went down.
    #[default]
    Press,
    /// The key is held and auto-repeating.
    Repeat,
    /// The key came up.
    Release,
}

/// Which key an event is about.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyCode {
    /// Backspace.
    Backspace,
    /// Enter / Return.
    Enter,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Tab.
    Tab,
    /// Shift+Tab, as terminals report it.
    BackTab,
    /// Forward delete.
    Delete,
    /// Insert.
    Insert,
    /// A function key, `F(1)` for F1.
    F(u8),
    /// A printable character (already shifted: `'A'`, not `'a'` + Shift).
    Char(char),
    /// No key, or a key Vix does not distinguish (media keys, bare modifiers).
    Null,
    /// Escape.
    Esc,
    /// Caps Lock.
    CapsLock,
    /// Scroll Lock.
    ScrollLock,
    /// Num Lock.
    NumLock,
    /// Print Screen.
    PrintScreen,
    /// Pause.
    Pause,
    /// Menu / context-menu key.
    Menu,
    /// Keypad 5 with Num Lock off.
    KeypadBegin,
}

impl KeyCode {
    /// Whether this is function key `n` (`F(n)`).
    #[must_use]
    pub fn is_function_key(&self, n: u8) -> bool {
        matches!(self, Self::F(m) if *m == n)
    }

    /// Whether this is the character `c`.
    #[must_use]
    pub fn is_char(&self, c: char) -> bool {
        matches!(self, Self::Char(m) if *m == c)
    }

    /// The character this key types, if it is a [`KeyCode::Char`].
    #[must_use]
    pub fn as_char(&self) -> Option<char> {
        match self {
            Self::Char(c) => Some(*c),
            _ => None,
        }
    }
}

/// One key event: the key, its modifiers, its kind, and lock state.
///
/// Equality and hashing are case-normalized exactly as crossterm's are: an
/// uppercase ASCII `Char` implies [`KeyModifiers::SHIFT`], and `Shift` plus a
/// lowercase ASCII char compares equal to the uppercase char with `Shift`.
#[derive(Debug, PartialOrd, Clone, Copy)]
pub struct KeyEvent {
    /// The key.
    pub code: KeyCode,
    /// Modifiers held.
    pub modifiers: KeyModifiers,
    /// Press, repeat, or release.
    pub kind: KeyEventKind,
    /// Keypad / lock state.
    pub state: KeyEventState,
}

impl KeyEvent {
    /// A key press with the given modifiers.
    #[must_use]
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self::new_with_kind_and_state(code, modifiers, KeyEventKind::Press, KeyEventState::NONE)
    }

    /// A key event of the given kind.
    #[must_use]
    pub const fn new_with_kind(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Self {
        Self::new_with_kind_and_state(code, modifiers, kind, KeyEventState::NONE)
    }

    /// A key event with every field given.
    #[must_use]
    pub const fn new_with_kind_and_state(
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: KeyEventKind,
        state: KeyEventState,
    ) -> Self {
        Self {
            code,
            modifiers,
            kind,
            state,
        }
    }

    /// Whether this is a press.
    #[must_use]
    pub fn is_press(&self) -> bool {
        self.kind == KeyEventKind::Press
    }

    /// Whether this is a release.
    #[must_use]
    pub fn is_release(&self) -> bool {
        self.kind == KeyEventKind::Release
    }

    /// Whether this is an auto-repeat.
    #[must_use]
    pub fn is_repeat(&self) -> bool {
        self.kind == KeyEventKind::Repeat
    }

    /// Make `Shift` present iff an uppercase ASCII char is present.
    fn normalize_case(mut self) -> Self {
        let KeyCode::Char(c) = self.code else {
            return self;
        };
        if c.is_ascii_uppercase() {
            self.modifiers.insert(KeyModifiers::SHIFT);
        } else if self.modifiers.contains(KeyModifiers::SHIFT) {
            self.code = KeyCode::Char(c.to_ascii_uppercase());
        }
        self
    }
}

impl From<KeyCode> for KeyEvent {
    fn from(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }
}

impl PartialEq for KeyEvent {
    fn eq(&self, other: &Self) -> bool {
        let a = self.normalize_case();
        let b = other.normalize_case();
        a.code == b.code && a.modifiers == b.modifiers && a.kind == b.kind && a.state == b.state
    }
}

impl Eq for KeyEvent {}

impl Hash for KeyEvent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let n = self.normalize_case();
        n.code.hash(state);
        n.modifiers.hash(state);
        n.kind.hash(state);
        n.state.hash(state);
    }
}

/// A mouse button.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Middle button / wheel click.
    Middle,
}

/// What a mouse event did.
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MouseEventKind {
    /// A button went down.
    Down(MouseButton),
    /// A button came up.
    Up(MouseButton),
    /// The pointer moved with a button held.
    Drag(MouseButton),
    /// The pointer moved with no button held.
    Moved,
    /// Wheel scrolled down (content moves up).
    ScrollDown,
    /// Wheel scrolled up.
    ScrollUp,
    /// Wheel scrolled left.
    ScrollLeft,
    /// Wheel scrolled right.
    ScrollRight,
}

/// One mouse event, positioned in character cells (0-based column/row).
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MouseEvent {
    /// What happened.
    pub kind: MouseEventKind,
    /// Cell column.
    pub column: u16,
    /// Cell row.
    pub row: u16,
    /// Modifiers held.
    pub modifiers: KeyModifiers,
}

#[cfg(feature = "crossterm")]
mod from_crossterm {
    //! Lossless conversion from crossterm's event types (terminal front end).

    use super::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    };
    use crossterm::event as ct;

    impl From<ct::KeyModifiers> for KeyModifiers {
        fn from(m: ct::KeyModifiers) -> Self {
            Self::from_bits_truncate(m.bits())
        }
    }

    impl From<ct::KeyEventState> for KeyEventState {
        fn from(s: ct::KeyEventState) -> Self {
            Self::from_bits_truncate(s.bits())
        }
    }

    impl From<ct::KeyEventKind> for KeyEventKind {
        fn from(k: ct::KeyEventKind) -> Self {
            match k {
                ct::KeyEventKind::Press => Self::Press,
                ct::KeyEventKind::Repeat => Self::Repeat,
                ct::KeyEventKind::Release => Self::Release,
            }
        }
    }

    impl From<ct::KeyCode> for KeyCode {
        fn from(c: ct::KeyCode) -> Self {
            match c {
                ct::KeyCode::Backspace => Self::Backspace,
                ct::KeyCode::Enter => Self::Enter,
                ct::KeyCode::Left => Self::Left,
                ct::KeyCode::Right => Self::Right,
                ct::KeyCode::Up => Self::Up,
                ct::KeyCode::Down => Self::Down,
                ct::KeyCode::Home => Self::Home,
                ct::KeyCode::End => Self::End,
                ct::KeyCode::PageUp => Self::PageUp,
                ct::KeyCode::PageDown => Self::PageDown,
                ct::KeyCode::Tab => Self::Tab,
                ct::KeyCode::BackTab => Self::BackTab,
                ct::KeyCode::Delete => Self::Delete,
                ct::KeyCode::Insert => Self::Insert,
                ct::KeyCode::F(n) => Self::F(n),
                ct::KeyCode::Char(ch) => Self::Char(ch),
                ct::KeyCode::Esc => Self::Esc,
                ct::KeyCode::CapsLock => Self::CapsLock,
                ct::KeyCode::ScrollLock => Self::ScrollLock,
                ct::KeyCode::NumLock => Self::NumLock,
                ct::KeyCode::PrintScreen => Self::PrintScreen,
                ct::KeyCode::Pause => Self::Pause,
                ct::KeyCode::Menu => Self::Menu,
                ct::KeyCode::KeypadBegin => Self::KeypadBegin,
                ct::KeyCode::Null | ct::KeyCode::Media(_) | ct::KeyCode::Modifier(_) => Self::Null,
            }
        }
    }

    impl From<ct::KeyEvent> for KeyEvent {
        fn from(k: ct::KeyEvent) -> Self {
            Self::new_with_kind_and_state(
                k.code.into(),
                k.modifiers.into(),
                k.kind.into(),
                k.state.into(),
            )
        }
    }

    impl From<ct::MouseButton> for MouseButton {
        fn from(b: ct::MouseButton) -> Self {
            match b {
                ct::MouseButton::Left => Self::Left,
                ct::MouseButton::Right => Self::Right,
                ct::MouseButton::Middle => Self::Middle,
            }
        }
    }

    impl From<ct::MouseEventKind> for MouseEventKind {
        fn from(k: ct::MouseEventKind) -> Self {
            match k {
                ct::MouseEventKind::Down(b) => Self::Down(b.into()),
                ct::MouseEventKind::Up(b) => Self::Up(b.into()),
                ct::MouseEventKind::Drag(b) => Self::Drag(b.into()),
                ct::MouseEventKind::Moved => Self::Moved,
                ct::MouseEventKind::ScrollDown => Self::ScrollDown,
                ct::MouseEventKind::ScrollUp => Self::ScrollUp,
                ct::MouseEventKind::ScrollLeft => Self::ScrollLeft,
                ct::MouseEventKind::ScrollRight => Self::ScrollRight,
            }
        }
    }

    impl From<ct::MouseEvent> for MouseEvent {
        fn from(m: ct::MouseEvent) -> Self {
            Self {
                kind: m.kind.into(),
                column: m.column,
                row: m.row,
                modifiers: m.modifiers.into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn shift_normalized_equality_and_hash() {
        let upper = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        let shifted = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);
        assert_eq!(upper, shifted);
        let set: HashSet<_> = [upper, shifted].into_iter().collect();
        assert_eq!(set.len(), 1);
        assert_ne!(upper, KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    }

    #[test]
    fn kind_and_state_participate_in_equality() {
        let press = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let release =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release);
        assert_ne!(press, release);
        assert!(press.is_press() && release.is_release());
        assert_eq!(KeyEvent::from(KeyCode::Enter), press);
    }

    #[test]
    fn keycode_helpers() {
        assert!(KeyCode::F(5).is_function_key(5));
        assert!(KeyCode::Char('x').is_char('x'));
        assert_eq!(KeyCode::Tab.as_char(), None);
    }

    #[cfg(feature = "crossterm")]
    #[test]
    fn converts_from_crossterm() {
        use crossterm::event as ct;
        let k = ct::KeyEvent::new_with_kind_and_state(
            ct::KeyCode::Char('q'),
            ct::KeyModifiers::CONTROL | ct::KeyModifiers::ALT,
            ct::KeyEventKind::Repeat,
            ct::KeyEventState::CAPS_LOCK,
        );
        let v = KeyEvent::from(k);
        assert_eq!(v.code, KeyCode::Char('q'));
        assert_eq!(v.modifiers, KeyModifiers::CONTROL | KeyModifiers::ALT);
        assert_eq!(v.kind, KeyEventKind::Repeat);
        assert_eq!(v.state, KeyEventState::CAPS_LOCK);
        assert_eq!(
            KeyCode::from(ct::KeyCode::Media(ct::MediaKeyCode::Play)),
            KeyCode::Null
        );
        let m = MouseEvent::from(ct::MouseEvent {
            kind: ct::MouseEventKind::Drag(ct::MouseButton::Middle),
            column: 3,
            row: 9,
            modifiers: ct::KeyModifiers::SHIFT,
        });
        assert_eq!(m.kind, MouseEventKind::Drag(MouseButton::Middle));
        assert_eq!((m.column, m.row), (3, 9));
        assert_eq!(m.modifiers, KeyModifiers::SHIFT);
    }
}
