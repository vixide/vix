//! Slint key events → `vix-input` events, via the rules in
//! `crate::key_rules`.
//!
//! A `FocusScope`'s `key-pressed` gives the key as `text`: the typed
//! character for a printable key (under the winit backend, the layout's
//! character with Shift applied and Ctrl/Cmd ignored), or a private-use code
//! point for a named key (`Key.Return`, `Key.UpArrow`, `Key.F5`, …), plus the
//! held modifiers. Slint's winit backend swaps Command and Control on macOS
//! (`control` is Command, `meta` is Control), which [`modifiers`] undoes.
//!
//! With Option held, macOS gives the Option-composed character (`ß` for
//! `Option+S`, nothing at all for a dead key such as `Option+E`), so the key
//! itself is lost from Slint's event. For "Option as Meta" the window takes
//! those key presses from winit directly instead (`view.rs`), through
//! [`option_chord`].

use vix_input::{KeyEvent, KeyModifiers};

/// Slint's modifier flags, as the window's `Mods` struct carries them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors Slint's KeyboardModifiers, four independent flags"
)]
pub struct Mods {
    /// Slint's `control` (Command on macOS).
    pub control: bool,
    /// Slint's `alt` (Option on macOS).
    pub alt: bool,
    /// Slint's `shift`.
    pub shift: bool,
    /// Slint's `meta` (Control on macOS; Windows / Super elsewhere).
    pub meta: bool,
}

/// Slint modifiers as `vix-input` flags. On Apple platforms (`apple`) Slint
/// reports Command as `control` and Control as `meta`; Command becomes
/// `SUPER` and Control `CONTROL`, as in the other builds (Vix folds Command
/// into Control on macOS itself). Elsewhere `meta` (Windows / Super) is
/// `SUPER`.
#[must_use]
pub fn modifiers(m: Mods, apple: bool) -> KeyModifiers {
    let (ctrl, sup) = if apple {
        (m.meta, m.control)
    } else {
        (m.control, m.meta)
    };
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::CONTROL, ctrl);
    out.set(KeyModifiers::ALT, m.alt);
    out.set(KeyModifiers::SHIFT, m.shift);
    out.set(KeyModifiers::SUPER, sup);
    out
}

/// The key rules' name for Slint's key `text` (`enter`, `pageup`, `f5`, or
/// the lowercased character), or `None` for keys Vix has no use for (the
/// modifier keys themselves, Caps Lock, Menu, …).
#[must_use]
pub fn key_name(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let named = match c {
        '\u{8}' => "backspace",
        '\t' | '\u{19}' => "tab",
        '\n' | '\r' => "enter",
        '\u{1b}' => "escape",
        '\u{7f}' => "delete",
        ' ' => "space",
        '\u{F700}' => "up",
        '\u{F701}' => "down",
        '\u{F702}' => "left",
        '\u{F703}' => "right",
        '\u{F704}'..='\u{F71B}' => return Some(format!("f{}", u32::from(c) - 0xF704 + 1)),
        '\u{F727}' => "insert",
        '\u{F729}' => "home",
        '\u{F72B}' => "end",
        '\u{F72C}' => "pageup",
        '\u{F72D}' => "pagedown",
        // Slint's other named keys, and the modifier keys' own presses.
        '\u{F700}'..='\u{F8FF}' => return None,
        c if c.is_control() => return None,
        c => return Some(c.to_lowercase().collect()),
    };
    Some(named.to_string())
}

/// The `vix-input` key press for one Slint `key-pressed` event, or `None`
/// for a key Vix ignores.
#[must_use]
pub fn key_event(text: &str, mods: Mods, apple: bool) -> Option<KeyEvent> {
    let name = key_name(text)?;
    let mut m = modifiers(mods, apple);
    // `Key.Backtab`: Shift+Tab, should a platform report it that way.
    if text == "\u{19}" {
        m |= KeyModifiers::SHIFT;
    }
    crate::key_rules::key_event(&name, Some(text), m)
}

/// The key press for an `Option`/`Alt` chord taken from winit on macOS:
/// `key` is winit's key without modifiers (`s` for `Option+S`), and the flags
/// are winit's own (not swapped). "Option as Meta", as in the other builds.
#[cfg(any(target_os = "macos", test))]
#[must_use]
pub fn option_chord(key: &str, shift: bool, control: bool, command: bool) -> Option<KeyEvent> {
    let name = key_name(key)?;
    let mut m = KeyModifiers::ALT;
    m.set(KeyModifiers::SHIFT, shift);
    m.set(KeyModifiers::CONTROL, control);
    m.set(KeyModifiers::SUPER, command);
    crate::key_rules::key_event(&name, None, m)
}

