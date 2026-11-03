//! The client side of the agent protocol (T717): one connection to a
//! running agent, shared by every thread that calls the authority.
//!
//! Requests carry ids, so several threads can wait at once; a reader thread
//! hands each reply to the caller waiting for it and each watch event to
//! its subscription. When the connection ends, every waiting caller is
//! released and the client stays dead.
//!
//! [`crate::agent::AgentAuthority`] is the public face; this module is its
//! engine. The central distinction is [`Fail`]: an operation that *changed*
//! something remotely and then lost the connection reports an I/O error (the
//! outcome is unknown), while a read-only one, or one that never fit in a
//! frame, reports `Unavailable` so the SSH authority can retry it as a shell
//! script. Message layout: [`super::wire`]. Owning spec:
//! `crates/vix-authority/spec/agent/index.md`.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use super::wire::{self, Entry, ErrorCode, Expect, MAX_BATCH, MAX_DATA, Message, Op, Reply};
use crate::ssh::quote::{bytes_path, path_bytes};
use crate::{DirEntry, FileStat, WalkOptions};

/// How long the agent has to answer the hello.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Why an agent call produced no answer from the remote file system.
#[derive(Debug)]
pub(crate) enum Fail {
    /// The remote side answered with this error (or the connection dropped
    /// while a request that changes something was in flight).
    Io(io::Error),
    /// Not answered, and nothing changed remotely: the agent is gone, or the
    /// request or its answer is too large for a frame. The caller may do
    /// the same operation another way.
    Unavailable,
}

/// What the reader thread shares with callers.
#[derive(Default)]
struct Shared {
    /// Waiting callers by request id. `alive` is only changed under this
    /// lock, so a caller never registers after the reader gave up.
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    /// Watch subscriptions by watch id; each receives the changed paths of
    /// every event the agent pushes for it.
    watches: Mutex<HashMap<u64, Sender<Vec<PathBuf>>>>,
    /// False once the reader thread has exited or a write failed.
    alive: AtomicBool,
}

/// Lock `m`, ignoring poisoning: the guarded maps and writer stay valid
/// after a panicking holder, and a dead connection must not cascade panics.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Mark the connection dead and release everyone waiting. Dropping the
    /// pending senders makes each blocked `rx.recv()` in [`AgentClient::call`]
    /// return `Err`. `alive` flips while holding `pending` (see its docs).
    fn die(&self) {
        let mut pending = lock(&self.pending);
        self.alive.store(false, Ordering::SeqCst);
        pending.clear();
        drop(pending);
        lock(&self.watches).clear();
    }
}

/// The writing half, which a [`Watch`] also reaches (to unsubscribe).
struct Core {
    /// The agent's stdin; the mutex keeps each frame contiguous when several
    /// threads send at once.
    writer: Mutex<Box<dyn Write + Send>>,
    /// State shared with the reader thread.
    shared: Arc<Shared>,
    /// Next request/watch id. Watch ids share the request id space.
    next: AtomicU64,
}

impl Core {
    /// Encode and write one frame without waiting for a reply.
    fn send(&self, message: &Message) -> io::Result<()> {
        let payload = wire::encode(message);
        let mut w = lock(&self.writer);
        wire::write_frame(&mut *w, &payload)
    }
}

/// One connection to a remote agent.
pub(crate) struct AgentClient {
    /// Writer half plus shared state.
    core: Arc<Core>,
    /// The agent's Vix version from its hello (verified equal to ours).
    version: String,
    /// The agent process, when [`AgentClient::spawn`] started it; killed and
    /// reaped on drop.
    #[cfg(feature = "process")]
    child: Mutex<Option<std::process::Child>>,
}

impl std::fmt::Debug for AgentClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentClient")
            .field("version", &self.version)
            .field("alive", &self.is_alive())
            .finish_non_exhaustive()
    }
}

