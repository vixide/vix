//! The agent side (T717): `vix --remote-agent` reads requests from stdin,
//! runs each against [`LocalAuthority`] on the machine it runs on, and
//! writes the replies to stdout.
//!
//! It only touches the file system: there is no request that starts a
//! process. It exits when its input ends (the SSH channel closed) or on the
//! first malformed frame.
//!
//! Every file operation is delegated to [`LocalAuthority`] so the agent
//! behaves exactly like a local Vix. Two things are specific to the agent:
//! replies larger than one frame degrade to a `TooBig` error (the client then
//! falls back to another route), and watches run on their own polling threads
//! that share the output writer with the request loop. The client half is
//! `client.rs`; the byte format is [`super::wire`]. Owning spec:
//! `crates/vix-authority/spec/agent/index.md`.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::wire::{
    self, Entry, ErrorCode, Expect, MAX_DATA, MAX_FRAME, Message, Op, PROTOCOL, Reply,
};
use crate::ssh::quote::{bytes_path, path_bytes};
use crate::{Authority, FileStat, LocalAuthority, TextReadError};

/// The most watch subscriptions one agent keeps at once.
const MAX_WATCHES: usize = 64;

/// The frame writer, shared with the watch threads.
type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// Serve the agent protocol: answer the client's [`Message::Hello`] with
/// this build's own, then answer requests until `input` ends. If the
/// client's protocol or Vix version differs, the agent still says hello (so
/// the client can tell why) and then stops.
///
/// # Errors
/// A malformed frame, or a read or write error.
pub fn serve(mut input: impl Read, output: impl Write + Send + 'static) -> io::Result<()> {
    let out: Out = Arc::new(Mutex::new(Box::new(output)));
    let Some(first) = wire::read_frame(&mut input)? else {
        return Ok(());
    };
    let Message::Hello { protocol, version } = wire::decode(&first)? else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected a hello",
        ));
    };
    send(
        &out,
        &Message::Hello {
            protocol: PROTOCOL,
            version: super::VERSION.to_string(),
        },
    )?;
    // Answer a mismatched client with our hello (so it can report why) but
    // serve nothing: different builds may disagree about message layouts.
    if protocol != PROTOCOL || version != super::VERSION {
        return Ok(());
    }
    let mut watches: HashMap<u64, Arc<AtomicBool>> = HashMap::new();
    // Loop result is held, not returned, so the watch threads are always
    // told to stop below, even when the connection ended with an error.
    let result = loop {
        let frame = match wire::read_frame(&mut input) {
            Ok(Some(frame)) => frame,
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        };
        let (id, op) = match wire::decode(&frame) {
            Ok(Message::Request { id, op }) => (id, op),
            Ok(_) => {
                break Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "expected a request",
                ));
            }
            Err(e) => break Err(e),
        };
        let reply = match op {
            Op::Watch {
                id: watch,
                paths,
                interval_ms,
            } => start_watch(&out, &mut watches, watch, &paths, interval_ms),
            Op::Unwatch { id: watch } => {
                if let Some(stop) = watches.remove(&watch) {
                    stop.store(true, Ordering::Relaxed);
                }
                Reply::Unit
            }
            op => handle(&op),
        };
        if let Err(e) = send_reply(&out, id, reply) {
            break Err(e);
        }
    };
    for stop in watches.values() {
        stop.store(true, Ordering::Relaxed);
    }
    result
}

/// [`serve`] on this process's stdin and stdout.
///
/// # Errors
/// As [`serve`].
pub fn serve_stdio() -> io::Result<()> {
    serve(io::stdin().lock(), io::stdout())
}

/// Write one message as a frame; the lock keeps frames from interleaving
/// with those the watch threads send.
fn send(out: &Out, message: &Message) -> io::Result<()> {
    let payload = wire::encode(message);
    let mut w = out.lock().unwrap_or_else(PoisonError::into_inner);
    wire::write_frame(&mut *w, &payload)
}

