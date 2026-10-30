//! The remote agent's wire format (T717): length-prefixed frames, and the
//! messages inside them.
//!
//! A frame is a 4-byte big-endian payload length followed by that many
//! bytes; no payload is longer than [`MAX_FRAME`]. A payload is one
//! [`Message`], encoded with a small hand-written binary codec: integers are
//! big-endian, byte strings and lists carry a `u32` count, every enum a
//! one-byte tag. The decoder trusts nothing it reads: every length is checked
//! against the bytes actually left before anything is allocated, lists hold
//! at most [`MAX_BATCH`] items, batches don't nest, and trailing bytes are an
//! error. See `crates/vix-authority/spec/agent/index.md`.

use std::io::{self, Read, Write};
use std::time::{Duration, SystemTime};

use crate::{FileKind, FileStat, TextReadError, WalkOptions, WriteConflict};

/// The protocol revision. The handshake also compares the Vix version
/// exactly, so this only changes when the codec does.
pub const PROTOCOL: u32 = 1;

/// The first bytes of every [`Message::Hello`].
pub const MAGIC: &[u8; 8] = b"VIXAGENT";

/// The largest payload one frame may carry (64 MiB).
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

/// The most file bytes one request or reply carries. A larger read or write
/// is refused with [`ErrorCode::TooBig`] (the SSH authority then uses its
/// shell scripts for that one operation).
pub const MAX_DATA: u64 = (MAX_FRAME - 64 * 1024) as u64;

/// The most operations in one [`Op::Batch`], paths in one [`Op::Watch`], or
/// items in any list other than a directory listing or a walk.
pub const MAX_BATCH: usize = 4096;

