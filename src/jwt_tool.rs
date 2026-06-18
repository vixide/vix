//! Decode a JSON Web Token's header and payload into readable JSON.
//!
//! A JWT is `header.payload.signature`, each part `Base64URL` (no padding). This
//! decodes the first two parts and pretty-prints them; the signature is left
//! untouched (it cannot be verified without the key). Used by Tools → Convert →
//! JWT Decode via `App::transform_selection_or_buffer_try`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// Decode the header and payload of JWT `input` into pretty-printed JSON.
///
/// # Errors
/// Returns an error when the token does not have at least two dot-separated
/// parts, a part is not valid `Base64URL`, or a part is not valid JSON.
pub fn decode(input: &str) -> Result<String, String> {
    let token = input.trim();
    let mut parts = token.split('.');
    let header = parts.next().filter(|s| !s.is_empty());
    let payload = parts.next().filter(|s| !s.is_empty());
    let (Some(header), Some(payload)) = (header, payload) else {
        return Err("not a JWT (expected header.payload.signature)".to_string());
    };
    let header = decode_part(header)?;
    let payload = decode_part(payload)?;
    Ok(format!("// header\n{header}\n\n// payload\n{payload}\n"))
}

/// Base64URL-decode one part and pretty-print it as JSON.
fn decode_part(part: &str) -> Result<String, String> {
    let bytes = URL_SAFE_NO_PAD.decode(part.as_bytes()).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
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
    }

    #[test]
    fn rejects_non_jwt() {
        assert!(decode("not-a-token").is_err());
        assert!(decode("").is_err());
    }
}
