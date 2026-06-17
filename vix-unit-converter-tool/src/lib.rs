//! Convert a number between physical units, plus the Unit Converter dialog's
//! editing state.
//!
//! Each [`Unit`] belongs to a [`Dimension`] and is described by an affine map to
//! a per-dimension base unit: `base = value * factor + offset`. Linear units
//! (metres, grams, …) have `offset == 0`; temperatures use the offset so Celsius,
//! Fahrenheit and Kelvin all reduce to the same affine form. [`convert`] maps a
//! value from one unit to another, returning `None` across incompatible
//! dimensions (e.g. metres to kilograms).
//!
//! The base units are: metre (length), gram (mass), kelvin (temperature), byte
//! (data) and second (time).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// A measurable quantity. Conversions are only defined within one dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// Length (base: metre).
    Length,
    /// Mass (base: gram).
    Mass,
    /// Temperature (base: kelvin).
    Temperature,
    /// Digital data size (base: byte).
    Data,
    /// Time (base: second).
    Time,
}

/// A unit of measure: a display label, its dimension, and the affine map
/// `base = value * factor + offset` to its dimension's base unit.
pub struct Unit {
    /// Display label, e.g. `"km"` or `"°C"`.
    pub label: &'static str,
    /// The dimension this unit measures.
    pub dimension: Dimension,
    /// Multiplier toward the base unit.
    factor: f64,
    /// Additive offset toward the base unit (nonzero only for temperatures).
    offset: f64,
}

impl Unit {
    /// Value expressed in this unit, converted to the dimension's base unit.
    fn to_base(&self, value: f64) -> f64 {
        value * self.factor + self.offset
    }

    /// A base-unit value converted back into this unit.
    fn of_base(&self, base: f64) -> f64 {
        (base - self.offset) / self.factor
    }
}

/// All supported units, grouped by dimension in display order.
pub static UNITS: &[Unit] = &[
    // Length (base: metre).
    Unit { label: "mm", dimension: Dimension::Length, factor: 0.001, offset: 0.0 },
    Unit { label: "cm", dimension: Dimension::Length, factor: 0.01, offset: 0.0 },
    Unit { label: "m", dimension: Dimension::Length, factor: 1.0, offset: 0.0 },
    Unit { label: "km", dimension: Dimension::Length, factor: 1000.0, offset: 0.0 },
    Unit { label: "in", dimension: Dimension::Length, factor: 0.0254, offset: 0.0 },
    Unit { label: "ft", dimension: Dimension::Length, factor: 0.3048, offset: 0.0 },
    Unit { label: "yd", dimension: Dimension::Length, factor: 0.9144, offset: 0.0 },
    Unit { label: "mi", dimension: Dimension::Length, factor: 1609.344, offset: 0.0 },
    // Mass (base: gram).
    Unit { label: "mg", dimension: Dimension::Mass, factor: 0.001, offset: 0.0 },
    Unit { label: "g", dimension: Dimension::Mass, factor: 1.0, offset: 0.0 },
    Unit { label: "kg", dimension: Dimension::Mass, factor: 1000.0, offset: 0.0 },
    Unit { label: "oz", dimension: Dimension::Mass, factor: 28.349_523_125, offset: 0.0 },
    Unit { label: "lb", dimension: Dimension::Mass, factor: 453.592_37, offset: 0.0 },
    // Temperature (base: kelvin).
    Unit { label: "°C", dimension: Dimension::Temperature, factor: 1.0, offset: 273.15 },
    Unit { label: "°F", dimension: Dimension::Temperature, factor: 5.0 / 9.0, offset: 459.67 * 5.0 / 9.0 },
    Unit { label: "K", dimension: Dimension::Temperature, factor: 1.0, offset: 0.0 },
    // Data (base: byte).
    Unit { label: "B", dimension: Dimension::Data, factor: 1.0, offset: 0.0 },
    Unit { label: "KB", dimension: Dimension::Data, factor: 1_000.0, offset: 0.0 },
    Unit { label: "MB", dimension: Dimension::Data, factor: 1_000_000.0, offset: 0.0 },
    Unit { label: "GB", dimension: Dimension::Data, factor: 1_000_000_000.0, offset: 0.0 },
    Unit { label: "KiB", dimension: Dimension::Data, factor: 1024.0, offset: 0.0 },
    Unit { label: "MiB", dimension: Dimension::Data, factor: 1_048_576.0, offset: 0.0 },
    Unit { label: "GiB", dimension: Dimension::Data, factor: 1_073_741_824.0, offset: 0.0 },
    // Time (base: second).
    Unit { label: "ms", dimension: Dimension::Time, factor: 0.001, offset: 0.0 },
    Unit { label: "s", dimension: Dimension::Time, factor: 1.0, offset: 0.0 },
    Unit { label: "min", dimension: Dimension::Time, factor: 60.0, offset: 0.0 },
    Unit { label: "h", dimension: Dimension::Time, factor: 3600.0, offset: 0.0 },
    Unit { label: "day", dimension: Dimension::Time, factor: 86400.0, offset: 0.0 },
];

/// Convert `value` from unit index `from` to unit index `to` (indices into
/// [`UNITS`]). Returns `None` when the indices are out of range or name
/// incompatible dimensions.
#[must_use]
pub fn convert(value: f64, from: usize, to: usize) -> Option<f64> {
    let from = UNITS.get(from)?;
    let to = UNITS.get(to)?;
    if from.dimension != to.dimension {
        return None;
    }
    Some(to.of_base(from.to_base(value)))
}

