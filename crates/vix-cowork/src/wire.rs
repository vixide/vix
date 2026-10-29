//! The application messages two Cowork peers exchange.
//!
//! This is the *payload* format only. Encryption, authentication and framing
//! belong to the transport (`vix-cowork-net`); a [`Message`] travels inside
//! one authenticated, encrypted frame. The codec is hand-written, length
//! checked on every field, and total: [`decode`] returns an error for any
//! input and never panics, which a property test pins.
//!
//! ## Message tags and channels
//!
//! The first byte of a message is its tag. Tags are grouped by [`Channel`] so
//! a later message family slots in without renumbering anything:
//!
//! | Tags | Channel | Messages |
//! | ---- | ------- | -------- |
//! | 1, 2, 9–13 | [`Channel::Control`] | `Hello`, `Welcome`, `PermissionChanged`, `PeerJoined`, `PeerLeft`, `Leave`, `Error` |
//! | 3–5, 7, 8 | [`Channel::Document`] | `Open`, `Close`, `Update`, `SaveRequest`, `Saved` |
//! | 6 | [`Channel::Presence`] | `Presence` |
//! | 14–16 | [`Channel::Workspace`] | `ListFiles`, `FileList`, `OpenRequest` (T696) |
//! | 48–63 | reserved: chat (T756) | none yet; see [`RESERVED_CHAT_TAGS`] |
//!
//! **The chat seam (T756).** Session chat will be its own message family
//! inside the same encrypted frames, in the reserved tag range, with a new
//! `Channel::Chat`. Until then [`decode`] reports a tag in that range as
//! [`WireProblem::Reserved`], which [`CoworkError::is_reserved_message`]
//! recognizes, so a receiver skips the message and keeps the connection
//! instead of treating it as a protocol violation. A peer that speaks chat
//! can therefore talk to one that does not.

use std::ops::RangeInclusive;

use crate::doc::Anchor;
use crate::error::{CoworkError, WireProblem};
use crate::path::SharedPath;
use crate::peer::{PeerId, Permission};

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: u8 = 1;

/// Largest single message, in bytes. A frame larger than this is refused
/// before it is allocated.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// Longest string field (names), in bytes.
const MAX_STR: usize = 256;

/// Most paths one [`Message::FileList`] carries; the host truncates beyond it.
pub const MAX_LISTED_FILES: usize = 20_000;

/// Tags kept free for session chat (T756, `spec/instant-message`). A message
/// in this range decodes to [`WireProblem::Reserved`], which a receiver skips.
pub const RESERVED_CHAT_TAGS: RangeInclusive<u8> = 48..=63;

/// Which family a [`Message`] belongs to. Lets the session layer route a
/// message family as a whole (for example, chat later) without matching on
/// every variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Channel {
    /// Joining, leaving, permissions, errors.
    Control,
    /// Shared buffer contents and saves.
    Document,
    /// Cursors and selections.
    Presence,
    /// The shared file tree (T696).
    Workspace,
}

/// Why the other side refused or ended something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The sender lacks the needed permission.
    PermissionDenied,
    /// The buffer would exceed the size cap.
    TooLarge,
    /// A path was malformed or not shared.
    BadPath,
    /// The protocol versions do not match.
    Version,
    /// The host removed the guest from the session.
    Removed,
    /// The host declined to let the guest join.
    Declined,
}

impl ErrorCode {
    const fn to_byte(self) -> u8 {
        match self {
            Self::PermissionDenied => 0,
            Self::TooLarge => 1,
            Self::BadPath => 2,
            Self::Version => 3,
            Self::Removed => 4,
            Self::Declined => 5,
        }
    }

    const fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::PermissionDenied,
            1 => Self::TooLarge,
            2 => Self::BadPath,
            3 => Self::Version,
            4 => Self::Removed,
            5 => Self::Declined,
            _ => return None,
        })
    }
}

/// Where a peer's cursor and selection are in one shared buffer.
///
/// On the way to the host the `peer` field is ignored: the host stamps the
/// connection's real peer id before relaying, so presence cannot be spoofed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    /// Whose cursor this is.
    pub peer: PeerId,
    /// Which buffer.
    pub path: SharedPath,
    /// The caret.
    pub head: Anchor,
    /// The other end of a selection, if any.
    pub anchor: Option<Anchor>,
}