/// Write one frame holding `payload`.
///
/// # Errors
/// `InvalidInput` for a payload over [`MAX_FRAME`], or the write error.
pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    let len = u32::try_from(payload.len()).map_err(|_| bad("frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Read one frame's payload; `None` at a clean end of input (no byte of a
/// new frame read).
///
/// # Errors
/// `InvalidData` for a length over [`MAX_FRAME`], `UnexpectedEof` for a
/// frame cut short, or the read error.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; 4];
    let mut got = 0;
    while got < head.len() {
        match r.read(&mut head[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes(head) as usize;
    if len > MAX_FRAME {
        return Err(bad("frame too large"));
    }
    // Read through `take`, so a lying length allocates only what arrives.
    let mut payload = Vec::new();
    r.take(len as u64).read_to_end(&mut payload)?;
    if payload.len() < len {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(Some(payload))
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// One frame's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// The handshake, sent first by each side.
    Hello {
        /// [`PROTOCOL`].
        protocol: u32,
        /// The sender's Vix version.
        version: String,
    },
    /// Client to agent: run `op` and answer with a [`Message::Reply`]
    /// carrying the same `id`.
    Request {
        /// Chosen by the client, echoed in the reply.
        id: u64,
        /// What to do.
        op: Op,
    },
    /// Agent to client: the answer to request `id`.
    Reply {
        /// The request's id.
        id: u64,
        /// The answer.
        reply: Reply,
    },
    /// Agent to client, unasked: watched paths changed.
    Event {
        /// The [`Op::Watch`] id.
        watch: u64,
        /// The paths whose kind, length or modification time changed (or
        /// that appeared or vanished) since the last check.
        paths: Vec<Vec<u8>>,
    },
}

/// What a save expects to find (the conflict check of
/// `Authority::write_atomic_if_unchanged`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Anything: a plain atomic write.
    Any,
    /// No file.
    Absent,
    /// The version this stat describes.
    Version(FileStat),
}

/// One request. Paths are the remote machine's bytes; `~` and `~/…` name its
/// home directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `metadata` (`follow`) or `symlink_metadata`.
    Stat {
        /// The path.
        path: Vec<u8>,
        /// Follow a symbolic link.
        follow: bool,
    },
    /// Read at most `max` bytes ([`Reply::Data`]).
    Read {
        /// The file.
        path: Vec<u8>,
        /// The most bytes wanted.
        max: u64,
    },
    /// The T571 guards (a regular file of at most `max` bytes), then up to
    /// `max + 1` bytes; the client decodes them.
    ReadText {
        /// The file.
        path: Vec<u8>,
        /// The size limit.
        max: u64,
    },
    /// A plain, truncating write.
    Write {
        /// The file.
        path: Vec<u8>,
        /// The new content.
        data: Vec<u8>,
    },
    /// An atomic write, with an optional conflict check.
    WriteAtomic {
        /// The file.
        path: Vec<u8>,
        /// The new content.
        data: Vec<u8>,
        /// What must be there first.
        expect: Expect,
    },
    /// The canonical path ([`Reply::Path`]).
    Canonicalize {
        /// The path.
        path: Vec<u8>,
    },
    /// A directory's entries ([`Reply::Entries`]).
    ReadDir {
        /// The directory.
        path: Vec<u8>,
    },
    /// `create_dir_all`.
    CreateDirAll {
        /// The directory.
        path: Vec<u8>,
    },
    /// `rename`.
    Rename {
        /// The source.
        from: Vec<u8>,
        /// The destination.
        to: Vec<u8>,
    },
    /// `remove_file`.
    RemoveFile {
        /// The file.
        path: Vec<u8>,
    },
    /// `remove_path` (a link is removed, never followed).
    RemovePath {
        /// The path.
        path: Vec<u8>,
    },
    /// `move_path`.
    MovePath {
        /// The source.
        from: Vec<u8>,
        /// The destination.
        to: Vec<u8>,
    },
    /// `copy_recursive`.
    CopyRecursive {
        /// The source.
        from: Vec<u8>,
        /// The destination.
        to: Vec<u8>,
    },
    /// Move to the remote user's trash.
    Trash {
        /// The path.
        path: Vec<u8>,
    },
    /// `is_unwritable` ([`Reply::Bool`]).
    IsUnwritable {
        /// The path.
        path: Vec<u8>,
    },
    /// `walk_files` ([`Reply::Paths`]).
    Walk {
        /// The root.
        root: Vec<u8>,
        /// How to walk.
        opts: WalkOptions,
    },
    /// `which` ([`Reply::MaybePath`]).
    Which {
        /// The program name.
        name: String,
    },
    /// Start reporting changes to `paths` as [`Message::Event`]s with this
    /// id (chosen by the client), checking every `interval_ms`.
    Watch {
        /// The subscription id.
        id: u64,
        /// The watched paths.
        paths: Vec<Vec<u8>>,
        /// How often to check, in milliseconds (clamped to 100–60000).
        interval_ms: u32,
    },
    /// Stop a [`Op::Watch`].
    Unwatch {
        /// The subscription id.
        id: u64,
    },
    /// Several operations in one round trip, answered by a [`Reply::Batch`]
    /// in the same order. A batch holds no batch, watch or unwatch.
    Batch(Vec<Op>),
}

impl Op {
    /// Whether running this twice is harmless: it changes nothing on the
    /// remote side. Such a request may be retried another way after the
    /// connection drops mid-request.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        match self {
            Self::Stat { .. }
            | Self::Read { .. }
            | Self::ReadText { .. }
            | Self::Canonicalize { .. }
            | Self::ReadDir { .. }
            | Self::IsUnwritable { .. }
            | Self::Walk { .. }
            | Self::Which { .. } => true,
            Self::Batch(ops) => ops.iter().all(Self::is_read_only),
            _ => false,
        }
    }
}

/// One directory entry of a [`Reply::Entries`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The entry's name.
    pub name: Vec<u8>,
    /// Its own kind (a link is a link).
    pub kind: FileKind,
    /// Whether it is a directory after following a link.
    pub is_dir: bool,
}

/// An error kind on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Anything else (the message says what).
    Other,
    /// `NotFound`.
    NotFound,
    /// `PermissionDenied`.
    PermissionDenied,
    /// `AlreadyExists`.
    AlreadyExists,
    /// `IsADirectory`.
    IsADirectory,
    /// `NotADirectory`.
    NotADirectory,
    /// `DirectoryNotEmpty`.
    DirectoryNotEmpty,
    /// A [`WriteConflict`].
    Conflict,
    /// [`TextReadError::NotRegular`].
    NotRegular,
    /// [`TextReadError::TooLarge`].
    TooLarge,
    /// [`TextReadError::Binary`].
    Binary,
    /// `Unsupported`.
    Unsupported,
    /// `InvalidInput`.
    InvalidInput,
    /// `InvalidData`.
    InvalidData,
    /// The request or its answer doesn't fit in a frame.
    TooBig,
}

const CODES: [ErrorCode; 15] = [
    ErrorCode::Other,
    ErrorCode::NotFound,
    ErrorCode::PermissionDenied,
    ErrorCode::AlreadyExists,
    ErrorCode::IsADirectory,
    ErrorCode::NotADirectory,
    ErrorCode::DirectoryNotEmpty,
    ErrorCode::Conflict,
    ErrorCode::NotRegular,
    ErrorCode::TooLarge,
    ErrorCode::Binary,
    ErrorCode::Unsupported,
    ErrorCode::InvalidInput,
    ErrorCode::InvalidData,
    ErrorCode::TooBig,
];

