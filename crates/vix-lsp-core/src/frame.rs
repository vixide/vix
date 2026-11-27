//! JSON-RPC message framing: the `Content-Length: N\r\n\r\n<body>` envelope LSP
//! uses over stdio.
//!
//! Both size limits are checked before the body is buffered (T848): a frame
//! whose header declares more than [`MAX_FRAME_BYTES`], or a header block
//! longer than [`MAX_HEADER_BYTES`], fails with a [`FrameError`] as soon as
//! the header has arrived, so a server cannot make the client hold an
//! arbitrarily large message in memory. The host drops the connection then.

use serde_json::Value;

/// The largest body a frame may declare, in bytes: 64 MiB (T848).
///
/// Measured against a real server (see `crates/vix-lsp-core/spec/index.md`):
/// on this repository's largest file (17 158 lines) rust-analyzer's largest
/// replies were a 1.4 MB completion list and 0.9 MB of semantic tokens, so
/// 64 MiB leaves more than forty times that for much larger files and
/// workspaces while still bounding what one message can make Vix buffer.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// The longest header block a frame may have, in bytes (T848). A real header
/// is one `Content-Length` line and at most a `Content-Type` line, well under
/// a hundred bytes.
pub const MAX_HEADER_BYTES: usize = 8 * 1024;

/// Why the framed stream from a server cannot be read any further (T848).
/// After one, [`Decoder`] drops what it buffered and ignores further input:
/// the stream has lost sync, and the host should close the connection.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameError {
    /// The header declares a body larger than `limit` bytes.
    TooLarge {
        /// The declared `Content-Length` (`u64::MAX` when it does not even fit
        /// in 64 bits).
        length: u64,
        /// The limit it exceeds, [`MAX_FRAME_BYTES`].
        limit: usize,
    },
    /// No header terminator (`\r\n\r\n`) within `limit` bytes.
    HeaderTooLarge {
        /// The limit, [`MAX_HEADER_BYTES`].
        limit: usize,
    },
    /// The header has no `Content-Length`, or its value is not a number.
    BadHeader,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { length, limit } => {
                write!(
                    f,
                    "message of {length} bytes exceeds the {limit}-byte limit"
                )
            }
            Self::HeaderTooLarge { limit } => {
                write!(f, "message header exceeds the {limit}-byte limit")
            }
            Self::BadHeader => f.write_str("message header has no valid Content-Length"),
        }
    }
}

impl std::error::Error for FrameError {}

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
/// Push raw stdout bytes as they arrive, then repeatedly call
/// [`Decoder::try_pop`] (or [`Decoder::pop`]) to pop each complete JSON
/// message. Partial messages stay buffered, up to the limits above.
#[derive(Default)]
pub struct Decoder {
    /// Bytes received but not yet consumed as a whole message.
    buf: Vec<u8>,
    /// The error that stopped this decoder, once one has (T848).
    failed: Option<FrameError>,
}

impl Decoder {
    /// A new, empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append freshly-read bytes to the internal buffer. Ignored once the
    /// decoder has failed, so a misbehaving server cannot grow it further.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buf.extend_from_slice(bytes);
        }
    }

    /// How many bytes are buffered and not yet consumed.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Pop the next complete message, or `None` if one is not fully buffered
    /// yet (or the decoder has failed: [`Decoder::try_pop`] says why).
    pub fn pop(&mut self) -> Option<Value> {
        self.try_pop().ok().flatten()
    }

    /// Pop the next complete message: `Ok(None)` while one is not fully
    /// buffered yet. A body that is not valid JSON is skipped, so the stream
    /// resyncs on the next frame rather than wedging.
    ///
    /// # Errors
    /// A [`FrameError`] when the next header is oversized or invalid, and
    /// the same error on every later call: the stream cannot be resynced.
    pub fn try_pop(&mut self) -> Result<Option<Value>, FrameError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        loop {
            match self.next_frame() {
                Ok(Step::Message(value)) => return Ok(Some(value)),
                Ok(Step::Skipped) => {}
                Ok(Step::Incomplete) => return Ok(None),
                Err(error) => {
                    self.buf = Vec::new();
                    self.failed = Some(error.clone());
                    return Err(error);
                }
            }
        }
    }

    /// Consume the next whole frame, if it is all here.
    fn next_frame(&mut self) -> Result<Step, FrameError> {
        const SEP: &[u8] = b"\r\n\r\n";
        // Only the first MAX_HEADER_BYTES (plus the separator) can hold the
        // end of a valid header, so the search never scans a buffered body.
        let window = MAX_HEADER_BYTES
            .checked_add(SEP.len())
            .unwrap_or(MAX_HEADER_BYTES);
        let head = self.buf.get(..window).unwrap_or(&self.buf);
        let Some(header_end) = find(head, SEP) else {
            if self.buf.len() > MAX_HEADER_BYTES {
                return Err(FrameError::HeaderTooLarge {
                    limit: MAX_HEADER_BYTES,
                });
            }
            return Ok(Step::Incomplete);
        };
        if header_end > MAX_HEADER_BYTES {
            return Err(FrameError::HeaderTooLarge {
                limit: MAX_HEADER_BYTES,
            });
        }
        let length = content_length(self.buf.get(..header_end).unwrap_or_default())?;
        // The size check comes before anything waits for (and so buffers)
        // the body.
        let len = usize::try_from(length)
            .ok()
            .filter(|&n| n <= MAX_FRAME_BYTES)
            .ok_or(FrameError::TooLarge {
                length,
                limit: MAX_FRAME_BYTES,
            })?;
        let body_start = header_end
            .checked_add(SEP.len())
            .ok_or(FrameError::BadHeader)?;
        let body_end = body_start.checked_add(len).ok_or(FrameError::TooLarge {
            length,
            limit: MAX_FRAME_BYTES,
        })?;
        let Some(body) = self.buf.get(body_start..body_end) else {
            return Ok(Step::Incomplete);
        };
        let step = serde_json::from_slice(body).map_or(Step::Skipped, Step::Message);
        self.buf.drain(..body_end);
        Ok(step)
    }
}

