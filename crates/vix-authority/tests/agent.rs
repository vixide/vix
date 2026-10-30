//! The remote agent (T717) in-process: the agent's `serve` on a thread at the
//! far end of two pipes, and an `AgentAuthority` at the near end. The same
//! conformance suite as every other authority runs over it, plus batching,
//! watching, the version handshake and hostile agents.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use vix_authority::agent::wire::{self, Message, PROTOCOL};
use vix_authority::agent::{AgentAuthority, VERSION, serve};
use vix_authority::{Authority, LocalAuthority, conformance};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-agent-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// An agent serving on a thread, connected through two pipes.
fn agent() -> AgentAuthority {
    let (client_reads, agent_writes) = std::io::pipe().unwrap();
    let (agent_reads, client_writes) = std::io::pipe().unwrap();
    std::thread::spawn(move || {
        let _ = serve(agent_reads, agent_writes);
    });
    AgentAuthority::connect(client_reads, client_writes)
        .unwrap()
        .with_commands(Arc::new(LocalAuthority))
}

#[test]
fn the_agent_authority_conforms() {
    let dir = scratch("conform");
    let auth = agent();
    let failures = conformance::run(&auth, &dir);
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(auth.name(), "agent");
    assert_eq!(auth.version(), VERSION);
    assert!(!auth.is_local());
}