/// One application message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message {
    /// Guest to host, first message: who I am.
    Hello {
        /// The guest's protocol version.
        version: u8,
        /// The name the guest asks to be shown as.
        name: String,
    },
    /// Host to guest: you are in, as this peer, with this permission.
    Welcome {
        /// The guest's session identity.
        peer: PeerId,
        /// What the host allows it.
        permission: Permission,
        /// The host's display name.
        host_name: String,
    },
    /// Host to guest: a buffer is shared; here is its state.
    Open {
        /// The buffer.
        path: SharedPath,
        /// A full snapshot to start from.
        snapshot: Vec<u8>,
    },
    /// Host to guest: the buffer is no longer shared.
    Close {
        /// The buffer.
        path: SharedPath,
    },
    /// Either direction: an incremental CRDT update.
    Update {
        /// The buffer.
        path: SharedPath,
        /// The update bytes.
        bytes: Vec<u8>,
    },
    /// Either direction: a cursor moved.
    Presence(Presence),
    /// Guest to host: please write this buffer to disk.
    SaveRequest {
        /// The buffer.
        path: SharedPath,
    },
    /// Host to guest: the buffer was written.
    Saved {
        /// The buffer.
        path: SharedPath,
    },
    /// Host to guest: your permission changed.
    PermissionChanged {
        /// The new level.
        permission: Permission,
    },
    /// Host to guests: someone joined.
    PeerJoined {
        /// Who.
        peer: PeerId,
        /// Their display name.
        name: String,
    },
    /// Host to guests: someone left.
    PeerLeft {
        /// Who.
        peer: PeerId,
    },
    /// Either direction: I am leaving.
    Leave,
    /// Either direction: a request was refused.
    Error {
        /// Why.
        code: ErrorCode,
    },
    /// Guest to host: list the files I may open (needs workspace sharing).
    ListFiles,
    /// Host to guest: the files the guest may open, relative to the root.
    FileList {
        /// The paths, at most [`MAX_LISTED_FILES`].
        paths: Vec<SharedPath>,
    },
    /// Guest to host: please share this file with the session.
    OpenRequest {
        /// The file, relative to the host's root.
        path: SharedPath,
    },
}

impl Message {
    /// The family this message belongs to.
    #[must_use]
    pub const fn channel(&self) -> Channel {
        match self {
            Self::Open { .. }
            | Self::Close { .. }
            | Self::Update { .. }
            | Self::SaveRequest { .. }
            | Self::Saved { .. } => Channel::Document,
            Self::Presence(_) => Channel::Presence,
            Self::ListFiles | Self::FileList { .. } | Self::OpenRequest { .. } => {
                Channel::Workspace
            }
            Self::Hello { .. }
            | Self::Welcome { .. }
            | Self::PermissionChanged { .. }
            | Self::PeerJoined { .. }
            | Self::PeerLeft { .. }
            | Self::Leave
            | Self::Error { .. } => Channel::Control,
        }
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        // Messages are bounded far below u32::MAX by MAX_MESSAGE_BYTES.
        let n = u32::try_from(b.len()).unwrap_or(u32::MAX);
        self.0.extend_from_slice(&n.to_le_bytes());
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireProblem> {
        let end = self.pos.checked_add(n).ok_or(WireProblem::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(WireProblem::Truncated)?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, WireProblem> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64, WireProblem> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(a))
    }
    fn bytes(&mut self, max: usize) -> Result<&'a [u8], WireProblem> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        let n = usize::try_from(u32::from_le_bytes(a)).map_err(|_| WireProblem::FieldTooLong)?;
        if n > max {
            return Err(WireProblem::FieldTooLong);
        }
        self.take(n)
    }
    fn str(&mut self) -> Result<String, WireProblem> {
        std::str::from_utf8(self.bytes(MAX_STR)?)
            .map(str::to_owned)
            .map_err(|_| WireProblem::BadUtf8)
    }
    fn path(&mut self) -> Result<SharedPath, CoworkError> {
        let raw = self.bytes(crate::path::MAX_PATH_BYTES)?;
        let s = std::str::from_utf8(raw).map_err(|_| WireProblem::BadUtf8)?;
        SharedPath::parse(s)
    }
    fn u32(&mut self) -> Result<u32, WireProblem> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(a))
    }
    fn blob(&mut self) -> Result<Vec<u8>, WireProblem> {
        Ok(self.bytes(MAX_MESSAGE_BYTES)?.to_vec())
    }
}

