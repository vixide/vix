//! The worker's bookkeeping against a scripted kernel: the test decides
//! exactly which shell and IOPub messages arrive and in what order, so the
//! reply/idle pairing, the tracking caps, and the dropped-message cases are
//! all deterministic. No Python, no timing assertions beyond lower bounds.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vix_jupyter::core::message::{Header, ReplyStatus};
use vix_jupyter::core::wire::{self, DecodeError, Limits, Signer};
use vix_jupyter::core::{ConnectionInfo, Message, Output, StreamName, Transport};
use vix_jupyter::{DeathReason, KernelEvent, KernelHandle, KernelState, LaunchOptions, RequestId};
use zeromq::{PubSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

const KEY: &str = "scripted-kernel-key";
const WAIT: Duration = Duration::from_secs(20);

/// `unwrap` for the helpers, which run outside `#[test]` functions (where the
/// workspace's `unwrap_used` lint still applies).
trait Must<T> {
    fn must(self) -> T;
}

impl<T, E: std::fmt::Debug> Must<T> for Result<T, E> {
    fn must(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("test helper: {e:?}"),
        }
    }
}

impl<T> Must<T> for Option<T> {
    fn must(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("test helper: missing value"),
        }
    }
}

/// Which socket a scripted message goes out on.
#[derive(Clone, Copy)]
enum On {
    Shell,
    IoPub,
}

/// One scripted message.
struct Act {
    on: On,
    msg_type: &'static str,
    content: Value,
    parent: Option<Header>,
    key: &'static str,
}

fn act(on: On, msg_type: &'static str, content: Value, parent: Option<&Header>) -> Act {
    Act {
        on,
        msg_type,
        content,
        parent: parent.cloned(),
        key: KEY,
    }
}

fn to_zmq(frames: Vec<Vec<u8>>) -> ZmqMessage {
    let mut frames = frames.into_iter();
    let mut m = ZmqMessage::from(frames.next().must());
    for f in frames {
        m.push_back(f.into());
    }
    m
}

/// A kernel that sends exactly what the test tells it to.
struct Scripted {
    info: ConnectionInfo,
    script: UnboundedSender<Option<Act>>,
    /// The worker's session id, learned from its `kernel_info_request`.
    session: mpsc::Receiver<String>,
    thread: thread::JoinHandle<()>,
}

impl Scripted {
    fn new(answer_info: bool) -> Self {
        let (info_tx, info_rx) = mpsc::channel();
        let (session_tx, session) = mpsc::channel();
        let (script, mut script_rx) = unbounded_channel::<Option<Act>>();
        let thread = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .must();
            rt.block_on(async move {
                let mut shell = RouterSocket::new();
                let shell_ep = shell.bind("tcp://127.0.0.1:0").await.must();
                let mut iopub = PubSocket::new();
                let iopub_ep = iopub.bind("tcp://127.0.0.1:0").await.must();
                let mut control = RouterSocket::new();
                let control_ep = control.bind("tcp://127.0.0.1:0").await.must();
                let port = |e: &zeromq::Endpoint| match e {
                    zeromq::Endpoint::Tcp(_, p) => *p,
                    _ => 0,
                };
                let ports = [port(&shell_ep), port(&iopub_ep), 3, port(&control_ep), 5];
                info_tx
                    .send(ConnectionInfo::new(Transport::Tcp, "127.0.0.1", ports, KEY, "scripted"))
                    .must();
                let mut n = 0u64;
                let mut identities: Vec<Vec<u8>> = Vec::new();
                let own = Signer::new(KEY.as_bytes()).must();
                loop {
                    tokio::select! {
                        m = shell.recv() => {
                            let Ok(m) = m else { break };
                            let req = wire::decode(&m.into_vec(), &own, Limits::default()).must();
                            identities.clone_from(&req.identities);
                            if req.header.msg_type == "kernel_info_request" {
                                let _ = session_tx.send(req.header.session.clone());
                                if answer_info {
                                    n += 1;
                                    let mut r = Message::new("kernel_info_reply", json!({"status": "ok", "protocol_version": "5.3", "implementation": "scripted", "language_info": {"name": "x"}}), &format!("i{n}"), "kernel", "kernel", "");
                                    r.identities.clone_from(&identities);
                                    r.parent_header = Some(req.header.clone());
                                    shell.send(to_zmq(wire::encode(&r, &own))).await.must();
                                }
                            }
                        }
                        _ = control.recv() => {}
                        next = script_rx.recv() => {
                            let Some(Some(act)) = next else { break };
                            n += 1;
                            let mut m = Message::new(act.msg_type, act.content, &format!("s{n}"), "kernel", "kernel", "");
                            m.parent_header = act.parent;
                            let signer = Signer::new(act.key.as_bytes()).must();
                            match act.on {
                                On::Shell => {
                                    m.identities.clone_from(&identities);
                                    shell.send(to_zmq(wire::encode(&m, &signer))).await.must();
                                }
                                On::IoPub => {
                                    m.identities = vec![format!("kernel.scripted.{}", act.msg_type).into_bytes()];
                                    iopub.send(to_zmq(wire::encode(&m, &signer))).await.must();
                                }
                            }
                        }
                    }
                }
            });
        });
        let info = info_rx.recv_timeout(WAIT).must();
        Self {
            info,
            script,
            session,
            thread,
        }
    }

    fn send(&self, act: Act) {
        self.script.send(Some(act)).must();
    }

    fn stop(self) {
        let _ = self.script.send(None);
        self.thread.join().must();
    }

    /// The parent header of an `execute_request` the worker "sent".
    fn request(&self, session: &str, id: &str) -> Header {
        let _ = self;
        Message::new("execute_request", json!({}), id, session, "vix", "").header
    }
}

