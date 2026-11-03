//! Convert an integer between decimal, hexadecimal, binary, and octal.
//!
//! The input number is parsed with auto-detected radix — `0x`/`0X` hex,
//! `0b`/`0B` binary, `0o`/`0O` octal, else decimal — tolerating surrounding
//! whitespace, an optional leading sign, and `_` digit separators. Each function
//! re-renders it in one base (with the conventional prefix). Used by Tools →
//! Convert → Number via `App::transform_selection_or_buffer_try`.
//!
//! Values are held as `i128`, so anything wider overflows into
//! [`BaseError::Invalid`]. Negative numbers are rendered as sign + magnitude
//! (`-0xff`), never two's complement. Owning spec:
//! `crates/vix-base-tool/spec/index.md`.

/// Why a number could not be converted.
///
/// `Display` text matches the earlier `String` errors, so the translated
/// `status.convert_failed` line is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BaseError {
    /// The input held no digits (empty, or only a sign / radix prefix).
    NoNumber,
    /// The digits are not valid in the detected base, or overflow `i128`.
    Invalid(std::num::ParseIntError),
}

impl std::fmt::Display for BaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoNumber => f.write_str("no number"),
            Self::Invalid(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for BaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoNumber => None,
            Self::Invalid(e) => Some(e),
        }
    }
}

/// Parse `s` as an integer with an auto-detected radix.
fn parse(s: &str) -> Result<i128, BaseError> {
    // `_` separators are stripped anywhere (even `1__0`), a deliberately
    // lenient choice for pasted values like `0xDEAD_BEEF`.
    let t = s.trim().replace('_', "");
    // Sign comes before the radix prefix (`-0x10`), as in Rust literals.
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(&t)),
    };
    let (radix, digits) =
        if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            (16, h)
        } else if let Some(b) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
            (2, b)
        } else if let Some(o) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
            (8, o)
        } else {
            (10, body)
        };
    if digits.is_empty() {
        return Err(BaseError::NoNumber);
    }
    // Parsing the magnitude then negating means `i128::MIN` itself overflows
    // here (reported as invalid): an accepted edge case.
    let n = i128::from_str_radix(digits, radix).map_err(BaseError::Invalid)?;
    Ok(if neg { -n } else { n })
}

/// Render with sign and `prefix`, applying `f` to the magnitude.
fn render(n: i128, prefix: &str, f: impl Fn(u128) -> String) -> String {
    let mag = n.unsigned_abs();
    let sign = if n < 0 { "-" } else { "" };
    format!("{sign}{prefix}{}", f(mag))
}

/// To decimal.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_base_tool::to_dec("0xFF").unwrap(), "255");
/// assert_eq!(vix_base_tool::to_hex("255").unwrap(), "0xff");
/// assert_eq!(vix_base_tool::to_bin("-5").unwrap(), "-0b101");
/// assert!(vix_base_tool::to_oct("nope").is_err());
/// ```
///
/// # Errors
/// Returns a [`BaseError`] when `input` is not a valid integer in any supported base.
pub fn to_dec(input: &str) -> Result<String, BaseError> {
    Ok(parse(input)?.to_string())
}

/// To hexadecimal (`0x`).
///
/// # Errors
/// Returns a [`BaseError`] when `input` is not a valid integer in any supported base.
pub fn to_hex(input: &str) -> Result<String, BaseError> {
    Ok(render(parse(input)?, "0x", |m| format!("{m:x}")))
}

/// To binary (`0b`).
///
/// # Errors
/// Returns a [`BaseError`] when `input` is not a valid integer in any supported base.
pub fn to_bin(input: &str) -> Result<String, BaseError> {
    Ok(render(parse(input)?, "0b", |m| format!("{m:b}")))
}

/// To octal (`0o`).
///
/// # Errors
/// Returns a [`BaseError`] when `input` is not a valid integer in any supported base.
pub fn to_oct(input: &str) -> Result<String, BaseError> {
    Ok(render(parse(input)?, "0o", |m| format!("{m:o}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detects_radix() {
        assert_eq!(to_dec("0xff").unwrap(), "255");
        assert_eq!(to_dec("0b1010").unwrap(), "10");
        assert_eq!(to_dec("0o17").unwrap(), "15");
        assert_eq!(to_dec("42").unwrap(), "42");
    }

    #[test]
    fn renders_each_base_with_prefix() {
        assert_eq!(to_hex("255").unwrap(), "0xff");
        assert_eq!(to_bin("10").unwrap(), "0b1010");
        assert_eq!(to_oct("15").unwrap(), "0o17");
    }

    #[test]
    fn handles_sign_and_separators() {
        assert_eq!(to_dec("-0x10").unwrap(), "-16");
        assert_eq!(to_hex("1_000").unwrap(), "0x3e8");
    }

    #[test]
    fn rejects_garbage() {
        assert!(to_dec("nope").is_err());
        assert!(to_hex("").is_err());
    }

    #[test]
    fn error_variants_and_display() {
        assert_eq!(to_dec(""), Err(BaseError::NoNumber));
        assert_eq!(to_dec("-0x"), Err(BaseError::NoNumber));
        let e = to_dec("12z").unwrap_err();
        assert!(matches!(e, BaseError::Invalid(_)));
        assert!(std::error::Error::source(&e).is_some());
        assert_eq!(BaseError::NoNumber.to_string(), "no number");
        assert_ne!(e.to_string(), "");
        let _: &dyn std::error::Error = &e;
    }
}
