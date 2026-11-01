//! Decode a JSON Web Token's header and payload into readable JSON.
//!
//! A JWT is `header.payload.signature`, each part `Base64URL` (no padding). This
//! decodes the first two parts and pretty-prints them; the signature is left
//! untouched (it cannot be verified without the key). Used by Tools → Convert →
//! JWT Decode via `App::transform_selection_or_buffer_try`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Why a JWT could not be decoded.
///
/// `Display` text matches the earlier `String` errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum JwtError {
    /// Fewer than two non-empty dot-separated parts.
    NotJwt,
    /// A part is not valid `Base64URL`.
    Base64(base64::DecodeError),
    /// A part decoded to bytes that are not UTF-8 text.
    NotUtf8(std::string::FromUtf8Error),
    /// A part decoded to text that is not JSON.
    Json {
        /// The JSON parser's own message.
        message: String,
    },
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotJwt => f.write_str("not a JWT (expected header.payload.signature)"),
            Self::Base64(e) => e.fmt(f),
            Self::NotUtf8(e) => e.fmt(f),
            Self::Json { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for JwtError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Base64(e) => Some(e),
            Self::NotUtf8(e) => Some(e),
            Self::NotJwt | Self::Json { .. } => None,
        }
    }
}

/// Decode the header and payload of JWT `input` into pretty-printed JSON.
///
/// # Errors
/// Returns a [`JwtError`] when the token does not have at least two dot-separated
/// parts, a part is not valid `Base64URL`, or a part is not valid JSON.
pub fn decode(input: &str) -> Result<String, JwtError> {
    let token = input.trim();
    let mut parts = token.split('.');
    let header = parts.next().filter(|s| !s.is_empty());
    let payload = parts.next().filter(|s| !s.is_empty());
    let (Some(header), Some(payload)) = (header, payload) else {
        return Err(JwtError::NotJwt);
    };
    let header = decode_part(header)?;
    let payload = decode_part(payload)?;
    // This tool only *decodes*; it cannot verify the signature (no key). Make
    // that explicit so a viewer never mistakes the claims for authenticated
    // data, and call out the `alg:none` case (an unsigned token) specifically.
    let mut banner =
        String::from("// WARNING: signature NOT verified — these claims are untrusted\n");
    if header_is_alg_none(&header) {
        banner.push_str("// WARNING: alg=none — this token is unsigned\n");
    }
    Ok(format!(
        "{banner}// header\n{header}\n\n// payload\n{payload}\n"
    ))
}

/// Whether the decoded header JSON declares `"alg": "none"` (case-insensitive).
fn header_is_alg_none(header_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(header_json)
        .ok()
        .and_then(|v| {
            v.get("alg")
                .and_then(|a| a.as_str())
                .map(|s| s.eq_ignore_ascii_case("none"))
        })
        .unwrap_or(false)
}

/// Base64URL-decode one part and pretty-print it as JSON.
fn decode_part(part: &str) -> Result<String, JwtError> {
    let json_err = |e: serde_json::Error| JwtError::Json {
        message: e.to_string(),
    };
    let bytes = URL_SAFE_NO_PAD
        .decode(part.as_bytes())
        .map_err(JwtError::Base64)?;
    let text = String::from_utf8(bytes).map_err(JwtError::NotUtf8)?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(json_err)?;
    serde_json::to_string_pretty(&value).map_err(json_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_standard_token() {
        // Example token from jwt.io ({"alg":"HS256","typ":"JWT"} / {"sub":"1234567890",...}).
        let tok = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                   eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
                   SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = decode(tok).unwrap();
        assert!(out.contains("\"alg\": \"HS256\""), "{out}");
        assert!(out.contains("\"name\": \"John Doe\""), "{out}");
        assert!(out.contains("// payload"), "{out}");
        // Every decode makes clear the signature was not checked.
        assert!(out.contains("NOT verified"), "{out}");
        assert!(!out.to_lowercase().contains("alg=none"), "HS256 is signed");
    }

    #[test]
    fn flags_alg_none_unsigned_tokens() {
        // {"alg":"none","typ":"JWT"} . {"admin":true} . (empty signature)
        let none = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJhZG1pbiI6dHJ1ZX0.";
        let out = decode(none).unwrap();
        assert!(out.contains("NOT verified"), "{out}");
        assert!(out.to_lowercase().contains("alg=none"), "{out}");
    }

    #[test]
    fn rejects_non_jwt() {
        assert!(decode("not-a-token").is_err());
        assert!(decode("").is_err());
    }

    #[test]
    fn errors_are_typed() {
        assert_eq!(decode("not-a-token"), Err(JwtError::NotJwt));
        assert_eq!(decode(""), Err(JwtError::NotJwt));
        assert!(matches!(decode("!!!.!!!.x"), Err(JwtError::Base64(_))));
        // "_w" is the single byte 0xFF (not UTF-8); "e30" is "{}".
        assert!(matches!(decode("_w.e30.x"), Err(JwtError::NotUtf8(_))));
        // "aGk" is "hi" (not JSON).
        let e = decode("aGk.e30.x").unwrap_err();
        assert!(matches!(e, JwtError::Json { .. }));
        assert_ne!(e.to_string(), "");
        for e in [
            decode("!!!.!!!.x").unwrap_err(),
            decode("_w.e30.x").unwrap_err(),
        ] {
            assert!(std::error::Error::source(&e).is_some());
            assert_ne!(e.to_string(), "");
        }
        assert!(std::error::Error::source(&JwtError::NotJwt).is_none());
    }

    // ---- property-based ("fuzz") tests ------------------------------------

    use proptest::prelude::*;

    proptest! {
        // Decoding arbitrary input never panics, and any successful decode always
        // carries the "not verified" warning (never presenting claims as trusted).
        #[test]
        fn decode_never_panics_and_always_warns(s in ".*") {
            if let Ok(out) = decode(&s) {
                prop_assert!(out.contains("NOT verified"), "missing warning: {out}");
            }
        }

        // Dotted base64-ish triples (the realistic shape) never panic.
        #[test]
        fn dotted_triples_never_panic(
            a in "[A-Za-z0-9_-]{0,40}",
            b in "[A-Za-z0-9_-]{0,40}",
            c in "[A-Za-z0-9_-]{0,40}",
        ) {
            let _ = decode(&format!("{a}.{b}.{c}"));
        }
    }
}
