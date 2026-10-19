//! Light/dark terminal background detection for `theme = "auto"` (T579).
//!
//! The terminal I/O lives in the binary; this module is the pure part: parsing
//! an OSC 11 reply or `COLORFGBG`, classifying the colour, remembering the
//! answer for the session, and turning `"auto"` into a concrete theme name.

use std::sync::OnceLock;

/// Whether the terminal's background is dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    /// A dark background.
    Dark,
    /// A light background.
    Light,
}

/// The `theme` setting value that means "follow the terminal".
pub const AUTO: &str = "auto";

static DETECTED: OnceLock<Option<Background>> = OnceLock::new();

/// Remember the detected background for this session (first call wins; `None`
/// records that detection ran and found nothing).
pub fn set_detected(background: Option<Background>) {
    let _ = DETECTED.set(background);
}

/// The background found at startup, if any.
#[must_use]
pub fn detected() -> Option<Background> {
    DETECTED.get().copied().flatten()
}

/// The concrete theme name for the `theme` setting `value`: `"auto"` becomes
/// `light` or `dark` by `background` (dark when unknown); anything else is
/// returned unchanged.
#[must_use]
pub fn resolve_theme(
    value: &str,
    light: &str,
    dark: &str,
    background: Option<Background>,
) -> String {
    if !value.eq_ignore_ascii_case(AUTO) {
        return value.to_string();
    }
    match background {
        Some(Background::Light) => light.to_string(),
        _ => dark.to_string(),
    }
}

/// Parse the colour out of an OSC 11 reply such as
/// `\x1b]11;rgb:ffff/ffff/ffff\x07`. Each component has 1–4 hex digits and is
/// scaled to a byte.
#[must_use]
pub fn parse_osc11_reply(reply: &str) -> Option<(u8, u8, u8)> {
    let rest = &reply[reply.find("rgb:")? + 4..];
    let mut parts = rest.split('/');
    let mut component = |last: bool| -> Option<u8> {
        let part = parts.next()?;
        let digits: String = part.chars().take_while(char::is_ascii_hexdigit).collect();
        // A component cut short by the end of a partial reply is not a colour.
        if digits.is_empty() || digits.len() > 4 || (!last && digits.len() != part.len()) {
            return None;
        }
        let value = u32::from_str_radix(&digits, 16).ok()?;
        let max = 16u32.pow(u32::try_from(digits.len()).ok()?) - 1;
        u8::try_from((value * 255 + max / 2) / max).ok()
    };
    Some((component(false)?, component(false)?, component(true)?))
}

/// Classify a colour by its relative luminance (Rec. 709 weights).
#[must_use]
pub fn classify(rgb: (u8, u8, u8)) -> Background {
    let (r, g, b) = rgb;
    let luma = 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b);
    if luma > 127.5 {
        Background::Light
    } else {
        Background::Dark
    }
}

/// The background implied by a `COLORFGBG` value (`"15;0"`, `"0;default;15"`):
/// the last field is the background's ANSI colour index.
#[must_use]
pub fn from_colorfgbg(value: &str) -> Option<Background> {
    let index: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
    Some(if index == 7 || (9..=15).contains(&index) {
        Background::Light
    } else {
        Background::Dark
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc11_replies_parse_at_every_component_width() {
        assert_eq!(
            parse_osc11_reply("\x1b]11;rgb:ffff/ffff/ffff\x07"),
            Some((255, 255, 255))
        );
        assert_eq!(
            parse_osc11_reply("]11;rgb:0000/0000/0000\\"),
            Some((0, 0, 0))
        );
        assert_eq!(parse_osc11_reply("rgb:ff/80/00"), Some((255, 128, 0)));
        assert_eq!(parse_osc11_reply("rgb:f/0/8"), Some((255, 0, 136)));
    }

    #[test]
    fn partial_or_garbled_replies_are_not_colours() {
        assert_eq!(parse_osc11_reply(""), None);
        assert_eq!(parse_osc11_reply("rgb:ffff/ffff"), None);
        assert_eq!(parse_osc11_reply("rgb:ffff/ff"), None);
        assert_eq!(parse_osc11_reply("rgb:zz/zz/zz"), None);
        assert_eq!(parse_osc11_reply("rgb:12345/00/00"), None);
    }

    #[test]
    fn luminance_splits_light_from_dark() {
        assert_eq!(classify((0, 0, 0)), Background::Dark);
        assert_eq!(classify((40, 40, 40)), Background::Dark);
        assert_eq!(classify((255, 255, 255)), Background::Light);
        assert_eq!(classify((253, 246, 227)), Background::Light);
        // Pure blue is dark despite a high channel value.
        assert_eq!(classify((0, 0, 255)), Background::Dark);
    }

    #[test]
    fn colorfgbg_reads_the_last_field() {
        assert_eq!(from_colorfgbg("15;0"), Some(Background::Dark));
        assert_eq!(from_colorfgbg("0;15"), Some(Background::Light));
        assert_eq!(from_colorfgbg("0;default;7"), Some(Background::Light));
        assert_eq!(from_colorfgbg("7;8"), Some(Background::Dark));
        assert_eq!(from_colorfgbg("0;default"), None);
        assert_eq!(from_colorfgbg(""), None);
    }

    #[test]
    fn auto_resolves_by_background_and_other_values_pass_through() {
        use Background::{Dark, Light};
        assert_eq!(resolve_theme("auto", "l", "d", Some(Light)), "l");
        assert_eq!(resolve_theme("AUTO", "l", "d", Some(Dark)), "d");
        assert_eq!(resolve_theme("auto", "l", "d", None), "d");
        assert_eq!(resolve_theme("nord", "l", "d", Some(Light)), "nord");
    }
}
