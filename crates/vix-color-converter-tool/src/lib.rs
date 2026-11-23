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
//!
//! The conversions are pure functions over integers and `f64`; float-to-int
//! conversion is done without `as` casts (see `floor_to_u32`) to satisfy the
//! workspace's pedantic lints. Round trips through HSL are lossy because HSL is
//! rendered with integer components. Spec:
//! `crates/vix-color-converter-tool/spec/index.md`.
//!
//! # Examples
//! ```
//! use vix_color_converter_tool::Color;
//!
//! let c = Color::from_hex("#f00").unwrap();
//! assert_eq!(c.to_rgb(), "rgb(255, 0, 0)");
//! assert_eq!(c.to_hsl(), "hsl(0, 100%, 50%)");
//! assert_eq!(Color::from_hsl("120, 100%, 50%").unwrap().to_hex(), "#00ff00");
//! ```

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

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
    ///
    /// Returns `None` for any other length or a non-hex digit. Leading and
    /// trailing whitespace is ignored; every leading `#` is stripped. The
    /// three-digit form doubles each digit (`#f80` is `#ff8800`).
    ///
    /// Non-ASCII input is `None`, whatever its byte length.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Color> {
        let hex = text.trim().trim_start_matches('#');
        let (red, green, blue) = match hex.len() {
            3 => {
                let digit = |ch: char| {
                    // Short form: a digit `d` means `dd`, i.e. d * 0x11 = d * 17.
                    ch.to_digit(16)
                        .and_then(|v| v.checked_mul(17))
                        .and_then(|v| u8::try_from(v).ok()) // 0xF -> 0xFF
                };
                let mut chars = hex.chars();
                (
                    digit(chars.next()?)?,
                    digit(chars.next()?)?,
                    digit(chars.next()?)?,
                )
            }
            6 => {
                let pair = |from: usize, to: usize| u8::from_str_radix(hex.get(from..to)?, 16).ok();
                (pair(0, 2)?, pair(2, 4)?, pair(4, 6)?)
            }
            _ => return None,
        };
        Some(Color {
            r: red,
            g: green,
            b: blue,
        })
    }

    /// Parse an RGB color: `r, g, b` (each 0–255), optionally wrapped in `rgb(...)`.
    ///
    /// Components must be exactly three decimal integers that fit in a `u8`;
    /// anything else (floats, percentages, 256) yields `None`.
    #[must_use]
    pub fn from_rgb(s: &str) -> Option<Color> {
        let [red, green, blue] = triple(s, "rgb")?;
        let byte = |part: &str| part.trim().parse::<u8>().ok();
        Some(Color {
            r: byte(&red)?,
            g: byte(&green)?,
            b: byte(&blue)?,
        })
    }

    /// Parse an HSL color: `h, s%, l%` (hue 0–360, sat/light 0–100), optionally
    /// wrapped in `hsl(...)`. The `%` signs are optional.
    ///
    /// Components may be fractional; out-of-range values yield `None` rather
    /// than being clamped. Hue 360 is accepted and equals hue 0.
    #[must_use]
    pub fn from_hsl(s: &str) -> Option<Color> {
        let parts = triple(s, "hsl")?;
        let h: f64 = parts[0].trim().parse().ok()?;
        let sp: f64 = parts[1].trim().trim_end_matches('%').trim().parse().ok()?;
        let lp: f64 = parts[2].trim().trim_end_matches('%').trim().parse().ok()?;
        if !(0.0..=360.0).contains(&h)
            || !(0.0..=100.0).contains(&sp)
            || !(0.0..=100.0).contains(&lp)
        {
            return None;
        }
        Some(hsl_to_rgb(h, sp / 100.0, lp / 100.0))
    }

    /// Render as `#RRGGBB` (lowercase).
    ///
    /// Feeding the result back to [`Color::from_hex`] always round-trips.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Render as `rgb(r, g, b)`.
    #[must_use]
    pub fn to_rgb(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Render as `hsl(h, s%, l%)` with integer components (rounded to nearest, so
    /// converting back may differ from the original by a channel step or two).
    #[must_use]
    pub fn to_hsl(self) -> String {
        let (hue, sat, light) = rgb_to_hsl(self);
        format!(
            "hsl({}, {}%, {}%)",
            round_bounded(hue),
            round_bounded(sat * 100.0),
            round_bounded(light * 100.0)
        )
    }
}

/// Floor of a non-negative, finite `value` as a `u32`, computed without a
/// lossy float-to-int `as` cast. Builds the result bit by bit (most significant
/// first), keeping each bit only while the running total stays `<= value`.
/// Saturates at [`u32::MAX`].
fn floor_to_u32(value: f64) -> u32 {
    // Anything not strictly positive (0.0, -0.0, negatives, NaN) floors to 0.
    if value.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return 0;
    }
    if value >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    let mut result: u32 = 0;
    let mut bit: u32 = 1 << 31;
    while bit != 0 {
        let candidate = result | bit;
        if f64::from(candidate) <= value {
            result = candidate;
        }
        bit >>= 1;
    }
    result
}

