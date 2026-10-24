//! GDK keystrokes and IM commits → `vix-input` events, via the rules in
//! `crate::key_rules`.
//!
//! GTK delivers a key press as a keyval (whose GDK name is `Return`,
//! `Page_Up`, `a`, `slash`, …) plus a modifier state; typed text comes
//! separately, from the input method's `commit` signal. So plain typing is
//! taken from the IM commit ([`committed`]) and the key press itself only for
//! named keys and chords ([`key_press`]) — giving the same events crossterm
//! delivers in the terminal. For a chord with `Alt` held, the view resolves
//! the key *without* `Alt` first ("Option as Meta", as in the other builds),
//! so `Option+S` reaches here as `s`, not `ß`.

use gtk4::gdk::ModifierType;
use vix_input::{KeyEvent, KeyModifiers};

/// GDK modifiers as `vix-input` flags. `SUPER` and `META` (macOS Command)
/// become `SUPER`; Vix folds Command into Control on macOS itself.
#[must_use]
pub fn modifiers(state: ModifierType) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(
        KeyModifiers::CONTROL,
        state.contains(ModifierType::CONTROL_MASK),
    );
    out.set(KeyModifiers::ALT, state.contains(ModifierType::ALT_MASK));
    out.set(
        KeyModifiers::SHIFT,
        state.contains(ModifierType::SHIFT_MASK),
    );
    out.set(
        KeyModifiers::SUPER,
        state.intersects(ModifierType::SUPER_MASK | ModifierType::META_MASK),
    );
    out
}

/// Whether `mods` make a chord, whose key is taken unmodified rather than
/// typed through the input method.
#[must_use]
pub fn is_chord(mods: KeyModifiers) -> bool {
    mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
}

/// The key rules' name (`enter`, `pageup`, `f5`, `a`, `/`) for a GDK keyval
/// name and the keyval's character, if it has one. `None` for a key Vix
/// ignores (a bare modifier, a media key).
#[must_use]
pub fn key_name(gdk_name: &str, unicode: Option<char>) -> Option<String> {
    let named = match gdk_name {
        "Return" | "KP_Enter" | "ISO_Enter" => "enter",
        "Escape" => "escape",
        "BackSpace" => "backspace",
        "Delete" | "KP_Delete" => "delete",
        "Insert" | "KP_Insert" => "insert",
        "Tab" | "ISO_Left_Tab" | "KP_Tab" => "tab",
        "Left" | "KP_Left" => "left",
        "Right" | "KP_Right" => "right",
        "Up" | "KP_Up" => "up",
        "Down" | "KP_Down" => "down",
        "Home" | "KP_Home" => "home",
        "End" | "KP_End" => "end",
        "Page_Up" | "Prior" | "KP_Page_Up" | "KP_Prior" => "pageup",
        "Page_Down" | "Next" | "KP_Page_Down" | "KP_Next" => "pagedown",
        "space" | "KP_Space" => "space",
        _ => {
            if let Some(n) = gdk_name.strip_prefix('F')
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                return Some(format!("f{n}"));
            }
            let c = unicode.filter(|c| !c.is_control())?;
            return Some(c.to_ascii_lowercase().to_string());
        }
    };
    Some(named.to_string())
}

/// A key press the input method did not take — a named key or a chord — as a
/// `vix-input` key press. `gdk_name`/`unicode` describe the keyval (already
/// resolved without `Alt` for an `Alt` chord); `state` is the event's
/// modifier state.
#[must_use]
pub fn key_press(gdk_name: &str, unicode: Option<char>, state: ModifierType) -> Option<KeyEvent> {
    let name = key_name(gdk_name, unicode)?;
    let mut mods = modifiers(state);
    // GDK reports Shift+Tab as its own keyval; make sure SHIFT is set.
    if gdk_name == "ISO_Left_Tab" {
        mods |= KeyModifiers::SHIFT;
    }
    let typed = unicode.filter(|c| !c.is_control()).map(String::from);
    crate::key_rules::key_event(&name, typed.as_deref(), mods)
}

/// The key presses for text the input method committed; `held` is the
/// modifier state of the key press that produced it.
#[must_use]
pub fn committed(text: &str, held: KeyModifiers) -> Vec<KeyEvent> {
    let mods = held & KeyModifiers::SHIFT;
    text.chars()
        .filter_map(|c| {
            let s = c.to_string();
            crate::key_rules::key_event(&s, Some(&s), mods)
        })
        .collect()
}