impl ErrorCode {
    /// The code that describes `e`.
    #[must_use]
    pub fn of(e: &io::Error) -> Self {
        use io::ErrorKind as K;
        if WriteConflict::is(e) {
            return Self::Conflict;
        }
        if let Some(t) = e.get_ref().and_then(|r| r.downcast_ref::<TextReadError>()) {
            return match t {
                TextReadError::NotRegular => Self::NotRegular,
                TextReadError::TooLarge => Self::TooLarge,
                TextReadError::Binary => Self::Binary,
            };
        }
        match e.kind() {
            K::NotFound => Self::NotFound,
            K::PermissionDenied => Self::PermissionDenied,
            K::AlreadyExists => Self::AlreadyExists,
            K::IsADirectory => Self::IsADirectory,
            K::NotADirectory => Self::NotADirectory,
            K::DirectoryNotEmpty => Self::DirectoryNotEmpty,
            K::Unsupported => Self::Unsupported,
            K::InvalidInput => Self::InvalidInput,
            K::InvalidData => Self::InvalidData,
            _ => Self::Other,
        }
    }

    /// The `io::Error` this code and `message` stand for.
    #[must_use]
    pub fn to_io(self, message: &str) -> io::Error {
        use io::ErrorKind as K;
        let kind = match self {
            Self::Conflict => return WriteConflict::error(),
            Self::NotRegular => return io::Error::new(K::InvalidData, TextReadError::NotRegular),
            Self::TooLarge => return io::Error::new(K::InvalidData, TextReadError::TooLarge),
            Self::Binary => return io::Error::new(K::InvalidData, TextReadError::Binary),
            Self::Other | Self::TooBig => K::Other,
            Self::NotFound => K::NotFound,
            Self::PermissionDenied => K::PermissionDenied,
            Self::AlreadyExists => K::AlreadyExists,
            Self::IsADirectory => K::IsADirectory,
            Self::NotADirectory => K::NotADirectory,
            Self::DirectoryNotEmpty => K::DirectoryNotEmpty,
            Self::Unsupported => K::Unsupported,
            Self::InvalidInput => K::InvalidInput,
            Self::InvalidData => K::InvalidData,
        };
        io::Error::new(kind, message.to_string())
    }

    fn tag(self) -> u8 {
        CODES
            .iter()
            .position(|c| *c == self)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(0)
    }
}

/// One answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The operation failed.
    Err {
        /// The error kind.
        code: ErrorCode,
        /// The remote error's text.
        message: String,
    },
    /// Done; nothing to return.
    Unit,
    /// A stat.
    Stat(FileStat),
    /// File bytes; `complete` when the file had no more.
    Data {
        /// The bytes read.
        bytes: Vec<u8>,
        /// Nothing was left unread.
        complete: bool,
    },
    /// One path.
    Path(Vec<u8>),
    /// A directory listing.
    Entries(Vec<Entry>),
    /// A yes or no.
    Bool(bool),
    /// Several paths.
    Paths(Vec<Vec<u8>>),
    /// A path, or none.
    MaybePath(Option<Vec<u8>>),
    /// A [`Op::Batch`]'s answers, in order.
    Batch(Vec<Reply>),
}

impl Reply {
    /// The reply for a failed operation.
    #[must_use]
    pub fn error(e: &io::Error) -> Self {
        Self::Err {
            code: ErrorCode::of(e),
            message: e.to_string(),
        }
    }
}

// ----- encoding -------------------------------------------------------------

struct Enc(Vec<u8>);

