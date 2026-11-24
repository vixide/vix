//! The remote agent's edges: the client against a scripted agent that answers
//! wrongly, too much, or not at all; the server driven frame by frame; and a
//! spawned agent process. Everything runs in-process over pipes or in a
//! throwaway `sh`; no host is contacted.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vix_authority::agent::wire::{
    self, Entry, ErrorCode, Expect, MAX_DATA, MAX_FRAME, Message, Op, PROTOCOL, Reply,
};
use vix_authority::agent::{AgentAuthority, VERSION, serve};
use vix_authority::{Authority, FileKind, FileStat, LocalAuthority, WalkOptions};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-agent-edges-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn hello() -> Message {
    Message::Hello {
        protocol: PROTOCOL,
        version: VERSION.into(),
    }
}

fn framed(m: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    wire::write_frame(&mut out, &wire::encode(m)).unwrap();
    out
}

fn bytes(p: &Path) -> Vec<u8> {
    p.to_str().unwrap().as_bytes().to_vec()
}

// ---------------------------------------------------------------------------
// The client against a scripted agent
// ---------------------------------------------------------------------------

/// What a scripted agent does with one request.
enum Act {
    /// Send these messages (in order) and keep serving.
    Send(Vec<Message>),
    /// Send these bytes as they are (not necessarily a valid frame).
    Raw(Vec<u8>),
    /// Hang up.
    Hangup,
}

/// An agent on a thread that says hello and then lets `script` decide, per
/// request `(id, op)`, what to send.
fn scripted(script: impl Fn(u64, Op) -> Act + Send + 'static) -> AgentAuthority {
    let (client_reads, mut agent_writes) = std::io::pipe().unwrap();
    let (mut agent_reads, client_writes) = std::io::pipe().unwrap();
    std::thread::spawn(move || {
        let Ok(Some(_)) = wire::read_frame(&mut agent_reads) else {
            return;
        };
        let _ = agent_writes.write_all(&framed(&hello()));
        while let Ok(Some(frame)) = wire::read_frame(&mut agent_reads) {
            let Ok(Message::Request { id, op }) = wire::decode(&frame) else {
                return;
            };
            match script(id, op) {
                Act::Send(messages) => {
                    for m in &messages {
                        if agent_writes.write_all(&framed(m)).is_err() {
                            return;
                        }
                    }
                }
                Act::Raw(bytes) => {
                    if agent_writes.write_all(&bytes).is_err() {
                        return;
                    }
                }
                Act::Hangup => return,
            }
        }
    });
    AgentAuthority::connect(client_reads, client_writes).unwrap()
}

fn reply(id: u64, reply: Reply) -> Act {
    Act::Send(vec![Message::Reply { id, reply }])
}

fn err(id: u64, code: ErrorCode, message: &str) -> Act {
    reply(
        id,
        Reply::Err {
            code,
            message: message.into(),
        },
    )
}

/// Answers every request with a reply of the wrong shape: `Unit` where data
/// is expected, `Bool` where `Unit` is.
fn wrong_shape(id: u64, op: &Op) -> Act {
    match op {
        Op::Stat { .. }
        | Op::Read { .. }
        | Op::ReadText { .. }
        | Op::Canonicalize { .. }
        | Op::ReadDir { .. }
        | Op::IsUnwritable { .. }
        | Op::Walk { .. }
        | Op::Which { .. }
        | Op::Batch(_) => reply(id, Reply::Unit),
        _ => reply(id, Reply::Bool(true)),
    }
}

