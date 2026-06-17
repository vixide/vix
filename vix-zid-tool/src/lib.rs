//! Generate a secure random **ZID**: a 32-character lowercase hexadecimal
//! string carrying 128 bits of cryptographic randomness.
//!
//! Vix's Tools → Generate → ZID command calls [`generate`] and inserts the
//! result at the cursor. The bytes come from the operating system's secure
//! random source (`getrandom`), so each ZID is unpredictable and collision-safe
//! for practical purposes — handy as an opaque identifier in code or data.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Number of random bytes behind a ZID (128 bits → 32 hex characters).
const ZID_BYTES: usize = 16;

/// Generate a fresh ZID: 16 secure-random bytes rendered as 32 lowercase hex
/// characters. Falls back to an all-zero string only if the OS RNG is somehow
/// unavailable (never expected on supported platforms).
#[must_use]
pub fn generate() -> String {
    let mut bytes = [0u8; ZID_BYTES];
    if getrandom::getrandom(&mut bytes).is_err() {
        bytes = [0u8; ZID_BYTES];
    }
    to_hex(&bytes)
}

/// Render bytes as a lowercase hex string (two characters per byte).
fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit(u32::from(b >> 4), 16).expect("nibble"));
        s.push(char::from_digit(u32::from(b & 0x0f), 16).expect("nibble"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_32_lowercase_hex() {
        let z = generate();
        assert_eq!(z.len(), 32, "ZID is 32 characters: {z}");
        assert!(
            z.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "ZID is lowercase hex: {z}"
        );
    }

    #[test]
    fn successive_zids_differ() {
        // Collisions across 128 bits are astronomically unlikely; a repeat here
        // means the RNG is broken or the fallback path fired.
        assert_ne!(generate(), generate());
    }

    #[test]
    fn to_hex_pads_each_byte() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    }
}