/// What [`Decoder`]'s next frame turned out to be.
enum Step {
    /// Not all of it has arrived yet.
    Incomplete,
    /// A whole frame whose body is not valid JSON; consumed.
    Skipped,
    /// A whole frame and its JSON body; consumed.
    Message(Value),
}

/// How [`read_messages`] stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadEnd {
    /// The stream ended or a read failed: the server has gone.
    Closed,
    /// The callback asked to stop (the host no longer listens).
    Stopped,
    /// The stream broke the framing rules; the caller should drop the
    /// connection (T848).
    Frame(FrameError),
}

/// Read framed messages from `reader` until it ends, handing each decoded
/// message to `on_message`, which returns `false` to stop. This is the loop
/// the LSP and DAP clients run on their reader threads. It reads in 8 KiB
/// chunks and checks each header as soon as it is complete, so an oversized
/// frame stops it after at most one chunk past the header, without the body
/// ever being buffered.
pub fn read_messages(
    reader: &mut impl std::io::Read,
    mut on_message: impl FnMut(Value) -> bool,
) -> ReadEnd {
    let mut decoder = Decoder::new();
    let mut chunk = [0u8; 8192];
    loop {
        let received = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => return ReadEnd::Closed,
            Ok(n) => chunk.get(..n).unwrap_or_default(),
        };
        decoder.push(received);
        loop {
            match decoder.try_pop() {
                Ok(Some(msg)) => {
                    if !on_message(msg) {
                        return ReadEnd::Stopped;
                    }
                }
                Ok(None) => break,
                Err(error) => return ReadEnd::Frame(error),
            }
        }
    }
}

