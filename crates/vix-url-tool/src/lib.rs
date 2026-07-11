//! URL percent-encode and decode for Vix's Tools → Convert → URL menu.
//!
//! [`encode`] percent-encodes every byte that is not an unreserved URL character
//! (`A–Z a–z 0–9 - _ . ~`), so the result is safe to drop into any part of a
//! URL — spaces become `%20`, not `+`. [`decode`] reverses any `%XX` escapes and
//! reports an error when the result is not valid UTF-8.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, percent_encode};

/// Everything except the RFC 3986 "unreserved" set is escaped: start from all
/// controls and add every ASCII punctuation/space character except `-`, `_`,
/// `.`, and `~`.
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

/// Percent-encode `input` for safe use anywhere in a URL.
///
/// # Errors
/// Never fails; returns `Result` for a uniform tool interface with [`decode`].
pub fn encode(input: &str) -> Result<String, String> {
    Ok(percent_encode(input.as_bytes(), ENCODE_SET).to_string())
}

/// Decode percent-encoded `input` back to text.
///
/// # Errors
/// Returns an error when the decoded bytes are not valid UTF-8.
pub fn decode(input: &str) -> Result<String, String> {
    percent_decode_str(input)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|e| e.to_string())
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
    fn decode_passes_plain_text() {
        assert_eq!(decode("hello").unwrap(), "hello");
    }
}