impl AgentClient {
    /// Say hello over `writer`, and wait (at most 20 seconds) for the
    /// agent's hello on `reader`, which must name this build's protocol and
    /// Vix version exactly.
    pub(crate) fn connect(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared::default());
        shared.alive.store(true, Ordering::SeqCst);
        let core = Arc::new(Core {
            writer: Mutex::new(Box::new(writer)),
            shared: Arc::clone(&shared),
            next: AtomicU64::new(1),
        });
        let (hello_tx, hello_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("vix-agent-reader".into())
            .spawn(move || read_loop(reader, &shared, &hello_tx))?;
        core.send(&Message::Hello {
            protocol: wire::PROTOCOL,
            version: super::VERSION.to_string(),
        })?;
        let (protocol, version) = match hello_rx.recv_timeout(HANDSHAKE_TIMEOUT) {
            Ok(Some(hello)) => hello,
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                core.shared.die();
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the remote agent did not say hello",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                core.shared.die();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the remote agent did not answer",
                ));
            }
        };
        if protocol != wire::PROTOCOL || version != super::VERSION {
            core.shared.die();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the remote agent is Vix {version} (protocol {protocol}), not {} (protocol {})",
                    super::VERSION,
                    wire::PROTOCOL
                ),
            ));
        }
        Ok(Self {
            core,
            version,
            #[cfg(feature = "process")]
            child: Mutex::new(None),
        })
    }

    /// Start `cmd` (whose stdin and stdout are the agent's) and connect to
    /// it. The agent's error output is discarded.
    #[cfg(feature = "process")]
    pub(crate) fn spawn(mut cmd: std::process::Command) -> io::Result<Self> {
        use std::process::Stdio;
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the agent's pipes are missing"));
        };
        match Self::connect(stdout, stdin) {
            Ok(client) => {
                *lock(&client.child) = Some(child);
                Ok(client)
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(e)
            }
        }
    }

    /// The agent's Vix version (always this build's).
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    /// Whether the connection is still up.
    pub(crate) fn is_alive(&self) -> bool {
        self.core.shared.alive.load(Ordering::SeqCst)
    }

    /// Send `op` and wait for its reply.
    ///
    /// The request id is registered before the frame is written so a fast
    /// reply cannot race ahead of its waiter. A request over
    /// [`wire::MAX_FRAME`] is never sent. An agent `TooBig` error becomes
    /// [`Fail::Unavailable`] (retry another way); any other error code
    /// becomes the matching `io::Error`.
    pub(crate) fn call(&self, op: Op) -> Result<Reply, Fail> {
        let read_only = op.is_read_only();
        let shared = &self.core.shared;
        let id = self.core.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = lock(&shared.pending);
            if !shared.alive.load(Ordering::SeqCst) {
                return Err(Fail::Unavailable);
            }
            pending.insert(id, tx);
        }
        let payload = wire::encode(&Message::Request { id, op });
        if payload.len() > wire::MAX_FRAME {
            lock(&shared.pending).remove(&id);
            return Err(Fail::Unavailable);
        }
        let sent = {
            let mut w = lock(&self.core.writer);
            wire::write_frame(&mut *w, &payload)
        };
        if sent.is_err() {
            shared.die();
            return Err(lost(read_only));
        }
        match rx.recv() {
            Ok(Reply::Err {
                code: ErrorCode::TooBig,
                ..
            }) => Err(Fail::Unavailable),
            Ok(Reply::Err { code, message }) => Err(Fail::Io(code.to_io(&message))),
            Ok(reply) => Ok(reply),
            Err(_) => Err(lost(read_only)),
        }
    }

    /// Subscribe to changes of `paths`, checked every `interval` on the
    /// remote side.
    pub(crate) fn watch(&self, paths: &[PathBuf], interval: Duration) -> io::Result<Watch> {
        if paths.len() > MAX_BATCH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many paths to watch at once",
            ));
        }
        let id = self.core.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        lock(&self.core.shared.watches).insert(id, tx);
        let op = Op::Watch {
            id,
            paths: paths.iter().map(|p| path_bytes(p)).collect(),
            interval_ms: u32::try_from(interval.as_millis()).unwrap_or(u32::MAX),
        };
        let result = expect_unit(self.call(op));
        if let Err(e) = result {
            lock(&self.core.shared.watches).remove(&id);
            return Err(e);
        }
        Ok(Watch {
            id,
            rx,
            core: Arc::downgrade(&self.core),
        })
    }

    // ----- typed operations ----------------------------------------------

    /// `stat` (`follow`) or `lstat` (not `follow`) of one path.
    pub(crate) fn stat(&self, path: &Path, follow: bool) -> Result<FileStat, Fail> {
        match self.call(Op::Stat {
            path: path_bytes(path),
            follow,
        })? {
            Reply::Stat(s) => Ok(s),
            _ => Err(unexpected()),
        }
    }

    /// Stat many paths in one round trip (batches of [`MAX_BATCH`]).
    pub(crate) fn stat_many(&self, paths: &[&Path]) -> Result<Vec<io::Result<FileStat>>, Fail> {
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(MAX_BATCH) {
            let ops = chunk
                .iter()
                .map(|p| Op::Stat {
                    path: path_bytes(p),
                    follow: true,
                })
                .collect();
            let Reply::Batch(replies) = self.call(Op::Batch(ops))? else {
                return Err(unexpected());
            };
            if replies.len() != chunk.len() {
                return Err(unexpected());
            }
            out.extend(replies.into_iter().map(|r| match r {
                Reply::Stat(s) => Ok(s),
                Reply::Err { code, message } => Err(code.to_io(&message)),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected reply from the remote agent",
                )),
            }));
        }
        Ok(out)
    }

    /// At most `max` bytes from the start of the file. When the agent had
    /// to stop short of `max` at the frame limit ([`MAX_DATA`]), that is
    /// [`Fail::Unavailable`].
    pub(crate) fn read(&self, path: &Path, max: u64) -> Result<Vec<u8>, Fail> {
        match self.call(Op::Read {
            path: path_bytes(path),
            max,
        })? {
            Reply::Data {
                complete: false, ..
            } if max > MAX_DATA => Err(Fail::Unavailable),
            Reply::Data { bytes, .. } => Ok(bytes),
            _ => Err(unexpected()),
        }
    }

    /// The whole file, if it fits in a frame.
    pub(crate) fn read_all(&self, path: &Path) -> Result<Vec<u8>, Fail> {
        self.read(path, u64::MAX)
    }

    /// The guarded text read's bytes (still to be decoded); a file over the
    /// frame limit is [`Fail::Unavailable`].
    pub(crate) fn read_text(&self, path: &Path, max: u64) -> Result<Vec<u8>, Fail> {
        match self.call(Op::ReadText {
            path: path_bytes(path),
            max,
        })? {
            Reply::Data {
                complete: false, ..
            } if max.saturating_add(1) > MAX_DATA => Err(Fail::Unavailable),
            Reply::Data { bytes, .. } => Ok(bytes),
            _ => Err(unexpected()),
        }
    }

    /// Plain truncating write; data over [`MAX_DATA`] is [`Fail::Unavailable`]
    /// without being sent.
    pub(crate) fn write(&self, path: &Path, data: &[u8]) -> Result<(), Fail> {
        if data.len() as u64 > MAX_DATA {
            return Err(Fail::Unavailable);
        }
        unit(self.call(Op::Write {
            path: path_bytes(path),
            data: data.to_vec(),
        }))
    }

    /// Atomic write guarded by `expect` (the agent checks and writes in one
    /// step); over [`MAX_DATA`] is [`Fail::Unavailable`] without being sent.
    pub(crate) fn write_atomic(
        &self,
        path: &Path,
        data: &[u8],
        expect: Expect,
    ) -> Result<(), Fail> {
        if data.len() as u64 > MAX_DATA {
            return Err(Fail::Unavailable);
        }
        unit(self.call(Op::WriteAtomic {
            path: path_bytes(path),
            data: data.to_vec(),
            expect,
        }))
    }

    /// The agent's canonical form of `path`.
    pub(crate) fn canonicalize(&self, path: &Path) -> Result<PathBuf, Fail> {
        match self.call(Op::Canonicalize {
            path: path_bytes(path),
        })? {
            Reply::Path(p) => Ok(bytes_path(&p)),
            _ => Err(unexpected()),
        }
    }

    /// The entries of `dir`. Names are validated ([`valid_name`]) because the
    /// reply comes from another machine and is joined onto `dir`.
    pub(crate) fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, Fail> {
        match self.call(Op::ReadDir {
            path: path_bytes(dir),
        })? {
            Reply::Entries(entries) => Ok(entries
                .into_iter()
                .filter(|Entry { name, .. }| valid_name(name))
                .map(|Entry { name, kind, is_dir }| {
                    DirEntry::new(dir.join(bytes_path(&name)), kind, is_dir)
                })
                .collect()),
            _ => Err(unexpected()),
        }
    }

    /// One of the operations that only succeed or fail.
    pub(crate) fn simple(&self, op: Op) -> Result<(), Fail> {
        unit(self.call(op))
    }

    /// Whether `path` exists but is not writable by the agent's user.
    pub(crate) fn is_unwritable(&self, path: &Path) -> Result<bool, Fail> {
        match self.call(Op::IsUnwritable {
            path: path_bytes(path),
        })? {
            Reply::Bool(b) => Ok(b),
            _ => Err(unexpected()),
        }
    }

    /// A remote project walk. The result is re-truncated to `opts.limit` so
    /// a misbehaving agent cannot return more than was asked for.
    pub(crate) fn walk(&self, root: &Path, opts: &WalkOptions) -> Result<Vec<PathBuf>, Fail> {
        match self.call(Op::Walk {
            root: path_bytes(root),
            opts: opts.clone(),
        })? {
            Reply::Paths(paths) => Ok(paths
                .iter()
                .take(opts.limit)
                .map(|p| bytes_path(p))
                .collect()),
            _ => Err(unexpected()),
        }
    }

    /// Resolve `name` on the agent's `PATH`.
    pub(crate) fn which(&self, name: &str) -> Result<Option<PathBuf>, Fail> {
        match self.call(Op::Which { name: name.into() })? {
            Reply::MaybePath(p) => Ok(p.map(|p| bytes_path(&p))),
            _ => Err(unexpected()),
        }
    }
}

