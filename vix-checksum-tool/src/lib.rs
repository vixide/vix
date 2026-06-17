//! Compute SHA-256 and SHA-512 checksums of text, returned as lowercase hex.
//!
//! Vix's Tools → Checksum submenu hashes the selection (or, with no selection,
//! the whole buffer) and replaces it with the digest. The functions here take
//! the text as UTF-8 bytes and return the digest as a lowercase hexadecimal
//! string — 64 characters for SHA-256, 128 for SHA-512.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use sha2::{Digest, Sha256, Sha512};

/// SHA-256 digest of `text` (UTF-8 bytes) as a 64-character lowercase hex string.
#[must_use]
pub fn sha256_hex(text: &str) -> String {
    to_hex(&Sha256::digest(text.as_bytes()))
}

/// SHA-512 digest of `text` (UTF-8 bytes) as a 128-character lowercase hex string.
#[must_use]
pub fn sha512_hex(text: &str) -> String {
    to_hex(&Sha512::digest(text.as_bytes()))
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
    fn sha256_known_vectors() {
        // Standard NIST test vectors.
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha512_known_vectors() {
        assert_eq!(
            sha512_hex(""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            sha512_hex("abc"),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn digest_lengths() {
        assert_eq!(sha256_hex("vix").len(), 64);
        assert_eq!(sha512_hex("vix").len(), 128);
    }
}
