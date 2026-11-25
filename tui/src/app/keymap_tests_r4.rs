//! Round-4 behavior tests for `app/keymap.rs` (test-only; a child module so
//! it can reach the private key-token helpers and panel key handlers).

use super::*;
use crate::app::cov4::{app_at, code, ctrl, key, temp};

// ----- keymap: panel keys ----------------------------------------------

/// Help scrolling keys do nothing when the panel is already gone.
#[test]
fn help_keys_without_a_panel_are_harmless() {
    let mut a = app_at(&temp("r4-km-help"));
    assert!(a.help.is_none());
    for c in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Backspace,
    ] {
        a.help_key(code(c));
    }
    a.help_key(key('x'));
    assert!(a.help.is_none());
    // F1 and Esc close an open panel.
    a.run_action("help.shortcuts");
    assert!(a.help.is_some());
    a.help_key(code(KeyCode::F(1)));
    assert!(a.help.is_none());
}

/// Dashboard and QR boxes close on their own keys and ignore others.
#[test]
fn dashboard_and_qrcode_keys() {
    let mut a = app_at(&temp("r4-km-dash"));
    a.qrcode = Some(String::from("qr"));
    a.qrcode_key(key('x'));
    assert!(a.qrcode.is_some());
    a.qrcode_key(key('q'));
    assert!(a.qrcode.is_none());
}

/// Ctrl-modified characters get a `C-` token; with Alt only when asked, and
/// non-characters never.
#[test]
fn control_tokens_encode_modifiers() {
    let both = KeyEvent::new(
        KeyCode::Char('K'),
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT,
    );
    assert_eq!(App::ctrl_token(&both, true).as_deref(), Some("C-A-S-k"));
    assert_eq!(App::ctrl_token(&both, false).as_deref(), Some("C-S-k"));
    assert_eq!(App::ctrl_token(&ctrl('x'), false).as_deref(), Some("C-x"));
    assert_eq!(App::ctrl_token(&key('x'), false), None);
    let arrow = KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL);
    assert_eq!(App::ctrl_token(&arrow, false), None);
}

/// The shared token shape: Shift counts only for function keys.
#[test]
fn shared_tokens_gate_shift_on_function_keys() {
    let f = KeyEvent::new(KeyCode::F(5), KeyModifiers::SHIFT | KeyModifiers::ALT);
    assert_eq!(App::shared_token(&f).as_deref(), Some("A-S-F5"));
    let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
    assert_eq!(App::shared_token(&tab).as_deref(), Some("Tab"));
}

fn open_calendar(a: &mut App) {
    a.run_action("tools.calendar");
    assert!(a.visible.contains(crate::app::Visible::CALENDAR));
}

/// Plain arrows move the calendar by a day or a week, Ctrl by a month, and
/// Ctrl+Shift by a year; Esc closes it.
#[test]
fn calendar_keys_move_by_day_week_month_and_year() {
    let mut a = app_at(&temp("r4-km-cal"));
    open_calendar(&mut a);
    let start = a.calendar.selected();
    a.calendar_key(code(KeyCode::Right));
    assert_eq!(a.calendar.selected(), start.tomorrow().unwrap());
    a.calendar_key(code(KeyCode::Left));
    assert_eq!(a.calendar.selected(), start);
    a.calendar_key(code(KeyCode::Down));
    assert_eq!(
        a.calendar.selected(),
        start.checked_add(jiff::Span::new().days(7)).unwrap()
    );
    a.calendar_key(code(KeyCode::Up));
    assert_eq!(a.calendar.selected(), start);

    let month = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
    a.calendar_key(month);
    assert_ne!(a.calendar.selected().month(), start.month());
    let back = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    a.calendar_key(back);
    assert_eq!(a.calendar.selected().month(), start.month());

    let year = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    a.calendar_key(year);
    assert_eq!(a.calendar.selected().year(), start.year() + 1);

    a.calendar_key(key('x')); // ignored
    a.calendar_key(code(KeyCode::Esc));
    assert!(!a.visible.contains(crate::app::Visible::CALENDAR));
}

