//! The typed error for every fallible Cowork operation.

use std::fmt;

/// Why a Cowork operation was refused or failed.
///
/// Every variant is a recoverable condition the caller can show to the user or
/// answer with a protocol error; none carries document text, so an error is
/// safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoworkError {
    /// The text is larger than the shared-buffer cap.
    TooLarge {
        /// The size that was refused, in bytes.
        size: usize,
        /// The cap that applied, in bytes.
        cap: usize,
    },
    /// The text looks binary (contains a NUL byte); binary files are never shared.
    Binary,
    /// An edit named a range outside the document.
    OutOfRange {
        /// The offending start offset, in characters.
        offset: usize,
        /// The document length, in characters.
        len: usize,
    },
    /// The sender is not allowed to do that (its permission is too low).
    PermissionDenied,
    /// An update tried to author operations as someone other than its sender.
    ForgedPeer,
    /// An update was not an incremental update (for example a full snapshot).
    NotAnUpdate,
    /// A path escapes the shared root or is malformed.
    BadPath(PathProblem),
    /// The CRDT library rejected the bytes.
    Crdt(String),
    /// A wire message could not be decoded.
    Wire(WireProblem),
}

/// What is wrong with a shared path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathProblem {
    /// The path is empty.
    Empty,
    /// The path is absolute or has a drive or UNC prefix.
    Absolute,
    /// The path contains a `..` component.
    ParentEscape,
    /// The path contains a NUL or other control character.
    Control,
    /// The path is longer than the limit.
    TooLong,
}

/// What is wrong with a wire message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireProblem {
    /// The input ended before the message did.
    Truncated,
    /// The first byte names no known message.
    UnknownTag(u8),
    /// A permission byte named no known level.
    UnknownPermission(u8),
    /// A string field was not valid UTF-8.
    BadUtf8,
    /// A length prefix exceeded the maximum for that field.
    FieldTooLong,
    /// Bytes remained after a complete message.
    TrailingBytes,
    /// The protocol version is not one this build speaks.
    Version(u8),
    /// The tag is in a range reserved for a later message family (chat,
    /// T756; see `wire::RESERVED_CHAT_TAGS`). The message is skipped, not
    /// fatal.
    Reserved(u8),
    /// A shared terminal's size is zero or above the caps (T697,
    /// `wire::MAX_TERMINAL_ROWS` / `wire::MAX_TERMINAL_COLS`).
    BadTerminalSize,
}

impl fmt::Display for PathProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty",
            Self::Absolute => "absolute",
            Self::ParentEscape => "escapes the root",
            Self::Control => "control character",
            Self::TooLong => "too long",
        })
    }
}

impl fmt::Display for WireProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated"),
            Self::UnknownTag(t) => write!(f, "unknown message tag {t}"),
            Self::UnknownPermission(p) => write!(f, "unknown permission {p}"),
            Self::BadUtf8 => f.write_str("invalid UTF-8"),
            Self::FieldTooLong => f.write_str("field too long"),
            Self::TrailingBytes => f.write_str("trailing bytes"),
            Self::Version(v) => write!(f, "unsupported protocol version {v}"),
            Self::Reserved(t) => write!(f, "reserved message tag {t}"),
            Self::BadTerminalSize => f.write_str("terminal size out of range"),
        }
    }
}

impl fmt::Display for CoworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size, cap } => write!(f, "{size} bytes exceeds the {cap} byte cap"),
            Self::Binary => f.write_str("binary content is not shared"),
            Self::OutOfRange { offset, len } => {
                write!(
                    f,
                    "offset {offset} is outside a document of {len} characters"
                )
            }
            Self::PermissionDenied => f.write_str("permission denied"),
            Self::ForgedPeer => f.write_str("update authored as another peer"),
            Self::NotAnUpdate => f.write_str("not an incremental update"),
            Self::BadPath(p) => write!(f, "bad path: {p}"),
            Self::Crdt(e) => write!(f, "crdt: {e}"),
            Self::Wire(w) => write!(f, "wire: {w}"),
        }
    }
}

impl std::error::Error for CoworkError {}

impl CoworkError {
    /// Whether this is a message from a reserved, not yet understood family
    /// (see `wire::RESERVED_CHAT_TAGS`): the receiver skips it and keeps the
    /// connection.
    #[must_use]
    pub const fn is_reserved_message(&self) -> bool {
        matches!(self, Self::Wire(WireProblem::Reserved(_)))
    }
}

impl From<WireProblem> for CoworkError {
    fn from(w: WireProblem) -> Self {
        Self::Wire(w)
    }
}

impl From<PathProblem> for CoworkError {
    fn from(p: PathProblem) -> Self {
        Self::BadPath(p)
    }
}