#[test]
fn replies_of_the_wrong_shape_are_invalid_data_not_panics() {
    let auth = scripted(|id, op| wrong_shape(id, &op));
    let p = Path::new("/x");
    let kind = |e: io::Error| e.kind();
    assert_eq!(
        kind(auth.metadata(p).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(auth.symlink_metadata(p).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(kind(auth.read(p).unwrap_err()), io::ErrorKind::InvalidData);
    assert_eq!(
        kind(auth.read_limited(p, 5).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(auth.read_to_string(p).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(auth.read_text_guarded(p, 10).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(auth.canonicalize(p).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(auth.read_dir(p).unwrap_err()),
        io::ErrorKind::InvalidData
    );
    for r in [
        auth.write(p, b"a"),
        auth.write_atomic(p, b"a"),
        auth.write_atomic_if_unchanged(p, b"a", None),
        auth.create_dir_all(p),
        auth.rename(p, p),
        auth.remove_file(p),
        auth.remove_path(p),
        auth.move_path(p, p),
        auth.copy_recursive(p, p),
        auth.trash(p),
    ] {
        assert_eq!(kind(r.unwrap_err()), io::ErrorKind::InvalidData);
    }
    // The boolean, list and option queries degrade to "no".
    assert!(!auth.is_unwritable(p));
    assert!(
        auth.walk_files(p, &WalkOptions::file_index(5)).is_empty(),
        "a failed walk lists nothing"
    );
    assert_eq!(auth.which("sh"), None);
    // A batch answered by a non-batch fails every item.
    let batch = auth.metadata_batch(&[p, p]);
    assert_eq!(batch.len(), 2);
    for r in batch {
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
    assert!(auth.is_connected(), "wrong answers do not kill the link");
}

#[test]
fn a_batch_reply_is_checked_item_by_item() {
    let stat = FileStat::new(FileKind::File, 3);
    let auth = scripted(move |id, op| match op {
        Op::Batch(ops) if ops.len() == 3 => reply(
            id,
            Reply::Batch(vec![
                Reply::Stat(stat),
                Reply::Err {
                    code: ErrorCode::NotFound,
                    message: "gone".into(),
                },
                Reply::Bool(false),
            ]),
        ),
        // The wrong number of answers.
        _ => reply(id, Reply::Batch(vec![Reply::Stat(stat)])),
    });
    let p = Path::new("/p");
    let three = auth.metadata_batch(&[p, p, p]);
    assert_eq!(three[0].as_ref().unwrap().len, 3);
    assert_eq!(
        three[1].as_ref().unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        three[2].as_ref().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let two = auth.metadata_batch(&[p, p]);
    assert!(
        two.iter().all(Result::is_err),
        "a short batch fails them all"
    );
}

#[test]
fn an_agent_error_becomes_the_matching_io_error() {
    let auth = scripted(|id, op| match op {
        Op::Stat { follow: true, .. } => err(id, ErrorCode::NotFound, "no such file"),
        Op::Stat { follow: false, .. } => err(id, ErrorCode::TooBig, "too big"),
        _ => err(id, ErrorCode::PermissionDenied, "no"),
    });
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotFound);
    assert_eq!(e.to_string(), "no such file");
    // TooBig asks the caller to do it another way: reported as Unsupported.
    let e = auth.symlink_metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    let e = auth.remove_file(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn partial_reads_are_refused_only_at_the_frame_limit() {
    let auth = scripted(|id, op| match op {
        Op::Read { .. } | Op::ReadText { .. } => reply(
            id,
            Reply::Data {
                bytes: b"part".to_vec(),
                complete: false,
            },
        ),
        _ => err(id, ErrorCode::Other, "?"),
    });
    let p = Path::new("/f");
    // The agent stopped short of a small limit: what it sent is the answer.
    assert_eq!(auth.read_limited(p, 4).unwrap(), b"part");
    // The agent stopped short of a limit beyond the frame: not answerable.
    let e = auth.read_limited(p, MAX_DATA + 1).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert_eq!(auth.read(p).unwrap_err().kind(), io::ErrorKind::Unsupported);
    // Text reads: a limit that cannot be incremented, or that is beyond a
    // frame, is unanswerable; a small one decodes what arrived.
    assert_eq!(
        auth.read_text_guarded(p, u64::MAX).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(auth.read_text_guarded(p, 100).unwrap().text, "part");
}

#[test]
fn data_too_large_for_a_frame_is_never_sent() {
    let sent = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&sent);
    let auth = scripted(move |id, _| {
        counter.fetch_add(1, Ordering::SeqCst);
        reply(id, Reply::Unit)
    });
    let big = vec![0u8; usize::try_from(MAX_DATA).unwrap() + 1];
    let p = Path::new("/f");
    assert_eq!(
        auth.write(p, &big).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(
        auth.write_atomic(p, &big).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(sent.load(Ordering::SeqCst), 0);
    auth.write(p, b"small").unwrap();
    assert_eq!(sent.load(Ordering::SeqCst), 1);
}

#[test]
fn hostile_directory_entries_are_dropped() {
    let entry = |name: &[u8]| Entry {
        name: name.to_vec(),
        kind: FileKind::File,
        is_dir: false,
    };
    let auth = scripted(move |id, _| {
        reply(
            id,
            Reply::Entries(vec![
                entry(b""),
                entry(b"."),
                entry(b".."),
                entry(b"a/b"),
                entry(b"fine"),
            ]),
        )
    });
    let entries = auth.read_dir(Path::new("/d")).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path(), Path::new("/d/fine"));
}

#[test]
fn a_dropped_connection_fails_read_only_calls_softly_and_writes_loudly() {
    let auth = scripted(|_, _| Act::Hangup);
    // A read-only request may be repeated another way: Unsupported.
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    // Once down, every call fails at once, without writing.
    assert!(!auth.is_connected());
    assert_eq!(
        auth.remove_file(Path::new("/x")).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    let e = auth.watch(&[PathBuf::from("/x")], Duration::from_millis(100));
    assert_eq!(e.unwrap_err().kind(), io::ErrorKind::NotConnected);
    // The debug form says so.
    let shown = format!("{auth:?}");
    assert!(shown.contains("AgentClient"), "{shown}");
    assert!(shown.contains("alive: false"), "{shown}");
    assert!(shown.contains(VERSION), "{shown}");

    // A request that may have changed something reports an unknown outcome.
    let auth = scripted(|_, _| Act::Hangup);
    let e = auth.write(Path::new("/x"), b"a").unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotConnected);
    assert_eq!(e.to_string(), "the connection to the remote agent closed");
}

/// A writer that works until `broken` is set.
struct Breakable(Arc<AtomicBool>);

impl Write for Breakable {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.0.load(Ordering::SeqCst) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failing_write_kills_the_connection() {
    for (read_only, expect) in [
        (true, io::ErrorKind::Unsupported),
        (false, io::ErrorKind::NotConnected),
    ] {
        let (client_reads, mut agent_writes) = std::io::pipe().unwrap();
        agent_writes.write_all(&framed(&hello())).unwrap();
        let broken = Arc::new(AtomicBool::new(false));
        let auth = AgentAuthority::connect(client_reads, Breakable(Arc::clone(&broken))).unwrap();
        broken.store(true, Ordering::SeqCst);
        let result = if read_only {
            auth.metadata(Path::new("/x")).map(|_| ())
        } else {
            auth.write(Path::new("/x"), b"a")
        };
        assert_eq!(result.unwrap_err().kind(), expect);
        assert!(!auth.is_connected());
        drop(agent_writes);
    }
}

#[test]
fn an_agent_that_sends_a_request_is_cut_off() {
    let auth = scripted(|id, _| {
        Act::Send(vec![Message::Request {
            id,
            op: Op::Which { name: "x".into() },
        }])
    });
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert!(!auth.is_connected());
}

#[test]
fn a_frame_before_the_hello_is_a_failed_handshake() {
    let (client_reads, mut agent_writes) = std::io::pipe().unwrap();
    agent_writes
        .write_all(&framed(&Message::Reply {
            id: 1,
            reply: Reply::Unit,
        }))
        .unwrap();
    let (_agent_reads, client_writes) = std::io::pipe().unwrap();
    let e = AgentAuthority::connect(client_reads, client_writes).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    assert_eq!(e.to_string(), "the remote agent did not say hello");
    drop(agent_writes);
}

#[test]
fn watch_events_are_merged_and_unknown_watches_ignored() {
    let (a, b, c) = (
        PathBuf::from("/w/a"),
        PathBuf::from("/w/b"),
        PathBuf::from("/w/c"),
    );
    let (ea, eb, ec) = (bytes(&a), bytes(&b), bytes(&c));
    let watch_id = Arc::new(AtomicU64::new(0));
    let chosen = Arc::clone(&watch_id);
    let auth = scripted(move |id, op| match op {
        Op::Watch { id: watch, .. } => {
            chosen.store(watch, Ordering::SeqCst);
            reply(id, Reply::Unit)
        }
        // The follow-up request: two events for the watch, one for nobody,
        // then the answer. Replies arrive after the events, in order.
        Op::IsUnwritable { .. } => Act::Send(vec![
            Message::Event {
                watch: chosen.load(Ordering::SeqCst),
                paths: vec![ea.clone(), eb.clone()],
            },
            Message::Event {
                watch: 9999,
                paths: vec![ec.clone()],
            },
            Message::Event {
                watch: chosen.load(Ordering::SeqCst),
                paths: vec![eb.clone(), ec.clone()],
            },
            Message::Reply {
                id,
                reply: Reply::Bool(true),
            },
        ]),
        _ => reply(id, Reply::Unit),
    });
    let watch = auth
        .watch(&[a.clone(), b.clone()], Duration::from_millis(100))
        .unwrap();
    assert!(watch.changed().is_empty(), "nothing yet");
    assert!(auth.is_unwritable(Path::new("/x")));
    // Both events arrived before the reply; paths are merged, first seen
    // first, without repeats.
    assert_eq!(watch.changed(), vec![a, b, c]);
    assert!(watch.changed().is_empty(), "drained");
    assert_eq!(watch.next_timeout(Duration::from_millis(10)), None);
}

#[test]
fn a_watch_over_too_many_paths_is_refused_before_sending() {
    let auth = scripted(|id, _| reply(id, Reply::Unit));
    let paths: Vec<PathBuf> = (0..4097).map(|i| PathBuf::from(format!("/p{i}"))).collect();
    let e = auth.watch(&paths, Duration::from_millis(100)).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
    assert!(auth.is_connected());
}

#[test]
fn a_refused_watch_leaves_nothing_subscribed() {
    let auth = scripted(|id, op| match op {
        Op::Watch { .. } => err(id, ErrorCode::Other, "too many watches"),
        _ => reply(id, Reply::Unit),
    });
    let e = auth
        .watch(&[PathBuf::from("/x")], Duration::from_millis(100))
        .unwrap_err();
    assert_eq!(e.to_string(), "too many watches");
}

#[test]
fn an_agent_without_a_command_authority_starts_no_processes() {
    let auth = scripted(|id, _| reply(id, Reply::Unit));
    let e = auth
        .command(&vix_authority::ProcessSpec::new("echo"))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert_eq!(e.to_string(), "the remote agent does not start processes");
}

// ---------------------------------------------------------------------------
// Spawning an agent process
// ---------------------------------------------------------------------------

#[cfg(all(unix, feature = "process"))]
mod spawned {
    use super::*;
    use std::process::Command;

    /// `printf` escapes (octal) for `data`.
    fn octal(data: &[u8]) -> String {
        use std::fmt::Write as _;
        data.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "\\{b:03o}");
            s
        })
    }

    /// A shell that plays the part of an agent which says a correct hello
    /// and then reads and ignores everything.
    fn mute_agent() -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(format!(
            "printf '{}'; cat >/dev/null",
            octal(&framed(&hello()))
        ));
        cmd
    }

    #[test]
    fn a_spawned_agent_that_greets_correctly_connects() {
        let auth = AgentAuthority::spawn(mute_agent()).unwrap();
        assert!(auth.is_connected());
        assert_eq!(auth.version(), VERSION);
        // Dropping closes its input and reaps the process.
        drop(auth);
    }

    #[test]
    fn a_spawned_process_that_never_greets_is_an_error() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("exit 0");
        let e = AgentAuthority::spawn(cmd).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_program_that_does_not_exist_is_an_error() {
        let cmd = Command::new("/nonexistent/vix-agent-for-tests");
        let e = AgentAuthority::spawn(cmd).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }
}

// ---------------------------------------------------------------------------
// The server, frame by frame
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Everything the server wrote, decoded.
fn messages(buf: &SharedBuf) -> Vec<Message> {
    let out = buf.0.lock().unwrap().clone();
    let mut r = out.as_slice();
    let mut all = Vec::new();
    while let Some(frame) = wire::read_frame(&mut r).unwrap() {
        all.push(wire::decode(&frame).unwrap());
    }
    all
}

/// Serve `ops` (ids 1, 2, …) after a correct hello and return the replies
/// in order.
fn run(ops: Vec<Op>) -> Vec<Reply> {
    let mut input = framed(&hello());
    for (i, op) in ops.into_iter().enumerate() {
        input.extend(framed(&Message::Request {
            id: u64::try_from(i).unwrap() + 1,
            op,
        }));
    }
    let out = SharedBuf::default();
    serve(input.as_slice(), out.clone()).unwrap();
    let mut all = messages(&out);
    assert!(matches!(all.remove(0), Message::Hello { .. }));
    all.into_iter()
        .map(|m| match m {
            Message::Reply { reply, .. } => reply,
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

fn code(r: &Reply) -> ErrorCode {
    match r {
        Reply::Err { code, .. } => *code,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn the_server_reads_and_writes_files() {
    let dir = scratch("server-files");
    let f = dir.join("f.txt");
    let g = dir.join("g.txt");
    let replies = run(vec![
        Op::Write {
            path: bytes(&f),
            data: b"hello".to_vec(),
        },
        Op::Stat {
            path: bytes(&f),
            follow: true,
        },
        Op::Stat {
            path: bytes(&f),
            follow: false,
        },
        Op::Read {
            path: bytes(&f),
            max: 3,
        },
        Op::ReadText {
            path: bytes(&f),
            max: 100,
        },
        Op::WriteAtomic {
            path: bytes(&g),
            data: b"g".to_vec(),
            expect: Expect::Absent,
        },
        // Absent no longer holds.
        Op::WriteAtomic {
            path: bytes(&g),
            data: b"g2".to_vec(),
            expect: Expect::Absent,
        },
        Op::WriteAtomic {
            path: bytes(&g),
            data: b"g3".to_vec(),
            expect: Expect::Any,
        },
        Op::IsUnwritable { path: bytes(&f) },
        Op::Batch(vec![
            Op::Stat {
                path: bytes(&dir),
                follow: true,
            },
            Op::Stat {
                path: bytes(&dir.join("absent")),
                follow: true,
            },
        ]),
    ]);
    assert_eq!(replies[0], Reply::Unit);
    assert!(matches!(&replies[1], Reply::Stat(s) if s.is_file() && s.len == 5));
    assert!(matches!(&replies[2], Reply::Stat(s) if s.is_file()));
    assert_eq!(
        replies[3],
        Reply::Data {
            bytes: b"hel".to_vec(),
            complete: false
        }
    );
    assert_eq!(
        replies[4],
        Reply::Data {
            bytes: b"hello".to_vec(),
            complete: true
        }
    );
    assert_eq!(replies[5], Reply::Unit);
    assert_eq!(code(&replies[6]), ErrorCode::Conflict);
    assert_eq!(replies[7], Reply::Unit);
    assert_eq!(std::fs::read(&g).unwrap(), b"g3");
    assert_eq!(replies[8], Reply::Bool(false));
    match &replies[9] {
        Reply::Batch(items) => {
            assert!(matches!(&items[0], Reply::Stat(s) if s.is_dir()));
            assert_eq!(code(&items[1]), ErrorCode::NotFound);
        }
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_server_organizes_directories() {
    let dir = scratch("server-dirs");
    let canonical = std::fs::canonicalize(&dir).unwrap();
    let f = dir.join("f.txt");
    let sub = dir.join("sub");
    std::fs::write(&f, b"x").unwrap();
    let replies = run(vec![
        Op::CreateDirAll {
            path: bytes(&sub.join("deep")),
        },
        Op::Rename {
            from: bytes(&f),
            to: bytes(&sub.join("g.txt")),
        },
        Op::CopyRecursive {
            from: bytes(&sub),
            to: bytes(&dir.join("copy")),
        },
        Op::MovePath {
            from: bytes(&dir.join("copy")),
            to: bytes(&dir.join("moved")),
        },
        Op::ReadDir { path: bytes(&dir) },
        Op::Canonicalize {
            path: bytes(&sub.join("g.txt")),
        },
        Op::Walk {
            root: bytes(&dir),
            opts: WalkOptions::file_index(50),
        },
        Op::Which { name: "sh".into() },
        Op::Which {
            name: "vix-no-such-program".into(),
        },
        Op::RemoveFile {
            path: bytes(&sub.join("g.txt")),
        },
        Op::RemovePath {
            path: bytes(&dir.join("moved")),
        },
    ]);
    for i in [0, 1, 2, 3, 9, 10] {
        assert_eq!(replies[i], Reply::Unit, "reply {i}");
    }
    match &replies[4] {
        Reply::Entries(entries) => {
            let mut names: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();
            names.sort();
            assert_eq!(names, [b"moved".to_vec(), b"sub".to_vec()]);
            assert!(entries.iter().all(|e| e.is_dir && e.kind == FileKind::Dir));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(replies[5], Reply::Path(bytes(&canonical.join("sub/g.txt"))));
    match &replies[6] {
        Reply::Paths(paths) => {
            let mut paths = paths.clone();
            paths.sort();
            assert_eq!(
                paths,
                [bytes(&dir.join("moved/g.txt")), bytes(&sub.join("g.txt"))]
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(&replies[7], Reply::MaybePath(Some(_))));
    assert_eq!(replies[8], Reply::MaybePath(None));
    assert!(!dir.join("moved").exists());
    assert!(!sub.join("g.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_server_reports_failures_by_code() {
    let dir = scratch("server-errs");
    let missing = dir.join("missing");
    let replies = run(vec![
        Op::Canonicalize {
            path: bytes(&missing),
        },
        // A trash of something that is not there fails and moves nothing.
        Op::Trash {
            path: bytes(&missing),
        },
        Op::ReadText {
            path: bytes(&missing),
            max: 10,
        },
        // A directory is not a regular file.
        Op::ReadText {
            path: bytes(&dir),
            max: 10,
        },
        // Over its own limit.
        Op::Read {
            path: bytes(&missing),
            max: 1,
        },
    ]);
    assert_eq!(code(&replies[0]), ErrorCode::NotFound);
    assert!(matches!(replies[1], Reply::Err { .. }));
    assert_eq!(code(&replies[2]), ErrorCode::NotFound);
    assert_eq!(code(&replies[3]), ErrorCode::NotRegular);
    assert_eq!(code(&replies[4]), ErrorCode::NotFound);

    let big = dir.join("big");
    std::fs::write(&big, b"0123456789").unwrap();
    let replies = run(vec![Op::ReadText {
        path: bytes(&big),
        max: 4,
    }]);
    assert_eq!(code(&replies[0]), ErrorCode::TooLarge);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_text_file_at_the_frame_limit_is_too_big_for_one_reply() {
    let dir = scratch("server-bigfile");
    let big = dir.join("big");
    // Sparse: nothing is read, the guard only looks at the length.
    std::fs::File::create(&big)
        .unwrap()
        .set_len(MAX_DATA)
        .unwrap();
    let replies = run(vec![Op::ReadText {
        path: bytes(&big),
        max: u64::MAX,
    }]);
    assert_eq!(code(&replies[0]), ErrorCode::TooBig);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reply_over_the_frame_limit_is_replaced_by_a_too_big_error() {
    let dir = scratch("server-bigreply");
    let big = dir.join("big");
    let half = (MAX_FRAME / 2) + 1024;
    std::fs::File::create(&big)
        .unwrap()
        .set_len(u64::try_from(half).unwrap())
        .unwrap();
    let read = || Op::Read {
        path: bytes(&big),
        max: u64::MAX,
    };
    let replies = run(vec![
        Op::Batch(vec![read(), read()]),
        Op::Which { name: "sh".into() },
    ]);
    assert_eq!(code(&replies[0]), ErrorCode::TooBig);
    // The connection carries on afterwards.
    assert!(matches!(replies[1], Reply::MaybePath(Some(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn home_is_expanded_by_the_agent() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let home = std::fs::canonicalize(home).unwrap();
    let replies = run(vec![
        Op::Canonicalize {
            path: b"~".to_vec(),
        },
        Op::Canonicalize {
            path: b"~/.".to_vec(),
        },
        // Not a home reference: `~x` is an ordinary name.
        Op::Canonicalize {
            path: b"~x".to_vec(),
        },
    ]);
    assert_eq!(replies[0], Reply::Path(bytes(&home)));
    assert_eq!(replies[1], Reply::Path(bytes(&home)));
    assert_eq!(code(&replies[2]), ErrorCode::NotFound);
}

#[test]
fn watch_ids_and_counts_are_policed() {
    let dir = scratch("server-watch");
    let watch = |id: u64, interval_ms: u32| Op::Watch {
        id,
        paths: vec![bytes(&dir)],
        interval_ms,
    };
    let mut ops = vec![
        watch(1, 60_000),
        watch(1, 60_000),
        watch(2, 99),
        watch(3, 60_001),
        Op::Unwatch { id: 1 },
        Op::Unwatch { id: 777 },
    ];
    // 64 live watches is the limit: ids 10..74 fill it, 100 is refused.
    ops.extend((10..74).map(|id| watch(id, 60_000)));
    ops.push(watch(100, 60_000));
    let replies = run(ops);
    assert_eq!(replies[0], Reply::Unit);
    assert_eq!(code(&replies[1]), ErrorCode::AlreadyExists);
    assert_eq!(code(&replies[2]), ErrorCode::InvalidInput);
    assert_eq!(code(&replies[3]), ErrorCode::InvalidInput);
    assert_eq!(replies[4], Reply::Unit);
    assert_eq!(replies[5], Reply::Unit, "unwatching a stranger is harmless");
    assert!(replies[6..70].iter().all(|r| *r == Reply::Unit));
    match &replies[70] {
        Reply::Err { message, .. } => assert_eq!(message, "too many watches"),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_server_refuses_what_is_not_a_hello_or_a_request() {
    // A request first.
    let input = framed(&Message::Request {
        id: 1,
        op: Op::Which { name: "sh".into() },
    });
    let out = SharedBuf::default();
    let e = serve(input.as_slice(), out.clone()).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    assert_eq!(e.to_string(), "expected a hello");
    assert!(messages(&out).is_empty(), "it said nothing");

    // A second hello where a request belongs.
    let mut input = framed(&hello());
    input.extend(framed(&hello()));
    let out = SharedBuf::default();
    let e = serve(input.as_slice(), out.clone()).unwrap_err();
    assert_eq!(e.to_string(), "expected a request");
    assert_eq!(messages(&out).len(), 1, "only its own hello");

    // A frame cut short mid-conversation.
    let mut input = framed(&hello());
    input.extend([0, 0, 0, 9, 1]);
    let e = serve(input.as_slice(), SharedBuf::default()).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn a_client_of_another_version_gets_a_hello_and_nothing_else() {
    for (protocol, version) in [(PROTOCOL + 1, VERSION), (PROTOCOL, "0.0.0-other")] {
        let mut input = framed(&Message::Hello {
            protocol,
            version: version.into(),
        });
        input.extend(framed(&Message::Request {
            id: 1,
            op: Op::Which { name: "sh".into() },
        }));
        let out = SharedBuf::default();
        serve(input.as_slice(), out.clone()).unwrap();
        assert_eq!(messages(&out), vec![hello()]);
    }
}

/// A writer that accepts `ok` writes and then fails.
struct FailAfter {
    ok: usize,
    seen: usize,
}

impl Write for FailAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.seen >= self.ok {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.seen += 1;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failing_output_ends_the_server_with_the_write_error() {
    // The hello is two writes (length, payload); fail on the first reply.
    let mut input = framed(&hello());
    input.extend(framed(&Message::Request {
        id: 1,
        op: Op::Which { name: "sh".into() },
    }));
    let e = serve(input.as_slice(), FailAfter { ok: 2, seen: 0 }).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
    // And failing on the hello itself.
    let e = serve(input.as_slice(), FailAfter { ok: 0, seen: 0 }).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn a_watch_thread_reports_a_change_to_the_client() {
    // Real agent, real watch thread: the 100 ms poll is the agent's own, the
    // test only waits (bounded) on the event channel.
    let dir = scratch("live-watch");
    let file = dir.join("w.txt");
    std::fs::write(&file, b"1").unwrap();
    let (client_reads, agent_writes) = std::io::pipe().unwrap();
    let (agent_reads, client_writes) = std::io::pipe().unwrap();
    std::thread::spawn(move || {
        let _ = serve(agent_reads, agent_writes);
    });
    let auth = AgentAuthority::connect(client_reads, client_writes)
        .unwrap()
        .with_commands(Arc::new(LocalAuthority));
    let watch = auth
        .watch(std::slice::from_ref(&file), Duration::from_millis(100))
        .unwrap();
    // The watch thread takes its baseline snapshot on its own schedule, so a
    // single write right after `watch` can land before the baseline and never
    // look like a change. Instead make a *new* change (a different length)
    // before every bounded wait on the event channel: whichever write follows
    // the baseline is reported, and nothing depends on timing.
    let mut seen = Vec::new();
    for round in 0..100usize {
        std::fs::write(&file, vec![b'x'; round + 2]).unwrap();
        if let Some(paths) = watch.next_timeout(Duration::from_millis(100)) {
            seen.extend(paths);
            break;
        }
    }
    assert_eq!(seen, vec![file]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frame_that_does_not_decode_ends_the_connection() {
    // A well-framed payload with an unknown message tag.
    let mut junk = Vec::new();
    wire::write_frame(&mut junk, &[0xee, 1, 2, 3]).unwrap();
    let auth = scripted(move |_, _| Act::Raw(junk.clone()));
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert!(!auth.is_connected());
}

#[test]
fn text_that_is_not_utf8_is_invalid_data_through_the_agent() {
    let auth = scripted(|id, _| {
        reply(
            id,
            Reply::Data {
                bytes: vec![0xff, 0xfe],
                complete: true,
            },
        )
    });
    let e = auth.read_to_string(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    assert_eq!(auth.read(Path::new("/x")).unwrap(), vec![0xff, 0xfe]);
}

#[test]
fn a_request_too_large_for_a_frame_is_never_sent() {
    let sent = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&sent);
    let auth = scripted(move |id, _| {
        counter.fetch_add(1, Ordering::SeqCst);
        reply(id, Reply::MaybePath(Some(b"/bin/x".to_vec())))
    });
    // A program name no frame can carry: the lookup is "not found" rather
    // than an attempt that would wedge the connection.
    let name = "a".repeat(MAX_FRAME + 1);
    assert_eq!(auth.which(&name), None);
    assert_eq!(sent.load(Ordering::SeqCst), 0);
    assert!(auth.is_connected());
    assert_eq!(auth.which("sh"), Some(PathBuf::from("/bin/x")));
    assert_eq!(sent.load(Ordering::SeqCst), 1);
}