/// First index of `needle` within `haystack`, if present.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse the `Content-Length` value (case-insensitive) from a header block.
/// Lines without a `:` are skipped. A value of digits too large for `u64`
/// reads as `u64::MAX`, so it fails the size check rather than the parse.
fn content_length(header: &[u8]) -> Result<u64, FrameError> {
    let text = std::str::from_utf8(header).map_err(|_| FrameError::BadHeader)?;
    for line in text.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(FrameError::BadHeader);
            }
            return Ok(value.parse().unwrap_or(u64::MAX));
        }
    }
    Err(FrameError::BadHeader)
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

    #[test]
    fn other_header_lines_are_allowed() {
        let mut d = Decoder::new();
        d.push(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 8\r\n\r\n{\"ok\":2}");
        assert_eq!(d.try_pop().unwrap().unwrap()["ok"], 2);
    }

    #[test]
    fn a_body_that_is_not_json_is_skipped_and_the_next_frame_still_pops() {
        let mut d = Decoder::new();
        d.push(b"Content-Length: 3\r\n\r\nnot");
        d.push(&encode(&json!({"id": 3})));
        assert_eq!(d.try_pop().unwrap().unwrap()["id"], 3);
    }

    // ----- T848: frame-size limits -------------------------------------------

    #[test]
    fn an_oversized_content_length_fails_before_the_body_is_buffered() {
        let mut d = Decoder::new();
        d.push(b"Content-Length: 1000000000000\r\n\r\n{");
        assert_eq!(
            d.try_pop(),
            Err(FrameError::TooLarge {
                length: 1_000_000_000_000,
                limit: MAX_FRAME_BYTES
            })
        );
        assert_eq!(d.buffered(), 0, "what was buffered is dropped");
        // Further input is ignored, and the error sticks.
        d.push(&[b'x'; 4096]);
        assert_eq!(d.buffered(), 0);
        assert!(d.try_pop().is_err());
        assert!(d.pop().is_none());
    }

    #[test]
    fn the_limit_itself_is_allowed_and_one_more_byte_is_not() {
        let mut d = Decoder::new();
        d.push(format!("Content-Length: {MAX_FRAME_BYTES}\r\n\r\n").as_bytes());
        assert_eq!(d.try_pop(), Ok(None), "waits for a body at the limit");
        let mut d = Decoder::new();
        d.push(format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1).as_bytes());
        assert!(matches!(d.try_pop(), Err(FrameError::TooLarge { .. })));
    }

    #[test]
    fn a_length_too_big_for_u64_is_too_large_not_unparsable() {
        let mut d = Decoder::new();
        d.push(b"Content-Length: 99999999999999999999999999\r\n\r\n");
        assert_eq!(
            d.try_pop(),
            Err(FrameError::TooLarge {
                length: u64::MAX,
                limit: MAX_FRAME_BYTES
            })
        );
    }

    #[test]
    fn a_missing_or_invalid_content_length_is_a_bad_header() {
        for header in [
            "Content-Type: x\r\n\r\n",
            "Content-Length: -1\r\n\r\n",
            "Content-Length:\r\n\r\n",
        ] {
            let mut d = Decoder::new();
            d.push(header.as_bytes());
            assert_eq!(d.try_pop(), Err(FrameError::BadHeader), "{header:?}");
        }
    }

    #[test]
    fn an_endless_header_fails_at_the_header_limit() {
        let mut d = Decoder::new();
        d.push(&vec![b'a'; MAX_HEADER_BYTES]);
        assert_eq!(d.try_pop(), Ok(None), "still within the limit");
        d.push(b"a");
        assert_eq!(
            d.try_pop(),
            Err(FrameError::HeaderTooLarge {
                limit: MAX_HEADER_BYTES
            })
        );
    }

    /// A fake server: a header declaring 10^12 bytes, then an endless body.
    /// Counts how much the client read.
    struct HugeFrame {
        header: Vec<u8>,
        sent: usize,
    }

    impl std::io::Read for HugeFrame {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = if self.sent < self.header.len() {
                let rest = &self.header[self.sent..];
                let n = rest.len().min(buf.len());
                buf[..n].copy_from_slice(&rest[..n]);
                n
            } else {
                buf.fill(b' ');
                buf.len()
            };
            self.sent += n;
            Ok(n)
        }
    }

    #[test]
    fn read_messages_stops_a_server_declaring_a_terabyte_message() {
        let mut server = HugeFrame {
            header: b"Content-Length: 1000000000000\r\n\r\n".to_vec(),
            sent: 0,
        };
        let mut seen = 0;
        let end = read_messages(&mut server, |_| {
            seen += 1;
            true
        });
        assert_eq!(
            end,
            ReadEnd::Frame(FrameError::TooLarge {
                length: 1_000_000_000_000,
                limit: MAX_FRAME_BYTES
            })
        );
        assert_eq!(seen, 0);
        assert!(
            server.sent <= 8192,
            "stopped after the header's chunk, read {} bytes",
            server.sent
        );
    }

    #[test]
    fn read_messages_delivers_messages_until_the_stream_closes_or_the_host_stops() {
        let mut bytes = encode(&json!({"id": 1}));
        bytes.extend(encode(&json!({"id": 2})));
        let mut got = Vec::new();
        let end = read_messages(&mut bytes.as_slice(), |m| {
            got.push(m["id"].clone());
            true
        });
        assert_eq!((end, got), (ReadEnd::Closed, vec![json!(1), json!(2)]));
        let end = read_messages(&mut bytes.as_slice(), |_| false);
        assert_eq!(end, ReadEnd::Stopped);
    }

    #[test]
    fn frame_errors_describe_themselves() {
        let e = FrameError::TooLarge {
            length: 9,
            limit: 8,
        }
        .to_string();
        assert!(e.contains('9') && e.contains('8'));
        assert!(
            FrameError::HeaderTooLarge { limit: 7 }
                .to_string()
                .contains('7')
        );
        assert!(FrameError::BadHeader.to_string().contains("Content-Length"));
    }
}