impl Enc {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn len(&mut self, n: usize) {
        // Longer than any frame: the frame check refuses the whole message.
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
    fn bytes(&mut self, v: &[u8]) {
        self.len(v.len());
        self.0.extend_from_slice(v);
    }
    fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn list<T>(&mut self, items: &[T], mut f: impl FnMut(&mut Self, &T)) {
        self.len(items.len());
        for item in items {
            f(self, item);
        }
    }
    fn time(&mut self, t: Option<SystemTime>) {
        let Some(t) = t else {
            self.u8(0);
            return;
        };
        let (tag, d) = match t.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(d) => (1, d),
            Err(e) => (2, e.duration()),
        };
        self.u8(tag);
        self.u64(d.as_secs());
        self.u32(d.subsec_nanos());
    }
    fn kind(&mut self, k: FileKind) {
        self.u8(match k {
            FileKind::File => 0,
            FileKind::Dir => 1,
            FileKind::Symlink => 2,
            FileKind::Other => 3,
        });
    }
    fn stat(&mut self, s: &FileStat) {
        self.kind(s.kind);
        self.u64(s.len);
        self.time(s.modified);
        self.time(s.created);
        self.bool(s.readonly);
        match s.mode {
            Some(m) => {
                self.u8(1);
                self.u32(m);
            }
            None => self.u8(0),
        }
    }
    fn op(&mut self, op: &Op) {
        match op {
            Op::Stat { path, follow } => {
                self.u8(1);
                self.bytes(path);
                self.bool(*follow);
            }
            Op::Read { path, max } => {
                self.u8(2);
                self.bytes(path);
                self.u64(*max);
            }
            Op::ReadText { path, max } => {
                self.u8(3);
                self.bytes(path);
                self.u64(*max);
            }
            Op::Write { path, data } => {
                self.u8(4);
                self.bytes(path);
                self.bytes(data);
            }
            Op::WriteAtomic { path, data, expect } => {
                self.u8(5);
                self.bytes(path);
                self.bytes(data);
                match expect {
                    Expect::Any => self.u8(0),
                    Expect::Absent => self.u8(1),
                    Expect::Version(s) => {
                        self.u8(2);
                        self.stat(s);
                    }
                }
            }
            Op::Canonicalize { path } => self.path_op(6, path),
            Op::ReadDir { path } => self.path_op(7, path),
            Op::CreateDirAll { path } => self.path_op(8, path),
            Op::Rename { from, to } => self.pair_op(9, from, to),
            Op::RemoveFile { path } => self.path_op(10, path),
            Op::RemovePath { path } => self.path_op(11, path),
            Op::MovePath { from, to } => self.pair_op(12, from, to),
            Op::CopyRecursive { from, to } => self.pair_op(13, from, to),
            Op::Trash { path } => self.path_op(14, path),
            Op::IsUnwritable { path } => self.path_op(15, path),
            Op::Walk { root, opts } => {
                self.u8(16);
                self.bytes(root);
                match opts.max_depth {
                    Some(d) => {
                        self.u8(1);
                        self.u64(d as u64);
                    }
                    None => self.u8(0),
                }
                self.bool(opts.skip_hidden);
                self.bool(opts.honor_ignore_files);
                self.list(&opts.prune_dirs, |e, d| e.str(d));
                self.u64(opts.limit as u64);
            }
            Op::Which { name } => {
                self.u8(17);
                self.str(name);
            }
            Op::Watch {
                id,
                paths,
                interval_ms,
            } => {
                self.u8(18);
                self.u64(*id);
                self.list(paths, |e, p| e.bytes(p));
                self.u32(*interval_ms);
            }
            Op::Unwatch { id } => {
                self.u8(19);
                self.u64(*id);
            }
            Op::Batch(ops) => {
                self.u8(20);
                self.list(ops, Self::op);
            }
        }
    }
    fn path_op(&mut self, tag: u8, path: &[u8]) {
        self.u8(tag);
        self.bytes(path);
    }
    fn pair_op(&mut self, tag: u8, from: &[u8], to: &[u8]) {
        self.u8(tag);
        self.bytes(from);
        self.bytes(to);
    }
    fn reply(&mut self, reply: &Reply) {
        match reply {
            Reply::Err { code, message } => {
                self.u8(0);
                self.u8(code.tag());
                self.str(message);
            }
            Reply::Unit => self.u8(1),
            Reply::Stat(s) => {
                self.u8(2);
                self.stat(s);
            }
            Reply::Data { bytes, complete } => {
                self.u8(3);
                self.bool(*complete);
                self.bytes(bytes);
            }
            Reply::Path(p) => {
                self.u8(4);
                self.bytes(p);
            }
            Reply::Entries(entries) => {
                self.u8(5);
                self.list(entries, |e, entry| {
                    e.bytes(&entry.name);
                    e.kind(entry.kind);
                    e.bool(entry.is_dir);
                });
            }
            Reply::Bool(b) => {
                self.u8(6);
                self.bool(*b);
            }
            Reply::Paths(paths) => {
                self.u8(7);
                self.list(paths, |e, p| e.bytes(p));
            }
            Reply::MaybePath(p) => {
                self.u8(8);
                match p {
                    Some(p) => {
                        self.u8(1);
                        self.bytes(p);
                    }
                    None => self.u8(0),
                }
            }
            Reply::Batch(replies) => {
                self.u8(9);
                self.list(replies, Self::reply);
            }
        }
    }
}