#[test]
fn a_batch_stats_many_paths_in_order() {
    let dir = scratch("batch");
    std::fs::create_dir_all(&dir).unwrap();
    let files: Vec<PathBuf> = (0..5000)
        .map(|i| {
            let p = dir.join(format!("f{i}"));
            if i % 2 == 0 {
                std::fs::write(&p, vec![b'x'; i % 7]).unwrap();
            }
            p
        })
        .collect();
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    let auth = agent();
    let stats = auth.metadata_batch(&refs);
    assert_eq!(stats.len(), files.len());
    for (i, s) in stats.iter().enumerate() {
        if i % 2 == 0 {
            assert_eq!(s.as_ref().unwrap().len, (i % 7) as u64, "file {i}");
        } else {
            assert_eq!(s.as_ref().unwrap_err().kind(), std::io::ErrorKind::NotFound);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_watch_reports_changes() {
    let dir = scratch("watch");
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"one").unwrap();
    let auth = agent();
    let watch = auth
        .watch(&[a.clone(), b.clone()], Duration::from_millis(100))
        .unwrap();
    std::thread::sleep(Duration::from_millis(250));
    assert!(watch.changed().is_empty(), "nothing changed yet");
    std::fs::write(&a, b"one two").unwrap();
    std::fs::write(&b, b"new").unwrap();
    let mut seen: Vec<PathBuf> = Vec::new();
    for _ in 0..50 {
        if let Some(paths) = watch.next_timeout(Duration::from_millis(100)) {
            seen.extend(paths);
        }
        if seen.contains(&a) && seen.contains(&b) {
            break;
        }
    }
    assert!(seen.contains(&a) && seen.contains(&b), "{seen:?}");
    drop(watch);
    // The agent still answers after the unsubscribe.
    assert!(auth.is_file(&a));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_threads_share_one_connection() {
    let dir = scratch("threads");
    std::fs::create_dir_all(&dir).unwrap();
    let auth = Arc::new(agent());
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let auth = Arc::clone(&auth);
            let dir = dir.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    let p = dir.join(format!("t{t}-{i}"));
                    auth.write_atomic(&p, format!("{t}/{i}").as_bytes())
                        .unwrap();
                    assert_eq!(auth.read_to_string(&p).unwrap(), format!("{t}/{i}"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 400);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fake agent at the far end of the pipes that answers the hello with
/// `reply` and then does `then` with the rest of the conversation.
fn fake_agent(
    reply: Vec<u8>,
    then: impl FnOnce(&mut std::io::PipeReader, &mut std::io::PipeWriter) + Send + 'static,
) -> std::io::Result<AgentAuthority> {
    let (client_reads, mut agent_writes) = std::io::pipe().unwrap();
    let (mut agent_reads, client_writes) = std::io::pipe().unwrap();
    std::thread::spawn(move || {
        let _ = wire::read_frame(&mut agent_reads);
        let _ = agent_writes.write_all(&reply);
        then(&mut agent_reads, &mut agent_writes);
    });
    AgentAuthority::connect(client_reads, client_writes)
}

fn framed(m: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    wire::write_frame(&mut out, &wire::encode(m)).unwrap();
    out
}

#[test]
fn another_version_is_refused_at_the_handshake() {
    let hello = framed(&Message::Hello {
        protocol: PROTOCOL,
        version: "0.0.0-other".into(),
    });
    let err = fake_agent(hello, |_, _| {}).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("0.0.0-other"));
    let other_protocol = framed(&Message::Hello {
        protocol: PROTOCOL + 1,
        version: VERSION.into(),
    });
    assert!(fake_agent(other_protocol, |_, _| {}).is_err());
}

#[test]
fn garbage_instead_of_a_hello_is_refused() {
    assert!(fake_agent(b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec(), |_, _| {}).is_err());
    // A length prefix promising 64 MiB that never arrives.
    assert!(fake_agent(vec![0x04, 0, 0, 0, 1], |_, _| {}).is_err());
    // An empty answer (the agent exited).
    assert!(fake_agent(Vec::new(), |_, _| {}).is_err());
}

#[test]
fn a_hostile_reply_ends_the_connection_cleanly() {
    let hello = framed(&Message::Hello {
        protocol: PROTOCOL,
        version: VERSION.into(),
    });
    let auth = fake_agent(hello, |r, w| {
        // Answer the first request with an oversized length, then garbage.
        let _ = wire::read_frame(r);
        let _ = w.write_all(&u32::MAX.to_be_bytes());
        let _ = w.write_all(b"junk");
        let mut sink = Vec::new();
        let _ = r.read_to_end(&mut sink);
    })
    .unwrap();
    let err = auth.metadata(Path::new("/")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
    assert!(!auth.is_connected());
    // Later calls fail at once rather than hanging.
    assert!(auth.read(Path::new("/x")).is_err());
}

#[test]
fn a_reply_to_an_unknown_id_or_of_the_wrong_type_is_harmless() {
    let mut script = framed(&Message::Hello {
        protocol: PROTOCOL,
        version: VERSION.into(),
    });
    script.extend(framed(&Message::Reply {
        id: 999,
        reply: wire::Reply::Unit,
    }));
    let auth = fake_agent(script, |r, w| {
        // Answer a stat with a Bool.
        if let Ok(Some(frame)) = wire::read_frame(r)
            && let Ok(Message::Request { id, .. }) = wire::decode(&frame)
        {
            let _ = w.write_all(&framed(&Message::Reply {
                id,
                reply: wire::Reply::Bool(true),
            }));
        }
        let mut sink = Vec::new();
        let _ = r.read_to_end(&mut sink);
    })
    .unwrap();
    let err = auth.metadata(Path::new("/")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn the_agent_stops_on_a_malformed_request_and_at_end_of_input() {
    let mut input = framed(&Message::Hello {
        protocol: PROTOCOL,
        version: VERSION.into(),
    });
    input.extend([0, 0, 0, 1, 99]);
    let w = SharedBuf::default();
    assert!(serve(input.as_slice(), w.clone()).is_err());
    let out = w.0.lock().unwrap().clone();
    // It said hello before giving up.
    let mut reader = out.as_slice();
    let hello = wire::decode(&wire::read_frame(&mut reader).unwrap().unwrap()).unwrap();
    assert!(matches!(hello, Message::Hello { .. }));
    // Clean end of input is a clean exit.
    assert!(serve([].as_slice(), SharedBuf::default()).is_ok());
}

#[derive(Clone, Default)]
struct SharedBuf(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
