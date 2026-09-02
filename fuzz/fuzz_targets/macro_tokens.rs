//! Fuzz keyboard-macro token decoding (`vix-macros`).
//!
//! `macros.toml` is a file on disk a user can hand-edit (or a stray editor
//! could corrupt), so `decode_key`/`decode` must survive arbitrary token text
//! — an empty string, a bare modifier prefix, an `F` with a huge or
//! non-numeric suffix, a multi-character "single key" — without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_macros::{decode, decode_key, encode_key};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // decode_key must be total, and every KeyEvent it does produce must
    // re-encode to *some* non-empty token (round-tripping to the exact same
    // token isn't guaranteed — e.g. "C-A-x" and "A-C-x" both decode to the
    // same modifiers but only one is `encode_key`'s canonical order).
    if let Some(key) = decode_key(text) {
        let token = encode_key(key);
        assert!(!token.is_empty(), "a decodable token re-encoded to empty");
    }

    // One token per line, mirroring how a macro's `keys` list is stored.
    let tokens: Vec<String> = text.lines().map(str::to_string).collect();
    let decoded = decode(&tokens);
    assert!(
        decoded.len() <= tokens.len(),
        "decode produced more events than input tokens"
    );
});
