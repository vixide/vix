//! Parse and convert colors between HEX, RGB, and HSL.
//!
//! A [`Color`] is stored as 8-bit RGB. It parses from any of the three textual
//! forms and renders back to all three, so Vix's Color Converter dialog can keep
//! three fields in sync: edit one, reparse it, and rewrite the other two.
//!
//! Accepted inputs (case-insensitive, surrounding spaces and an optional
//! `rgb(...)` / `hsl(...)` wrapper allowed):
//! - HEX: `#RGB`, `#RRGGBB`, or the same without the leading `#`.
//! - RGB: `r, g, b` with each component 0–255.
//! - HSL: `h, s%, l%` with hue 0–360 and saturation/lightness 0–100 (the `%`
//!   signs are optional).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// An RGB color (8 bits per channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel (0–255).
    pub r: u8,
    /// Green channel (0–255).
    pub g: u8,
    /// Blue channel (0–255).
    pub b: u8,
}

impl Color {
    /// Parse a HEX color: `#RGB`, `#RRGGBB`, or either without the `#`.
    #[must_use]
    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.trim().trim_start_matches('#');
        let (r, g, b) = match h.len() {
            3 => {
                let d = |i: usize| {
                    let c = h.as_bytes()[i] as char;
                    c.to_digit(16).map(|v| (v * 17) as u8) // 0xF -> 0xFF
                };
                (d(0)?, d(1)?, d(2)?)
            }
            6 => {
                let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
                (p(0)?, p(2)?, p(4)?)
            }
            _ => return None,
        };
        Some(Color { r, g, b })
    }

    /// Parse an RGB color: `r, g, b` (each 0–255), optionally wrapped in `rgb(...)`.
    #[must_use]
    pub fn from_rgb(s: &str) -> Option<Color> {
        let parts = triple(s, "rgb")?;
        let v: Vec<u8> = parts.iter().map(|p| p.trim().parse::<u8>().ok()).collect::<Option<_>>()?;
        Some(Color { r: v[0], g: v[1], b: v[2] })
    }

    /// Parse an HSL color: `h, s%, l%` (hue 0–360, sat/light 0–100), optionally
    /// wrapped in `hsl(...)`. The `%` signs are optional.
    #[must_use]
    pub fn from_hsl(s: &str) -> Option<Color> {
        let parts = triple(s, "hsl")?;
        let h: f64 = parts[0].trim().parse().ok()?;
        let sp: f64 = parts[1].trim().trim_end_matches('%').trim().parse().ok()?;
        let lp: f64 = parts[2].trim().trim_end_matches('%').trim().parse().ok()?;
        if !(0.0..=360.0).contains(&h) || !(0.0..=100.0).contains(&sp) || !(0.0..=100.0).contains(&lp) {
            return None;
        }
        Some(hsl_to_rgb(h, sp / 100.0, lp / 100.0))
    }

    /// Render as `#RRGGBB` (lowercase).
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Render as `rgb(r, g, b)`.
    #[must_use]
    pub fn to_rgb(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Render as `hsl(h, s%, l%)` with integer components.
    #[must_use]
    pub fn to_hsl(self) -> String {
        let (h, s, l) = rgb_to_hsl(self);
        format!("hsl({}, {}%, {}%)", h.round() as i64, (s * 100.0).round() as i64, (l * 100.0).round() as i64)
    }
}

/// Split `s` into three comma-separated parts, tolerating an optional
/// `name(...)` wrapper (e.g. `rgb(...)`).
fn triple(s: &str, name: &str) -> Option<[String; 3]> {
    let mut t = s.trim().to_string();
    let lower = t.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix(name) {
        let rest = rest.trim_start();
        if rest.starts_with('(') && t.trim_end().ends_with(')') {
            let open = t.find('(')? + 1;
            let close = t.rfind(')')?;
            t = t[open..close].to_string();
        }
    }
    let parts: Vec<&str> = t.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    Some([parts[0].to_string(), parts[1].to_string(), parts[2].to_string()])
}

/// Convert HSL (hue degrees, sat/light 0–1) to an RGB [`Color`].
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color { r: to(r1), g: to(g1), b: to(b1) }
}

/// Convert an RGB [`Color`] to HSL (hue degrees, sat/light 0–1).
fn rgb_to_hsl(c: Color) -> (f64, f64, f64) {
    let r = f64::from(c.r) / 255.0;
    let g = f64::from(c.g) / 255.0;
    let b = f64::from(c.b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = f64::midpoint(max, min);
    let d = max - min;
    if d.abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() < f64::EPSILON {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if (max - g).abs() < f64::EPSILON {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h.rem_euclid(360.0), s, l)
}

/// Which of the three fields is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The `#RRGGBB` field.
    Hex,
    /// The `rgb(r, g, b)` field.
    Rgb,
    /// The `hsl(h, s%, l%)` field.
    Hsl,
}

impl Field {
    /// The three fields in display order.
    pub const ALL: [Field; 3] = [Field::Hex, Field::Rgb, Field::Hsl];

    /// Slot index (0–2) into [`Converter::fields`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Field::Hex => 0,
            Field::Rgb => 1,
            Field::Hsl => 2,
        }
    }

    /// Short uppercase label for the field row.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Field::Hex => "HEX",
            Field::Rgb => "RGB",
            Field::Hsl => "HSL",
        }
    }
}

/// The Color Converter dialog's editing state: the text of all three fields and
/// which one has focus. Editing the focused field reparses it and, when valid,
/// rewrites the other two — so the three views stay in sync.
#[derive(Debug, Clone)]
pub struct Converter {
    /// Field text in [`Field`] order: HEX, RGB, HSL.
    pub fields: [String; 3],
    /// The focused field.
    pub focus: Field,
}

