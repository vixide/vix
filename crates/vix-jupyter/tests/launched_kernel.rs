//! Kernels Vix launches itself (a shell script stands in for the kernel
//! process) with an in-process fake kernel bound on the launched
//! connection's ports. Covers interrupt by signal and by message, restart
//! (with and without the process obeying `shutdown_request`), shutdown, and
//! relaunch failure. Unix only: the stand-in kernel is a `sh` script.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vix_jupyter::core::wire::{self, Limits, Signer};
use vix_jupyter::core::{ConnectionInfo, KernelSpec, Message, Transport};
use vix_jupyter::{DeathReason, KernelEvent, KernelHandle, KernelState, LaunchOptions};
use zeromq::{PubSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

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

/// A scratch directory removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("vjlk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).must();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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

/// A stand-in kernel script: copies its connection file to `$DEST`, then
/// runs `body`. Returns the script's path.
fn write_script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("kernel.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncp \"$1\" \"$DEST.tmp\" && mv \"$DEST.tmp\" \"$DEST\"\n{body}\n"),
    )
    .must();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).must();
    path
}

fn spec_for(script: &Path, dest: &Path, mode: &str) -> KernelSpec {
    let json = json!({
        "argv": [script.to_string_lossy(), "{connection_file}"],
        "env": {"DEST": dest.to_string_lossy()},
        "language": "sh",
        "interrupt_mode": mode,
    })
    .to_string();
    KernelSpec::parse("launched", Path::new("/nonexistent"), &json).must()
}

/// Wait for the stand-in kernel to publish its connection file.
fn wait_for_file(path: &Path) -> ConnectionInfo {
    let end = Instant::now() + WAIT;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(info) = ConnectionInfo::parse(&text)
        {
            return info;
        }
        assert!(
            Instant::now() < end,
            "kernel never published its connection file"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// What the fake kernel saw on the control channel.
#[derive(Debug, PartialEq, Eq)]
enum Control {
    Interrupt,
    Shutdown { restart: bool },
}

struct Fake {
    signer: Signer,
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
}

/// Bind a fake kernel on `info`'s TCP ports. It reports control requests on
/// the returned channel and exits on a `shutdown_request` with
/// `restart: false`.
fn fake_on(info: &ConnectionInfo) -> (mpsc::Receiver<Control>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let info = info.clone();
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .must();
        rt.block_on(async move {
            let mut shell = RouterSocket::new();
            shell.bind(&info.endpoint(vix_jupyter::core::Channel::Shell)).await.must();
            let mut iopub = PubSocket::new();
            iopub.bind(&info.endpoint(vix_jupyter::core::Channel::IoPub)).await.must();
            let mut control = RouterSocket::new();
            control.bind(&info.endpoint(vix_jupyter::core::Channel::Control)).await.must();
            let mut fake = Fake { signer: info.signer().must(), n: 0 };
            loop {
                tokio::select! {
                    m = shell.recv() => {
                        let Ok(m) = m else { break };
                        let req = wire::decode(&m.into_vec(), &fake.signer, Limits::default()).must();
                        if req.header.msg_type == "kernel_info_request" {
                            let r = fake.reply(&req, "kernel_info_reply", json!({"status": "ok", "protocol_version": "5.3", "implementation": "launched-fake", "language_info": {"name": "sh"}}));
                            shell.send(to_zmq(r)).await.must();
                        }
                    }
                    m = control.recv() => {
                        let Ok(m) = m else { break };
                        let req = wire::decode(&m.into_vec(), &fake.signer, Limits::default()).must();
                        match req.header.msg_type.as_str() {
                            "interrupt_request" => {
                                let _ = tx.send(Control::Interrupt);
                                let r = fake.reply(&req, "interrupt_reply", json!({"status": "ok"}));
                                control.send(to_zmq(r)).await.must();
                            }
                            "shutdown_request" => {
                                let restart = req.content["restart"].as_bool().unwrap_or(false);
                                let _ = tx.send(Control::Shutdown { restart });
                                let r = fake.reply(&req, "shutdown_reply", json!({"status": "ok", "restart": restart}));
                                control.send(to_zmq(r)).await.must();
                                if !restart {
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            drop(iopub);
        });
    });
    (rx, handle)
}

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

fn options(dir: &Path, grace: Duration) -> LaunchOptions {
    LaunchOptions {
        runtime_base: dir.to_path_buf(),
        transport: Transport::Tcp,
        shutdown_grace: grace,
        ..LaunchOptions::default()
    }
}

fn is_ready(e: &KernelEvent) -> bool {
    matches!(e, KernelEvent::Ready(_))
}

#[test]
fn interrupt_by_signal_reaches_the_launched_process() {
    let dir = Scratch::new("sig");
    let dest = dir.0.join("conn.json");
    let script = write_script(
        &dir.0,
        "trap 'echo got-sigint >&2; exit 9' INT\nwhile :; do sleep 0.05; done",
    );
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "signal"),
        &options(&dir.0, Duration::from_secs(2)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (control, fake) = fake_on(&info);
    until(&handle, is_ready);

    handle.interrupt();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    let Some(KernelEvent::Dead(DeathReason::Exited { code, stderr })) = events.last() else {
        panic!("{events:?}");
    };
    assert_eq!(*code, Some(9));
    assert!(stderr.contains("got-sigint"), "{stderr}");
    // A signal interrupt sends nothing on the control channel.
    assert!(control.try_recv().is_err());
    drop(handle);
    drop(fake); // detached; it exits when its sockets' peer is gone with the process
}

#[test]
fn interrupt_by_message_uses_the_control_channel() {
    let dir = Scratch::new("msg");
    let dest = dir.0.join("conn.json");
    let script = write_script(&dir.0, "while :; do sleep 0.05; done");
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "message"),
        &options(&dir.0, Duration::from_millis(100)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (control, fake) = fake_on(&info);
    until(&handle, is_ready);

    handle.interrupt();
    assert_eq!(control.recv_timeout(WAIT).must(), Control::Interrupt);
    // The process is untouched.
    handle.shutdown();
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: false }
    );
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    fake.join().must();
}

#[test]
fn restart_stops_the_process_and_relaunches_it_on_the_same_connection() {
    let dir = Scratch::new("restart");
    let dest = dir.0.join("conn.json");
    // The stand-in ignores the shutdown request, so the grace period ends
    // with the worker killing it.
    let script = write_script(&dir.0, "while :; do sleep 0.05; done");
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "signal"),
        &options(&dir.0, Duration::from_millis(100)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (control, fake) = fake_on(&info);
    until(&handle, is_ready);

    std::fs::remove_file(&dest).must();
    handle.restart();
    let events = until(&handle, is_ready);
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: true }
    );
    let pos = |wanted: &KernelEvent| events.iter().position(|e| e == wanted);
    let restarting = pos(&KernelEvent::State(KernelState::Restarting)).must();
    let restarted = pos(&KernelEvent::Restarted).must();
    assert!(restarting < restarted, "{events:?}");
    // The relaunched process published the very same connection.
    assert_eq!(wait_for_file(&dest), info);

    handle.shutdown();
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    fake.join().must();
}

#[test]
fn restart_returns_early_when_the_process_exits_within_the_grace() {
    let dir = Scratch::new("restart-exit");
    let dest = dir.0.join("conn.json");
    // The stand-in exits on SIGTERM-less polling of a flag file instead:
    // it leaves as soon as the file `stop` appears.
    let stop = dir.0.join("stop");
    let script = write_script(
        &dir.0,
        &format!(
            "while [ ! -e '{}' ]; do sleep 0.02; done\nrm -f '{}'",
            stop.display(),
            stop.display()
        ),
    );
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "signal"),
        &options(&dir.0, Duration::from_secs(10)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (control, fake) = fake_on(&info);
    until(&handle, is_ready);

    std::fs::remove_file(&dest).must();
    std::fs::write(&stop, b"").must();
    handle.restart();
    // The process exits on its own, well inside the 10 s grace.
    let started = Instant::now();
    until(&handle, is_ready);
    assert!(started.elapsed() < Duration::from_secs(9));
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: true }
    );
    handle.shutdown();
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    fake.join().must();
}

