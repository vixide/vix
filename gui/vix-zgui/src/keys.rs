//! zgui keystrokes → `vix-input` events, via the rules in `crate::key_rules`.
//!
//! zgui reports one `key_down` per press, with what the key means under the
//! layout and the held modifiers (`key`: `Character("A")`, `Named(Enter)`),
//! what it would mean with no modifier (`key_without_modifiers`:
//! `Character("a")`), and the modifiers beside it. zgui 0.1 emits no separate
//! text event for typing, so the typed character is read from `key` and a
//! chord's key from `key_without_modifiers` — one press, one Vix key event.
//! Lone modifier presses, dead keys and keys Vix has no name for are dropped.

use vix_input::{KeyEvent, KeyModifiers};
use zgui::prelude::{Key, Modifiers, NamedKey};

/// zgui modifiers as `vix-input` flags. `META` (Command on macOS, the
/// Windows or Super key elsewhere) becomes `SUPER`; Vix folds Command into
/// Control on macOS itself (`App::command_as_control`).
#[must_use]
pub fn modifiers(m: Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::SHIFT, m.shift());
    out.set(KeyModifiers::CONTROL, m.control());
    out.set(KeyModifiers::ALT, m.alt());
    out.set(KeyModifiers::SUPER, m.meta());
    out
}

/// The key rules' name for a zgui key (`enter`, `pageup`, `f5`, `a`, `/`),
/// or `None` for a key Vix has no use for (a lone modifier, Caps Lock, a
/// dead key ...).
#[must_use]
pub fn key_name(key: &Key) -> Option<String> {
    match key {
        Key::Named(named) => named_key(*named),
        Key::Character(text) => {
            let mut chars = text.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Some(c.to_lowercase().collect())
        }
        _ => None,
    }
}

/// The key rules' name for a named key.
fn named_key(key: NamedKey) -> Option<String> {
    let name = match key {
        NamedKey::Enter => "enter",
        NamedKey::Tab => "tab",
        NamedKey::Space => "space",
        NamedKey::ArrowDown => "down",
        NamedKey::ArrowLeft => "left",
        NamedKey::ArrowRight => "right",
        NamedKey::ArrowUp => "up",
        NamedKey::End => "end",
        NamedKey::Home => "home",
        NamedKey::PageDown => "pagedown",
        NamedKey::PageUp => "pageup",
        NamedKey::Backspace => "backspace",
        NamedKey::Delete => "delete",
        NamedKey::Insert => "insert",
        NamedKey::Escape => "escape",
        other => {
            // F1 … F24: the standard name is `F5`; the rules say `f5`.
            let name = other.as_str();
            let n = name.strip_prefix('F')?;
            if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            return Some(format!("f{n}"));
        }
    };
    Some(name.to_string())
}

/// The `vix-input` key press for one zgui `key_down`: `key` is what the
/// layout made of it, `unmodified` the same key with no modifier applied.
/// `None` for a key Vix ignores.
#[must_use]
pub fn key_event(key: &Key, unmodified: &Key, mods: Modifiers) -> Option<KeyEvent> {
    let name = key_name(unmodified).or_else(|| key_name(key))?;
    let typed = match key {
        Key::Character(text) => Some(text.as_str()),
        _ => None,
    };
    crate::key_rules::key_event(&name, typed, modifiers(mods))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    /// A character key.
    fn ch(s: &str) -> Key {
        Key::character(s)
    }

    #[test]
    fn typing_uses_the_layouts_character() {
        let a = key_event(&ch("a"), &ch("a"), Modifiers::NONE).unwrap();
        assert_eq!(a, KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let upper = key_event(&ch("A"), &ch("a"), Modifiers::SHIFT).unwrap();
        assert_eq!(upper.code, KeyCode::Char('A'));
        assert_eq!(upper.modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        let q = key_event(&ch("?"), &ch("/"), Modifiers::SHIFT).unwrap();
        assert_eq!(q, KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    }

    #[test]
    fn chords_use_the_unmodified_key() {
        // Option+S types ß on a US Mac layout; Vix wants Alt+s.
        let alt = key_event(&ch("ß"), &ch("s"), Modifiers::ALT).unwrap();
        assert_eq!(alt, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        let ctrl = key_event(&ch("\u{3}"), &ch("c"), Modifiers::CONTROL).unwrap();
        assert_eq!(
            ctrl,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        );
        let cmd = key_event(&ch("s"), &ch("s"), Modifiers::META).unwrap();
        assert_eq!(cmd, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SUPER));
    }

    #[test]
    fn named_keys_map_to_their_codes() {
        let named = |k: NamedKey, m: Modifiers| {
            key_event(&Key::Named(k), &Key::Named(k), m).map(|e| e.code)
        };
        assert_eq!(
            named(NamedKey::Enter, Modifiers::NONE),
            Some(KeyCode::Enter)
        );
        assert_eq!(named(NamedKey::Escape, Modifiers::NONE), Some(KeyCode::Esc));
        assert_eq!(named(NamedKey::F12, Modifiers::NONE), Some(KeyCode::F(12)));
        assert_eq!(
            named(NamedKey::PageDown, Modifiers::NONE),
            Some(KeyCode::PageDown)
        );
        assert_eq!(
            named(NamedKey::Tab, Modifiers::SHIFT),
            Some(KeyCode::BackTab)
        );
        assert_eq!(
            named(NamedKey::Space, Modifiers::NONE),
            Some(KeyCode::Char(' '))
        );
    }

    #[test]
    fn modifiers_dead_keys_and_unknown_keys_are_dropped() {
        let none = |k: Key| key_event(&k, &k, Modifiers::NONE);
        assert_eq!(none(Key::Named(NamedKey::Shift)), None);
        assert_eq!(none(Key::Named(NamedKey::CapsLock)), None);
        assert_eq!(none(Key::Dead(Some('´'))), None);
        assert_eq!(none(Key::Unidentified), None);
    }

    #[test]
    fn meta_becomes_super() {
        let m = modifiers(Modifiers::META.with(Modifiers::SHIFT, true));
        assert_eq!(m, KeyModifiers::SUPER | KeyModifiers::SHIFT);
    }
}
