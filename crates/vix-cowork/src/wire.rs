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
//! | 17–21 | [`Channel::Terminal`] | `TerminalScreen`, `TerminalOutput`, `TerminalKeys`, `TerminalInputAllowed`, `TerminalClose` (T697) |
//! | 48, 49 | [`Channel::Chat`] | `Chat`, `ChatMuted` (T756) |
//! | 50–63 | reserved: later chat messages | see [`RESERVED_CHAT_TAGS`] |
//!
//! **Session chat (T756).** Chat is its own message family inside the same
//! encrypted frames, in the tag range 48–63 that T697 kept free for it. The
//! tags this build does not use yet (50–63) still decode to
//! [`WireProblem::Reserved`], which [`CoworkError::is_reserved_message`]
//! recognizes, so a receiver skips such a message and keeps the connection
//! instead of treating it as a protocol violation: a later build can add a
//! chat message and still share a session with this one.

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
pub const MAX_NAME_BYTES: usize = 256;
const MAX_STR: usize = MAX_NAME_BYTES;

/// Most paths one [`Message::FileList`] carries; the host truncates beyond it.
pub const MAX_LISTED_FILES: usize = 20_000;

/// Largest terminal screen or output chunk in one message (T697), in bytes.
/// A full screen is a few hundred kilobytes at most at the size caps below.
pub const MAX_TERMINAL_BYTES: usize = 4 * 1024 * 1024;

/// Largest burst of guest keystrokes in one [`Message::TerminalKeys`], in
/// bytes (a longer paste is refused by the sender).
pub const MAX_TERMINAL_KEYS: usize = 4096;

/// Most rows a shared terminal may have; a larger size is refused on decode,
/// so a hostile host cannot make a guest allocate a huge screen.
pub const MAX_TERMINAL_ROWS: u16 = 500;

/// Most columns a shared terminal may have (see [`MAX_TERMINAL_ROWS`]).
pub const MAX_TERMINAL_COLS: u16 = 1000;

/// Tags kept free for later session-chat messages (`spec/instant-message`).
/// T756 uses 48 and 49 from the original 48–63 range; a message in the rest
/// decodes to [`WireProblem::Reserved`], which a receiver skips.
pub const RESERVED_CHAT_TAGS: RangeInclusive<u8> = 50..=63;

/// Longest chat message body, or a snippet's code, in bytes (T756). Chat is
/// for people, so this is generous for a message and small for the link.
pub const MAX_CHAT_BYTES: usize = 16 * 1024;

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
    /// The host's shared terminal (T697).
    Terminal,
    /// Session chat (T756).
    Chat,
}

/// What a chat line carries besides its body (T756). Mirrors the code-aware
/// kinds of `vix-im`'s messages; system lines are never sent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChatKind {
    /// Ordinary text.
    Text,
    /// An action, from `/me`.
    Action,
    /// A cursor position in a file relative to the host's root.
    Location {
        /// The file.
        path: SharedPath,
        /// The line, 1-based.
        line: u32,
        /// The column, 1-based.
        column: u32,
    },
    /// Shared code: lines of a file relative to the host's root.
    Snippet {
        /// The language id for highlighting, possibly empty.
        language: String,
        /// The file.
        path: SharedPath,
        /// The first line, 1-based.
        start_line: u32,
        /// The last line, 1-based.
        end_line: u32,
        /// The code, at most [`MAX_CHAT_BYTES`].
        code: String,
    },
}

