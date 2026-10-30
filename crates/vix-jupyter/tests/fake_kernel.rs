//! The transport against an in-process fake kernel: ROUTER (shell and
//! control) and PUB (IOPub) sockets from the same `zeromq` crate, speaking
//! the signed wire protocol with scripted replies. No Python needed.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vix_jupyter::core::message::ReplyStatus;
use vix_jupyter::core::wire::{self, DecodeError, Limits, Signer};
use vix_jupyter::core::{ConnectionInfo, Message, Output, StreamName, Transport};
use vix_jupyter::{DeathReason, KernelEvent, KernelHandle, KernelState, LaunchOptions, RequestId};
use zeromq::{PubSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

const KEY: &str = "5b3e2f7c-fake-kernel-key";
const WAIT: Duration = Duration::from_secs(10);

/// `unwrap` for the fake kernel's helpers, which run outside `#[test]`
/// functions (where the workspace's `unwrap_used` lint still applies).
trait Must<T> {
    fn must(self) -> T;
}

impl<T, E: std::fmt::Debug> Must<T> for Result<T, E> {
    fn must(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("fake kernel: {e:?}"),
        }
    }
}

impl<T> Must<T> for Option<T> {
    fn must(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("fake kernel: missing value"),
        }
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

struct Fake {
    signer: Signer,
    count: u64,
    n: u64,
}

impl Fake {
    fn reply(&mut self, to: &Message, msg_type: &str, content: Value) -> Vec<Vec<u8>> {
        self.n += 1;
        let mut m = Message::new(
            msg_type,
            content,
            &format!("k{}", self.n),
            "kernel",
            "kernel",
            "",
        );
        m.identities.clone_from(&to.identities);
        m.parent_header = Some(to.header.clone());
        wire::encode(&m, &self.signer)
    }

    fn publish(&mut self, parent: &Message, msg_type: &str, content: Value) -> Vec<Vec<u8>> {
        self.n += 1;
        let mut m = Message::new(
            msg_type,
            content,
            &format!("k{}", self.n),
            "kernel",
            "kernel",
            "",
        );
        m.identities = vec![format!("kernel.fake.{msg_type}").into_bytes()];
        m.parent_header = Some(parent.header.clone());
        wire::encode(&m, &self.signer)
    }

    fn status(&mut self, parent: &Message, state: &str) -> Vec<Vec<u8>> {
        self.publish(parent, "status", json!({"execution_state": state}))
    }

