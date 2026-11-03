//! An established, encrypted connection and its framing.
//!
//! After the Noise handshake every byte is a frame: a big-endian `u16`
//! ciphertext length, then one Noise transport message. A logical
//! [`Message`] larger than one frame is split into chunks; each chunk's
//! plaintext starts with a flag byte (`1` = last chunk) and each uses the next
//! nonce, so reordering, dropping or replaying a frame fails authentication.
//! Reassembly is capped at [`MAX_MESSAGE_BYTES`].
//!
//! Each direction has its own monotonically increasing nonce counter (the
//! transport is a snow *stateless* state, so nonces are explicit); the
//! counters are never reused, and a counter at `u64::MAX` is refused as a
//! [`NetError::Crypto`] rather than wrapping. Spec:
//! `crates/vix-cowork-net/spec/index.md`.

use crate::error::NetError;
use snow::StatelessTransportState;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;
use vix_cowork::wire::{self, MAX_MESSAGE_BYTES, Message};

/// Plaintext bytes per chunk (plus one flag byte and a 16-byte tag stays under
/// the 65,535-byte Noise limit).
const CHUNK: usize = 60_000;
/// Poly1305 tag length.
const TAG: usize = 16;

/// The send side's mutable state, behind one `Mutex` so concurrent `send`s
/// serialize and nonces are assigned in the order frames hit the wire.
struct Writer {
    /// A clone of the socket used only for writing.
    stream: TcpStream,
    /// The next outgoing Noise nonce.
    nonce: u64,
}

/// The sending half of a connection. Cheap to clone; sends are serialized.
#[derive(Clone)]
pub struct Link {
    /// Shared with the reader half; stateless, so `&self` suffices to encrypt.
    state: Arc<StatelessTransportState>,
    writer: Arc<Mutex<Writer>>,
    /// Remote address captured at connect time (it may be unavailable later).
    peer: Option<SocketAddr>,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

/// The receiving half of a connection.
pub struct LinkReader {
    state: Arc<StatelessTransportState>,
    /// A clone of the socket used only for reading.
    stream: TcpStream,
    /// The next expected incoming Noise nonce; a mismatch fails decryption.
    nonce: u64,
}

impl std::fmt::Debug for LinkReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkReader").finish_non_exhaustive()
    }
}

/// What a forwarder thread reports.
#[derive(Debug)]
pub enum LinkEvent {
    /// A message arrived.
    Message(Message),
    /// The connection ended (cleanly or not); nothing follows.
    Closed(NetError),
}

/// Build both halves from a finished handshake and its socket.
pub(crate) fn split(
    state: StatelessTransportState,
    stream: &TcpStream,
) -> Result<(Link, LinkReader), NetError> {
    let state = Arc::new(state);
    let peer = stream.peer_addr().ok();
    let link = Link {
        state: Arc::clone(&state),
        writer: Arc::new(Mutex::new(Writer {
            stream: stream.try_clone()?,
            nonce: 0,
        })),
        peer,
    };
    let reader = LinkReader {
        state,
        stream: stream.try_clone()?,
        nonce: 0,
    };
    Ok((link, reader))
}

/// Write one raw frame (big-endian `u16` length prefix + body) and flush.
/// Errors with [`NetError::TooLarge`] if `body` does not fit in a `u16`.
pub(crate) fn write_frame(stream: &mut TcpStream, body: &[u8]) -> Result<(), NetError> {
    let len = u16::try_from(body.len()).map_err(|_| NetError::TooLarge)?;
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(body);
    stream.write_all(&out)?;
    stream.flush()?;
    Ok(())
}

/// Read one raw frame. The length prefix is a `u16`, so the allocation is
/// bounded at 64 KiB per frame regardless of what the peer claims.
pub(crate) fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, NetError> {
    let mut len = [0u8; 2];
    stream.read_exact(&mut len)?;
    let mut body = vec![0u8; usize::from(u16::from_be_bytes(len))];
    stream.read_exact(&mut body)?;
    Ok(body)
}

impl Link {
    /// The peer's socket address, if known.
    #[must_use]
    pub const fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Encrypt and send one message.
    ///
    /// # Errors
    ///
    /// [`NetError::TooLarge`] for a message over [`MAX_MESSAGE_BYTES`], or an
    /// I/O error.
    pub fn send(&self, msg: &Message) -> Result<(), NetError> {
        self.send_encoded(&msg.encode())
    }