impl Message {
    /// Encode to bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        match self {
            Self::Hello { version, name } => {
                w.u8(1);
                w.u8(*version);
                w.str(name);
            }
            Self::Welcome {
                peer,
                permission,
                host_name,
            } => {
                w.u8(2);
                w.u64(peer.0);
                w.u8(permission.to_byte());
                w.str(host_name);
            }
            Self::Open { path, snapshot } => {
                w.u8(3);
                w.str(path.as_str());
                w.bytes(snapshot);
            }
            Self::Close { path } => {
                w.u8(4);
                w.str(path.as_str());
            }
            Self::Update { path, bytes } => {
                w.u8(5);
                w.str(path.as_str());
                w.bytes(bytes);
            }
            Self::Presence(p) => {
                w.u8(6);
                w.u64(p.peer.0);
                w.str(p.path.as_str());
                w.bytes(p.head.as_bytes());
                match &p.anchor {
                    Some(a) => {
                        w.u8(1);
                        w.bytes(a.as_bytes());
                    }
                    None => w.u8(0),
                }
            }
            Self::SaveRequest { path } => {
                w.u8(7);
                w.str(path.as_str());
            }
            Self::Saved { path } => {
                w.u8(8);
                w.str(path.as_str());
            }
            Self::PermissionChanged { permission } => {
                w.u8(9);
                w.u8(permission.to_byte());
            }
            Self::PeerJoined { peer, name } => {
                w.u8(10);
                w.u64(peer.0);
                w.str(name);
            }
            Self::PeerLeft { peer } => {
                w.u8(11);
                w.u64(peer.0);
            }
            Self::Leave => w.u8(12),
            Self::Error { code } => {
                w.u8(13);
                w.u8(code.to_byte());
            }
            Self::ListFiles => w.u8(14),
            Self::FileList { paths } => {
                w.u8(15);
                let shown = &paths[..paths.len().min(MAX_LISTED_FILES)];
                // Bounded by MAX_LISTED_FILES, far below u32::MAX.
                w.u32(u32::try_from(shown.len()).unwrap_or(u32::MAX));
                for p in shown {
                    w.str(p.as_str());
                }
            }
            Self::OpenRequest { path } => {
                w.u8(16);
                w.str(path.as_str());
            }
        }
        w.0
    }
}