fn until(handle: &KernelHandle, mut done: impl FnMut(&KernelEvent) -> bool) -> Vec<KernelEvent> {
    let end = Instant::now() + WAIT;
    let mut seen = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let event = handle
            .events()
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("timed out; saw {} events", seen.len()));
        let stop = done(&event);
        seen.push(event);
        if stop {
            return seen;
        }
    }
}

/// A connected handle and the scripted kernel, past `Ready`.
fn ready() -> (KernelHandle, Scripted, String) {
    let kernel = Scripted::new(true);
    let handle = KernelHandle::connect(kernel.info.clone(), &LaunchOptions::default()).must();
    until(&handle, |e| matches!(e, KernelEvent::Ready(_)));
    let session = kernel.session.recv_timeout(WAIT).must();
    (handle, kernel, session)
}

fn ok_reply(count: u64) -> Value {
    json!({"status": "ok", "execution_count": count})
}

fn status(state: &str) -> Value {
    json!({"execution_state": state})
}

fn reply_ids(events: &[KernelEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            KernelEvent::Reply { request, .. } => Some(request.0.clone()),
            _ => None,
        })
        .collect()
}

fn shutdown(handle: &KernelHandle, kernel: Scripted) {
    handle.shutdown();
    until(handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    kernel.stop();
}

#[test]
fn a_reply_that_follows_its_idle_status_is_reported_at_once() {
    let (handle, kernel, session) = ready();
    let req = kernel.request(&session, "req1");
    kernel.send(act(On::IoPub, "status", status("busy"), Some(&req)));
    kernel.send(act(On::IoPub, "status", status("idle"), Some(&req)));
    until(&handle, |e| *e == KernelEvent::State(KernelState::Idle));
    kernel.send(act(On::Shell, "execute_reply", ok_reply(4), Some(&req)));
    let events = until(&handle, |e| matches!(e, KernelEvent::Reply { .. }));
    assert_eq!(
        events.last(),
        Some(&KernelEvent::Reply {
            request: RequestId("req1".into()),
            status: ReplyStatus::Ok,
            execution_count: Some(4),
        })
    );
    shutdown(&handle, kernel);
}

#[test]
fn a_reply_whose_idle_never_comes_is_released_after_the_wait() {
    let (handle, kernel, session) = ready();
    let req = kernel.request(&session, "lonely");
    let sent = Instant::now();
    kernel.send(act(
        On::Shell,
        "execute_reply",
        json!({"status": "error"}),
        Some(&req),
    ));
    let events = until(&handle, |e| matches!(e, KernelEvent::Reply { .. }));
    // Held for the full wait (one second) before it is given up on.
    assert!(
        sent.elapsed() >= Duration::from_secs(1),
        "{:?}",
        sent.elapsed()
    );
    assert_eq!(
        events.last(),
        Some(&KernelEvent::Reply {
            request: RequestId("lonely".into()),
            status: ReplyStatus::Error,
            execution_count: None,
        })
    );
    shutdown(&handle, kernel);
}

#[test]
fn held_replies_are_capped_and_the_oldest_is_released_first() {
    let (handle, kernel, session) = ready();
    // One more than the 256 tracked: the 257th pushes the first out.
    for i in 0..257 {
        let req = kernel.request(&session, &format!("r{i}"));
        kernel.send(act(On::Shell, "execute_reply", ok_reply(i), Some(&req)));
    }
    let mut ids = Vec::new();
    until(&handle, |e| {
        if let KernelEvent::Reply { request, .. } = e {
            ids.push(request.0.clone());
        }
        ids.len() == 257
    });
    assert_eq!(ids.first().map(String::as_str), Some("r0"));
    let mut sorted = ids;
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 257, "every reply reported exactly once");
    shutdown(&handle, kernel);
}

