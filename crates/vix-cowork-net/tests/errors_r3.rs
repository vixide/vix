//! Round-3: the transport error's text and its classification of I/O errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::io::{Error, ErrorKind};

use vix_cowork::{CoworkError, ErrorCode};
use vix_cowork_net::NetError;

#[test]
fn display_is_stable_english_for_logs() {
    assert_eq!(
        NetError::Io(ErrorKind::PermissionDenied).to_string(),
        "i/o error: permission denied"
    );
    assert_eq!(NetError::Timeout.to_string(), "timed out");
    assert_eq!(NetError::Crypto.to_string(), "authentication failed");
    assert_eq!(NetError::Protocol.to_string(), "protocol violation");
    assert_eq!(NetError::BadInvite.to_string(), "malformed invite");
    assert_eq!(
        NetError::UnknownInvite.to_string(),
        "invite unknown, expired or used"
    );
    assert_eq!(NetError::TooLarge.to_string(), "message too large");
    assert_eq!(NetError::Closed.to_string(), "connection closed");
    assert_eq!(
        NetError::Refused(ErrorCode::PermissionDenied).to_string(),
        format!("refused: {:?}", ErrorCode::PermissionDenied)
    );
    assert_eq!(
        NetError::Cowork(CoworkError::Binary).to_string(),
        "binary content is not shared"
    );
    let _: &dyn std::error::Error = &NetError::Closed;
}

#[test]
fn io_errors_are_classified_by_kind() {
    for kind in [ErrorKind::WouldBlock, ErrorKind::TimedOut] {
        assert_eq!(
            NetError::from(Error::from(kind)),
            NetError::Timeout,
            "{kind:?}"
        );
    }
    for kind in [
        ErrorKind::UnexpectedEof,
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::BrokenPipe,
    ] {
        assert_eq!(
            NetError::from(Error::from(kind)),
            NetError::Closed,
            "{kind:?}"
        );
    }
    assert_eq!(
        NetError::from(Error::from(ErrorKind::AddrInUse)),
        NetError::Io(ErrorKind::AddrInUse)
    );
    assert_eq!(
        NetError::from(CoworkError::PermissionDenied),
        NetError::Cowork(CoworkError::PermissionDenied)
    );
}
