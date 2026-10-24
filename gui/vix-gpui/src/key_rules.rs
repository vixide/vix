//! The rules turning a GUI keystroke into a backend-neutral `vix-input` key
//! event. The toolkit's own key type is first mapped to the lowercase key
//! *names* used here (`enter`,
//! `escape`, `pageup`, `f5`, or the key's character such as `a` or `/`).
//!
//! The conversion reproduces what the terminal front end receives from
//! crossterm for the same physical key, so every keymap, macro and modal
//! binding written against the terminal behaves the same in the window:
//!
//! - A printable key arrives as its typed character (`'A'`, `'!'`), with
//!   `SHIFT` kept only for an ASCII uppercase letter — exactly the
//!   crossterm/legacy-terminal shape (`Char('?')` carries no `SHIFT`).
//! - With `Ctrl`, `Alt` or `Cmd` held the *unmodified* key is used (`Ctrl+C`
//!   is `Char('c')` + `CONTROL`, `Option+S` is `Char('s')` + `ALT`, not `ß`),
//!   which is how a terminal with "Option as Meta" reports it. Vix uses
//!   `Alt+letter` for menu mnemonics, so this is the useful default.
//! - `Shift+Tab` is `BackTab` + `SHIFT`, as terminals report it.
//! - `Cmd` (macOS) / the Windows or Super key becomes `SUPER`; Vix folds
//!   `Cmd` into `Ctrl` on macOS itself (`App::command_as_control`).

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a keystroke — its `key` name, the optional character the layout
/// typed (`key_char`), and the held modifiers — into a `vix-input` key press.
/// `None` for a key Vix ignores.
#[must_use]
pub fn key_event(key: &str, key_char: Option<&str>, mods: KeyModifiers) -> Option<KeyEvent> {
    let shift = mods.contains(KeyModifiers::SHIFT);
    let chord = mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    let mut modifiers = mods;
    let code = match key {
        "enter" => KeyCode::Enter,
        "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "tab" if shift => KeyCode::BackTab,
        "tab" => KeyCode::Tab,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        f if f.len() > 1 && f.starts_with('f') && f[1..].bytes().all(|b| b.is_ascii_digit()) => {
            KeyCode::F(f[1..].parse().ok()?)
        }
        _ => {
            // Plain typing uses the character the layout produced; a chord
            // uses the key itself (see the module docs).
            let typed = if chord { None } else { key_char };
            let c = single_char(typed.unwrap_or(key))?;
            let c = if shift && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            };
            if !c.is_ascii_uppercase() && !chord {
                modifiers.remove(KeyModifiers::SHIFT);
            }
            KeyCode::Char(c)
        }
    };
    Some(KeyEvent::new(code, modifiers))
}

/// The one character in `s`, or `None` when it is empty or longer.
fn single_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: KeyModifiers = KeyModifiers::NONE;
    const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
    const CTRL: KeyModifiers = KeyModifiers::CONTROL;

    fn key(k: &str, c: Option<&str>, m: KeyModifiers) -> KeyEvent {
        key_event(k, c, m).unwrap()
    }

    #[test]
    fn plain_typing_uses_the_typed_character() {
        assert_eq!(
            key("a", Some("a"), NONE),
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
        );
        let upper = key("a", Some("A"), SHIFT);
        assert_eq!(upper.code, KeyCode::Char('A'));
        assert_eq!(upper.modifiers, KeyModifiers::SHIFT);
        // Shifted punctuation carries no SHIFT, like a legacy terminal.
        let q = key("/", Some("?"), SHIFT);
        assert_eq!(q.code, KeyCode::Char('?'));
        assert_eq!(q.modifiers, KeyModifiers::NONE);
        assert_eq!(key("space", Some(" "), NONE).code, KeyCode::Char(' '));
    }

    #[test]
    fn chords_use_the_unmodified_key() {
        let c = key("c", None, CTRL);
        assert_eq!(c, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let alt = key("s", Some("ß"), KeyModifiers::ALT);
        assert_eq!(alt, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        let redo = key("z", None, CTRL | SHIFT);
        assert_eq!(redo.code, KeyCode::Char('Z'));
        assert!(redo.modifiers.contains(CTRL | SHIFT));
        let cmd = key("s", None, KeyModifiers::SUPER);
        assert_eq!(cmd, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SUPER));
    }

    #[test]
    fn named_keys() {
        assert_eq!(key("enter", Some("\n"), NONE).code, KeyCode::Enter);
        assert_eq!(key("escape", None, NONE).code, KeyCode::Esc);
        assert_eq!(key("f12", None, NONE).code, KeyCode::F(12));
        assert_eq!(key("pagedown", None, NONE).code, KeyCode::PageDown);
        let back = key("tab", Some("\t"), SHIFT);
        assert_eq!(back.code, KeyCode::BackTab);
        assert_eq!(back.modifiers, KeyModifiers::SHIFT);
        assert_eq!(key("tab", Some("\t"), NONE).code, KeyCode::Tab);
        // "f" alone is the letter, not a function key.
        assert_eq!(key("f", Some("f"), NONE).code, KeyCode::Char('f'));
    }

    #[test]
    fn unknown_multi_char_keys_are_ignored() {
        assert_eq!(key_event("back", None, NONE), None);
        assert_eq!(key_event("", None, NONE), None);
    }
}