impl Drop for AgentClient {
    fn drop(&mut self) {
        // Closing our end of the pipe ends the agent (it exits on EOF); the
        // process is then reaped rather than left behind.
        #[cfg(feature = "process")]
        if let Some(mut child) = lock(&self.child).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A directory entry name the agent may send: one non-empty component.
fn valid_name(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&b'/')
}

/// The failure for a connection that dropped mid-request. A read-only
/// request is safe to repeat elsewhere; for one that may have changed
/// something the caller must be told the outcome is unknown.
fn lost(read_only: bool) -> Fail {
    if read_only {
        Fail::Unavailable
    } else {
        Fail::Io(io::Error::new(
            io::ErrorKind::NotConnected,
            "the connection to the remote agent closed",
        ))
    }
}

/// The agent answered with a reply variant the request cannot produce.
fn unexpected() -> Fail {
    Fail::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "unexpected reply from the remote agent",
    ))
}

/// Require a `Reply::Unit` (an operation that only succeeds or fails).
fn unit(r: Result<Reply, Fail>) -> Result<(), Fail> {
    match r? {
        Reply::Unit => Ok(()),
        _ => Err(unexpected()),
    }
}

/// [`unit()`], flattened to `io::Result` for [`AgentClient::watch`], where
/// "unavailable" means the agent is not running.
fn expect_unit(r: Result<Reply, Fail>) -> io::Result<()> {
    match unit(r) {
        Ok(()) => Ok(()),
        Err(Fail::Io(e)) => Err(e),
        Err(Fail::Unavailable) => Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "the remote agent is not running",
        )),
    }
}