/// Round a small non-negative bounded value (hue 0–360, percent 0–100) to the
/// nearest integer, without a lossy float-to-int `as` cast.
fn round_bounded(value: f64) -> u32 {
    floor_to_u32(value.round())
}

/// Split `s` into three comma-separated parts, tolerating an optional
/// `name(...)` wrapper (e.g. `rgb(...)`).
fn triple(s: &str, name: &str) -> Option<[String; 3]> {
    let mut t = s.trim().to_string();
    let lower = t.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix(name) {
        let rest = rest.trim_start();
        if rest.starts_with('(') && t.trim_end().ends_with(')') {
            let open = t.find('(')?.checked_add(1)?;
            let close = t.rfind(')')?;
            t = t.get(open..close)?.to_string();
        }
    }
    let mut parts = t.split(',');
    let (first, second, third) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some([first.to_string(), second.to_string(), third.to_string()])
}

/// Convert HSL (hue degrees, sat/light 0–1) to an RGB [`Color`].
fn hsl_to_rgb(hue: f64, sat: f64, light: f64) -> Color {
    let chroma = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    // Standard HSL→RGB: `sector` is which of the six 60° wedges the hue is in,
    // `chroma` the color intensity, `mid` the intermediate channel's value.
    let sector = hue / 60.0;
    let mid = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    // Truncate toward zero to pick the 60-degree sector (matches `as u32`).
    let (red1, green1, blue1) = match floor_to_u32(sector) {
        0 => (chroma, mid, 0.0),
        1 => (mid, chroma, 0.0),
        2 => (0.0, chroma, mid),
        3 => (0.0, mid, chroma),
        4 => (mid, 0.0, chroma),
        _ => (chroma, 0.0, mid),
    };
    let base = light - chroma / 2.0;
    let to_byte = |channel: f64| {
        let scaled = ((channel + base) * 255.0).round().clamp(0.0, 255.0);
        u8::try_from(floor_to_u32(scaled)).unwrap_or(255)
    };
    Color {
        r: to_byte(red1),
        g: to_byte(green1),
        b: to_byte(blue1),
    }
}

/// Convert an RGB [`Color`] to HSL (hue degrees, sat/light 0–1).
fn rgb_to_hsl(color: Color) -> (f64, f64, f64) {
    let red = f64::from(color.r) / 255.0;
    let green = f64::from(color.g) / 255.0;
    let blue = f64::from(color.b) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let light = f64::midpoint(max, min);
    let delta = max - min;
    // Achromatic (grey): hue is undefined, report 0 with zero saturation.
    if delta.abs() < f64::EPSILON {
        return (0.0, 0.0, light);
    }
    let sat = delta / (1.0 - (2.0 * light - 1.0).abs());
    // The hue wedge depends on which channel is the maximum; the comparisons
    // use EPSILON because the channels come from exact u8 / 255 divisions.
    let hue = if (max - red).abs() < f64::EPSILON {
        60.0 * (((green - blue) / delta).rem_euclid(6.0))
    } else if (max - green).abs() < f64::EPSILON {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), sat, light)
}