    /// The IOPub messages and the shell reply for one execute request.
    fn execute(&mut self, req: &Message) -> (Vec<Vec<Vec<u8>>>, Vec<Vec<u8>>) {
        self.count += 1;
        let code = req.content["code"].as_str().unwrap_or_default().to_owned();
        let mut iopub = vec![self.status(req, "busy")];
        let mut status = "ok";
        match code.as_str() {
            "print('hi')" => iopub.push(self.publish(req, "stream", json!({"name": "stdout", "text": "hi\n"}))),
            "forge" => {
                // Signed with the wrong key: must be dropped.
                let mut forged = Message::new("stream", json!({"name": "stdout", "text": "forged\n"}), "x", "kernel", "kernel", "");
                forged.parent_header = Some(req.header.clone());
                iopub.push(wire::encode(&forged, &Signer::new(b"attacker").must()));
                // Another client's output (other session): must be ignored.
                let mut other = req.clone();
                other.header.session = "someone-else".into();
                iopub.push(self.publish(&other, "stream", json!({"name": "stdout", "text": "other\n"})));
                iopub.push(self.publish(req, "stream", json!({"name": "stdout", "text": "real\n"})));
            }
            "1/0" => {
                status = "error";
                iopub.push(self.publish(req, "error", json!({"ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": ["\u{1b}[31mZeroDivisionError\u{1b}[39m: division by zero"]})));
            }
            "img" => iopub.push(self.publish(req, "display_data", json!({"data": {"image/png": "iVBORw0KGgo=", "text/plain": "<Image>"}, "metadata": {}, "transient": {}}))),
            _ => iopub.push(self.publish(req, "execute_result", json!({"execution_count": self.count, "data": {"text/plain": code}, "metadata": {}}))),
        }
        iopub.push(self.status(req, "idle"));
        let reply = self.reply(
            req,
            "execute_reply",
            json!({"status": status, "execution_count": self.count}),
        );
        (iopub, reply)
    }
}

/// Bind a fake kernel; return its connection and the thread.
fn fake_kernel(transport: Transport) -> (ConnectionInfo, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .must();
        rt.block_on(async move {
            let dir = std::env::temp_dir().join(format!("vjfk-{}-{:?}", std::process::id(), thread::current().id()).replace(['(', ')'], ""));
            let (base, ip) = match transport {
                Transport::Tcp => ("tcp://127.0.0.1:0".to_owned(), "127.0.0.1".to_owned()),
                Transport::Ipc => {
                    std::fs::create_dir_all(&dir).must();
                    let prefix = dir.join("k").to_string_lossy().into_owned();
                    (prefix.clone(), prefix)
                }
            };
            let addr = |n: u16| match transport {
                Transport::Tcp => base.clone(),
                Transport::Ipc => format!("ipc://{base}-{n}"),
            };
            let port = |e: &zeromq::Endpoint, n: u16| match e {
                zeromq::Endpoint::Tcp(_, p) => *p,
                _ => n,
            };
            let mut shell = RouterSocket::new();
            let shell_ep = shell.bind(&addr(1)).await.must();
            let mut iopub = PubSocket::new();
            let iopub_ep = iopub.bind(&addr(2)).await.must();
            let mut control = RouterSocket::new();
            let control_ep = control.bind(&addr(4)).await.must();
            let ports = [port(&shell_ep, 1), port(&iopub_ep, 2), 3, port(&control_ep, 4), 5];
            tx.send(ConnectionInfo::new(transport, &ip, ports, KEY, "fake")).must();

            let mut fake = Fake { signer: Signer::new(KEY.as_bytes()).must(), count: 0, n: 0 };
            loop {
                tokio::select! {
                    m = shell.recv() => {
                        let Ok(m) = m else { break };
                        let req = wire::decode(&m.into_vec(), &fake.signer, Limits::default()).must();
                        match req.header.msg_type.as_str() {
                            "kernel_info_request" => {
                                let r = fake.reply(&req, "kernel_info_reply", json!({"status": "ok", "protocol_version": "5.3", "implementation": "fake", "language_info": {"name": "python"}}));
                                shell.send(to_zmq(r)).await.must();
                            }
                            "execute_request" => {
                                assert_eq!(req.content["allow_stdin"], json!(false));
                                let (out, reply) = fake.execute(&req);
                                for frames in out {
                                    iopub.send(to_zmq(frames)).await.must();
                                }
                                shell.send(to_zmq(reply)).await.must();
                            }
                            _ => {}
                        }
                    }
                    m = control.recv() => {
                        let Ok(m) = m else { break };
                        let req = wire::decode(&m.into_vec(), &fake.signer, Limits::default()).must();
                        match req.header.msg_type.as_str() {
                            "interrupt_request" => {
                                let note = fake.publish(&req, "stream", json!({"name": "stderr", "text": "interrupted"}));
                                iopub.send(to_zmq(note)).await.must();
                                let r = fake.reply(&req, "interrupt_reply", json!({"status": "ok"}));
                                control.send(to_zmq(r)).await.must();
                            }
                            "shutdown_request" => {
                                let r = fake.reply(&req, "shutdown_reply", json!({"status": "ok", "restart": false}));
                                control.send(to_zmq(r)).await.must();
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
        });
    });
    (rx.recv_timeout(WAIT).must(), handle)
}

/// Collect events until `done` matches one; panic on timeout.
fn until(handle: &KernelHandle, mut done: impl FnMut(&KernelEvent) -> bool) -> Vec<KernelEvent> {
    let end = Instant::now() + WAIT;
    let mut seen = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let event = handle
            .events()
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("timed out; saw {seen:#?}"));
        let stop = done(&event);
        seen.push(event);
        if stop {
            return seen;
        }
    }
}

fn outputs_of(events: &[KernelEvent], id: &RequestId) -> Vec<Output> {
    events
        .iter()
        .filter_map(|e| match e {
            KernelEvent::Output { request, output } if request == id => Some(output.clone()),
            _ => None,
        })
        .collect()
}

fn reply_of(id: &RequestId) -> impl FnMut(&KernelEvent) -> bool + '_ {
    move |e| matches!(e, KernelEvent::Reply { request, .. } if request == id)
}

fn session(transport: Transport) {
    let (info, kernel) = fake_kernel(transport);
    let handle = KernelHandle::connect(info, &LaunchOptions::default()).must();
    let start = until(&handle, |e| matches!(e, KernelEvent::Ready(_)));
    assert_eq!(start[0], KernelEvent::State(KernelState::Starting));
    let Some(KernelEvent::Ready(info)) = start.last() else {
        unreachable!()
    };
    assert_eq!(info.implementation, "fake");

    // A stream, with busy/idle around it and the reply last.
    let id = handle.execute("print('hi')");
    let events = until(&handle, reply_of(&id));
    assert_eq!(
        outputs_of(&events, &id),
        [Output::Stream {
            name: StreamName::Stdout,
            text: "hi\n".into()
        }]
    );
    assert!(events.contains(&KernelEvent::State(KernelState::Busy)));
    assert!(matches!(
        events.last(),
        Some(KernelEvent::Reply {
            status: ReplyStatus::Ok,
            execution_count: Some(1),
            ..
        })
    ));

    // A forged message is rejected; another session's output is ignored.
    let id = handle.execute("forge");
    let events = until(&handle, reply_of(&id));
    assert!(events.contains(&KernelEvent::Rejected(DecodeError::BadSignature)));
    assert_eq!(
        outputs_of(&events, &id),
        [Output::Stream {
            name: StreamName::Stdout,
            text: "real\n".into()
        }]
    );

    // An error.
    let id = handle.execute("1/0");
    let events = until(&handle, reply_of(&id));
    assert!(matches!(
        events.last(),
        Some(KernelEvent::Reply {
            status: ReplyStatus::Error,
            ..
        })
    ));
    let out = outputs_of(&events, &id);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].plain_text(),
        "ZeroDivisionError: division by zero\nZeroDivisionError: division by zero"
    );

    // A rich display picks the PNG.
    let id = handle.execute("img");
    let events = until(&handle, reply_of(&id));
    let out = outputs_of(&events, &id);
    let Output::Display { data, .. } = &out[0] else {
        panic!("{out:?}")
    };
    assert_eq!(data.choose().map(|c| c.mime), Some("image/png"));

    // Interrupt goes to control (a kernel Vix did not launch).
    handle.interrupt();
    until(
        &handle,
        |e| matches!(e, KernelEvent::Output { output: Output::Stream { text, .. }, .. } if text == "interrupted"),
    );

    // Shutdown.
    handle.shutdown();
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    kernel.join().must();
}