/// One session-chat message (T756).
///
/// On the way to the host the `from` field is ignored: the host stamps the
/// connection's verified peer id before relaying, as it does for presence,
/// so nobody can speak as someone else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    /// Who said it.
    pub from: PeerId,
    /// When the sender sent it, in milliseconds since the Unix epoch, UTC.
    pub time_utc: i64,
    /// The text, at most [`MAX_CHAT_BYTES`]. The receiver sanitizes it.
    pub body: String,
    /// What else it carries.
    pub kind: ChatKind,
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
    /// Host to guest: the host's terminal is shared (T697), or its whole
    /// screen is sent again (a resize, or a guest that just joined). The
    /// guest starts a fresh `rows` x `cols` emulator and feeds it `screen`.
    TerminalScreen {
        /// Screen height, 1 to [`MAX_TERMINAL_ROWS`].
        rows: u16,
        /// Screen width, 1 to [`MAX_TERMINAL_COLS`].
        cols: u16,
        /// Whether the host accepts keystrokes from guests with terminal
        /// permission. Off until the host allows it.
        input: bool,
        /// Escape sequences that draw the whole screen, at most
        /// [`MAX_TERMINAL_BYTES`].
        screen: Vec<u8>,
    },
    /// Host to guest: what changed on the shared terminal since the last
    /// screen or output, as escape sequences to feed the guest's emulator.
    TerminalOutput {
        /// At most [`MAX_TERMINAL_BYTES`].
        bytes: Vec<u8>,
    },
    /// Guest to host: keystrokes for the shared terminal, already encoded as
    /// terminal input. The host accepts them only while it allows input and
    /// the guest has terminal permission.
    TerminalKeys {
        /// At most [`MAX_TERMINAL_KEYS`].
        bytes: Vec<u8>,
    },
    /// Host to guest: the host allowed or stopped terminal input.
    TerminalInputAllowed {
        /// Whether guests with terminal permission may type.
        allowed: bool,
    },
    /// Host to guest: the terminal is no longer shared.
    TerminalClose,
    /// Either direction: a session-chat message (T756). The host relays a
    /// guest's line to everyone else with `from` stamped.
    Chat(ChatLine),
    /// Host to guest: the host muted (`true`) or unmuted this guest in the
    /// session chat (T756).
    ChatMuted {
        /// Whether the guest is muted now.
        muted: bool,
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
            Self::TerminalScreen { .. }
            | Self::TerminalOutput { .. }
            | Self::TerminalKeys { .. }
            | Self::TerminalInputAllowed { .. }
            | Self::TerminalClose => Channel::Terminal,
            Self::Chat(_) | Self::ChatMuted { .. } => Channel::Chat,
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
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
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
    fn i64(&mut self, v: i64) {
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
    fn u16(&mut self) -> Result<u16, WireProblem> {
        let mut a = [0u8; 2];
        a.copy_from_slice(self.take(2)?);
        Ok(u16::from_le_bytes(a))
    }
    fn flag(&mut self) -> Result<bool, WireProblem> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(WireProblem::UnknownTag(other)),
        }
    }
    fn u32(&mut self) -> Result<u32, WireProblem> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(a))
    }
    fn blob(&mut self) -> Result<Vec<u8>, WireProblem> {
        Ok(self.bytes(MAX_MESSAGE_BYTES)?.to_vec())
    }
    fn i64(&mut self) -> Result<i64, WireProblem> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(i64::from_le_bytes(a))
    }
    /// A string of at most `max` bytes (chat bodies and code).
    fn text(&mut self, max: usize) -> Result<String, WireProblem> {
        std::str::from_utf8(self.bytes(max)?)
            .map(str::to_owned)
            .map_err(|_| WireProblem::BadUtf8)
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
            Self::TerminalScreen { .. }
            | Self::TerminalOutput { .. }
            | Self::TerminalKeys { .. }
            | Self::TerminalInputAllowed { .. }
            | Self::TerminalClose => self.encode_terminal(&mut w),
            Self::Chat(line) => encode_chat(line, &mut w),
            Self::ChatMuted { muted } => {
                w.u8(49);
                w.u8(u8::from(*muted));
            }
        }
        w.0
    }

    /// The [`Channel::Terminal`] messages (T697), tags 17–21.
    fn encode_terminal(&self, w: &mut Writer) {
        match self {
            Self::TerminalScreen {
                rows,
                cols,
                input,
                screen,
            } => {
                w.u8(17);
                w.u16(*rows);
                w.u16(*cols);
                w.u8(u8::from(*input));
                w.bytes(screen);
            }
            Self::TerminalOutput { bytes } => {
                w.u8(18);
                w.bytes(bytes);
            }
            Self::TerminalKeys { bytes } => {
                w.u8(19);
                w.bytes(bytes);
            }
            Self::TerminalInputAllowed { allowed } => {
                w.u8(20);
                w.u8(u8::from(*allowed));
            }
            Self::TerminalClose => w.u8(21),
            _ => {}
        }
    }
}

