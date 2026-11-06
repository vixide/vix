//! BUI keystrokes → `vix-input` events, via the rules in `crate::key_rules`.
//!
//! BUI reports a key the way Tk does: a keysym (`Return`, `BackSpace`,
//! `Prior`, `F5`, or the character the key makes ignoring every modifier but
//! Shift: `a`, `P`, `/`) plus the text the layout typed (empty for chords and
//! named keys) and Shift/Control/Alt flags. On macOS the kit folds Command
//! into Control, which is what Vix does with it anyway, so `Cmd+S` arrives as
//! `Ctrl+S`. Modifier presses arrive as keys of their own (`Shift_L` ...) and
//! are ignored.

use vix_input::{KeyEvent, KeyModifiers};

/// The modifiers BUI reports (`Shift`, `Control` — Command included on macOS —
/// and `Alt`/Option) as `vix-input` flags.
#[must_use]
pub fn modifiers(shift: bool, control: bool, alt: bool) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::SHIFT, shift);
    out.set(KeyModifiers::CONTROL, control);
    out.set(KeyModifiers::ALT, alt);
    out
}

/// The key rules' name for a BUI keysym (`enter`, `pageup`, `f5`, `a`, `/`),
/// or `None` for a key Vix has no use for (a lone modifier, Caps Lock ...).
#[must_use]
pub fn key_name(keysym: &str) -> Option<String> {
    let named = match keysym {
        "Return" | "KP_Enter" => "enter",
        "BackSpace" => "backspace",
        "Delete" | "KP_Delete" => "delete",
        "Escape" => "escape",
        "Tab" | "ISO_Left_Tab" => "tab",
        "Insert" | "KP_Insert" => "insert",
        "Home" | "KP_Home" => "home",
        "End" | "KP_End" => "end",
        "Prior" | "KP_Prior" => "pageup",
        "Next" | "KP_Next" => "pagedown",
        "Left" | "KP_Left" => "left",
        "Right" | "KP_Right" => "right",
        "Up" | "KP_Up" => "up",
        "Down" | "KP_Down" => "down",
        "space" => "space",
        "minus" | "KP_Subtract" => "-",
        "KP_Add" => "+",
        "KP_Multiply" => "*",
        "KP_Divide" => "/",
        "KP_Decimal" => ".",
        "KP_Equal" => "=",
        _ => return other_key_name(keysym),
    };
    Some(named.to_string())
}

/// Function keys, keypad digits and single characters.
fn other_key_name(keysym: &str) -> Option<String> {
    if let Some(n) = keysym.strip_prefix('F')
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(format!("f{n}"));
    }
    if let Some(d) = keysym.strip_prefix("KP_")
        && d.len() == 1
        && d.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(d.to_string());
    }
    let mut chars = keysym.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // Shift_L, Caps_Lock, Meta_L, ...
    }
    Some(c.to_ascii_lowercase().to_string())
}

/// The `vix-input` key press for one BUI key-down, or `None` for a key Vix
/// ignores. `text` is what the layout typed (BUI's `key_char`).
#[must_use]
pub fn key_event(
    keysym: &str,
    text: Option<&str>,
    shift: bool,
    control: bool,
    alt: bool,
) -> Option<KeyEvent> {
    let mut mods = modifiers(shift, control, alt);
    // `ISO_Left_Tab` is X11's Shift+Tab, with or without the flag.
    if keysym == "ISO_Left_Tab" {
        mods.insert(KeyModifiers::SHIFT);
    }
    let name = key_name(keysym)?;
    // Control characters (X11 types U+0010 for Ctrl+P) are not typed text.
    let text = text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
    crate::key_rules::key_event(&name, text, mods)
}

/// A scripted keystroke in GPUI syntax (`ctrl-p`, `shift-t`, `alt-x`, `enter`,
/// `a`) as the BUI key-down it stands for: keysym, typed text, and the
/// Shift / Control / Alt flags. For the scripted render run, so scripted keys
/// take the same path as real ones.
#[cfg(any(test, feature = "screenshot"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scripted {
    /// BUI's keysym.
    pub keysym: String,
    /// The text the layout would type (empty for chords and named keys).
    pub text: String,
    /// Shift held.
    pub shift: bool,
    /// Control (or Command) held.
    pub control: bool,
    /// Alt (Option) held.
    pub alt: bool,
}

