//! egui input events → `vix-input` events, via the rules in
//! `crate::key_rules`.
//!
//! egui reports a typed key twice: an [`Event::Key`] for the physical key and
//! an [`Event::Text`] for the character the layout produced (none for
//! `Ctrl`/`Cmd` chords). So plain typing is taken from `Text` (or an IME
//! `Commit`) and `Key` is used only for named keys and chords — giving the
//! same events crossterm delivers in the terminal. With `Alt` held the `Key`
//! is used ("Option as Meta", as in the GPUI build) and the layout's `Text`
//! (`ß` for `Option+S`) is dropped.

use eframe::egui::{Event, ImeEvent, Key, Modifiers};
use vix_input::{KeyEvent, KeyModifiers};

/// egui modifiers as `vix-input` flags. `mac_cmd` (Command) becomes `SUPER`;
/// Vix folds Command into Control on macOS itself.
#[must_use]
pub fn modifiers(m: Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::CONTROL, m.ctrl);
    out.set(KeyModifiers::ALT, m.alt);
    out.set(KeyModifiers::SHIFT, m.shift);
    out.set(KeyModifiers::SUPER, m.mac_cmd);
    out
}

/// The key rules' name for an egui key (`enter`, `pageup`, `f5`, `a`, `/`).
#[must_use]
pub fn key_name(key: Key) -> Option<String> {
    let named = match key {
        Key::ArrowDown => "down",
        Key::ArrowLeft => "left",
        Key::ArrowRight => "right",
        Key::ArrowUp => "up",
        Key::Escape => "escape",
        Key::Tab => "tab",
        Key::Backspace => "backspace",
        Key::Enter => "enter",
        Key::Space => "space",
        Key::Insert => "insert",
        Key::Delete => "delete",
        Key::Home => "home",
        Key::End => "end",
        Key::PageUp => "pageup",
        Key::PageDown => "pagedown",
        // egui's own symbol for Minus is U+2212 MINUS SIGN; the key types '-'.
        Key::Minus => "-",
        _ => {
            let name = key.name();
            if let Some(n) = name.strip_prefix('F')
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                return Some(format!("f{n}"));
            }
            if let Some(d) = name.strip_prefix("Num") {
                return Some(d.to_string());
            }
            let sym = key.symbol_or_name();
            let mut chars = sym.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // a named key Vix has no use for
            }
            return Some(c.to_ascii_lowercase().to_string());
        }
    };
    Some(named.to_string())
}

/// Whether a key's character arrives separately as [`Event::Text`] when no
/// chord modifier is held.
fn types_text(key: Key) -> bool {
    key == Key::Space
        || key_name(key).is_some_and(|n| {
            let mut c = n.chars();
            c.next().is_some() && c.next().is_none()
        })
}

/// The `vix-input` key presses for one egui event. `held` is the modifier
/// state at the time (for `Text`, which carries none of its own).
#[must_use]
pub fn key_events(event: &Event, held: Modifiers) -> Vec<KeyEvent> {
    match event {
        Event::Key {
            key,
            pressed: true,
            modifiers: m,
            ..
        } => {
            let mods = modifiers(*m);
            let chord =
                mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
            if types_text(*key) && !chord {
                return Vec::new(); // the Text event carries it
            }
            key_name(*key)
                .and_then(|name| crate::key_rules::key_event(&name, None, mods))
                .into_iter()
                .collect()
        }
        Event::Text(text) | Event::Ime(ImeEvent::Commit(text)) => {
            if held.alt {
                return Vec::new(); // Option as Meta: the Key event carried it
            }
            let mods = modifiers(held) & KeyModifiers::SHIFT;
            text.chars()
                .filter_map(|c| {
                    let s = c.to_string();
                    crate::key_rules::key_event(&s, Some(&s), mods)
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// egui events for a GPUI-syntax keystroke (`ctrl-p`, `shift-t`, `enter`,
/// `a`), shaped like the ones egui-winit produces — for the scripted render
/// smoke test, so scripted keys take the same path as real ones.
#[cfg(any(test, feature = "screenshot"))]
#[must_use]
pub fn scripted(stroke: &str) -> (Vec<Event>, Modifiers) {
    let mut m = Modifiers::NONE;
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    for p in parts {
        match p {
            "ctrl" => m.ctrl = true,
            "alt" => m.alt = true,
            "shift" => m.shift = true,
            "cmd" => {
                m.mac_cmd = true;
                m.command = true;
            }
            _ => {}
        }
    }
    let egui_key = match key {
        "enter" => Some(Key::Enter),
        "escape" => Some(Key::Escape),
        "tab" => Some(Key::Tab),
        "backspace" => Some(Key::Backspace),
        "space" => Some(Key::Space),
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        other => Key::from_name(&other.to_ascii_uppercase()),
    };
    let mut events = Vec::new();
    if let Some(k) = egui_key {
        events.push(Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        });
    }
    let chord = m.ctrl || m.mac_cmd;
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && !chord
    {
        let c = if m.shift { c.to_ascii_uppercase() } else { c };
        events.push(Event::Text(c.to_string()));
    }
    (events, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    fn key(k: Key, m: Modifiers) -> Event {
        Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        }
    }

    #[test]
    fn plain_typing_comes_from_text_not_key() {
        assert_eq!(
            key_events(&key(Key::A, Modifiers::NONE), Modifiers::NONE),
            []
        );
        assert_eq!(
            key_events(&Event::Text("a".into()), Modifiers::NONE),
            [KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)]
        );
        let upper = key_events(&Event::Text("A".into()), Modifiers::SHIFT);
        assert_eq!(upper[0].code, KeyCode::Char('A'));
        assert_eq!(upper[0].modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        let q = key_events(&Event::Text("?".into()), Modifiers::SHIFT);
        assert_eq!(q[0], KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        // An IME commit types like Text.
        assert_eq!(
            key_events(
                &Event::Ime(ImeEvent::Commit("日本".into())),
                Modifiers::NONE
            )
            .len(),
            2
        );
    }

    #[test]
    fn chords_and_named_keys_come_from_key() {
        assert_eq!(
            key_events(&key(Key::P, Modifiers::CTRL), Modifiers::CTRL),
            [KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)]
        );
        assert_eq!(
            key_events(&key(Key::Enter, Modifiers::NONE), Modifiers::NONE)[0].code,
            KeyCode::Enter
        );
        assert_eq!(
            key_events(&key(Key::F5, Modifiers::NONE), Modifiers::NONE)[0].code,
            KeyCode::F(5)
        );
        let back = key_events(&key(Key::Tab, Modifiers::SHIFT), Modifiers::SHIFT);
        assert_eq!(back[0].code, KeyCode::BackTab);
        assert_eq!(
            key_events(&key(Key::Minus, Modifiers::CTRL), Modifiers::CTRL)[0].code,
            KeyCode::Char('-')
        );
    }

    #[test]
    fn option_is_meta() {
        let m = Modifiers::ALT;
        assert_eq!(
            key_events(&key(Key::S, m), m),
            [KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT)]
        );
        assert_eq!(key_events(&Event::Text("ß".into()), m), []);
    }

    #[test]
    fn scripted_strokes_round_trip() {
        let (events, held) = scripted("ctrl-p");
        let keys: Vec<_> = events.iter().flat_map(|e| key_events(e, held)).collect();
        assert_eq!(
            keys,
            [KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)]
        );
        let (events, held) = scripted("shift-t");
        let keys: Vec<_> = events.iter().flat_map(|e| key_events(e, held)).collect();
        assert_eq!(keys[0].code, KeyCode::Char('T'));
    }
}