/// Enter in the calendar inserts the selected date at the cursor and closes.
#[test]
fn calendar_enter_inserts_the_selected_date() {
    let mut a = app_at(&temp("r4-km-cal-enter"));
    crate::app::cov4::buffer_with(&mut a, "", 0);
    open_calendar(&mut a);
    a.calendar_key(code(KeyCode::Right));
    let expected = a.calendar.selected_formatted(App::locale_date_pattern());
    a.calendar_key(code(KeyCode::Enter));
    assert!(!a.visible.contains(crate::app::Visible::CALENDAR));
    assert_eq!(crate::app::cov4::content(&mut a), expected);
}

/// The clock box moves its selection, inserts the chosen time on Enter and
/// closes on Esc or `q`; other keys are ignored.
#[test]
fn clock_keys_pick_insert_and_close() {
    let mut a = app_at(&temp("r4-km-clock"));
    crate::app::cov4::buffer_with(&mut a, "", 0);
    a.run_action("tools.clock");
    assert!(a.visible.contains(crate::app::Visible::CLOCK));
    assert_eq!(a.clock.selected, 0);
    a.clock_key(code(KeyCode::Down));
    assert_eq!(a.clock.selected, 1);
    a.clock_key(code(KeyCode::Up));
    assert_eq!(a.clock.selected, 0);
    a.clock_key(key('x'));
    assert!(a.visible.contains(crate::app::Visible::CLOCK));
    a.clock_key(code(KeyCode::Enter));
    assert!(!a.visible.contains(crate::app::Visible::CLOCK));
    assert!(
        !crate::app::cov4::content(&mut a).is_empty(),
        "a time was inserted"
    );

    for closer in [code(KeyCode::Esc), key('q')] {
        a.run_action("tools.clock");
        a.clock_key(closer);
        assert!(!a.visible.contains(crate::app::Visible::CLOCK));
    }
}

/// A plain dialog closes on Enter, Esc, Space or O and ignores the rest.
#[test]
fn a_plain_dialog_closes_on_its_own_keys_only() {
    let mut a = app_at(&temp("r4-km-dialog"));
    for closer in [
        code(KeyCode::Enter),
        code(KeyCode::Esc),
        key(' '),
        key('o'),
        key('O'),
    ] {
        a.dialog = Some(crate::app::Dialog {
            title: "About".into(),
            body: "v1".into(),
            editor: None,
        });
        a.dialog_key(key('z'));
        assert!(a.dialog.is_some(), "other keys do nothing");
        a.dialog_key(closer);
        assert!(a.dialog.is_none());
    }
}

/// The dashboard closes on Esc or Enter and ignores other keys.
#[test]
fn dashboard_closes_on_escape_or_enter() {
    let mut a = app_at(&temp("r4-km-dashboard"));
    a.dashboard_key(key('x'));
    a.dashboard_key(code(KeyCode::Esc));
    a.dashboard_key(code(KeyCode::Enter));
}

/// A key release is ignored; any other key dismisses a hover tooltip, and Esc
/// is consumed by that dismissal instead of reaching the editor.
#[test]
fn hover_is_dismissed_by_the_next_key_and_releases_are_ignored() {
    let mut a = app_at(&temp("r4-km-hover"));
    crate::app::cov4::buffer_with(&mut a, "text", 4);
    a.hover = Some(crate::app::HoverPopup { text: "tip".into() });
    let mut release = key('x');
    release.kind = vix_input::KeyEventKind::Release;
    a.on_key(release);
    assert!(a.hover.is_some(), "a release does not dismiss it");
    assert_eq!(crate::app::cov4::content(&mut a), "text");

    a.on_key(code(KeyCode::Esc));
    assert!(a.hover.is_none());

    a.hover = Some(crate::app::HoverPopup { text: "tip".into() });
    a.on_key(key('!'));
    assert!(a.hover.is_none());
    assert_eq!(
        crate::app::cov4::content(&mut a),
        "text!",
        "the key still types"
    );
}