/// The reader thread: hand out replies and events until the input ends or
/// a frame is malformed, then mark the connection dead.
fn read_loop(
    mut reader: impl Read,
    shared: &Shared,
    hello: &mpsc::SyncSender<Option<(u32, String)>>,
) {
    // Protocol order is enforced here: the first frame must be the hello,
    // after which only replies and events are legal (a request from the
    // "agent" is a protocol violation and ends the connection).
    let mut greeted = false;
    while let Ok(Some(frame)) = wire::read_frame(&mut reader) {
        let Ok(message) = wire::decode(&frame) else {
            break;
        };
        match message {
            Message::Hello { protocol, version } if !greeted => {
                greeted = true;
                let _ = hello.send(Some((protocol, version)));
            }
            Message::Reply { id, reply } if greeted => {
                if let Some(tx) = lock(&shared.pending).remove(&id) {
                    let _ = tx.send(reply);
                }
            }
            Message::Event { watch, paths } if greeted => {
                let mut watches = lock(&shared.watches);
                let paths = paths.iter().map(|p| bytes_path(p)).collect();
                if let Some(tx) = watches.get(&watch)
                    && tx.send(paths).is_err()
                {
                    watches.remove(&watch);
                }
            }
            // A second hello, a request, or anything before the hello.
            _ => break,
        }
    }
    if !greeted {
        let _ = hello.send(None);
    }
    shared.die();
}

/// A change subscription on the remote side (from `AgentAuthority::watch`
/// or `SshAuthority::watch`). Each event lists the watched paths whose kind,
/// length or modification time changed, or that appeared or vanished, since
/// the agent last looked. Dropping it ends the subscription.
#[derive(Debug)]
pub struct Watch {
    /// The subscription id, as the agent knows it.
    id: u64,
    /// Events pushed by the reader thread.
    rx: Receiver<Vec<PathBuf>>,
    /// Weak so a `Watch` does not keep a dead connection's writer alive.
    core: Weak<Core>,
}

impl Watch {
    /// Every event that arrived since the last call, without waiting; the
    /// paths are deduplicated, in first-seen order.
    #[must_use]
    pub fn changed(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        while let Ok(paths) = self.rx.try_recv() {
            for p in paths {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Wait up to `timeout` for the next event. `None` on a timeout, or
    /// once the connection is gone.
    #[must_use]
    pub fn next_timeout(&self, timeout: Duration) -> Option<Vec<PathBuf>> {
        self.rx.recv_timeout(timeout).ok()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(core) = self.core.upgrade() {
            lock(&core.shared.watches).remove(&self.id);
            // Fire and forget: the reply has no waiting caller.
            let id = core.next.fetch_add(1, Ordering::Relaxed);
            let _ = core.send(&Message::Request {
                id,
                op: Op::Unwatch { id: self.id },
            });
        }
    }
}
