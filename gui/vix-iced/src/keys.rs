//! iced keyboard and input-method events → `vix-input` events, via the rules
//! in `crate::key_rules`.
//!
//! iced reports a key press once, as [`keyboard::Event::KeyPressed`], whose
//! `key` is winit's key *without* modifiers (`s` for `Option+S`) and whose
//! `text` is what the layout typed with them (`ß`). So `key` maps to the key
//! rules' name and `text` is passed as the typed character: plain typing
//! uses the text, chords use the key ("Option as Meta", as in the other
//! builds). Composed input arrives separately as an input-method `Commit`
//! and types like text; on macOS winit sends it only for marked
//! (composing) text, so it never repeats a key press.

use iced::keyboard::{self, Key, Modifiers, key::Named};
use vix_input::{KeyEvent, KeyModifiers};

/// iced modifiers as `vix-input` flags. `logo` (Command, Windows, Super)
/// becomes `SUPER`; Vix folds Command into Control on macOS itself.
#[must_use]
pub fn modifiers(m: Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::CONTROL, m.control());
    out.set(KeyModifiers::ALT, m.alt());
    out.set(KeyModifiers::SHIFT, m.shift());
    out.set(KeyModifiers::SUPER, m.logo());
    out
}

/// The key rules' name for an iced key (`enter`, `pageup`, `f5`, `a`, `/`).
#[must_use]
pub fn key_name(key: &Key) -> Option<String> {
    let named = match key {
        Key::Character(c) => {
            let mut chars = c.chars();
            let c = chars.next()?;
            return chars.next().is_none().then(|| c.to_lowercase().collect());
        }
        Key::Named(named) => named,
        Key::Unidentified => return None,
    };
    let name = match named {
        Named::ArrowDown => "down",
        Named::ArrowLeft => "left",
        Named::ArrowRight => "right",
        Named::ArrowUp => "up",
        Named::Escape => "escape",
        Named::Tab => "tab",
        Named::Backspace => "backspace",
        Named::Enter => "enter",
        Named::Space => "space",
        Named::Insert => "insert",
        Named::Delete => "delete",
        Named::Home => "home",
        Named::End => "end",
        Named::PageUp => "pageup",
        Named::PageDown => "pagedown",
        other => return function_key(*other).map(|n| format!("f{n}")),
    };
    Some(name.to_string())
}

/// The number of a function key (`F1` → 1), or `None` for any other key.
fn function_key(named: Named) -> Option<u8> {
    const KEYS: [Named; 24] = [
        Named::F1,
        Named::F2,
        Named::F3,
        Named::F4,
        Named::F5,
        Named::F6,
        Named::F7,
        Named::F8,
        Named::F9,
        Named::F10,
        Named::F11,
        Named::F12,
        Named::F13,
        Named::F14,
        Named::F15,
        Named::F16,
        Named::F17,
        Named::F18,
        Named::F19,
        Named::F20,
        Named::F21,
        Named::F22,
        Named::F23,
        Named::F24,
    ];
    let i = KEYS.iter().position(|k| *k == named)?;
    u8::try_from(i + 1).ok()
}

/// The `vix-input` key press for one iced keyboard event, if any.
#[must_use]
pub fn key_event(event: &keyboard::Event) -> Option<KeyEvent> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers: m,
        text,
        ..
    } = event
    else {
        return None;
    };
    let name = key_name(key)?;
    crate::key_rules::key_event(&name, text.as_deref(), modifiers(*m))
}

/// The `vix-input` key presses for composed (IME) text.
#[must_use]
pub fn commit_events(text: &str) -> Vec<KeyEvent> {
    text.chars()
        .filter_map(|c| {
            let s = c.to_string();
            crate::key_rules::key_event(&s, Some(&s), KeyModifiers::NONE)
        })
        .collect()
}