/// Decode one message. Total: any input yields a message or an error.
///
/// # Errors
///
/// [`CoworkError::Wire`] for a malformed message, [`CoworkError::BadPath`] for
/// a path that fails [`SharedPath::parse`], [`CoworkError::Crdt`] for an
/// anchor that does not decode.
pub fn decode(buf: &[u8]) -> Result<Message, CoworkError> {
    if buf.len() > MAX_MESSAGE_BYTES {
        return Err(WireProblem::FieldTooLong.into());
    }
    let mut r = Reader { buf, pos: 0 };
    let tag = r.u8()?;
    let msg = match tag {
        1 => Message::Hello {
            version: r.u8()?,
            name: r.str()?,
        },
        2 => Message::Welcome {
            peer: PeerId(r.u64()?),
            permission: Permission::from_byte(r.u8()?)?,
            host_name: r.str()?,
        },
        3 => Message::Open {
            path: r.path()?,
            snapshot: r.blob()?,
        },
        4 => Message::Close { path: r.path()? },
        5 => Message::Update {
            path: r.path()?,
            bytes: r.blob()?,
        },
        6 => {
            let peer = PeerId(r.u64()?);
            let path = r.path()?;
            let head = Anchor::from_bytes(r.bytes(MAX_STR)?)?;
            let anchor = match r.u8()? {
                0 => None,
                1 => Some(Anchor::from_bytes(r.bytes(MAX_STR)?)?),
                other => return Err(WireProblem::UnknownTag(other).into()),
            };
            Message::Presence(Presence {
                peer,
                path,
                head,
                anchor,
            })
        }
        7 => Message::SaveRequest { path: r.path()? },
        8 => Message::Saved { path: r.path()? },
        9 => Message::PermissionChanged {
            permission: Permission::from_byte(r.u8()?)?,
        },
        10 => Message::PeerJoined {
            peer: PeerId(r.u64()?),
            name: r.str()?,
        },
        11 => Message::PeerLeft {
            peer: PeerId(r.u64()?),
        },
        12 => Message::Leave,
        13 => {
            let b = r.u8()?;
            Message::Error {
                code: ErrorCode::from_byte(b).ok_or(WireProblem::UnknownTag(b))?,
            }
        }
        14 => Message::ListFiles,
        15 => {
            let n = usize::try_from(r.u32()?).map_err(|_| WireProblem::FieldTooLong)?;
            if n > MAX_LISTED_FILES {
                return Err(WireProblem::FieldTooLong.into());
            }
            // Grown as paths decode, so a lying count allocates nothing.
            let mut paths = Vec::new();
            for _ in 0..n {
                paths.push(r.path()?);
            }
            Message::FileList { paths }
        }
        16 => Message::OpenRequest { path: r.path()? },
        t if RESERVED_CHAT_TAGS.contains(&t) => return Err(WireProblem::Reserved(t).into()),
        other => return Err(WireProblem::UnknownTag(other).into()),
    };
    if r.pos != buf.len() {
        return Err(WireProblem::TrailingBytes.into());
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{DEFAULT_CAP_BYTES, SharedDoc};
    use proptest::prelude::*;

    fn p(s: &str) -> SharedPath {
        SharedPath::parse(s).unwrap()
    }

    fn samples() -> Vec<Message> {
        let doc = SharedDoc::host(PeerId::HOST, "hello", DEFAULT_CAP_BYTES).unwrap();
        let a = doc.anchor_at(2).unwrap();
        vec![
            Message::Hello {
                version: PROTOCOL_VERSION,
                name: "Ada".into(),
            },
            Message::Welcome {
                peer: PeerId(7),
                permission: Permission::Edit,
                host_name: "Host".into(),
            },
            Message::Open {
                path: p("src/a.rs"),
                snapshot: vec![1, 2, 3],
            },
            Message::Close { path: p("a") },
            Message::Update {
                path: p("a"),
                bytes: vec![9; 40],
            },
            Message::Presence(Presence {
                peer: PeerId(3),
                path: p("a"),
                head: a.clone(),
                anchor: Some(a),
            }),
            Message::SaveRequest { path: p("a") },
            Message::Saved { path: p("a") },
            Message::PermissionChanged {
                permission: Permission::View,
            },
            Message::PeerJoined {
                peer: PeerId(4),
                name: "Bo".into(),
            },
            Message::PeerLeft { peer: PeerId(4) },
            Message::Leave,
            Message::Error {
                code: ErrorCode::Removed,
            },
            Message::ListFiles,
            Message::FileList {
                paths: vec![p("a"), p("src/b.rs")],
            },
            Message::OpenRequest {
                path: p("src/b.rs"),
            },
        ]
    }

    #[test]
    fn every_message_has_a_channel() {
        assert_eq!(Message::Leave.channel(), Channel::Control);
        assert_eq!(Message::ListFiles.channel(), Channel::Workspace);
        assert_eq!(
            Message::SaveRequest { path: p("a") }.channel(),
            Channel::Document
        );
    }

    #[test]
    fn a_reserved_chat_tag_is_skippable_not_fatal() {
        for tag in RESERVED_CHAT_TAGS {
            let err = decode(&[tag, 1, 2, 3]).unwrap_err();
            assert_eq!(err, WireProblem::Reserved(tag).into());
            assert!(err.is_reserved_message());
        }
        assert!(!decode(&[200]).unwrap_err().is_reserved_message());
        assert!(!decode(&[]).unwrap_err().is_reserved_message());
    }

    #[test]
    fn a_file_list_with_a_lying_count_or_a_hostile_path_is_refused() {
        let mut w = Writer(Vec::new());
        w.u8(15);
        w.u32(u32::MAX);
        assert_eq!(decode(&w.0), Err(WireProblem::FieldTooLong.into()));
        let mut w = Writer(Vec::new());
        w.u8(15);
        w.u32(3);
        w.str("a");
        assert_eq!(decode(&w.0), Err(WireProblem::Truncated.into()));
        let mut w = Writer(Vec::new());
        w.u8(15);
        w.u32(1);
        w.str("/etc/passwd");
        assert!(matches!(decode(&w.0), Err(CoworkError::BadPath(_))));
    }

    #[test]
    fn every_message_round_trips() {
        for m in samples() {
            assert_eq!(decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn truncation_at_every_length_is_an_error() {
        for m in samples() {
            let bytes = m.encode();
            for n in 0..bytes.len() {
                assert!(decode(&bytes[..n]).is_err(), "{m:?} cut to {n}");
            }
        }
    }

    #[test]
    fn trailing_bytes_and_unknown_tags_are_refused() {
        let mut b = Message::Leave.encode();
        b.push(0);
        assert_eq!(decode(&b), Err(WireProblem::TrailingBytes.into()));
        assert_eq!(decode(&[200]), Err(WireProblem::UnknownTag(200).into()));
        assert_eq!(decode(&[]), Err(WireProblem::Truncated.into()));
    }

    #[test]
    fn a_hostile_path_in_a_message_is_refused() {
        let mut w = Writer(Vec::new());
        w.u8(4);
        w.str("../../etc/passwd");
        assert!(matches!(decode(&w.0), Err(CoworkError::BadPath(_))));
    }

    #[test]
    fn an_oversized_length_prefix_is_refused_without_allocating() {
        let mut b = vec![5u8];
        b.extend_from_slice(&1u32.to_le_bytes());
        b.push(b'a');
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode(&b), Err(WireProblem::FieldTooLong.into()));
    }

    proptest! {
        #[test]
        fn decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
            let _ = decode(&bytes);
        }

        #[test]
        fn decode_never_panics_on_mutated_valid_messages(idx in 0usize..16, pos in 0usize..64, val in any::<u8>()) {
            let mut bytes = samples()[idx].encode();
            let at = pos % bytes.len();
            bytes[at] = val;
            let _ = decode(&bytes);
        }
    }
}