/// The key presses for a GPUI-syntax keystroke (`ctrl-p`, `shift-t`,
/// `enter`, `a`), taking the path a real one takes: a plain character as an
/// IM commit, anything else as a GDK key press (GDK names and modifier
/// flags) — for the scripted render smoke test.
#[cfg(any(test, feature = "screenshot"))]
#[must_use]
pub fn scripted(stroke: &str) -> Vec<KeyEvent> {
    let mut state = ModifierType::empty();
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    for p in parts {
        match p {
            "ctrl" => state |= ModifierType::CONTROL_MASK,
            "alt" => state |= ModifierType::ALT_MASK,
            "shift" => state |= ModifierType::SHIFT_MASK,
            "cmd" => state |= ModifierType::META_MASK,
            _ => {}
        }
    }
    let shift = state.contains(ModifierType::SHIFT_MASK);
    let gdk_name = match key {
        "enter" => "Return".to_string(),
        "escape" => "Escape".to_string(),
        "tab" if shift => "ISO_Left_Tab".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "BackSpace".to_string(),
        "delete" => "Delete".to_string(),
        "space" => "space".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "pageup" => "Page_Up".to_string(),
        "pagedown" => "Page_Down".to_string(),
        f if f.len() > 1 && f.starts_with('f') && f[1..].bytes().all(|b| b.is_ascii_digit()) => {
            format!("F{}", &f[1..])
        }
        other => other.to_string(),
    };
    let mut chars = key.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(if shift { c.to_ascii_uppercase() } else { c }),
        _ => None,
    };
    if let Some(c) = single
        && !is_chord(modifiers(state))
    {
        return committed(&c.to_string(), modifiers(state));
    }
    key_press(&gdk_name, single, state).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    const NONE: ModifierType = ModifierType::empty();

    #[test]
    fn modifier_flags() {
        let m = ModifierType::CONTROL_MASK | ModifierType::META_MASK;
        assert_eq!(modifiers(m), KeyModifiers::CONTROL | KeyModifiers::SUPER);
        assert_eq!(modifiers(ModifierType::SUPER_MASK), KeyModifiers::SUPER);
        assert!(is_chord(modifiers(ModifierType::ALT_MASK)));
        assert!(!is_chord(modifiers(ModifierType::SHIFT_MASK)));
    }

    #[test]
    fn gdk_names_map_to_key_rule_names() {
        assert_eq!(key_name("Return", Some('\r')).as_deref(), Some("enter"));
        assert_eq!(key_name("KP_Enter", None).as_deref(), Some("enter"));
        assert_eq!(key_name("Page_Up", None).as_deref(), Some("pageup"));
        assert_eq!(key_name("F12", None).as_deref(), Some("f12"));
        assert_eq!(key_name("slash", Some('/')).as_deref(), Some("/"));
        assert_eq!(key_name("P", Some('P')).as_deref(), Some("p"));
        // "F" alone is the letter, not a function key.
        assert_eq!(key_name("F", Some('F')).as_deref(), Some("f"));
        // Bare modifiers are ignored.
        assert_eq!(key_name("Shift_L", None), None);
        assert_eq!(key_name("Control_L", None), None);
    }

    #[test]
    fn named_keys_and_chords() {
        assert_eq!(
            key_press("Return", Some('\r'), NONE).map(|k| k.code),
            Some(KeyCode::Enter)
        );
        assert_eq!(
            key_press("p", Some('p'), ModifierType::CONTROL_MASK),
            Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        );
        // Ctrl+Shift+Z arrives as keyval `Z`.
        let redo = key_press(
            "Z",
            Some('Z'),
            ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK,
        );
        assert_eq!(
            redo,
            Some(KeyEvent::new(
                KeyCode::Char('Z'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ))
        );
        // Option as Meta: the view resolved the key without Alt.
        assert_eq!(
            key_press("s", Some('s'), ModifierType::ALT_MASK),
            Some(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT))
        );
        let back = key_press("ISO_Left_Tab", None, ModifierType::SHIFT_MASK);
        assert_eq!(back.map(|k| k.code), Some(KeyCode::BackTab));
        assert_eq!(
            key_press("F5", None, NONE).map(|k| k.code),
            Some(KeyCode::F(5))
        );
    }

    #[test]
    fn commits_type_like_text() {
        assert_eq!(
            committed("a", KeyModifiers::NONE),
            [KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)]
        );
        let upper = committed("A", KeyModifiers::SHIFT);
        assert_eq!(upper[0].modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        assert_eq!(
            committed("?", KeyModifiers::SHIFT),
            [KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)]
        );
        assert_eq!(committed("日本", KeyModifiers::NONE).len(), 2);
    }

    #[test]
    fn scripted_strokes_take_the_real_paths() {
        assert_eq!(
            scripted("ctrl-p"),
            [KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)]
        );
        assert_eq!(scripted("shift-t")[0].code, KeyCode::Char('T'));
        assert_eq!(scripted("enter")[0].code, KeyCode::Enter);
        assert_eq!(scripted("shift-tab")[0].code, KeyCode::BackTab);
        assert_eq!(
            scripted("c"),
            [KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)]
        );
    }
}
