//! The two pipes to Vix: request lines in on stdin, reply lines out on
//! stdout (`spec/index.md`, "Input and output").
//!
//! Each runs on its own OS thread so a blocked read or write never stalls
//! the async runtime. The reader hands complete lines to the request loop;
//! the writer is the **only** code that touches stdout, and it writes
//! nothing but [`encode_reply`] lines, flushing each one, and stops after
//! [`Reply::Closed`] so that line is always the last.

use std::io::{self, BufRead, Read, Write};
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;

use tokio::sync::mpsc;
use vix_matrix_protocol::{MAX_LINE, Reply, encode_reply};

/// One line read from stdin, before it is decoded.
#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    /// A complete line, its line break removed.
    Line(String),
    /// A line longer than [`MAX_LINE`]; its bytes were discarded.
    TooLong,
    /// A line that is not UTF-8.
    NotUtf8,
}

/// Start the reader thread. The returned channel yields one [`Input`] per
/// line and closes at end of input (or on a read error), which the request
/// loop treats like [`vix_matrix_protocol::Request::Quit`].
pub fn spawn_reader<R: BufRead + Send + 'static>(mut input: R) -> mpsc::UnboundedReceiver<Input> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        while let Ok(Some(line)) = read_line(&mut input) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Read one line, never buffering more than [`MAX_LINE`] plus a CR LF.
/// `Ok(None)` at end of input.
fn read_line<R: BufRead>(input: &mut R) -> io::Result<Option<Input>> {
    // Room for the longest accepted line and its "\r\n".
    let limit = MAX_LINE + 2;
    let mut buf = Vec::new();
    let mut bounded = Read::take(&mut *input, u64::try_from(limit).unwrap_or(u64::MAX));
    let read = bounded.read_until(b'\n', &mut buf)?;
    if read == 0 {
        return Ok(None);
    }
    if buf.last() != Some(&b'\n') && buf.len() >= limit {
        skip_to_newline(input)?;
        return Ok(Some(Input::TooLong));
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    if buf.len() > MAX_LINE {
        return Ok(Some(Input::TooLong));
    }
    Ok(Some(match String::from_utf8(buf) {
        Ok(line) => Input::Line(line),
        Err(_) => Input::NotUtf8,
    }))
}

/// Discard input up to and including the next line break (or to the end).
fn skip_to_newline<R: BufRead>(input: &mut R) -> io::Result<()> {
    loop {
        let buf = input.fill_buf()?;
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(at) = buf.iter().position(|&b| b == b'\n') {
            input.consume(at + 1);
            return Ok(());
        }
        let len = buf.len();
        input.consume(len);
    }
}

/// A handle for sending replies to the writer thread. Cheap to clone; a
/// send after [`Reply::Closed`] was written (or after stdout broke) is
/// silently dropped.
#[derive(Debug, Clone)]
pub struct Out(std_mpsc::Sender<Reply>);

impl Out {
    /// Queue `reply` for stdout.
    pub fn send(&self, reply: Reply) {
        // An error means the writer already stopped: Closed was written or
        // stdout is gone. Either way nothing more may be written.
        let _ = self.0.send(reply);
    }
}

/// Start the writer thread over `output`. It returns `output` when it
/// stops: after writing [`Reply::Closed`], when every [`Out`] is dropped, or
/// when a write fails.
pub fn spawn_writer<W: Write + Send + 'static>(mut output: W) -> (Out, JoinHandle<W>) {
    let (tx, rx) = std_mpsc::channel::<Reply>();
    let handle = std::thread::spawn(move || {
        for reply in rx {
            let mut line = encode_reply(&reply);
            line.push('\n');
            if output
                .write_all(line.as_bytes())
                .and_then(|()| output.flush())
                .is_err()
            {
                break;
            }
            if reply == Reply::Closed {
                break;
            }
        }
        output
    });
    (Out(tx), handle)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn all(bytes: &[u8]) -> Vec<Input> {
        let mut input = Cursor::new(bytes.to_vec());
        let mut out = Vec::new();
        while let Some(line) = read_line(&mut input).unwrap() {
            out.push(line);
        }
        out
    }

    #[test]
    fn lines_lose_their_breaks() {
        assert_eq!(
            all(b"one\r\ntwo\nthree"),
            vec![
                Input::Line("one".into()),
                Input::Line("two".into()),
                Input::Line("three".into())
            ]
        );
    }

    #[test]
    fn an_overlong_line_is_skipped_and_the_next_one_read() {
        let mut bytes = vec![b'x'; MAX_LINE + 10];
        bytes.extend_from_slice(b"\nnext\n");
        assert_eq!(
            all(&bytes),
            vec![Input::TooLong, Input::Line("next".into())]
        );
        let exact = vec![b'y'; MAX_LINE];
        assert_eq!(all(&exact), vec![Input::Line("y".repeat(MAX_LINE))]);
    }

    #[test]
    fn non_utf8_is_flagged() {
        assert_eq!(
            all(b"\xff\xfe\nok\n"),
            vec![Input::NotUtf8, Input::Line("ok".into())]
        );
    }

    #[test]
    fn the_writer_stops_after_closed() {
        let (out, handle) = spawn_writer(Vec::new());
        out.send(Reply::Closed);
        let bytes = handle.join().unwrap();
        out.send(Reply::Left { id: "!r:h".into() });
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"type\":\"closed\",\"v\":1}\n"
        );
    }
}
