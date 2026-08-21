//! Fuzz the LSP `Content-Length` framing against hostile server output.
//!
//! This decoder is the one place Vix parses bytes it did not produce: a
//! language server is an external process, and a malformed or malicious header
//! (a huge `Content-Length`, a truncated body, no separator at all) must leave
//! the decoder buffering rather than panicking, over-reading, or allocating the
//! whole address space.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_lsp_core::frame::Decoder;

fuzz_target!(|data: &[u8]| {
    let mut decoder = Decoder::new();

    // Feed the bytes in small, arbitrary-sized chunks, the way a real read loop
    // sees them: a message may be split across any number of reads.
    let mut rest = data;
    while !rest.is_empty() {
        let take = (usize::from(rest[0]) % 32) + 1;
        let take = take.min(rest.len());
        let (chunk, tail) = rest.split_at(take);
        decoder.push(chunk);
        rest = tail;

        // Drain whatever is complete. Bounded so a fuzz case that decodes a
        // stream of empty messages cannot spin forever.
        for _ in 0..64 {
            if decoder.pop().is_none() {
                break;
            }
        }
    }
});