#[test]
fn idle_statuses_are_capped_and_the_oldest_is_forgotten() {
    let (handle, kernel, session) = ready();
    for i in 0..257 {
        let req = kernel.request(&session, &format!("q{i}"));
        kernel.send(act(On::IoPub, "status", status("idle"), Some(&req)));
    }
    let mut idles = 0;
    until(&handle, |e| {
        idles += usize::from(*e == KernelEvent::State(KernelState::Idle));
        idles == 257
    });
    // q256's idle is remembered, so its reply is reported at once; q0's was
    // forgotten, so its reply waits out the hold and arrives after.
    let oldest = kernel.request(&session, "q0");
    let newest = kernel.request(&session, "q256");
    kernel.send(act(On::Shell, "execute_reply", ok_reply(1), Some(&oldest)));
    kernel.send(act(On::Shell, "execute_reply", ok_reply(2), Some(&newest)));
    let mut ids = Vec::new();
    until(&handle, |e| {
        if let KernelEvent::Reply { request, .. } = e {
            ids.push(request.0.clone());
        }
        ids.len() == 2
    });
    assert_eq!(ids, ["q256", "q0"]);
    shutdown(&handle, kernel);
}

#[test]
fn foreign_forged_and_unrelated_messages_never_become_events() {
    let (handle, kernel, session) = ready();
    let mine = kernel.request(&session, "mine");
    let theirs = kernel.request("another-session", "theirs");
    let stream = json!({"name": "stdout", "text": "x\n"});

    // Shell: a forged message, an unrelated reply type, an execute_reply
    // with no parent, then a second forged message as the end marker.
    let mut forged = act(On::Shell, "execute_reply", ok_reply(1), Some(&mine));
    forged.key = "attacker";
    kernel.send(forged);
    kernel.send(act(
        On::Shell,
        "interrupt_reply",
        json!({"status": "ok"}),
        Some(&mine),
    ));
    kernel.send(act(On::Shell, "execute_reply", ok_reply(2), None));
    let mut marker = act(On::Shell, "execute_reply", ok_reply(3), Some(&mine));
    marker.key = "attacker";
    kernel.send(marker);

    // IOPub: statuses that change nothing but the state, an idle for another
    // session's request, outputs with no parent or another session's parent,
    // a message that is no output, and finally one real output.
    kernel.send(act(On::IoPub, "status", status("starting"), Some(&mine)));
    kernel.send(act(On::IoPub, "status", status("mystery"), Some(&mine)));
    kernel.send(act(On::IoPub, "status", status("idle"), Some(&theirs)));
    kernel.send(act(On::IoPub, "stream", stream.clone(), None));
    kernel.send(act(On::IoPub, "stream", stream.clone(), Some(&theirs)));
    kernel.send(act(
        On::IoPub,
        "execute_input",
        json!({"code": "1", "execution_count": 1}),
        Some(&mine),
    ));
    kernel.send(act(
        On::IoPub,
        "comm_open",
        json!({"anything": true}),
        Some(&mine),
    ));
    kernel.send(act(On::IoPub, "stream", stream, Some(&mine)));

    let mut rejected = 0;
    let mut output = false;
    let events = until(&handle, |e| {
        rejected += usize::from(*e == KernelEvent::Rejected(DecodeError::BadSignature));
        output |= matches!(e, KernelEvent::Output { .. });
        rejected == 2 && output
    });
    assert_eq!(rejected, 2);
    let outputs: Vec<&KernelEvent> = events
        .iter()
        .filter(|e| matches!(e, KernelEvent::Output { .. }))
        .collect();
    assert_eq!(
        outputs,
        [&KernelEvent::Output {
            request: RequestId("mine".into()),
            output: Output::Stream {
                name: StreamName::Stdout,
                text: "x\n".into()
            },
        }]
    );
    // The `starting` status and the other session's `idle` are state changes
    // the status bar shows; `mystery` is not a state at all. No reply was
    // ever reported.
    let states: Vec<&KernelEvent> = events
        .iter()
        .filter(|e| matches!(e, KernelEvent::State(_)))
        .collect();
    assert_eq!(
        states,
        [
            &KernelEvent::State(KernelState::Starting),
            &KernelEvent::State(KernelState::Idle)
        ]
    );
    assert_eq!(reply_ids(&events), Vec::<String>::new());
    shutdown(&handle, kernel);
}

#[test]
fn a_kernel_that_never_answers_kernel_info_times_out() {
    let kernel = Scripted::new(false);
    let options = LaunchOptions {
        start_timeout: Duration::from_millis(300),
        ..LaunchOptions::default()
    };
    let handle = KernelHandle::connect(kernel.info.clone(), &options).must();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    assert_eq!(
        events.last(),
        Some(&KernelEvent::Dead(DeathReason::StartTimeout))
    );
    assert!(!events.iter().any(|e| matches!(e, KernelEvent::Ready(_))));
    drop(handle);
    kernel.stop();
}

#[test]
fn a_start_timeout_too_large_to_be_a_deadline_is_a_start_timeout() {
    let info = ConnectionInfo::new(Transport::Tcp, "127.0.0.1", [1, 2, 3, 4, 5], KEY, "x");
    let options = LaunchOptions {
        start_timeout: Duration::MAX,
        ..LaunchOptions::default()
    };
    let handle = KernelHandle::connect(info, &options).must();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    assert_eq!(
        events,
        [
            KernelEvent::State(KernelState::Starting),
            KernelEvent::State(KernelState::Dead),
            KernelEvent::Dead(DeathReason::StartTimeout),
        ]
    );
}
