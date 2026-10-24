//! GPUI keystrokes and mouse buttons → `vix-input` events, via the shared
//! rules in `vix_gui_common::keys`.

use vix_input::{KeyEvent, KeyModifiers, MouseButton};

/// A GPUI modifier state as `vix-input` modifier flags (`Cmd` / Windows /
/// Super → `SUPER`; GPUI's `fn` key has no terminal equivalent and is dropped).
#[must_use]
pub fn modifiers(m: gpui_kit::Modifiers) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    out.set(KeyModifiers::CONTROL, m.control);
    out.set(KeyModifiers::ALT, m.alt);
    out.set(KeyModifiers::SHIFT, m.shift);
    out.set(KeyModifiers::SUPER, m.platform);
    out
}

/// A GPUI keystroke as a `vix-input` key press. GPUI's key names are the
/// shared rules' own spelling, so this is a direct hand-off.
#[must_use]
pub fn key_event(ks: &gpui_kit::Keystroke) -> Option<KeyEvent> {
    vix_gui_common::keys::key_event(&ks.key, ks.key_char.as_deref(), modifiers(ks.modifiers))
}

/// A GPUI mouse button as a `vix-input` one; `None` for back/forward buttons.
#[must_use]
pub fn mouse_button(b: gpui_kit::MouseButton) -> Option<MouseButton> {
    match b {
        gpui_kit::MouseButton::Left => Some(MouseButton::Left),
        gpui_kit::MouseButton::Right => Some(MouseButton::Right),
        gpui_kit::MouseButton::Middle => Some(MouseButton::Middle),
        gpui_kit::MouseButton::Navigate(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyCode;

    #[test]
    fn modifier_flags() {
        let m = gpui_kit::Modifiers {
            control: true,
            platform: true,
            ..gpui_kit::Modifiers::default()
        };
        assert_eq!(modifiers(m), KeyModifiers::CONTROL | KeyModifiers::SUPER);
    }

    #[test]
    fn parsed_keystrokes_convert() {
        let ks = gpui_kit::Keystroke::parse("ctrl-p").unwrap();
        assert_eq!(
            key_event(&ks),
            Some(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
        );
        let ks = gpui_kit::Keystroke::parse("shift-tab").unwrap();
        assert_eq!(key_event(&ks).map(|k| k.code), Some(KeyCode::BackTab));
    }
}
