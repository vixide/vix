//! Fuzz the `.http` request-buffer parser.
//!
//! The buffer is whatever the user typed, and it is parsed on every send: a
//! missing verb, a header without a colon, a body with no blank line before it,
//! a URL with control characters. Parsing must be total — `send` is what may
//! fail, and only over the network.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if let Some(request) = vix_http_client::parse_request(text) {
        // A parsed request must be self-consistent enough to describe.
        assert!(
            !request.method.is_empty(),
            "parsed a request with no method"
        );
        assert!(!request.url.is_empty(), "parsed a request with no URL");
    }
});