/// A [`Message::Chat`] (T756), tag 48: sender, time, body, then the kind.
fn encode_chat(line: &ChatLine, w: &mut Writer) {
    w.u8(48);
    w.u64(line.from.0);
    w.i64(line.time_utc);
    w.str(&line.body);
    match &line.kind {
        ChatKind::Text => w.u8(0),
        ChatKind::Action => w.u8(1),
        ChatKind::Location { path, line, column } => {
            w.u8(2);
            w.str(path.as_str());
            w.u32(*line);
            w.u32(*column);
        }
        ChatKind::Snippet {
            language,
            path,
            start_line,
            end_line,
            code,
        } => {
            w.u8(3);
            w.str(language);
            w.str(path.as_str());
            w.u32(*start_line);
            w.u32(*end_line);
            w.str(code);
        }
    }
}

/// Decode a [`Message::Chat`] after its tag. A body or code over
/// [`MAX_CHAT_BYTES`], a language over the name limit, a hostile path and an
/// unknown kind are refused.
fn decode_chat(r: &mut Reader<'_>) -> Result<Message, CoworkError> {
    let from = PeerId(r.u64()?);
    let time_utc = r.i64()?;
    let body = r.text(MAX_CHAT_BYTES)?;
    let kind = match r.u8()? {
        0 => ChatKind::Text,
        1 => ChatKind::Action,
        2 => ChatKind::Location {
            path: r.path()?,
            line: r.u32()?,
            column: r.u32()?,
        },
        3 => ChatKind::Snippet {
            language: r.str()?,
            path: r.path()?,
            start_line: r.u32()?,
            end_line: r.u32()?,
            code: r.text(MAX_CHAT_BYTES)?,
        },
        other => return Err(WireProblem::UnknownTag(other).into()),
    };
    Ok(Message::Chat(ChatLine {
        from,
        time_utc,
        body,
        kind,
    }))
}

