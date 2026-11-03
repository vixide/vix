//! URL percent-encode and decode for Vix's Tools → Convert → URL menu.
//!
//! [`encode`] percent-encodes every byte that is not an unreserved URL character
//! (`A–Z a–z 0–9 - _ . ~`), so the result is safe to drop into any part of a
//! URL — spaces become `%20`, not `+`. [`decode`] reverses any `%XX` escapes and
//! reports an error when the result is not valid UTF-8.
//!
//! Owning spec: `crates/vix-url-tool/spec/index.md`.

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, percent_encode};

/// Everything except the RFC 3986 "unreserved" set is escaped: start from all
/// controls and add every ASCII punctuation/space character except `-`, `_`,
/// `.`, and `~`. Non-ASCII bytes are always escaped by `percent_encoding`
/// itself (they are outside any `AsciiSet`), so UTF-8 text encodes byte-wise.
const ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Why percent-encoded text could not be decoded.
///
/// `Display` text matches the earlier `String` error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UrlError {
    /// The decoded bytes are not valid UTF-8 text.
    NotUtf8(std::str::Utf8Error),
}

impl std::fmt::Display for UrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUtf8(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for UrlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotUtf8(e) => Some(e),
        }
    }
}

/// Percent-encode `input` for safe use anywhere in a URL.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_url_tool::encode("a b/c?").unwrap(), "a%20b%2Fc%3F");
/// assert_eq!(vix_url_tool::decode("a%20b%2Fc%3F").unwrap(), "a b/c?");
/// ```
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface with [`decode`].
pub fn encode(input: &str) -> Result<String, UrlError> {
    Ok(percent_encode(input.as_bytes(), ENCODE_SET).to_string())
}

/// Decode percent-encoded `input` back to text.
///
/// # Errors
/// Returns [`UrlError::NotUtf8`] when the decoded bytes are not valid UTF-8.
pub fn decode(input: &str) -> Result<String, UrlError> {
    // Malformed escapes (`%zz`, a trailing `%`) pass through literally rather
    // than erroring; only invalid UTF-8 in the *decoded* bytes is an error.
    // `+` is not treated as a space (this is not form decoding).
    percent_decode_str(input)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(UrlError::NotUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(encode("a b/c?").unwrap(), "a%20b%2Fc%3F");
        assert_eq!(decode("a%20b%2Fc%3F").unwrap(), "a b/c?");
    }

    #[test]
    fn leaves_unreserved_alone() {
        assert_eq!(encode("Aa0-_.~").unwrap(), "Aa0-_.~");
    }

    #[test]
    fn invalid_utf8_is_a_typed_error() {
        let e = decode("%FF").unwrap_err();
        assert!(matches!(e, UrlError::NotUtf8(_)));
        assert!(std::error::Error::source(&e).is_some());
        assert_ne!(e.to_string(), "");
    }

    #[test]
    fn decode_passes_plain_text() {
        assert_eq!(decode("hello").unwrap(), "hello");
    }

    proptest::proptest! {
        // Decoding arbitrary input (stray %, non-hex, truncated escapes) never
        // panics.
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
