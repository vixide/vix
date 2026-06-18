//! Base64 encode and decode for Vix's Tools → Convert → Base64 menu.
//!
//! [`encode`] renders the text's UTF-8 bytes as standard Base64 (RFC 4648, with
//! `+`/`/` and `=` padding). [`decode`] is lenient about surrounding whitespace
//! and newlines — common when pasting wrapped Base64 — and reports an error when
//! the input is not valid Base64 or does not decode to UTF-8 text.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Encode `input`'s UTF-8 bytes as standard Base64.
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface with [`decode`].
pub fn encode(input: &str) -> Result<String, String> {
    Ok(STANDARD.encode(input.as_bytes()))
}

/// Decode standard Base64 `input` back to text, ignoring ASCII whitespace.
///
/// # Errors
/// Returns an error when the input is not valid Base64 or is not valid UTF-8.
pub fn decode(input: &str) -> Result<String, String> {
    let cleaned: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = STANDARD.decode(cleaned.as_bytes()).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(encode("hello").unwrap(), "aGVsbG8=");
        assert_eq!(decode("aGVsbG8=").unwrap(), "hello");
    }

    #[test]
    fn decode_ignores_whitespace() {
        assert_eq!(decode("aGVs\nbG8=\n").unwrap(), "hello");
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode("not valid base64!!!").is_err());
    }
}
