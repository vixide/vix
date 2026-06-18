//! Convert an integer between decimal, hexadecimal, binary, and octal.
//!
//! The input number is parsed with auto-detected radix — `0x`/`0X` hex,
//! `0b`/`0B` binary, `0o`/`0O` octal, else decimal — tolerating surrounding
//! whitespace, an optional leading sign, and `_` digit separators. Each function
//! re-renders it in one base (with the conventional prefix). Used by Tools →
//! Convert → Number via `App::transform_selection_or_buffer_try`.

#![warn(clippy::pedantic)]

/// Parse `s` as an integer with an auto-detected radix.
fn parse(s: &str) -> Result<i128, String> {
    let t = s.trim().replace('_', "");
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(&t)),
    };
    let (radix, digits) = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        (16, h)
    } else if let Some(b) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        (2, b)
    } else if let Some(o) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        (8, o)
    } else {
        (10, body)
    };
    if digits.is_empty() {
        return Err("no number".to_string());
    }
    let n = i128::from_str_radix(digits, radix).map_err(|e| e.to_string())?;
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
/// # Errors
/// Returns an error when `input` is not a valid integer in any supported base.
pub fn to_dec(input: &str) -> Result<String, String> {
    Ok(parse(input)?.to_string())
}

/// To hexadecimal (`0x`).
///
/// # Errors
/// Returns an error when `input` is not a valid integer in any supported base.
pub fn to_hex(input: &str) -> Result<String, String> {
    Ok(render(parse(input)?, "0x", |m| format!("{m:x}")))
}

/// To binary (`0b`).
///
/// # Errors
/// Returns an error when `input` is not a valid integer in any supported base.
pub fn to_bin(input: &str) -> Result<String, String> {
    Ok(render(parse(input)?, "0b", |m| format!("{m:b}")))
}

/// To octal (`0o`).
///
/// # Errors
/// Returns an error when `input` is not a valid integer in any supported base.
pub fn to_oct(input: &str) -> Result<String, String> {
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
}
