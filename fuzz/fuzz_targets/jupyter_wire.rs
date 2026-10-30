//! Fuzz the Jupyter wire codec (`vix-jupyter-core`) against hostile kernels.
//!
//! A kernel is an external process, and on a shared machine anything that
//! reaches its sockets can send frames. Decoding must be total; a frame list
//! that was not signed with the key must never decode; and a message that
//! does decode must re-encode to frames that decode to the same message.
//! The typed content, the outputs and ANSI stripping must not panic either.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_jupyter_core::Output;
use vix_jupyter_core::ansi::strip_ansi;
use vix_jupyter_core::wire::{self, DELIMITER, DecodeError, Limits, Signer};

fuzz_target!(|data: &[u8]| {
    let Ok(signer) = Signer::new(b"fuzz-key") else {
        return;
    };
    let limits = Limits {
        max_frames: 32,
        max_total_bytes: 1 << 16,
    };

    // Split the input into frames at 0xFF bytes; a frame of exactly "D"
    // stands for the delimiter so the fuzzer reaches the signature check.
    let frames: Vec<Vec<u8>> = data
        .split(|b| *b == 0xff)
        .map(|f| {
            if f == b"D" {
                DELIMITER.to_vec()
            } else {
                f.to_vec()
            }
        })
        .collect();
    match wire::decode(&frames, &signer, limits) {
        Ok(message) => {
            // Reaching here means the fuzzer forged an HMAC-SHA256: a bug.
            let again = wire::encode(&message, &signer);
            assert_eq!(
                wire::decode(&again, &signer, Limits::default()),
                Ok(message)
            );
        }
        Err(
            DecodeError::TooManyFrames
            | DecodeError::TooLarge
            | DecodeError::NoDelimiter
            | DecodeError::Truncated
            | DecodeError::BadSignature,
        ) => {}
        Err(other) => panic!("unsigned frames reached JSON parsing: {other:?}"),
    }

    // Signed garbage: the signature passes, so JSON parsing and typing run
    // on attacker-chosen content. Must be total.
    let text = String::from_utf8_lossy(data);
    let parts: Vec<&[u8]> = data.splitn(4, |b| *b == 0xff).collect();
    if parts.len() == 4 {
        let signature = signer.sign([parts[0], parts[1], parts[2], parts[3]]);
        let signed = vec![
            DELIMITER.to_vec(),
            signature.into_bytes(),
            parts[0].to_vec(),
            parts[1].to_vec(),
            parts[2].to_vec(),
            parts[3].to_vec(),
        ];
        if let Ok(message) = wire::decode(&signed, &signer, limits)
            && let Ok(content) = message.typed()
            && let Some(output) = Output::from_content(content)
        {
            let plain = output.plain_text();
            if let Output::Error { .. } = output {
                assert!(!plain.contains('\u{1b}'));
            }
        }
    }

    let stripped = strip_ansi(&text);
    assert!(
        !stripped
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    );
    assert_eq!(strip_ansi(&stripped), stripped);
});