#[test]
fn a_failed_relaunch_is_reported_as_a_transport_death() {
    let dir = Scratch::new("relaunch-fail");
    let dest = dir.0.join("conn.json");
    let script = write_script(&dir.0, "while :; do sleep 0.05; done");
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "signal"),
        &options(&dir.0, Duration::from_millis(100)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (_control, fake) = fake_on(&info);
    until(&handle, is_ready);

    // The program disappears, so starting it again fails.
    std::fs::remove_file(&script).must();
    handle.restart();
    let events = until(&handle, |e| matches!(e, KernelEvent::Dead(_)));
    assert!(
        matches!(
            events.last(),
            Some(KernelEvent::Dead(DeathReason::Transport(_)))
        ),
        "{events:?}"
    );
    assert!(events.contains(&KernelEvent::State(KernelState::Dead)));
    drop(handle);
    drop(fake);
}

#[test]
fn restarting_a_kernel_vix_did_not_launch_asks_it_to_restart() {
    // An attached kernel: its manager does the restart, Vix only asks.
    let dir = Scratch::new("attached");
    let dest = dir.0.join("conn.json");
    let script = write_script(&dir.0, "while :; do sleep 0.05; done");
    let launcher = KernelHandle::launch(
        &spec_for(&script, &dest, "signal"),
        &options(&dir.0, Duration::from_millis(100)),
    )
    .must();
    let info = wait_for_file(&dest);
    let (control, fake) = fake_on(&info);
    until(&launcher, is_ready);

    let attached = KernelHandle::connect(info, &LaunchOptions::default()).must();
    until(&attached, is_ready);
    attached.restart();
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: true }
    );
    // No `Restarting` state: the worker did not stop anything itself.
    attached.interrupt();
    assert_eq!(control.recv_timeout(WAIT).must(), Control::Interrupt);
    // A shutdown of an attached kernel waits for the shutdown_reply.
    attached.shutdown();
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: false }
    );
    until(&attached, |e| {
        *e == KernelEvent::Dead(DeathReason::Shutdown)
    });
    fake.join().must();
    drop(launcher);
}

#[test]
fn commands_sent_while_connecting_are_replayed_once_connected() {
    let dir = Scratch::new("pending");
    let dest = dir.0.join("conn.json");
    let script = write_script(&dir.0, "while :; do sleep 0.05; done");
    let handle = KernelHandle::launch(
        &spec_for(&script, &dest, "message"),
        &options(&dir.0, Duration::from_millis(100)),
    )
    .must();
    // Nothing is bound on the launched ports yet, so the worker is still
    // connecting: these are queued, not sent.
    let info = wait_for_file(&dest);
    handle.interrupt();
    let _ = handle.execute("1");
    let (control, fake) = fake_on(&info);
    until(&handle, is_ready);
    assert_eq!(control.recv_timeout(WAIT).must(), Control::Interrupt);
    handle.shutdown();
    assert_eq!(
        control.recv_timeout(WAIT).must(),
        Control::Shutdown { restart: false }
    );
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
    fake.join().must();
}
