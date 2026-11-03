//! MLLP framing: `<VT> payload <FS><CR>` (0x0B ... 0x1C 0x0D). Pure -- it only
//! moves bytes between buffers; the socket lives in the `net` module.

use std::fmt;

/// Start-of-block byte (vertical tab).
pub const START: u8 = 0x0B;
/// End-of-block byte (file separator).
pub const END: u8 = 0x1C;
/// Trailing carriage return.
pub const CR: u8 = 0x0D;

/// A framing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MllpError {
    /// A frame grew past the configured limit before it ended.
    TooLarge,
}

impl fmt::Display for MllpError {
    /// Renders the fixed human-readable message for the error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MllpError::TooLarge => f.write_str("MLLP frame exceeds the size limit"),
        }
    }
}

impl std::error::Error for MllpError {}

/// Normalize the segment separators of editor text to `\r` (the HL7 wire
/// form), dropping blank lines and any framing bytes already present.
#[must_use]
pub fn to_wire(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\n")
        .split(['\n', '\r'])
        .map(|l| l.trim_matches(|c| c == '\u{0B}' || c == '\u{1C}'))
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\r")
        .into_bytes()
}

/// Wrap `payload` in an MLLP frame.
#[must_use]
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 3);
    out.push(START);
    out.extend_from_slice(payload);
    out.push(END);
    out.push(CR);
    out
}

/// Incremental frame extractor: feed it bytes as they arrive, take whole
/// payloads out. Bytes outside a frame (noise before `START`, the `CR` after
/// `END`) are discarded, and a stray `START` inside a frame restarts it.
#[derive(Debug, Clone)]
pub struct Deframer {
    /// Payload bytes of the frame being collected (empty between frames).
    buf: Vec<u8>,
    /// True between a `START` byte and its `END`; bytes outside a frame are discarded.
    in_frame: bool,
    /// Largest payload accepted; exceeding it is [`MllpError::TooLarge`].
    limit: usize,
}

impl Deframer {
    /// A deframer that rejects any frame longer than `limit` bytes.
    #[must_use]
    pub fn new(limit: usize) -> Deframer {
        Deframer {
            buf: Vec::new(),
            in_frame: false,
            limit,
        }
    }

    /// Feed `bytes` and return every payload completed by them.
    ///
    /// # Errors
    /// [`MllpError::TooLarge`] when a frame exceeds the limit; the partial
    /// frame is dropped.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, MllpError> {
        let mut done = Vec::new();
        for &b in bytes {
            if b == START {
                self.in_frame = true;
                self.buf.clear();
            } else if !self.in_frame {
                // Noise between frames (including the CR after END).
            } else if b == END {
                self.in_frame = false;
                done.push(std::mem::take(&mut self.buf));
            } else {
                if self.buf.len() >= self.limit {
                    self.buf.clear();
                    self.in_frame = false;
                    return Err(MllpError::TooLarge);
                }
                self.buf.push(b);
            }
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_deframes() {
        let f = frame(b"MSH|^~\\&|A\rPID|1");
        assert_eq!(f.first(), Some(&0x0B));
        assert_eq!(&f[f.len() - 2..], &[0x1C, 0x0D]);
        let mut d = Deframer::new(1024);
        assert_eq!(d.push(&f).unwrap(), vec![b"MSH|^~\\&|A\rPID|1".to_vec()]);
    }

    #[test]
    fn reassembles_across_chunks_and_skips_noise() {
        let mut d = Deframer::new(1024);
        assert_eq!(d.push(b"noise\x0Bhel").unwrap().len(), 0);
        let out = d.push(b"lo\x1C\r\x0Bsecond\x1C\r").unwrap();
        assert_eq!(out, vec![b"hello".to_vec(), b"second".to_vec()]);
    }

    #[test]
    fn enforces_the_limit() {
        let mut d = Deframer::new(4);
        assert_eq!(d.push(b"\x0Babcdef"), Err(MllpError::TooLarge));
        // Recovers on the next frame.
        assert_eq!(d.push(b"\x0Bab\x1C").unwrap(), vec![b"ab".to_vec()]);
    }

    #[test]
    fn to_wire_normalizes_separators() {
        assert_eq!(to_wire("MSH|a\r\nPID|1\n\nPV1|x\n"), b"MSH|a\rPID|1\rPV1|x");
        assert_eq!(to_wire("\u{0B}MSH|a\r\u{1C}\r"), b"MSH|a");
    }
}