/// Send a reply, or a [`ErrorCode::TooBig`] error when it doesn't fit.
fn send_reply(out: &Out, id: u64, reply: Reply) -> io::Result<()> {
    let message = Message::Reply { id, reply };
    let payload = wire::encode(&message);
    let payload = if payload.len() > MAX_FRAME {
        wire::encode(&Message::Reply {
            id,
            reply: Reply::Err {
                code: ErrorCode::TooBig,
                message: "the reply is too large for one frame".into(),
            },
        })
    } else {
        payload
    };
    let mut w = out.lock().unwrap_or_else(PoisonError::into_inner);
    wire::write_frame(&mut *w, &payload)
}

/// A wire path as a local one: `~` and `~/…` are the home directory (the
/// client cannot know the remote home, so it sends `~` and the agent
/// expands it). Without `HOME` the path is used as written.
fn local_path(bytes: &[u8]) -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if bytes == b"~"
        && let Some(h) = home()
    {
        return h;
    }
    if let Some(rest) = bytes.strip_prefix(b"~/")
        && let Some(h) = home()
    {
        return h.join(bytes_path(rest));
    }
    bytes_path(bytes)
}

/// An operation that only succeeds or fails, as a reply.
fn unit(r: io::Result<()>) -> Reply {
    r.map_or_else(|e| Reply::error(&e), |()| Reply::Unit)
}

/// Run one operation (not a watch) against the local file system.
fn handle(op: &Op) -> Reply {
    let auth = LocalAuthority;
    let p = local_path;
    match op {
        Op::Stat { path, follow } => {
            let r = if *follow {
                auth.metadata(&p(path))
            } else {
                auth.symlink_metadata(&p(path))
            };
            r.map_or_else(|e| Reply::error(&e), Reply::Stat)
        }
        Op::Read { path, max } => read(&p(path), *max, false),
        Op::ReadText { path, max } => read(&p(path), *max, true),
        Op::Write { path, data } => unit(auth.write(&p(path), data)),
        Op::WriteAtomic { path, data, expect } => {
            let path = p(path);
            unit(match expect {
                Expect::Any => auth.write_atomic(&path, data),
                Expect::Absent => auth.write_atomic_if_unchanged(&path, data, None),
                Expect::Version(stat) => auth.write_atomic_if_unchanged(&path, data, Some(stat)),
            })
        }
        Op::Canonicalize { path } => auth
            .canonicalize(&p(path))
            .map_or_else(|e| Reply::error(&e), |c| Reply::Path(path_bytes(&c))),
        Op::ReadDir { path } => auth.read_dir(&p(path)).map_or_else(
            |e| Reply::error(&e),
            |entries| {
                Reply::Entries(
                    entries
                        .iter()
                        .map(|e| Entry {
                            name: path_bytes(Path::new(e.name())),
                            kind: e.kind,
                            is_dir: e.is_dir,
                        })
                        .collect(),
                )
            },
        ),
        Op::CreateDirAll { path } => unit(auth.create_dir_all(&p(path))),
        Op::Rename { from, to } => unit(auth.rename(&p(from), &p(to))),
        Op::RemoveFile { path } => unit(auth.remove_file(&p(path))),
        Op::RemovePath { path } => unit(auth.remove_path(&p(path))),
        Op::MovePath { from, to } => unit(auth.move_path(&p(from), &p(to))),
        Op::CopyRecursive { from, to } => unit(auth.copy_recursive(&p(from), &p(to))),
        Op::Trash { path } => unit(auth.trash(&p(path))),
        Op::IsUnwritable { path } => Reply::Bool(auth.is_unwritable(&p(path))),
        Op::Walk { root, opts } => Reply::Paths(
            auth.walk_files(&p(root), opts)
                .iter()
                .map(|f| path_bytes(f))
                .collect(),
        ),
        Op::Which { name } => Reply::MaybePath(auth.which(name).map(|f| path_bytes(&f))),
        Op::Batch(ops) => Reply::Batch(ops.iter().map(handle).collect()),
        Op::Watch { .. } | Op::Unwatch { .. } => Reply::Err {
            code: ErrorCode::Unsupported,
            message: "a watch cannot be batched".into(),
        },
    }
}

