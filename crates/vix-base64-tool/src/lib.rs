//! Base64 encode and decode for Vix's Tools → Convert → Base64 menu.
//!
//! [`encode`] renders the text's UTF-8 bytes as standard Base64 (RFC 4648, with
//! `+`/`/` and `=` padding). [`decode`] is lenient about surrounding whitespace
//! and newlines — common when pasting wrapped Base64 — and reports an error when
//! the input is not valid Base64 or does not decode to UTF-8 text.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Why Base64 text could not be decoded.
///
/// `Display` text matches the earlier `String` errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Base64Error {
    /// The input is not valid standard Base64.
    Decode(base64::DecodeError),
    /// The decoded bytes are not valid UTF-8 text.
    NotUtf8(std::string::FromUtf8Error),
}

impl std::fmt::Display for Base64Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => e.fmt(f),
            Self::NotUtf8(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Base64Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(e) => Some(e),
            Self::NotUtf8(e) => Some(e),
        }
    }
}

/// Encode `input`'s UTF-8 bytes as standard Base64.
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface with [`decode`].
pub fn encode(input: &str) -> Result<String, Base64Error> {
    Ok(STANDARD.encode(input.as_bytes()))
}

/// Decode standard Base64 `input` back to text, ignoring ASCII whitespace.
///
/// # Errors
/// Returns [`Base64Error::Decode`] when the input is not valid Base64, or
/// [`Base64Error::NotUtf8`] when it does not decode to UTF-8 text.
pub fn decode(input: &str) -> Result<String, Base64Error> {
    let cleaned: String = input.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = STANDARD
        .decode(cleaned.as_bytes())
        .map_err(Base64Error::Decode)?;
    String::from_utf8(bytes).map_err(Base64Error::NotUtf8)
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

    #[test]
    fn error_variants_and_display() {
        let e = decode("not valid base64!!!").unwrap_err();
        assert!(matches!(e, Base64Error::Decode(_)));
        assert!(std::error::Error::source(&e).is_some());
        assert_ne!(e.to_string(), "");
        // "/w==" decodes to the single byte 0xFF, which is not UTF-8.
        let e = decode("/w==").unwrap_err();
        assert!(matches!(e, Base64Error::NotUtf8(_)));
        assert!(std::error::Error::source(&e).is_some());
        assert_ne!(e.to_string(), "");
    }

    proptest::proptest! {
        // Decoding arbitrary input never panics (returns Err on invalid data).
        #[test]
        fn decode_never_panics(s in ".*") {
            let _ = decode(&s);
        }

        // encode → decode round-trips exactly for any UTF-8 input.
        #[test]
        fn encode_then_decode_round_trips(s in ".*") {
            let encoded = encode(&s).unwrap();
            proptest::prop_assert_eq!(decode(&encoded).unwrap(), s);
        }
    }
}