/// Encode `message` as a frame payload (not yet length-prefixed).
#[must_use]
pub fn encode(message: &Message) -> Vec<u8> {
    let mut e = Enc(Vec::new());
    match message {
        Message::Hello { protocol, version } => {
            e.u8(1);
            e.0.extend_from_slice(MAGIC);
            e.u32(*protocol);
            e.str(version);
        }
        Message::Request { id, op } => {
            e.u8(2);
            e.u64(*id);
            e.op(op);
        }
        Message::Reply { id, reply } => {
            e.u8(3);
            e.u64(*id);
            e.reply(reply);
        }
        Message::Event { watch, paths } => {
            e.u8(4);
            e.u64(*watch);
            e.list(paths, |e, p| e.bytes(p));
        }
    }
    e.0
}

// ----- decoding -------------------------------------------------------------

struct Dec<'a> {
    buf: &'a [u8],
    pos: usize,
}

type R<T> = Result<T, &'static str>;

impl<'a> Dec<'a> {
    fn left(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if n > self.left() {
            return Err("truncated");
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> R<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("bad bool"),
        }
    }
    fn u32(&mut self) -> R<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> R<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }
    fn bytes(&mut self) -> R<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn str(&mut self) -> R<String> {
        String::from_utf8(self.bytes()?).map_err(|_| "bad utf-8")
    }
    /// A list count, checked against the bytes left (each item takes at
    /// least `min` bytes) and against `cap`.
    fn count(&mut self, min: usize, cap: usize) -> R<usize> {
        let n = self.u32()? as usize;
        if n > cap || n.saturating_mul(min) > self.left() {
            return Err("bad count");
        }
        Ok(n)
    }
    fn list<T>(
        &mut self,
        min: usize,
        cap: usize,
        mut f: impl FnMut(&mut Self) -> R<T>,
    ) -> R<Vec<T>> {
        let n = self.count(min, cap)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }
    fn time(&mut self) -> R<Option<SystemTime>> {
        let tag = self.u8()?;
        if tag == 0 {
            return Ok(None);
        }
        let secs = self.u64()?;
        let nanos = self.u32()?;
        if nanos >= 1_000_000_000 {
            return Err("bad time");
        }
        let d = Duration::new(secs, nanos);
        let t = match tag {
            1 => SystemTime::UNIX_EPOCH.checked_add(d),
            2 => SystemTime::UNIX_EPOCH.checked_sub(d),
            _ => return Err("bad time"),
        };
        t.map(Some).ok_or("bad time")
    }
    fn kind(&mut self) -> R<FileKind> {
        Ok(match self.u8()? {
            0 => FileKind::File,
            1 => FileKind::Dir,
            2 => FileKind::Symlink,
            3 => FileKind::Other,
            _ => return Err("bad kind"),
        })
    }
    fn stat(&mut self) -> R<FileStat> {
        let kind = self.kind()?;
        let len = self.u64()?;
        let modified = self.time()?;
        let created = self.time()?;
        let readonly = self.bool()?;
        let mode = match self.u8()? {
            0 => None,
            1 => Some(self.u32()?),
            _ => return Err("bad mode"),
        };
        Ok(FileStat::new(kind, len)
            .with_modified(modified)
            .with_created(created)
            .with_readonly(readonly)
            .with_mode(mode))
    }
    fn op(&mut self, nested: bool) -> R<Op> {
        let tag = self.u8()?;
        Ok(match tag {
            1 => Op::Stat {
                path: self.bytes()?,
                follow: self.bool()?,
            },
            2 => Op::Read {
                path: self.bytes()?,
                max: self.u64()?,
            },
            3 => Op::ReadText {
                path: self.bytes()?,
                max: self.u64()?,
            },
            4 => Op::Write {
                path: self.bytes()?,
                data: self.bytes()?,
            },
            5 => Op::WriteAtomic {
                path: self.bytes()?,
                data: self.bytes()?,
                expect: match self.u8()? {
                    0 => Expect::Any,
                    1 => Expect::Absent,
                    2 => Expect::Version(self.stat()?),
                    _ => return Err("bad expect"),
                },
            },
            6 => Op::Canonicalize {
                path: self.bytes()?,
            },
            7 => Op::ReadDir {
                path: self.bytes()?,
            },
            8 => Op::CreateDirAll {
                path: self.bytes()?,
            },
            9 => Op::Rename {
                from: self.bytes()?,
                to: self.bytes()?,
            },
            10 => Op::RemoveFile {
                path: self.bytes()?,
            },
            11 => Op::RemovePath {
                path: self.bytes()?,
            },
            12 => Op::MovePath {
                from: self.bytes()?,
                to: self.bytes()?,
            },
            13 => Op::CopyRecursive {
                from: self.bytes()?,
                to: self.bytes()?,
            },
            14 => Op::Trash {
                path: self.bytes()?,
            },
            15 => Op::IsUnwritable {
                path: self.bytes()?,
            },
            16 => {
                let root = self.bytes()?;
                let max_depth = match self.u8()? {
                    0 => None,
                    1 => Some(usize::try_from(self.u64()?).unwrap_or(usize::MAX)),
                    _ => return Err("bad depth"),
                };
                let skip_hidden = self.bool()?;
                let honor_ignore_files = self.bool()?;
                let prune_dirs = self.list(4, MAX_BATCH, Self::str)?;
                let limit = usize::try_from(self.u64()?).unwrap_or(usize::MAX);
                Op::Walk {
                    root,
                    opts: WalkOptions {
                        max_depth,
                        skip_hidden,
                        honor_ignore_files,
                        prune_dirs,
                        limit,
                    },
                }
            }
            17 => Op::Which { name: self.str()? },
            18 if !nested => Op::Watch {
                id: self.u64()?,
                paths: self.list(4, MAX_BATCH, Self::bytes)?,
                interval_ms: self.u32()?,
            },
            19 if !nested => Op::Unwatch { id: self.u64()? },
            20 if !nested => Op::Batch(self.list(1, MAX_BATCH, |d| d.op(true))?),
            _ => return Err("bad op"),
        })
    }
    fn reply(&mut self, nested: bool) -> R<Reply> {
        let tag = self.u8()?;
        Ok(match tag {
            0 => {
                let code = *CODES.get(self.u8()? as usize).ok_or("bad code")?;
                Reply::Err {
                    code,
                    message: self.str()?,
                }
            }
            1 => Reply::Unit,
            2 => Reply::Stat(self.stat()?),
            3 => {
                let complete = self.bool()?;
                Reply::Data {
                    bytes: self.bytes()?,
                    complete,
                }
            }
            4 => Reply::Path(self.bytes()?),
            5 => Reply::Entries(self.list(6, usize::MAX, |d| {
                Ok(Entry {
                    name: d.bytes()?,
                    kind: d.kind()?,
                    is_dir: d.bool()?,
                })
            })?),
            6 => Reply::Bool(self.bool()?),
            7 => Reply::Paths(self.list(4, usize::MAX, Self::bytes)?),
            8 => Reply::MaybePath(match self.u8()? {
                0 => None,
                1 => Some(self.bytes()?),
                _ => return Err("bad option"),
            }),
            9 if !nested => Reply::Batch(self.list(1, MAX_BATCH, |d| d.reply(true))?),
            _ => return Err("bad reply"),
        })
    }
}