/// The Slint `key-pressed` event for a GPUI-syntax keystroke (`ctrl-p`,
/// `shift-t`, `enter`, `a`), shaped like the one Slint's winit backend
/// delivers on this platform — for the scripted render smoke test, so
/// scripted keys take the same path as real ones.
#[cfg(any(test, feature = "screenshot"))]
#[must_use]
pub fn scripted(stroke: &str, apple: bool) -> (String, Mods) {
    let mut m = Mods::default();
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    for p in parts {
        match p {
            // Slint reports Control as `meta` and Command as `control` on
            // Apple platforms.
            "ctrl" if apple => m.meta = true,
            "ctrl" => m.control = true,
            "cmd" if apple => m.control = true,
            "cmd" => m.meta = true,
            "alt" => m.alt = true,
            "shift" => m.shift = true,
            _ => {}
        }
    }
    let text = match key {
        "enter" => "\n".to_string(),
        "escape" => "\u{1b}".to_string(),
        "tab" => "\t".to_string(),
        "backspace" => "\u{8}".to_string(),
        "delete" => "\u{7f}".to_string(),
        "space" => " ".to_string(),
        "up" => "\u{F700}".to_string(),
        "down" => "\u{F701}".to_string(),
        "left" => "\u{F702}".to_string(),
        "right" => "\u{F703}".to_string(),
        k if m.shift => k.to_uppercase(),
        k => k.to_string(),
    };
    (text, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    const NONE: Mods = Mods {
        control: false,
        alt: false,
        shift: false,
        meta: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        ..NONE
    };

    fn key(text: &str, m: Mods, apple: bool) -> KeyEvent {
        key_event(text, m, apple).unwrap()
    }

    #[test]
    fn plain_typing_uses_the_typed_character() {
        assert_eq!(
            key("a", NONE, false),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
        );
        let upper = key("A", SHIFT, false);
        assert_eq!(upper.code, KeyCode::Char('A'));
        assert_eq!(upper.modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        assert_eq!(
            key("?", SHIFT, false),
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)
        );
        assert_eq!(key("é", NONE, true).code, KeyCode::Char('é'));
    }

    #[test]
    fn named_keys_map_from_slint_code_points() {
        assert_eq!(key("\n", NONE, false).code, KeyCode::Enter);
        assert_eq!(key("\u{1b}", NONE, false).code, KeyCode::Esc);
        assert_eq!(key("\u{8}", NONE, false).code, KeyCode::Backspace);
        assert_eq!(key("\u{7f}", NONE, false).code, KeyCode::Delete);
        assert_eq!(key("\u{F700}", NONE, false).code, KeyCode::Up);
        assert_eq!(key("\u{F708}", NONE, false).code, KeyCode::F(5));
        assert_eq!(key("\u{F70F}", NONE, false).code, KeyCode::F(12));
        assert_eq!(key("\u{F72D}", NONE, false).code, KeyCode::PageDown);
        assert_eq!(key("\u{F729}", NONE, false).code, KeyCode::Home);
        assert_eq!(key(" ", NONE, false).code, KeyCode::Char(' '));
        let back = key("\t", SHIFT, false);
        assert_eq!(back.code, KeyCode::BackTab);
        assert_eq!(back.modifiers, KeyModifiers::SHIFT);
        assert_eq!(key("\u{19}", NONE, false).code, KeyCode::BackTab);
        // Modifier keys and other named keys are ignored.
        assert_eq!(key_event("\u{10}", SHIFT, false), None);
        assert_eq!(key_event("\u{17}", NONE, false), None);
        assert_eq!(key_event("\u{F735}", NONE, false), None);
        assert_eq!(key_event("", NONE, false), None);
    }

    #[test]
    fn apple_swaps_command_and_control_back() {
        // Physical Control on macOS: Slint says `meta`.
        let ctrl = Mods { meta: true, ..NONE };
        assert_eq!(
            key("p", ctrl, true),
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
        );
        // Command on macOS: Slint says `control`.
        let cmd = Mods {
            control: true,
            ..NONE
        };
        assert_eq!(
            key("s", cmd, true),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SUPER)
        );
        // Elsewhere `control` is Control and `meta` is Super.
        assert_eq!(key("s", cmd, false).modifiers, KeyModifiers::CONTROL);
        assert_eq!(key("s", ctrl, false).modifiers, KeyModifiers::SUPER);
        // Ctrl+Shift+Z arrives as "Z" and stays a shifted chord.
        let redo = key(
            "Z",
            Mods {
                meta: true,
                shift: true,
                ..NONE
            },
            true,
        );
        assert_eq!(redo.code, KeyCode::Char('Z'));
        assert!(
            redo.modifiers
                .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn option_is_meta() {
        assert_eq!(
            option_chord("s", false, false, false),
            Some(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT))
        );
        let shifted = option_chord("s", true, false, false).unwrap();
        assert_eq!(shifted.code, KeyCode::Char('S'));
        assert!(shifted.modifiers.contains(KeyModifiers::ALT));
        // Off macOS, Alt+letter arrives from Slint as the plain letter.
        let alt = Mods { alt: true, ..NONE };
        assert_eq!(
            key("x", alt, false),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)
        );
    }

    #[test]
    fn scripted_strokes_round_trip() {
        for apple in [true, false] {
            let (text, m) = scripted("ctrl-p", apple);
            assert_eq!(
                key_event(&text, m, apple),
                Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
            );
            let (text, m) = scripted("shift-t", apple);
            assert_eq!(key_event(&text, m, apple).unwrap().code, KeyCode::Char('T'));
            let (text, m) = scripted("enter", apple);
            assert_eq!(key_event(&text, m, apple).unwrap().code, KeyCode::Enter);
        }
    }
}
