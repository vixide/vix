//! The transport's typed error.
//!
//! One flat enum so callers can match on *why* a connection ended without
//! parsing strings. Several variants are deliberately vague (see
//! [`NetError::Crypto`]) so a failing peer learns nothing from the error path.
//! Spec: `crates/vix-cowork-net/spec/index.md`.

use std::fmt;
use vix_cowork::{CoworkError, ErrorCode};

/// Why a Cowork connection failed or was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetError {
    /// A socket operation failed.
    Io(std::io::ErrorKind),
    /// A wait ran out of time.
    Timeout,
    /// The Noise handshake or a frame failed authentication or decryption.
    /// Deliberately carries no detail: an attacker learns nothing from it.
    Crypto,
    /// The peer did not speak the Cowork protocol (bad preamble or order).
    Protocol,
    /// The invite string is malformed.
    BadInvite,
    /// The invite is unknown, expired, already used or revoked.
    UnknownInvite,
    /// A frame or message was larger than the limit.
    TooLarge,
    /// The other side closed the connection.
    Closed,
    /// The host refused the join (or removed the guest).
    Refused(ErrorCode),
    /// A decrypted message was not valid Cowork data.
    Cowork(CoworkError),
}

/// Human-readable, English-only text for logs; the UI maps variants to
/// translated strings itself, so these are not user-facing.
impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(k) => write!(f, "i/o error: {k}"),
            Self::Timeout => f.write_str("timed out"),
            Self::Crypto => f.write_str("authentication failed"),
            Self::Protocol => f.write_str("protocol violation"),
            Self::BadInvite => f.write_str("malformed invite"),
            Self::UnknownInvite => f.write_str("invite unknown, expired or used"),
            Self::TooLarge => f.write_str("message too large"),
            Self::Closed => f.write_str("connection closed"),
            Self::Refused(c) => write!(f, "refused: {c:?}"),
            Self::Cowork(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NetError {}

/// Classify an I/O error: timeouts and "peer went away" kinds get their own
/// variants (so callers can retry or tear down), anything else keeps its kind.
impl From<std::io::Error> for NetError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            // A socket read timeout surfaces as `WouldBlock` on Unix and
            // `TimedOut` on Windows.
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Self::Timeout,
            std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe => Self::Closed,
            k => Self::Io(k),
        }
    }
}

impl From<CoworkError> for NetError {
    fn from(e: CoworkError) -> Self {
        Self::Cowork(e)
    }
}

/// Every `snow` failure collapses to [`NetError::Crypto`], discarding detail on
/// purpose (no oracle for an attacker).
impl From<snow::Error> for NetError {
    fn from(_: snow::Error) -> Self {
        Self::Crypto
    }
}