/// Parse a GPUI-syntax keystroke (see [`Scripted`]).
#[cfg(any(test, feature = "screenshot"))]
#[must_use]
pub fn scripted(stroke: &str) -> Scripted {
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let mut key = parts.pop().unwrap_or_default();
    if key.is_empty() {
        key = "-"; // a trailing dash is the minus key itself
    }
    let has = |m: &str| parts.contains(&m);
    let (shift, control, alt) = (has("shift"), has("ctrl") || has("cmd"), has("alt"));
    let keysym = match key {
        "enter" => "Return",
        "escape" => "Escape",
        "tab" => "Tab",
        "backspace" => "BackSpace",
        "space" => "space",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "pageup" => "Prior",
        "pagedown" => "Next",
        "home" => "Home",
        "end" => "End",
        "delete" => "Delete",
        "-" => "minus",
        other => other,
    };
    let mut chars = key.chars();
    let typed = match (chars.next(), chars.next()) {
        (Some(c), None) if !control && !alt => Some(c),
        _ if key == "space" && !control && !alt => Some(' '),
        _ => None,
    };
    let (keysym, text) = match typed {
        Some(c) if shift && c.is_ascii_lowercase() => {
            let up = c.to_ascii_uppercase();
            (up.to_string(), up.to_string())
        }
        Some(c) => (keysym.to_string(), c.to_string()),
        None => (keysym.to_string(), String::new()),
    };
    Scripted {
        keysym,
        text,
        shift,
        control,
        alt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    fn ev(keysym: &str, text: Option<&str>, s: bool, c: bool, a: bool) -> KeyEvent {
        key_event(keysym, text, s, c, a).expect("a key Vix handles")
    }

    #[test]
    fn plain_typing_uses_the_typed_text() {
        assert_eq!(
            ev("a", Some("a"), false, false, false),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
        );
        let upper = ev("A", Some("A"), true, false, false);
        assert_eq!(upper.code, KeyCode::Char('A'));
        assert_eq!(upper.modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a terminal.
        assert_eq!(
            ev("/", Some("?"), true, false, false),
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)
        );
        // An IME-style multi-byte commit of one character types it.
        assert_eq!(
            ev("a", Some("日"), false, false, false).code,
            KeyCode::Char('日')
        );
    }

    #[test]
    fn chords_use_the_key_not_the_text() {
        assert_eq!(
            ev("p", None, false, true, false),
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
        );
        // X11 reports Ctrl+P's text as a control character: not typed text.
        assert_eq!(
            ev("p", Some("\u{10}"), false, true, false).code,
            KeyCode::Char('p')
        );
        // Option as Meta: the unmodified key, not the layout's `ß`.
        assert_eq!(
            ev("s", Some("ß"), false, false, true),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT)
        );
        // Shift+Ctrl+P arrives as an uppercase key.
        let both = ev("P", None, true, true, false);
        assert_eq!(both.code, KeyCode::Char('P'));
        assert!(both.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn named_keys() {
        assert_eq!(ev("Return", None, false, false, false).code, KeyCode::Enter);
        assert_eq!(ev("Escape", None, false, false, false).code, KeyCode::Esc);
        assert_eq!(
            ev("BackSpace", None, false, false, false).code,
            KeyCode::Backspace
        );
        assert_eq!(ev("Prior", None, false, false, false).code, KeyCode::PageUp);
        assert_eq!(
            ev("Next", None, false, false, false).code,
            KeyCode::PageDown
        );
        assert_eq!(ev("F5", None, false, false, false).code, KeyCode::F(5));
        assert_eq!(ev("F12", None, false, false, false).code, KeyCode::F(12));
        assert_eq!(ev("Left", None, false, false, false).code, KeyCode::Left);
        assert_eq!(
            ev("space", Some(" "), false, false, false).code,
            KeyCode::Char(' ')
        );
        assert_eq!(
            ev("minus", Some("-"), false, true, false).code,
            KeyCode::Char('-')
        );
        assert_eq!(
            ev("KP_5", Some("5"), false, false, false).code,
            KeyCode::Char('5')
        );
    }

    #[test]
    fn shift_tab_is_backtab_either_way() {
        assert_eq!(ev("Tab", None, true, false, false).code, KeyCode::BackTab);
        assert_eq!(
            ev("ISO_Left_Tab", None, false, false, false).code,
            KeyCode::BackTab
        );
        assert_eq!(ev("Tab", None, false, false, false).code, KeyCode::Tab);
    }

    #[test]
    fn scripted_strokes_round_trip_through_the_key_rules() {
        let go = |s: &str| {
            let k = scripted(s);
            let text = (!k.text.is_empty()).then_some(k.text.as_str());
            key_event(&k.keysym, text, k.shift, k.control, k.alt).expect("handled")
        };
        assert_eq!(
            go("ctrl-p"),
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
        );
        assert_eq!(go("shift-t").code, KeyCode::Char('T'));
        assert_eq!(go("enter").code, KeyCode::Enter);
        assert_eq!(go("a").code, KeyCode::Char('a'));
        assert_eq!(go("space").code, KeyCode::Char(' '));
        assert_eq!(go("alt-x").modifiers, KeyModifiers::ALT);
        assert_eq!(go("escape").code, KeyCode::Esc);
    }

    #[test]
    fn lone_modifiers_and_unknown_keys_are_ignored() {
        for k in ["Shift_L", "Control_L", "Alt_L", "Meta_L", "Caps_Lock", ""] {
            assert!(key_event(k, None, false, false, false).is_none(), "{k}");
        }
    }
}
