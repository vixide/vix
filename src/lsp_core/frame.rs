//! JSON-RPC message framing: the `Content-Length: N\r\n\r\n<body>` envelope LSP
//! uses over stdio.

#![warn(clippy::pedantic)]

use serde_json::Value;

/// Frame a JSON value into a `Content-Length`-delimited LSP message ready to
/// write to the server's stdin.
#[must_use]
pub fn encode(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Incremental decoder for the framed byte stream coming back from a server.
///
/// Push raw stdout bytes as they arrive, then repeatedly call [`Decoder::next`]
/// to pop each complete JSON message. Partial messages stay buffered.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    /// A new, empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append freshly-read bytes to the internal buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pop the next complete message, or `None` if one is not fully buffered yet.
    pub fn pop(&mut self) -> Option<Value> {
        let sep = b"\r\n\r\n";
        let header_end = find(&self.buf, sep)?;
        let len = content_length(&self.buf[..header_end])?;
        let body_start = header_end + sep.len();
        if self.buf.len() < body_start + len {
            return None;
        }
        let value = serde_json::from_slice(&self.buf[body_start..body_start + len]).ok();
        self.buf.drain(..body_start + len);
        // A body that failed to parse is still consumed, so the stream resyncs on
        // the next frame rather than wedging.
        value
    }
}

/// First index of `needle` within `haystack`, if present.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Parse the `Content-Length` value (case-insensitive) from a header block.
fn content_length(header: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(header).ok()?;
    for line in text.split("\r\n") {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn encode_prefixes_content_length() {
        let bytes = encode(&json!({"a": 1}));
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text, "Content-Length: 7\r\n\r\n{\"a\":1}");
    }

    #[test]
    fn decoder_pops_one_message_at_a_time() {
        let mut d = Decoder::new();
        d.push(&encode(&json!({"id": 1})));
        d.push(&encode(&json!({"id": 2})));
        assert_eq!(d.pop().unwrap()["id"], 1);
        assert_eq!(d.pop().unwrap()["id"], 2);
        assert!(d.pop().is_none());
    }

    #[test]
    fn decoder_waits_for_the_full_body() {
        let mut d = Decoder::new();
        let msg = encode(&json!({"hello": "world"}));
        d.push(&msg[..msg.len() - 3]); // withhold the last few body bytes
        assert!(d.pop().is_none(), "incomplete body is not yet a message");
        d.push(&msg[msg.len() - 3..]);
        assert_eq!(d.pop().unwrap()["hello"], "world");
    }

    #[test]
    fn decoder_is_case_insensitive_about_the_header() {
        let mut d = Decoder::new();
        d.push(b"content-length: 8\r\n\r\n{\"ok\":1}");
        assert_eq!(d.pop().unwrap()["ok"], 1);
    }
}