    /// Encrypt and send a payload that is already encoded. [`send`](Self::send)
    /// is the normal door; this one exists so a message family this build
    /// does not model yet (the reserved chat tags, T756) can be carried and
    /// tested. The receiver decodes and validates it like any other message.
    ///
    /// # Errors
    ///
    /// As [`send`](Self::send).
    pub fn send_encoded(&self, data: &[u8]) -> Result<(), NetError> {
        if data.len() > MAX_MESSAGE_BYTES {
            return Err(NetError::TooLarge);
        }
        // Hold the lock across the whole message so chunks of two concurrent
        // sends can never interleave on the wire.
        let mut w = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        // An empty message still needs one (empty, final) chunk.
        let chunks: Vec<&[u8]> = if data.is_empty() {
            vec![&[][..]]
        } else {
            data.chunks(CHUNK).collect()
        };
        let last = chunks.len() - 1;
        for (i, chunk) in chunks.into_iter().enumerate() {
            if w.nonce == u64::MAX {
                return Err(NetError::Crypto);
            }
            // Flag byte: 1 marks the final chunk of the message.
            let mut plain = Vec::with_capacity(1 + chunk.len());
            plain.push(u8::from(i == last));
            plain.extend_from_slice(chunk);
            let mut ct = vec![0u8; plain.len() + TAG];
            let n = self.state.write_message(w.nonce, &plain, &mut ct)?;
            w.nonce += 1;
            write_frame(&mut w.stream, &ct[..n])?;
        }
        Ok(())
    }

    /// Close the connection; a blocked [`LinkReader::recv`] returns.
    pub fn close(&self) {
        let w = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        // Shutting down the shared socket (not just our clone) is what wakes a
        // reader blocked in `recv` on the other half. Errors mean already closed.
        let _ = w.stream.shutdown(Shutdown::Both);
    }
}

impl LinkReader {
    /// Bound how long [`recv`](Self::recv) waits (`None` = forever). A timeout
    /// mid-message is fatal to the connection: partial frames are not resumed.
    ///
    /// # Errors
    ///
    /// When the socket rejects the timeout.
    pub fn set_timeout(&self, timeout: Option<Duration>) -> Result<(), NetError> {
        self.stream.set_read_timeout(timeout)?;
        Ok(())
    }

    /// Receive the next message, blocking.
    ///
    /// # Errors
    ///
    /// [`NetError::Closed`] at end of stream, [`NetError::Timeout`],
    /// [`NetError::Crypto`] for a frame that fails authentication,
    /// [`NetError::TooLarge`] for an oversized message, or
    /// [`NetError::Cowork`] for a decrypted payload that is not a message.
    /// A message in a reserved family (see
    /// `vix_cowork::wire::RESERVED_CHAT_TAGS`) is skipped, not an error, so a
    /// newer peer's chat does not end an older peer's session.
    pub fn recv(&mut self) -> Result<Message, NetError> {
        let mut assembled: Vec<u8> = Vec::new();
        loop {
            let ct = read_frame(&mut self.stream)?;
            // A valid frame holds at least the flag byte plus the auth tag.
            if ct.len() < TAG + 1 {
                return Err(NetError::Crypto);
            }
            if self.nonce == u64::MAX {
                return Err(NetError::Crypto);
            }
            let mut plain = vec![0u8; ct.len()];
            let n = self.state.read_message(self.nonce, &ct, &mut plain)?;
            self.nonce += 1;
            let (flag, body) = plain[..n].split_first().ok_or(NetError::Crypto)?;
            if assembled.len() + body.len() > MAX_MESSAGE_BYTES {
                return Err(NetError::TooLarge);
            }
            assembled.extend_from_slice(body);
            match flag {
                // Final chunk: decode. A reserved (future chat) family is dropped
                // so a newer peer cannot end an older peer's session.
                1 => match wire::decode(&assembled) {
                    Err(e) if e.is_reserved_message() => assembled.clear(),
                    decoded => return Ok(decoded?),
                },
                0 => {}
                _ => return Err(NetError::Protocol),
            }
        }
    }

    /// Run a reader thread that forwards every message, tagged with `tag`,
    /// into `tx`, and ends with [`LinkEvent::Closed`]. The thread also ends
    /// silently if the receiving end of `tx` is dropped. `tag` identifies the
    /// connection when many readers share one channel (the host's guests).
    pub fn spawn<K>(mut self, tag: K, tx: Sender<(K, LinkEvent)>) -> JoinHandle<()>
    where
        K: Clone + Send + 'static,
    {
        std::thread::spawn(move || {
            loop {
                match self.recv() {
                    Ok(msg) => {
                        if tx.send((tag.clone(), LinkEvent::Message(msg))).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send((tag, LinkEvent::Closed(e)));
                        return;
                    }
                }
            }
        })
    }
}