/// Decode the body of a [`Channel::Terminal`] message (T697) after its tag.
fn decode_terminal(tag: u8, r: &mut Reader<'_>) -> Result<Message, WireProblem> {
    Ok(match tag {
        17 => {
            let rows = r.u16()?;
            let cols = r.u16()?;
            if !(1..=MAX_TERMINAL_ROWS).contains(&rows) || !(1..=MAX_TERMINAL_COLS).contains(&cols)
            {
                return Err(WireProblem::BadTerminalSize);
            }
            Message::TerminalScreen {
                rows,
                cols,
                input: r.flag()?,
                screen: r.bytes(MAX_TERMINAL_BYTES)?.to_vec(),
            }
        }
        18 => Message::TerminalOutput {
            bytes: r.bytes(MAX_TERMINAL_BYTES)?.to_vec(),
        },
        19 => Message::TerminalKeys {
            bytes: r.bytes(MAX_TERMINAL_KEYS)?.to_vec(),
        },
        20 => Message::TerminalInputAllowed { allowed: r.flag()? },
        21 => Message::TerminalClose,
        other => return Err(WireProblem::UnknownTag(other)),
    })
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
        17..=21 => decode_terminal(tag, &mut r)?,
        48 => decode_chat(&mut r)?,
        49 => Message::ChatMuted { muted: r.flag()? },
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
            Message::TerminalScreen {
                rows: 24,
                cols: 80,
                input: false,
                screen: b"\x1b[H$ ls".to_vec(),
            },
            Message::TerminalOutput {
                bytes: b"\x1b[2;1Hdone".to_vec(),
            },
            Message::TerminalKeys {
                bytes: b"ls\r".to_vec(),
            },
            Message::TerminalInputAllowed { allowed: true },
            Message::TerminalClose,
            Message::Chat(ChatLine {
                from: PeerId(2),
                time_utc: 1_761_800_000_000,
                body: "see src/a.rs:3".into(),
                kind: ChatKind::Text,
            }),
            Message::Chat(ChatLine {
                from: PeerId::HOST,
                time_utc: -1,
                body: "waves".into(),
                kind: ChatKind::Action,
            }),
            Message::Chat(ChatLine {
                from: PeerId(3),
                time_utc: 0,
                body: "src/a.rs:3:7".into(),
                kind: ChatKind::Location {
                    path: p("src/a.rs"),
                    line: 3,
                    column: 7,
                },
            }),
            Message::Chat(ChatLine {
                from: PeerId(3),
                time_utc: 5,
                body: "src/a.rs:1-2".into(),
                kind: ChatKind::Snippet {
                    language: "rust".into(),
                    path: p("src/a.rs"),
                    start_line: 1,
                    end_line: 2,
                    code: "fn main() {\n}".into(),
                },
            }),
            Message::ChatMuted { muted: true },
        ]
    }

    fn chat(body: String, kind: ChatKind) -> Message {
        Message::Chat(ChatLine {
            from: PeerId(2),
            time_utc: 0,
            body,
            kind,
        })
    }

    /// T756: a body or code over the cap, a hostile path, an unknown kind
    /// and a mute flag other than 0 or 1 are refused on decode.
    #[test]
    fn hostile_chat_messages_are_refused() {
        let long = "x".repeat(MAX_CHAT_BYTES + 1);
        assert_eq!(
            decode(&chat(long.clone(), ChatKind::Text).encode()),
            Err(WireProblem::FieldTooLong.into())
        );
        let snippet = ChatKind::Snippet {
            language: String::new(),
            path: p("a"),
            start_line: 1,
            end_line: 1,
            code: long,
        };
        assert_eq!(
            decode(&chat(String::new(), snippet).encode()),
            Err(WireProblem::FieldTooLong.into())
        );
        // A body that claims 4 GB is refused before anything is allocated.
        let mut w = Writer(Vec::new());
        w.u8(48);
        w.u64(2);
        w.i64(0);
        w.u32(u32::MAX);
        assert_eq!(decode(&w.0), Err(WireProblem::FieldTooLong.into()));
        // A location outside the root.
        let mut w = Writer(Vec::new());
        w.u8(48);
        w.u64(2);
        w.i64(0);
        w.str("");
        w.u8(2);
        w.str("../secret");
        w.u32(1);
        w.u32(1);
        assert!(matches!(decode(&w.0), Err(CoworkError::BadPath(_))));
        // Invalid UTF-8 in the body, an unknown kind, a bad mute flag.
        let mut w = Writer(Vec::new());
        w.u8(48);
        w.u64(2);
        w.i64(0);
        w.bytes(&[0xff, 0xfe]);
        w.u8(0);
        assert_eq!(decode(&w.0), Err(WireProblem::BadUtf8.into()));
        let mut w = Writer(Vec::new());
        w.u8(48);
        w.u64(2);
        w.i64(0);
        w.str("hi");
        w.u8(9);
        assert_eq!(decode(&w.0), Err(WireProblem::UnknownTag(9).into()));
        assert_eq!(decode(&[49, 2]), Err(WireProblem::UnknownTag(2).into()));
        assert_eq!(chat("hi".into(), ChatKind::Text).channel(), Channel::Chat);
        assert_eq!(Message::ChatMuted { muted: false }.channel(), Channel::Chat);
    }

    #[test]
    fn every_message_has_a_channel() {
        assert_eq!(Message::Leave.channel(), Channel::Control);
        assert_eq!(Message::ListFiles.channel(), Channel::Workspace);
        assert_eq!(
            Message::SaveRequest { path: p("a") }.channel(),
            Channel::Document
        );
        assert_eq!(Message::TerminalClose.channel(), Channel::Terminal);
        assert_eq!(
            Message::TerminalKeys { bytes: vec![b'a'] }.channel(),
            Channel::Terminal
        );
    }

    /// T697: a terminal size outside the caps, an over-long burst of keys
    /// and a flag byte other than 0 or 1 are refused on decode.
    #[test]
    fn hostile_terminal_messages_are_refused() {
        for (rows, cols) in [
            (0, 80),
            (24, 0),
            (MAX_TERMINAL_ROWS + 1, 80),
            (24, MAX_TERMINAL_COLS + 1),
            (u16::MAX, u16::MAX),
        ] {
            let mut w = Writer(Vec::new());
            w.u8(17);
            w.u16(rows);
            w.u16(cols);
            w.u8(0);
            w.bytes(b"");
            assert_eq!(
                decode(&w.0),
                Err(WireProblem::BadTerminalSize.into()),
                "{rows}x{cols}"
            );
        }
        let keys = Message::TerminalKeys {
            bytes: vec![b'x'; MAX_TERMINAL_KEYS + 1],
        }
        .encode();
        assert_eq!(decode(&keys), Err(WireProblem::FieldTooLong.into()));
        assert_eq!(decode(&[20, 2]), Err(WireProblem::UnknownTag(2).into()));
        let mut w = Writer(Vec::new());
        w.u8(17);
        w.u16(24);
        w.u16(80);
        w.u8(7);
        w.bytes(b"");
        assert_eq!(decode(&w.0), Err(WireProblem::UnknownTag(7).into()));
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
        // T756 took 48 and 49 for chat: they are real messages now.
        assert!(!decode(&[48]).unwrap_err().is_reserved_message());
        assert_eq!(decode(&[49, 0]), Ok(Message::ChatMuted { muted: false }));
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
        fn decode_never_panics_on_mutated_valid_messages(idx in 0usize..64, pos in 0usize..64, val in any::<u8>()) {
            let all = samples();
            let mut bytes = all[idx % all.len()].encode();
            let at = pos % bytes.len();
            bytes[at] = val;
            let _ = decode(&bytes);
        }

        /// T697: every terminal message round-trips for any in-range size
        /// and payload.
        #[test]
        fn terminal_messages_round_trip(
            rows in 1u16..=MAX_TERMINAL_ROWS,
            cols in 1u16..=MAX_TERMINAL_COLS,
            input in any::<bool>(),
            bytes in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            for m in [
                Message::TerminalScreen { rows, cols, input, screen: bytes.clone() },
                Message::TerminalOutput { bytes: bytes.clone() },
                Message::TerminalInputAllowed { allowed: input },
                Message::TerminalKeys { bytes },
            ] {
                prop_assert_eq!(decode(&m.encode()).unwrap(), m);
            }
        }

        /// T756: every chat line round-trips for any text, time and range
        /// within the caps.
        #[test]
        fn chat_messages_round_trip(
            from in any::<u64>(),
            time_utc in any::<i64>(),
            body in ".{0,200}",
            code in "(.|\n){0,200}",
            language in "[a-z]{0,12}",
            line in any::<u32>(),
            column in any::<u32>(),
            kind in 0u8..4,
        ) {
            let kind = match kind {
                0 => ChatKind::Text,
                1 => ChatKind::Action,
                2 => ChatKind::Location { path: p("src/lib.rs"), line, column },
                _ => ChatKind::Snippet {
                    language,
                    path: p("src/lib.rs"),
                    start_line: line,
                    end_line: column,
                    code,
                },
            };
            let m = Message::Chat(ChatLine { from: PeerId(from), time_utc, body, kind });
            prop_assert_eq!(decode(&m.encode()).unwrap(), m);
        }
    }
}
