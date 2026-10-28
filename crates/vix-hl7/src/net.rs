//! The blocking MLLP sender (feature `net`): one connection, one message, one
//! reply, with timeouts on every step. It dials only the endpoint it is given
//! and keeps nothing -- no log, no cache -- since the payload may be PHI.

use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::mllp::{self, Deframer, MllpError};

/// An explicit `host:port` the user supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The host name or IP address (brackets of an IPv6 literal removed).
    pub host: String,
    /// The TCP port (never 0).
    pub port: u16,
}

/// Why an endpoint string was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EndpointError {
    /// The text is empty.
    Empty,
    /// There is no `:port` part.
    MissingPort,
    /// The port is not a number in 1..=65535.
    BadPort,
    /// The host is empty or contains whitespace or control characters.
    BadHost,
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EndpointError::Empty => "empty endpoint",
            EndpointError::MissingPort => "missing port",
            EndpointError::BadPort => "bad port",
            EndpointError::BadHost => "bad host",
        })
    }
}

impl std::error::Error for EndpointError {}

impl Endpoint {
    /// Read `host:port` or `[ipv6]:port`. The port is required: nothing is
    /// ever sent to a default.
    ///
    /// # Errors
    /// An [`EndpointError`] naming what is wrong with the text.
    pub fn parse(text: &str) -> Result<Endpoint, EndpointError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(EndpointError::Empty);
        }
        let (host, port) = text.rsplit_once(':').ok_or(EndpointError::MissingPort)?;
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(EndpointError::BadHost);
        }
        match port.parse::<u16>() {
            Ok(p) if p > 0 => Ok(Endpoint {
                host: host.to_string(),
                port: p,
            }),
            _ => Err(EndpointError::BadPort),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// Timeouts and limits for [`send`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendOptions {
    /// Per-address connect timeout.
    pub connect_timeout: Duration,
    /// Per-read and per-write timeout.
    pub io_timeout: Duration,
    /// How long to wait in total for the reply frame.
    pub reply_timeout: Duration,
    /// The largest reply frame accepted, in bytes.
    pub max_reply: usize,
}

impl Default for SendOptions {
    fn default() -> Self {
        SendOptions {
            connect_timeout: Duration::from_secs(5),
            io_timeout: Duration::from_secs(10),
            reply_timeout: Duration::from_secs(30),
            max_reply: 1 << 20,
        }
    }
}

/// Why a send failed. Carries error *kinds* only, never message content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SendError {
    /// The host name did not resolve.
    Resolve,
    /// No address accepted the connection.
    Connect(ErrorKind),
    /// A timeout elapsed while connecting, writing or waiting for the reply.
    Timeout,
    /// Writing the message failed.
    Write(ErrorKind),
    /// Reading the reply failed.
    Read(ErrorKind),
    /// The peer closed the connection before a whole reply frame arrived.
    Closed,
    /// The reply was not well-framed.
    Framing(MllpError),
}

impl SendError {
    /// A short stable code for the error class (for choosing a translation).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SendError::Resolve => "resolve",
            SendError::Connect(_) => "connect",
            SendError::Timeout => "timeout",
            SendError::Write(_) | SendError::Read(_) => "io",
            SendError::Closed => "closed",
            SendError::Framing(_) => "framing",
        }
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MLLP send failed ({})", self.code())
    }
}

impl std::error::Error for SendError {}

fn is_timeout(kind: ErrorKind) -> bool {
    matches!(kind, ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// Send one message (`payload`, segments separated by `\r`) and return the
/// reply frame's payload.
///
/// # Errors
/// A [`SendError`] describing which step failed.
pub fn send(ep: &Endpoint, payload: &[u8], opts: &SendOptions) -> Result<Vec<u8>, SendError> {
    let addrs = (ep.host.as_str(), ep.port)
        .to_socket_addrs()
        .map_err(|_| SendError::Resolve)?;
    let mut last = ErrorKind::NotFound;
    let mut stream = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, opts.connect_timeout) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) if is_timeout(e.kind()) => last = ErrorKind::TimedOut,
            Err(e) => last = e.kind(),
        }
    }
    let Some(mut stream) = stream else {
        return Err(if last == ErrorKind::TimedOut {
            SendError::Timeout
        } else {
            SendError::Connect(last)
        });
    };
    let _ = stream.set_read_timeout(Some(opts.io_timeout));
    let _ = stream.set_write_timeout(Some(opts.io_timeout));
    stream
        .write_all(&mllp::frame(payload))
        .and_then(|()| stream.flush())
        .map_err(|e| {
            if is_timeout(e.kind()) {
                SendError::Timeout
            } else {
                SendError::Write(e.kind())
            }
        })?;
    read_reply(&mut stream, opts)
}

fn read_reply(stream: &mut impl Read, opts: &SendOptions) -> Result<Vec<u8>, SendError> {
    let deadline = Instant::now() + opts.reply_timeout;
    let mut deframer = Deframer::new(opts.max_reply);
    let mut chunk = [0u8; 4096];
    loop {
        if Instant::now() >= deadline {
            return Err(SendError::Timeout);
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(SendError::Closed),
            Ok(n) => {
                let frames = deframer.push(&chunk[..n]).map_err(SendError::Framing)?;
                if let Some(first) = frames.into_iter().next() {
                    return Ok(first);
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if is_timeout(e.kind()) => return Err(SendError::Timeout),
            Err(e) => return Err(SendError::Read(e.kind())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoints() {
        let e = Endpoint::parse(" example.org:2575 ").unwrap();
        assert_eq!((e.host.as_str(), e.port), ("example.org", 2575));
        let v6 = Endpoint::parse("[::1]:2575").unwrap();
        assert_eq!(v6.host, "::1");
        assert_eq!(v6.to_string(), "[::1]:2575");
    }

    #[test]
    fn refuses_incomplete_endpoints() {
        assert_eq!(Endpoint::parse(""), Err(EndpointError::Empty));
        assert_eq!(Endpoint::parse("host"), Err(EndpointError::MissingPort));
        assert_eq!(Endpoint::parse("host:0"), Err(EndpointError::BadPort));
        assert_eq!(Endpoint::parse("host:99999"), Err(EndpointError::BadPort));
        assert_eq!(Endpoint::parse(":2575"), Err(EndpointError::BadHost));
        assert_eq!(Endpoint::parse("a b:1"), Err(EndpointError::BadHost));
    }
}