#[test]
fn tcp_session() {
    session(Transport::Tcp);
}

#[cfg(unix)]
#[test]
fn ipc_session() {
    session(Transport::Ipc);
}

#[test]
fn remote_kernels_are_refused() {
    let info = ConnectionInfo::new(Transport::Tcp, "192.0.2.1", [1, 2, 3, 4, 5], KEY, "x");
    assert!(matches!(
        KernelHandle::connect(info, &LaunchOptions::default()),
        Err(vix_jupyter::LaunchError::NotLocal)
    ));
}

#[test]
fn nothing_listening_times_out_and_drop_is_prompt() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").must();
    let port = listener.local_addr().must().port();
    drop(listener);
    let info = ConnectionInfo::new(Transport::Tcp, "127.0.0.1", [port; 5], KEY, "x");
    let options = LaunchOptions {
        start_timeout: Duration::from_millis(300),
        ..LaunchOptions::default()
    };
    let handle = KernelHandle::connect(info.clone(), &options).must();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    assert!(matches!(
        events.last(),
        Some(KernelEvent::Dead(
            DeathReason::StartTimeout | DeathReason::Transport(_)
        ))
    ));

    // A handle dropped while still connecting does not wait out the timeout.
    let slow = LaunchOptions {
        start_timeout: Duration::from_secs(60),
        ..LaunchOptions::default()
    };
    let handle = KernelHandle::connect(info, &slow).must();
    thread::sleep(Duration::from_millis(100));
    let t = Instant::now();
    drop(handle);
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn launched_kernel_gets_a_connection_file_and_its_exit_is_reported() {
    use vix_jupyter::core::KernelSpec;
    // The "kernel" checks that its connection file exists, is private and
    // holds the key, and that no argument carries the key; then exits 7.
    let script = r#"f="$1"; [ -f "$f" ] || exit 2; case "$(ls -l "$f")" in -rw-------*) ;; *) exit 3;; esac; grep -q '"key"' "$f" || exit 4; echo "kernel says bye" >&2; exit 7"#;
    let json = serde_json::json!({"argv": ["sh", "-c", script, "sh", "{connection_file}"], "language": "sh"}).to_string();
    let spec = KernelSpec::parse("fake-sh", std::path::Path::new("/nonexistent"), &json).must();
    let handle = KernelHandle::launch(&spec, &LaunchOptions::default()).must();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    let Some(KernelEvent::Dead(DeathReason::Exited { code, stderr })) = events.last() else {
        panic!("{events:?}");
    };
    assert_eq!(*code, Some(7));
    assert!(stderr.contains("kernel says bye"));
}

#[test]
fn missing_kernel_program_is_a_spawn_error() {
    use vix_jupyter::core::KernelSpec;
    let spec = KernelSpec::parse(
        "nope",
        std::path::Path::new("/nonexistent"),
        r#"{"argv": ["vix-no-such-kernel-binary", "{connection_file}"]}"#,
    )
    .must();
    assert!(matches!(
        KernelHandle::launch(&spec, &LaunchOptions::default()),
        Err(vix_jupyter::LaunchError::Spawn(_))
    ));
}