/// An iced key press for a GPUI-syntax keystroke (`ctrl-p`, `shift-t`,
/// `enter`, `a`), shaped like the ones `iced_winit` produces — for the
/// scripted render smoke test, so scripted keys take the same path as real
/// ones.
#[cfg(any(test, feature = "screenshot"))]
#[must_use]
pub fn scripted(stroke: &str) -> Option<keyboard::Event> {
    use iced::keyboard::{Location, key::Physical};

    let mut m = Modifiers::empty();
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let name = parts.pop().unwrap_or_default();
    for p in parts {
        match p {
            "ctrl" => m |= Modifiers::CTRL,
            "alt" => m |= Modifiers::ALT,
            "shift" => m |= Modifiers::SHIFT,
            "cmd" => m |= Modifiers::LOGO,
            _ => {}
        }
    }
    let named = match name {
        "enter" => Some(Named::Enter),
        "escape" => Some(Named::Escape),
        "tab" => Some(Named::Tab),
        "backspace" => Some(Named::Backspace),
        "space" => Some(Named::Space),
        "up" => Some(Named::ArrowUp),
        "down" => Some(Named::ArrowDown),
        "left" => Some(Named::ArrowLeft),
        "right" => Some(Named::ArrowRight),
        _ => None,
    };
    let (key, modified_key, typed) = if let Some(n) = named {
        let typed = match n {
            Named::Enter => Some("\r".to_string()),
            Named::Tab => Some("\t".to_string()),
            Named::Space => Some(" ".to_string()),
            _ => None,
        };
        (Key::Named(n), Key::Named(n), typed)
    } else {
        let mut chars = name.chars();
        let c = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        let shown = if m.shift() { c.to_ascii_uppercase() } else { c };
        (
            Key::Character(c.to_string().into()),
            Key::Character(shown.to_string().into()),
            Some(shown.to_string()),
        )
    };
    // winit types no text for a Control/Command chord.
    let text = typed.filter(|_| !m.control() && !m.logo());
    Some(keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key: Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
        location: Location::Standard,
        modifiers: m,
        text: text.map(Into::into),
        repeat: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Location, key::Physical};
    use vix_input::KeyCode;

    fn press(key: Key, m: Modifiers, text: Option<&str>) -> Option<KeyEvent> {
        key_event(&keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: m,
            text: text.map(Into::into),
            repeat: false,
        })
    }

    fn ch(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn plain_typing_uses_the_typed_text() {
        assert_eq!(
            press(ch("a"), Modifiers::empty(), Some("a")),
            Some(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        );
        let upper = press(ch("a"), Modifiers::SHIFT, Some("A")).unwrap();
        assert_eq!(upper.code, KeyCode::Char('A'));
        assert_eq!(upper.modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        let q = press(ch("/"), Modifiers::SHIFT, Some("?")).unwrap();
        assert_eq!(q, KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        // An IME commit types like text.
        assert_eq!(commit_events("日本").len(), 2);
    }

    #[test]
    fn chords_and_named_keys() {
        assert_eq!(
            press(ch("p"), Modifiers::CTRL, Some("\u{10}")),
            Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        );
        let enter = press(Key::Named(Named::Enter), Modifiers::empty(), Some("\r"));
        assert_eq!(enter.unwrap().code, KeyCode::Enter);
        let f5 = press(Key::Named(Named::F5), Modifiers::empty(), None);
        assert_eq!(f5.unwrap().code, KeyCode::F(5));
        let back = press(Key::Named(Named::Tab), Modifiers::SHIFT, Some("\t")).unwrap();
        assert_eq!(back.code, KeyCode::BackTab);
        let space = press(Key::Named(Named::Space), Modifiers::empty(), Some(" "));
        assert_eq!(space.unwrap().code, KeyCode::Char(' '));
        let cmd = press(ch("s"), Modifiers::LOGO, None).unwrap();
        assert_eq!(cmd, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SUPER));
        assert_eq!(press(Key::Unidentified, Modifiers::empty(), None), None);
    }

    #[test]
    fn option_is_meta() {
        assert_eq!(
            press(ch("s"), Modifiers::ALT, Some("ß")),
            Some(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT))
        );
    }

    #[test]
    fn scripted_strokes_round_trip() {
        let key = |s: &str| scripted(s).as_ref().and_then(key_event);
        assert_eq!(
            key("ctrl-p"),
            Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        );
        assert_eq!(key("shift-t").unwrap().code, KeyCode::Char('T'));
        assert_eq!(key("enter").unwrap().code, KeyCode::Enter);
        assert_eq!(
            key("c"),
            Some(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
        );
    }
}
