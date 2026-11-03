//! The Jupyter wire protocol over ZeroMQ multipart frames.
//!
//! A message on the wire is:
//!
//! ```text
//! [identity, …] <IDS|MSG> signature header parent_header metadata content [buffer, …]
//! ```
//!
//! The signature is the lowercase hex HMAC-SHA256, keyed with the connection
//! file's `key`, of the four JSON frames in order. [`decode`] rejects a
//! message whose frame count or size is over [`Limits`], or whose signature
//! does not verify, before it parses a byte of JSON. The comparison is
//! constant-time (`hmac`'s `verify_slice`).

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Map, Value};
use sha2::Sha256;

use crate::message::{Header, Message};

/// The frame that separates routing identities from the message.
pub const DELIMITER: &[u8] = b"<IDS|MSG>";

/// The only signature scheme Vix accepts.
pub const SIGNATURE_SCHEME: &str = "hmac-sha256";

/// HMAC-SHA256, the only signature scheme Vix accepts.
type HmacSha256 = Hmac<Sha256>;

/// An empty key: Jupyter allows one (no authentication), Vix does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyError;

impl fmt::Display for KeyError {
    /// Renders a message that explains why an unsigned connection is refused.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the connection key is empty; Vix requires signed messages")
    }
}

impl std::error::Error for KeyError {}

/// Signs and verifies messages with one connection's key.
///
/// Its `Debug` form never shows the key.
#[derive(Clone)]
pub struct Signer {
    /// A MAC state already keyed, cloned for each message so the key schedule runs once per connection.
    mac: HmacSha256,
}

impl fmt::Debug for Signer {
    /// Prints a placeholder; never the key material.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Signer { .. }")
    }
}

impl Signer {
    /// A signer for `key` (the connection file's `key`, as bytes).
    ///
    /// # Errors
    ///
    /// [`KeyError`] when `key` is empty: an unsigned connection would let any
    /// local process inject messages.
    pub fn new(key: &[u8]) -> Result<Self, KeyError> {
        if key.is_empty() {
            return Err(KeyError);
        }
        // HMAC accepts a key of any length; this cannot fail.
        HmacSha256::new_from_slice(key)
            .map(|mac| Self { mac })
            .map_err(|_| KeyError)
    }

    /// A fresh MAC state seeded with the key, fed the four JSON parts (header, parent header, metadata, content) in order. Cloning the pre-keyed state avoids re-deriving the key for every message.
    fn mac_of(&self, parts: [&[u8]; 4]) -> HmacSha256 {
        let mut mac = self.mac.clone();
        for part in parts {
            mac.update(part);
        }
        mac
    }

    /// The lowercase hex signature of the four JSON parts.
    #[must_use]
    pub fn sign(&self, parts: [&[u8]; 4]) -> String {
        let bytes = self.mac_of(parts).finalize().into_bytes();
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(hex_digit(b >> 4));
            out.push(hex_digit(b & 0x0f));
        }
        out
    }

    /// Whether `signature` (lowercase hex, as Jupyter writes it) is the
    /// signature of `parts`. Constant-time in the signature's value; any
    /// other spelling of the same value is refused, so a signature has
    /// exactly one accepted form.
    #[must_use]
    pub fn verify(&self, signature: &[u8], parts: [&[u8]; 4]) -> bool {
        let Some(tag) = decode_hex_32(signature) else {
            return false;
        };
        self.mac_of(parts).verify_slice(&tag).is_ok()
    }
}

/// Lowercase hex digit for the low nibble of `n`.
fn hex_digit(n: u8) -> char {
    char::from(b"0123456789abcdef"[usize::from(n & 0x0f)])
}

/// Value of a lowercase hex digit. Uppercase is rejected on purpose: signatures are produced and compared as lowercase hex, so only that form can be genuine.
fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Decode a 64-character lowercase hex string into 32 bytes (a SHA-256 digest); `None` for any other length or character.
fn decode_hex_32(text: &[u8]) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(text.as_chunks::<2>().0) {
        *slot = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Some(out)
}

/// Size caps applied by [`decode`] before any parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most frames in one message, identities and buffers included.
    pub max_frames: usize,
    /// Most bytes across every frame of one message.
    pub max_total_bytes: usize,
}