/// Which dialog element has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The number-entry field.
    Value,
    /// The input-unit selector.
    From,
    /// The output-unit selector.
    To,
}

/// The Unit Converter dialog's editing state: the typed number, the chosen input
/// and output units (indices into [`UNITS`]), and the focused element.
#[derive(Debug, Clone)]
pub struct Converter {
    /// The number the user typed (kept as text so partial input like `"1."` is OK).
    pub value: String,
    /// Index into [`UNITS`] of the input unit.
    pub from: usize,
    /// Index into [`UNITS`] of the output unit.
    pub to: usize,
    /// The focused element.
    pub focus: Focus,
}

impl Default for Converter {
    fn default() -> Self {
        Self::new()
    }
}

impl Converter {
    /// A fresh converter: `1` metre → kilometres, focused on the value field.
    #[must_use]
    pub fn new() -> Self {
        // Default to the first two length units (m → km look-up by label).
        let m = unit_index("m").unwrap_or(0);
        let km = unit_index("km").unwrap_or(0);
        Converter { value: "1".to_string(), from: m, to: km, focus: Focus::Value }
    }

    /// Move focus to the next element (Value → From → To → Value).
    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Value => Focus::From,
            Focus::From => Focus::To,
            Focus::To => Focus::Value,
        };
    }

    /// Move focus to the previous element.
    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Focus::Value => Focus::To,
            Focus::From => Focus::Value,
            Focus::To => Focus::From,
        };
    }

    /// Append a character to the value field (digits, sign, decimal point only).
    pub fn push(&mut self, c: char) {
        if c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E') {
            self.value.push(c);
        }
    }

    /// Delete the last character of the value field.
    pub fn backspace(&mut self) {
        self.value.pop();
    }

    /// Cycle the focused selector by `delta` (wrapping); a no-op when the value
    /// field is focused.
    pub fn cycle(&mut self, delta: i32) {
        let slot = match self.focus {
            Focus::From => &mut self.from,
            Focus::To => &mut self.to,
            Focus::Value => return,
        };
        let n = UNITS.len() as i32;
        *slot = (((*slot as i32 + delta) % n + n) % n) as usize;
    }

    /// The converted output value, if the typed number parses and the units are
    /// compatible.
    #[must_use]
    pub fn output(&self) -> Option<f64> {
        let v: f64 = self.value.trim().parse().ok()?;
        convert(v, self.from, self.to)
    }

    /// The output formatted for display: a trimmed number, or `"—"` when the
    /// input is incomplete or the dimensions are incompatible.
    #[must_use]
    pub fn output_text(&self) -> String {
        self.output().map_or_else(|| "—".to_string(), format_number)
    }

    /// The text inserted into the editor: `"<output> <unit>"`, or empty when
    /// there is no valid output.
    #[must_use]
    pub fn insert_text(&self) -> String {
        match self.output() {
            Some(v) => format!("{} {}", format_number(v), UNITS[self.to].label),
            None => String::new(),
        }
    }
}

/// Index of the unit with the given label.
fn unit_index(label: &str) -> Option<usize> {
    UNITS.iter().position(|u| u.label == label)
}

/// Format a number without a trailing `.0`, trimming float noise to 6 decimals.
fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.6}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(label: &str) -> usize {
        unit_index(label).unwrap()
    }

    #[test]
    fn length_conversions() {
        assert_eq!(convert(1.0, idx("km"), idx("m")), Some(1000.0));
        assert_eq!(convert(100.0, idx("cm"), idx("m")), Some(1.0));
        let mi = convert(1.0, idx("mi"), idx("km")).unwrap();
        assert!((mi - 1.609344).abs() < 1e-9);
    }

    #[test]
    fn temperature_conversions() {
        assert!((convert(100.0, idx("°C"), idx("°F")).unwrap() - 212.0).abs() < 1e-9);
        assert!((convert(32.0, idx("°F"), idx("°C")).unwrap() - 0.0).abs() < 1e-9);
        assert!((convert(0.0, idx("°C"), idx("K")).unwrap() - 273.15).abs() < 1e-9);
    }

    #[test]
    fn incompatible_dimensions_are_none() {
        assert_eq!(convert(1.0, idx("m"), idx("kg")), None);
    }

    #[test]
    fn dialog_output_and_insert() {
        let mut c = Converter::new(); // 1 m → km
        assert_eq!(c.value, "1");
        assert_eq!(c.output_text(), "0.001");
        assert_eq!(c.insert_text(), "0.001 km");
        // Switch the output unit to metres: 1 m → 1 m.
        c.focus = Focus::To;
        c.to = idx("m");
        assert_eq!(c.output_text(), "1");
        assert_eq!(c.insert_text(), "1 m");
    }

    #[test]
    fn incomplete_input_shows_dash() {
        let mut c = Converter::new();
        c.value.clear();
        assert_eq!(c.output_text(), "—");
        assert_eq!(c.insert_text(), "");
    }

    #[test]
    fn cycle_wraps_within_units() {
        let mut c = Converter::new();
        c.focus = Focus::From;
        c.from = 0;
        c.cycle(-1);
        assert_eq!(c.from, UNITS.len() - 1);
        c.cycle(1);
        assert_eq!(c.from, 0);
    }
}