impl Default for Converter {
    fn default() -> Self {
        Self::new()
    }
}

impl Converter {
    /// A fresh converter with empty fields, focused on the HEX field.
    #[must_use]
    pub fn new() -> Self {
        Converter { fields: [String::new(), String::new(), String::new()], focus: Field::Hex }
    }

    /// Seed every field from `color`, keeping the current focus.
    pub fn set_color(&mut self, color: Color) {
        self.fields[Field::Hex.index()] = color.to_hex();
        self.fields[Field::Rgb.index()] = color.to_rgb();
        self.fields[Field::Hsl.index()] = color.to_hsl();
    }

    /// The focused field's text.
    #[must_use]
    pub fn current(&self) -> &str {
        &self.fields[self.focus.index()]
    }

    /// Move focus to the next field (wrapping).
    pub fn focus_next(&mut self) {
        let i = (self.focus.index() + 1) % 3;
        self.focus = Field::ALL[i];
    }

    /// Move focus to the previous field (wrapping).
    pub fn focus_prev(&mut self) {
        let i = (self.focus.index() + 2) % 3;
        self.focus = Field::ALL[i];
    }

    /// Set focus directly (e.g. from a mouse click on a row).
    pub fn set_focus(&mut self, field: Field) {
        self.focus = field;
    }

    /// Append a character to the focused field and resync the others.
    pub fn push(&mut self, c: char) {
        self.fields[self.focus.index()].push(c);
        self.sync();
    }

    /// Delete the last character of the focused field and resync the others.
    pub fn backspace(&mut self) {
        self.fields[self.focus.index()].pop();
        self.sync();
    }

    /// Parse the focused field; on success rewrite the other two fields.
    fn sync(&mut self) {
        if let Some(color) = self.color() {
            for field in Field::ALL {
                if field != self.focus {
                    self.fields[field.index()] = match field {
                        Field::Hex => color.to_hex(),
                        Field::Rgb => color.to_rgb(),
                        Field::Hsl => color.to_hsl(),
                    };
                }
            }
        }
    }

    /// The color currently described by the focused field, if it parses.
    #[must_use]
    pub fn color(&self) -> Option<Color> {
        let text = self.current();
        match self.focus {
            Field::Hex => Color::from_hex(text),
            Field::Rgb => Color::from_rgb(text),
            Field::Hsl => Color::from_hsl(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color { r: 255, g: 0, b: 0 };

    #[test]
    fn parses_hex_long_and_short() {
        assert_eq!(Color::from_hex("#ff0000"), Some(RED));
        assert_eq!(Color::from_hex("f00"), Some(RED));
        assert_eq!(Color::from_hex("#FFF"), Some(Color { r: 255, g: 255, b: 255 }));
        assert_eq!(Color::from_hex("#zz0000"), None);
        assert_eq!(Color::from_hex("#ff00"), None);
    }

    #[test]
    fn parses_rgb() {
        assert_eq!(Color::from_rgb("255, 0, 0"), Some(RED));
        assert_eq!(Color::from_rgb("rgb(255, 0, 0)"), Some(RED));
        assert_eq!(Color::from_rgb("256, 0, 0"), None);
        assert_eq!(Color::from_rgb("1, 2"), None);
    }

    #[test]
    fn parses_hsl() {
        assert_eq!(Color::from_hsl("0, 100%, 50%"), Some(RED));
        assert_eq!(Color::from_hsl("hsl(120, 100%, 50%)"), Some(Color { r: 0, g: 255, b: 0 }));
        assert_eq!(Color::from_hsl("0, 0, 100"), Some(Color { r: 255, g: 255, b: 255 }));
        assert_eq!(Color::from_hsl("400, 100%, 50%"), None);
    }

    #[test]
    fn renders_all_forms() {
        assert_eq!(RED.to_hex(), "#ff0000");
        assert_eq!(RED.to_rgb(), "rgb(255, 0, 0)");
        assert_eq!(RED.to_hsl(), "hsl(0, 100%, 50%)");
    }

    #[test]
    fn editing_hex_syncs_other_fields() {
        let mut c = Converter::new();
        for ch in "#ff0000".chars() {
            c.push(ch);
        }
        assert_eq!(c.fields[Field::Rgb.index()], "rgb(255, 0, 0)");
        assert_eq!(c.fields[Field::Hsl.index()], "hsl(0, 100%, 50%)");
        assert_eq!(c.color(), Some(RED));
    }

    #[test]
    fn focus_cycles_and_backspace_edits() {
        let mut c = Converter::new();
        c.focus_next();
        assert_eq!(c.focus, Field::Rgb);
        c.focus_prev();
        assert_eq!(c.focus, Field::Hex);
        c.push('a');
        c.push('b');
        c.backspace();
        assert_eq!(c.current(), "a");
    }

    #[test]
    fn round_trips_through_hsl() {
        for c in [RED, Color { r: 18, g: 52, b: 86 }, Color { r: 200, g: 200, b: 50 }] {
            let back = Color::from_hsl(&c.to_hsl()).unwrap();
            // HSL rounding can shift each channel by a small amount.
            assert!((i32::from(back.r) - i32::from(c.r)).unsigned_abs() <= 3);
            assert!((i32::from(back.g) - i32::from(c.g)).unsigned_abs() <= 3);
            assert!((i32::from(back.b) - i32::from(c.b)).unsigned_abs() <= 3);
        }
    }
}