/// Decode one frame payload.
///
/// # Errors
/// `InvalidData` for anything malformed: an unknown tag, a length past the
/// end, a list over its cap, a nested batch, a bad UTF-8 string, an
/// impossible time, or bytes left over.
pub fn decode(payload: &[u8]) -> io::Result<Message> {
    let mut d = Dec {
        buf: payload,
        pos: 0,
    };
    let message = (|| -> R<Message> {
        Ok(match d.u8()? {
            1 => {
                if d.take(MAGIC.len())? != MAGIC {
                    return Err("bad magic");
                }
                Message::Hello {
                    protocol: d.u32()?,
                    version: d.str()?,
                }
            }
            2 => Message::Request {
                id: d.u64()?,
                op: d.op(false)?,
            },
            3 => Message::Reply {
                id: d.u64()?,
                reply: d.reply(false)?,
            },
            4 => Message::Event {
                watch: d.u64()?,
                paths: d.list(4, MAX_BATCH, Dec::bytes)?,
            },
            _ => return Err("bad message"),
        })
    })()
    .map_err(bad)?;
    if d.left() != 0 {
        return Err(bad("trailing bytes"));
    }
    Ok(message)
}

/// Decode every frame in `input`, in order, until it ends or one is
/// malformed; returns how many decoded. It never panics or allocates more
/// than the input holds, whatever the bytes: the `agent_frame` fuzz target
/// and the property tests call it.
#[must_use]
pub fn decode_stream(input: &[u8]) -> usize {
    let mut r = input;
    let mut n = 0;
    while let Ok(Some(payload)) = read_frame(&mut r) {
        if decode(&payload).is_err() {
            break;
        }
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn roundtrip(m: &Message) {
        let mut framed = Vec::new();
        write_frame(&mut framed, &encode(m)).unwrap();
        let back = read_frame(&mut framed.as_slice()).unwrap().unwrap();
        assert_eq!(&decode(&back).unwrap(), m);
    }

    fn stat() -> FileStat {
        FileStat::new(FileKind::File, 42)
            .with_modified(Some(
                SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, 5),
            ))
            .with_created(Some(SystemTime::UNIX_EPOCH - Duration::from_secs(3)))
            .with_readonly(true)
            .with_mode(Some(0o100_644))
    }

    #[test]
    fn every_request_round_trips() {
        let p = b"/a b/\xff\n".to_vec();
        let ops = vec![
            Op::Stat {
                path: p.clone(),
                follow: true,
            },
            Op::Read {
                path: p.clone(),
                max: 9,
            },
            Op::ReadText {
                path: p.clone(),
                max: 9,
            },
            Op::Write {
                path: p.clone(),
                data: vec![0, 1, 2],
            },
            Op::WriteAtomic {
                path: p.clone(),
                data: vec![],
                expect: Expect::Version(stat()),
            },
            Op::WriteAtomic {
                path: p.clone(),
                data: vec![1],
                expect: Expect::Absent,
            },
            Op::Canonicalize { path: p.clone() },
            Op::ReadDir { path: p.clone() },
            Op::CreateDirAll { path: p.clone() },
            Op::Rename {
                from: p.clone(),
                to: b"x".to_vec(),
            },
            Op::RemoveFile { path: p.clone() },
            Op::RemovePath { path: p.clone() },
            Op::MovePath {
                from: p.clone(),
                to: b"x".to_vec(),
            },
            Op::CopyRecursive {
                from: p.clone(),
                to: b"x".to_vec(),
            },
            Op::Trash { path: p.clone() },
            Op::IsUnwritable { path: p.clone() },
            Op::Walk {
                root: p.clone(),
                opts: WalkOptions::file_index(7),
            },
            Op::Which { name: "rg".into() },
        ];
        for op in &ops {
            roundtrip(&Message::Request {
                id: 7,
                op: op.clone(),
            });
        }
        roundtrip(&Message::Request {
            id: 1,
            op: Op::Batch(ops),
        });
        roundtrip(&Message::Request {
            id: 2,
            op: Op::Watch {
                id: 3,
                paths: vec![p],
                interval_ms: 500,
            },
        });
        roundtrip(&Message::Request {
            id: 2,
            op: Op::Unwatch { id: 3 },
        });
    }

    #[test]
    fn every_reply_round_trips() {
        let p = b"/a b/\xff\n".to_vec();
        let replies = vec![
            Reply::Err {
                code: ErrorCode::TooBig,
                message: "big".into(),
            },
            Reply::Unit,
            Reply::Stat(stat()),
            Reply::Stat(FileStat::new(FileKind::Other, 0)),
            Reply::Data {
                bytes: vec![9; 5],
                complete: false,
            },
            Reply::Path(p.clone()),
            Reply::Entries(vec![Entry {
                name: p.clone(),
                kind: FileKind::Symlink,
                is_dir: true,
            }]),
            Reply::Bool(true),
            Reply::Paths(vec![p.clone(), vec![]]),
            Reply::MaybePath(None),
            Reply::MaybePath(Some(p.clone())),
        ];
        for reply in &replies {
            roundtrip(&Message::Reply {
                id: u64::MAX,
                reply: reply.clone(),
            });
        }
        roundtrip(&Message::Reply {
            id: 0,
            reply: Reply::Batch(replies),
        });
        roundtrip(&Message::Hello {
            protocol: PROTOCOL,
            version: "1.2.3".into(),
        });
        roundtrip(&Message::Event {
            watch: 4,
            paths: vec![p],
        });
    }

    #[test]
    fn every_error_code_round_trips_through_io() {
        for code in CODES {
            let e = code.to_io("m");
            let back = ErrorCode::of(&e);
            let expected = if code == ErrorCode::TooBig {
                ErrorCode::Other
            } else {
                code
            };
            assert_eq!(back, expected, "{code:?}");
        }
    }

    #[test]
    fn hostile_frames_are_refused_without_allocating() {
        // A length over the cap.
        let mut huge = (u32::MAX).to_be_bytes().to_vec();
        huge.push(1);
        assert!(read_frame(&mut huge.as_slice()).is_err());
        // A length longer than the input: cut short, not a 64 MiB buffer.
        let mut short = 1000u32.to_be_bytes().to_vec();
        short.extend_from_slice(b"abc");
        assert_eq!(
            read_frame(&mut short.as_slice()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        // A list count far past the bytes left.
        let mut e = Enc(Vec::new());
        e.u8(3);
        e.u64(1);
        e.u8(7);
        e.u32(u32::MAX);
        assert!(decode(&e.0).is_err());
        // A nested batch.
        let nested = encode(&Message::Request {
            id: 1,
            op: Op::Batch(vec![Op::Batch(vec![])]),
        });
        assert!(decode(&nested).is_err());
        // Trailing bytes.
        let mut extra = encode(&Message::Reply {
            id: 1,
            reply: Reply::Unit,
        });
        extra.push(0);
        assert!(decode(&extra).is_err());
        // A clean end of input is not an error.
        assert!(read_frame(&mut [].as_slice()).unwrap().is_none());
        // A watch inside a batch.
        let mut w = Enc(Vec::new());
        w.u8(2);
        w.u64(1);
        w.u8(20);
        w.u32(1);
        w.u8(19);
        w.u64(1);
        assert!(decode(&w.0).is_err());
    }

    #[test]
    fn writing_an_oversized_frame_is_refused() {
        let big = vec![0u8; MAX_FRAME + 1];
        assert!(write_frame(&mut Vec::new(), &big).is_err());
    }

    fn arb_bytes() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(any::<u8>(), 0..24)
    }

    fn arb_stat() -> impl Strategy<Value = FileStat> {
        (
            0u8..4,
            any::<u64>(),
            proptest::option::of((0u64..4_000_000_000, 0u32..1_000_000_000)),
            any::<bool>(),
            proptest::option::of(any::<u32>()),
        )
            .prop_map(|(k, len, t, ro, mode)| {
                let kind = [
                    FileKind::File,
                    FileKind::Dir,
                    FileKind::Symlink,
                    FileKind::Other,
                ][k as usize];
                FileStat::new(kind, len)
                    .with_modified(t.map(|(s, n)| SystemTime::UNIX_EPOCH + Duration::new(s, n)))
                    .with_readonly(ro)
                    .with_mode(mode)
            })
    }

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (arb_bytes(), any::<bool>()).prop_map(|(path, follow)| Op::Stat { path, follow }),
            (arb_bytes(), any::<u64>()).prop_map(|(path, max)| Op::Read { path, max }),
            (arb_bytes(), arb_bytes()).prop_map(|(path, data)| Op::Write { path, data }),
            (arb_bytes(), arb_bytes(), arb_stat()).prop_map(|(path, data, s)| {
                Op::WriteAtomic {
                    path,
                    data,
                    expect: Expect::Version(s),
                }
            }),
            (arb_bytes(), arb_bytes()).prop_map(|(from, to)| Op::Rename { from, to }),
            "[a-z]{0,8}".prop_map(|name| Op::Which { name }),
        ]
    }

    proptest! {
        #[test]
        fn arbitrary_bytes_never_panic(input in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = decode(&input);
            let _ = decode_stream(&input);
        }

        #[test]
        fn framed_garbage_never_panics(payload in proptest::collection::vec(any::<u8>(), 0..512)) {
            let mut framed = Vec::new();
            write_frame(&mut framed, &payload).unwrap();
            let _ = decode_stream(&framed);
        }

        #[test]
        fn arbitrary_requests_round_trip(id in any::<u64>(), ops in proptest::collection::vec(arb_op(), 0..6)) {
            let m = Message::Request { id, op: Op::Batch(ops) };
            prop_assert_eq!(decode(&encode(&m)).unwrap(), m);
        }

        #[test]
        fn arbitrary_stats_round_trip(s in arb_stat()) {
            let m = Message::Reply { id: 1, reply: Reply::Stat(s) };
            prop_assert_eq!(decode(&encode(&m)).unwrap(), m);
        }

        #[test]
        fn a_truncated_message_is_an_error(ops in proptest::collection::vec(arb_op(), 1..4), cut in any::<prop::sample::Index>()) {
            let bytes = encode(&Message::Request { id: 1, op: Op::Batch(ops) });
            let n = cut.index(bytes.len());
            prop_assert!(decode(&bytes[..n]).is_err());
        }
    }
}