/// Which of the three fields is being edited.
///
/// The discriminant order (Hex, Rgb, Hsl) matches [`Converter::fields`] and
/// [`Field::ALL`].
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

    /// The next field in display order, wrapping from the last to the first.
    #[must_use]
    pub fn next(self) -> Field {
        match self {
            Field::Hex => Field::Rgb,
            Field::Rgb => Field::Hsl,
            Field::Hsl => Field::Hex,
        }
    }

    /// The previous field in display order, wrapping from the first to the last.
    #[must_use]
    pub fn previous(self) -> Field {
        match self {
            Field::Hex => Field::Hsl,
            Field::Rgb => Field::Hex,
            Field::Hsl => Field::Rgb,
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
///
/// While the focused text does not parse (mid-typing), the other two fields are
/// left as they were, so a half-typed value never blanks them.
///
/// # Examples
/// ```
/// use vix_color_converter_tool::Converter;
///
/// let mut c = Converter::new();
/// for ch in "#00f".chars() {
///     c.push(ch);
/// }
/// assert_eq!(c.fields[1], "rgb(0, 0, 255)");
/// ```
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
        Converter {
            fields: [String::new(), String::new(), String::new()],
            focus: Field::Hex,
        }
    }

    /// Seed every field from `color`, keeping the current focus (including the
    /// focused one, which is overwritten in its canonical form).
    pub fn set_color(&mut self, color: Color) {
        self.fields = [color.to_hex(), color.to_rgb(), color.to_hsl()];
    }

    /// The text of `field`.
    fn field(&self, field: Field) -> &str {
        let [hex, rgb, hsl] = &self.fields;
        match field {
            Field::Hex => hex,
            Field::Rgb => rgb,
            Field::Hsl => hsl,
        }
    }

    /// The text of `field`, to edit.
    fn field_mut(&mut self, field: Field) -> &mut String {
        let [hex, rgb, hsl] = &mut self.fields;
        match field {
            Field::Hex => hex,
            Field::Rgb => rgb,
            Field::Hsl => hsl,
        }
    }

    /// The focused field's text.
    #[must_use]
    pub fn current(&self) -> &str {
        self.field(self.focus)
    }

    /// Move focus to the next field (wrapping).
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    /// Move focus to the previous field (wrapping).
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.previous();
    }

    /// Set focus directly (e.g. from a mouse click on a row).
    pub fn set_focus(&mut self, field: Field) {
        self.focus = field;
    }

    /// Append a character to the focused field and resync the others.
    pub fn push(&mut self, c: char) {
        self.field_mut(self.focus).push(c);
        self.sync();
    }

    /// Delete the last character of the focused field and resync the others.
    pub fn backspace(&mut self) {
        self.field_mut(self.focus).pop();
        self.sync();
    }

    /// Parse the focused field; on success rewrite the other two fields.
    fn sync(&mut self) {
        if let Some(color) = self.color() {
            for field in Field::ALL {
                if field != self.focus {
                    *self.field_mut(field) = match field {
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
        assert_eq!(
            Color::from_hex("#FFF"),
            Some(Color {
                r: 255,
                g: 255,
                b: 255
            })
        );
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
        assert_eq!(
            Color::from_hsl("hsl(120, 100%, 50%)"),
            Some(Color { r: 0, g: 255, b: 0 })
        );
        assert_eq!(
            Color::from_hsl("0, 0, 100"),
            Some(Color {
                r: 255,
                g: 255,
                b: 255
            })
        );
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
        for c in [
            RED,
            Color {
                r: 18,
                g: 52,
                b: 86,
            },
            Color {
                r: 200,
                g: 200,
                b: 50,
            },
        ] {
            let back = Color::from_hsl(&c.to_hsl()).unwrap();
            // HSL rounding can shift each channel by a small amount.
            assert!((i32::from(back.r) - i32::from(c.r)).unsigned_abs() <= 3);
            assert!((i32::from(back.g) - i32::from(c.g)).unsigned_abs() <= 3);
            assert!((i32::from(back.b) - i32::from(c.b)).unsigned_abs() <= 3);
        }
    }

    #[test]
    fn multibyte_hex_is_none_not_a_panic() {
        // Six bytes, but not six ASCII digits: the slice boundary is not a char boundary.
        assert_eq!(Color::from_hex("#éééé"), None);
        assert_eq!(Color::from_hex("#aé"), None);
        assert_eq!(Color::from_hex("é1"), None);
    }
}
