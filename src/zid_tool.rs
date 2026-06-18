//! Generate a secure random **ZID**: a lowercase hexadecimal string carrying a
//! chosen number of bits of cryptographic randomness.
//!
//! Vix's Tools → Generate → ZID submenu offers three sizes — 128, 256, and 512
//! bits (32, 64, and 128 hex characters) — each calling [`generate`] with the
//! matching byte count and inserting the result at the cursor. The bytes come
//! from the operating system's secure random source (`getrandom`), so each ZID
//! is unpredictable and collision-safe for practical purposes.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Generate a fresh ZID of `byte_len` secure-random bytes, rendered as
/// `2 * byte_len` lowercase hex characters. Falls back to all-zero bytes only if
/// the OS RNG is somehow unavailable (never expected on supported platforms).
#[must_use]
pub fn generate(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if getrandom::getrandom(&mut bytes).is_err() {
        bytes.fill(0);
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
    fn lengths_match_bit_sizes() {
        assert_eq!(generate(16).len(), 32, "128-bit ZID is 32 hex chars");
        assert_eq!(generate(32).len(), 64, "256-bit ZID is 64 hex chars");
        assert_eq!(generate(64).len(), 128, "512-bit ZID is 128 hex chars");
    }

    #[test]
    fn is_lowercase_hex() {
        let z = generate(16);
        assert!(z.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "lowercase hex: {z}");
    }

    #[test]
    fn successive_zids_differ() {
        // Collisions across 128 bits are astronomically unlikely; a repeat here
        // means the RNG is broken or the fallback path fired.
        assert_ne!(generate(16), generate(16));
    }

    #[test]
    fn to_hex_pads_each_byte() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    }
}