impl Default for Limits {
    /// 64 frames and 64 MiB: room for a large plot, not for a memory bomb.
    fn default() -> Self {
        Self {
            max_frames: 64,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Why a frame list is not an acceptable message. The variants carry no
/// message bytes, so they are safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// More frames than [`Limits::max_frames`].
    TooManyFrames,
    /// More bytes than [`Limits::max_total_bytes`].
    TooLarge,
    /// No `<IDS|MSG>` delimiter frame.
    NoDelimiter,
    /// The delimiter is not followed by a signature and four JSON frames.
    Truncated,
    /// The signature is missing, malformed, or wrong.
    BadSignature,
    /// A JSON frame (named) did not parse as its expected shape.
    Json(&'static str),
}

impl fmt::Display for DecodeError {
    /// Renders a fixed human-readable message for the error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyFrames => f.write_str("too many frames"),
            Self::TooLarge => f.write_str("message too large"),
            Self::NoDelimiter => f.write_str("no <IDS|MSG> delimiter"),
            Self::Truncated => f.write_str("truncated message"),
            Self::BadSignature => f.write_str("bad signature"),
            Self::Json(part) => write!(f, "malformed {part}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// The frames of `message`, signed with `signer`.
#[must_use]
pub fn encode(message: &Message, signer: &Signer) -> Vec<Vec<u8>> {
    let header = to_bytes(&message.header);
    let parent = message
        .parent_header
        .as_ref()
        .map_or_else(|| b"{}".to_vec(), to_bytes);
    let metadata = to_bytes(&message.metadata);
    let content = to_bytes(&message.content);
    let signature = signer.sign([&header, &parent, &metadata, &content]);

    let mut frames = Vec::with_capacity(message.identities.len() + 6 + message.buffers.len());
    frames.extend(message.identities.iter().cloned());
    frames.push(DELIMITER.to_vec());
    frames.push(signature.into_bytes());
    frames.push(header);
    frames.push(parent);
    frames.push(metadata);
    frames.push(content);
    frames.extend(message.buffers.iter().cloned());
    frames
}

/// Serialize to JSON bytes. The `{}` fallback is unreachable in practice; it only exists to avoid a panic path.
fn to_bytes<T: serde::Serialize>(value: &T) -> Vec<u8> {
    // Serializing a `Header` or a `serde_json` value to a Vec cannot fail:
    // every key is a string and there is no I/O.
    serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec())
}

/// Parse and authenticate a received message.
///
/// Order matters: caps first, then the delimiter, then the signature, and
/// only then JSON. A forged or oversized message costs one HMAC at most.
///
/// # Errors
///
/// A [`DecodeError`] naming the first check that failed.
pub fn decode<B: AsRef<[u8]>>(
    frames: &[B],
    signer: &Signer,
    limits: Limits,
) -> Result<Message, DecodeError> {
    if frames.len() > limits.max_frames {
        return Err(DecodeError::TooManyFrames);
    }
    let mut total = 0usize;
    for frame in frames {
        total = total.saturating_add(frame.as_ref().len());
        if total > limits.max_total_bytes {
            return Err(DecodeError::TooLarge);
        }
    }
    let delimiter = frames
        .iter()
        .position(|frame| frame.as_ref() == DELIMITER)
        .ok_or(DecodeError::NoDelimiter)?;
    let rest = &frames[delimiter + 1..];
    if rest.len() < 5 {
        return Err(DecodeError::Truncated);
    }
    let [signature, header, parent, metadata, content] = [
        rest[0].as_ref(),
        rest[1].as_ref(),
        rest[2].as_ref(),
        rest[3].as_ref(),
        rest[4].as_ref(),
    ];
    if !signer.verify(signature, [header, parent, metadata, content]) {
        return Err(DecodeError::BadSignature);
    }

    let header: Header = serde_json::from_slice(header).map_err(|_| DecodeError::Json("header"))?;
    let parent: Value =
        serde_json::from_slice(parent).map_err(|_| DecodeError::Json("parent_header"))?;
    let parent_header = match parent {
        Value::Object(map) if map.is_empty() => None,
        Value::Null => None,
        other => Some(
            serde_json::from_value::<Header>(other)
                .map_err(|_| DecodeError::Json("parent_header"))?,
        ),
    };
    let metadata: Map<String, Value> =
        serde_json::from_slice(metadata).map_err(|_| DecodeError::Json("metadata"))?;
    let content: Value =
        serde_json::from_slice(content).map_err(|_| DecodeError::Json("content"))?;

    Ok(Message {
        identities: frames[..delimiter]
            .iter()
            .map(|f| f.as_ref().to_vec())
            .collect(),
        header,
        parent_header,
        metadata,
        content,
        buffers: rest[5..].iter().map(|f| f.as_ref().to_vec()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn signer() -> Signer {
        Signer::new(b"a0436f6c-1916-498b-8eb9-e81ab9368e84").unwrap_or_else(|_| unreachable!())
    }

    fn sample() -> Message {
        Message::new(
            "execute_request",
            json!({"code": "1 + 1"}),
            "msg-1",
            "session-1",
            "vix",
            "2026-10-30T00:00:00Z",
        )
    }

    #[test]
    fn empty_key_is_refused() {
        assert_eq!(Signer::new(b"").map(|_| ()), Err(KeyError));
    }

    #[test]
    fn known_signature() {
        // HMAC-SHA256("key", "The quick brown fox jumps over the lazy dog")
        let s = Signer::new(b"key").unwrap_or_else(|_| unreachable!());
        let sig = s.sign([
            b"The quick brown fox ",
            b"jumps over ",
            b"the lazy dog",
            b"",
        ]);
        assert_eq!(
            sig,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
        let parts: [&[u8]; 4] = [
            b"The quick brown fox ",
            b"jumps over ",
            b"the lazy dog",
            b"",
        ];
        assert!(s.verify(sig.as_bytes(), parts));
        assert!(!s.verify(sig.to_uppercase().as_bytes(), parts));
        assert!(!s.verify(&sig.as_bytes()[1..], parts));
    }

    #[test]
    fn round_trip_with_identities_and_buffers() {
        let mut message = sample();
        message.identities = vec![b"peer-a".to_vec(), b"peer-b".to_vec()];
        message.buffers = vec![vec![0, 1, 2]];
        message.parent_header = Some(sample().header);
        let frames = encode(&message, &signer());
        let back = decode(&frames, &signer(), Limits::default());
        assert_eq!(back, Ok(message));
    }

    #[test]
    fn empty_parent_is_none() {
        let frames = encode(&sample(), &signer());
        assert_eq!(frames[3], b"{}");
        let back = decode(&frames, &signer(), Limits::default()).ok();
        assert_eq!(back.and_then(|m| m.parent_header), None);
    }

    #[test]
    fn bad_signature_rejected_before_json() {
        let mut frames = encode(&sample(), &signer());
        frames[4] = b"not json at all".to_vec();
        assert_eq!(
            decode(&frames, &signer(), Limits::default()),
            Err(DecodeError::BadSignature)
        );
        let other = Signer::new(b"other").unwrap_or_else(|_| unreachable!());
        let frames = encode(&sample(), &other);
        assert_eq!(
            decode(&frames, &signer(), Limits::default()),
            Err(DecodeError::BadSignature)
        );
    }

    #[test]
    fn missing_delimiter_and_truncation() {
        let frames = encode(&sample(), &signer());
        assert_eq!(
            decode(&frames[1..], &signer(), Limits::default()),
            Err(DecodeError::NoDelimiter)
        );
        assert_eq!(
            decode(&frames[..5], &signer(), Limits::default()),
            Err(DecodeError::Truncated)
        );
    }

    #[test]
    fn caps_apply_first() {
        let frames = encode(&sample(), &signer());
        let tight = Limits {
            max_frames: 3,
            max_total_bytes: usize::MAX,
        };
        assert_eq!(
            decode(&frames, &signer(), tight),
            Err(DecodeError::TooManyFrames)
        );
        let tiny = Limits {
            max_frames: 64,
            max_total_bytes: 10,
        };
        assert_eq!(decode(&frames, &signer(), tiny), Err(DecodeError::TooLarge));
    }

    #[test]
    fn signed_but_malformed_json_is_a_json_error() {
        let s = signer();
        let parts: [&[u8]; 4] = [b"[]", b"{}", b"{}", b"{}"];
        let sig = s.sign(parts);
        let frames: Vec<Vec<u8>> = vec![
            DELIMITER.to_vec(),
            sig.into_bytes(),
            b"[]".to_vec(),
            b"{}".to_vec(),
            b"{}".to_vec(),
            b"{}".to_vec(),
        ];
        assert_eq!(
            decode(&frames, &s, Limits::default()),
            Err(DecodeError::Json("header"))
        );
    }

    #[test]
    fn debug_hides_key() {
        assert_eq!(format!("{:?}", signer()), "Signer { .. }");
    }
}