/// Read up to `max` bytes, but never more than [`MAX_DATA`]; `complete`
/// says nothing was left beyond what is sent. With `text`, apply the T571
/// guards first (a regular file no larger than `max`) and read up to
/// `max + 1` bytes, so the client's decoder sees a file that grew.
fn read(path: &Path, max: u64, text: bool) -> Reply {
    let auth = LocalAuthority;
    let refuse = |why| Reply::error(&io::Error::new(io::ErrorKind::InvalidData, why));
    let want = if text {
        match auth.metadata(path) {
            Err(e) => return Reply::error(&e),
            Ok(m) if !m.is_file() => return refuse(TextReadError::NotRegular),
            Ok(m) if m.len > max => return refuse(TextReadError::TooLarge),
            Ok(m) if m.len >= MAX_DATA => {
                return Reply::Err {
                    code: ErrorCode::TooBig,
                    message: "the file is too large for one frame".into(),
                };
            }
            Ok(_) => max.saturating_add(1),
        }
    } else {
        max
    }
    .min(MAX_DATA);
    match auth.read_limited(path, want.saturating_add(1)) {
        Err(e) => Reply::error(&e),
        Ok(mut bytes) => {
            let complete = bytes.len() as u64 <= want;
            bytes.truncate(usize::try_from(want).unwrap_or(usize::MAX));
            Reply::Data { bytes, complete }
        }
    }
}

/// Start polling `paths` every `interval_ms` on a thread, sending a
/// [`Message::Event`] whenever one changes.
fn start_watch(
    out: &Out,
    watches: &mut HashMap<u64, Arc<AtomicBool>>,
    id: u64,
    paths: &[Vec<u8>],
    interval_ms: u32,
) -> Reply {
    if watches.contains_key(&id) {
        return Reply::error(&io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the watch id is in use",
        ));
    }
    // Forget watches whose thread already stopped (output closed) so they
    // do not count against the limit.
    watches.retain(|_, stop| !stop.load(Ordering::Relaxed));
    if watches.len() >= MAX_WATCHES {
        return Reply::error(&io::Error::other("too many watches"));
    }
    let stop = Arc::new(AtomicBool::new(false));
    // Clamp to 0.1–60 s so a client cannot make the agent busy-poll or
    // effectively never look.
    let interval = Duration::from_millis(u64::from(interval_ms.clamp(100, 60_000)));
    let targets: Vec<(Vec<u8>, PathBuf)> =
        paths.iter().map(|b| (b.clone(), local_path(b))).collect();
    let thread_stop = Arc::clone(&stop);
    let thread_out = Arc::clone(out);
    let spawned = std::thread::Builder::new()
        .name("vix-agent-watch".into())
        .spawn(move || poll_watch(&thread_out, &thread_stop, id, &targets, interval));
    match spawned {
        Ok(_) => {
            watches.insert(id, stop);
            Reply::Unit
        }
        Err(e) => Reply::error(&e),
    }
}

/// The watch thread's body: remember each path's stat, and every `interval`
/// send one event naming the paths whose kind, length or mtime changed or
/// that appeared or vanished. Exits when `stop` is set or the output fails;
/// it sets `stop` on exit so the request loop can prune the entry. Paths go
/// back to the client as the exact bytes it sent, not the expanded form.
fn poll_watch(
    out: &Out,
    stop: &AtomicBool,
    id: u64,
    paths: &[(Vec<u8>, PathBuf)],
    interval: Duration,
) {
    let auth = LocalAuthority;
    let look = |p: &Path| -> Option<FileStat> { auth.metadata(p).ok() };
    let mut last: Vec<Option<FileStat>> = paths.iter().map(|(_, p)| look(p)).collect();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(interval);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut changed = Vec::new();
        for ((bytes, path), seen) in paths.iter().zip(last.iter_mut()) {
            let now = look(path);
            let same = match (&*seen, &now) {
                (Some(a), Some(b)) => {
                    a.kind == b.kind && a.modified == b.modified && a.len == b.len
                }
                (None, None) => true,
                _ => false,
            };
            if !same {
                changed.push(bytes.clone());
            }
            *seen = now;
        }
        if !changed.is_empty()
            && send(
                out,
                &Message::Event {
                    watch: id,
                    paths: changed,
                },
            )
            .is_err()
        {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
}
