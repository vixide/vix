//! T734c–T734g: the App's Jupyter wiring against an in-process fake kernel
//! (the `jupyter` feature). The fake speaks the signed wire protocol over
//! loopback TCP with ROUTER (shell, control) and PUB (IOPub) sockets from
//! the same `zeromq` crate as `crates/vix-jupyter/tests/fake_kernel.rs`; the
//! App attaches to it through an explicit kernelspec in a temporary
//! directory (`with_jupyter_kernel_dirs` + `with_jupyter_connection`), so no
//! test edits `PATH` or the environment, and CI needs no Python. The
//! kernelspec directory sits next to the workspace, not inside it: a
//! kernelspec inside an untrusted workspace is not used (T734f).
//!
//! The authority slot is process-wide and the remote test installs a fake
//! one, so every test here takes [`LOCK`] first.
//!
//! `real_ipykernel_runs_code` runs only when `VIX_JUPYTER_PYTHON` names a
//! Python with `ipykernel` installed.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use serde_json::{Value, json};
use vix::app::App;
use vix::settings::Settings;
use vix_authority::FakeAuthority;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_jupyter::core::wire::{self, Limits, Signer};
use vix_jupyter::core::{ConnectionInfo, Message, Transport};
use zeromq::{PubSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

const KEY: &str = "0f1e2d3c-app-fake-kernel";
/// A valid 2×2 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=";
/// A PNG header claiming 100000×10 pixels (a "decompression bomb").
const BOMB: &str = "iVBORw0KGgoAAAANSUhEUgABhqAAAAAKCAYAAAA3ueYW";
const WAIT: Duration = Duration::from_secs(10);

/// Serializes the tests (the authority slot is process-wide).
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

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

/// The fake kernel's signing state and execution counter.
struct Fake {
    signer: Signer,
    count: u64,
    n: u64,
}

impl Fake {
    fn message(&mut self, parent: &Message, msg_type: &str, content: Value) -> Message {
        self.n += 1;
        let mut m = Message::new(
            msg_type,
            content,
            &format!("k{}", self.n),
            "kernel",
            "kernel",
            "",
        );
        m.parent_header = Some(parent.header.clone());
        m
    }

    fn reply(&mut self, to: &Message, msg_type: &str, content: Value) -> Vec<Vec<u8>> {
        let mut m = self.message(to, msg_type, content);
        m.identities.clone_from(&to.identities);
        wire::encode(&m, &self.signer)
    }

    fn publish(&mut self, parent: &Message, msg_type: &str, content: Value) -> Vec<Vec<u8>> {
        let mut m = self.message(parent, msg_type, content);
        m.identities = vec![format!("kernel.fake.{msg_type}").into_bytes()];
        wire::encode(&m, &self.signer)
    }

    /// IOPub messages and the shell reply for one `execute_request`.
    fn execute(&mut self, req: &Message, code: &str) -> (Vec<Vec<Vec<u8>>>, Vec<Vec<u8>>) {
        self.count += 1;
        let mut iopub = vec![self.publish(req, "status", json!({"execution_state": "busy"}))];
        let mut status = "ok";
        match code {
            "print('hi')" => {
                iopub.push(self.publish(req, "stream", json!({"name": "stdout", "text": "hi\n"})));
            }
            "1/0" => {
                status = "error";
                iopub.push(self.publish(
                    req,
                    "error",
                    json!({
                        "ename": "ZeroDivisionError",
                        "evalue": "division by zero",
                        "traceback": ["\u{1b}[31mZeroDivisionError\u{1b}[39m: division by zero"],
                    }),
                ));
            }
            "img" | "bomb" => iopub.push(self.publish(
                req,
                "display_data",
                json!({
                    "data": {
                        "image/png": if code == "img" { PNG } else { BOMB },
                        "text/plain": "<Image>",
                    },
                    "metadata": {},
                    "transient": {},
                }),
            )),
            "both" => {
                iopub.push(self.publish(req, "stream", json!({"name": "stdout", "text": "out\n"})));
                iopub.push(self.publish(
                    req,
                    "execute_result",
                    json!({
                        "execution_count": self.count,
                        "data": {"text/plain": "=both"},
                        "metadata": {},
                    }),
                ));
            }
            _ => iopub.push(self.publish(
                req,
                "execute_result",
                json!({
                    "execution_count": self.count,
                    "data": {"text/plain": format!("={code}")},
                    "metadata": {},
                }),
            )),
        }
        iopub.push(self.publish(req, "status", json!({"execution_state": "idle"})));
        let reply = self.reply(
            req,
            "execute_reply",
            json!({"status": status, "execution_count": self.count}),
        );
        (iopub, reply)
    }
}

/// What the fake kernel saw: every executed code string.
type Seen = Arc<Mutex<Vec<String>>>;

/// Bind a fake kernel on loopback TCP; return its connection, the codes it
/// executes, and its thread (which ends on `shutdown_request`).
fn fake_kernel() -> (ConnectionInfo, Seen, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let seen: Seen = Arc::default();
    let log = Arc::clone(&seen);
    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .must();
        rt.block_on(serve(tx, log));
    });
    (rx.recv_timeout(WAIT).must(), seen, handle)
}

async fn serve(tx: mpsc::Sender<ConnectionInfo>, log: Seen) {
    let port = |e: &zeromq::Endpoint| match e {
        zeromq::Endpoint::Tcp(_, p) => *p,
        _ => 0,
    };
    let mut shell = RouterSocket::new();
    let shell_ep = shell.bind("tcp://127.0.0.1:0").await.must();
    let mut iopub = PubSocket::new();
    let iopub_ep = iopub.bind("tcp://127.0.0.1:0").await.must();
    let mut control = RouterSocket::new();
    let control_ep = control.bind("tcp://127.0.0.1:0").await.must();
    let ports = [port(&shell_ep), port(&iopub_ep), 0, port(&control_ep), 0];
    tx.send(ConnectionInfo::new(
        Transport::Tcp,
        "127.0.0.1",
        ports,
        KEY,
        "fake",
    ))
    .must();
    let mut fake = Fake {
        signer: Signer::new(KEY.as_bytes()).must(),
        count: 0,
        n: 0,
    };
    loop {
        tokio::select! {
            m = shell.recv() => {
                let Ok(m) = m else { break };
                let req = wire::decode(&m.into_vec(), &fake.signer, Limits::default()).must();
                match req.header.msg_type.as_str() {
                    "kernel_info_request" => {
                        let r = fake.reply(&req, "kernel_info_reply", json!({
                            "status": "ok",
                            "protocol_version": "5.3",
                            "implementation": "fake",
                            "language_info": {"name": "python"},
                        }));
                        shell.send(to_zmq(r)).await.must();
                    }
                    "execute_request" => {
                        let code = req.content["code"].as_str().unwrap_or_default().to_owned();
                        log.lock().unwrap_or_else(PoisonError::into_inner).push(code.clone());
                        let (out, reply) = fake.execute(&req, &code);
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
                        let note = fake.publish(&req, "stream", json!({"name": "stderr", "text": "KeyboardInterrupt\n"}));
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
}

/// A fresh temporary directory.
fn scratch(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vix-jupyter-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).must();
    dir
}

/// A `kernels` directory next to the workspace `root` (not inside it)
/// holding one kernelspec `name` for `language` with `argv`.
fn kernels_dir(root: &Path, name: &str, language: &str, argv: &[&str]) -> PathBuf {
    write_kernelspec(&root.with_extension("kernels"), name, language, argv)
}

/// Write kernelspec `name` under `base/kernels`; return that directory.
fn write_kernelspec(base: &Path, name: &str, language: &str, argv: &[&str]) -> PathBuf {
    let kernels = base.join("kernels");
    std::fs::create_dir_all(kernels.join(name)).must();
    std::fs::write(
        kernels.join(name).join("kernel.json"),
        json!({"argv": argv, "display_name": format!("Fake {name}"), "language": language})
            .to_string(),
    )
    .must();
    kernels
}

/// An App rooted at `root` whose only kernelspec, `fake` (Python), attaches
/// to `info`. Its argv names a program that does not exist, so a launch
/// would fail loudly instead of running anything.
fn app(root: &Path, info: ConnectionInfo) -> App {
    app_with(root, info, Settings::default())
}

/// [`app`] with `settings`.
fn app_with(root: &Path, info: ConnectionInfo, settings: Settings) -> App {
    let kernels = kernels_dir(
        root,
        "fake",
        "python",
        &["vix-test-no-such-kernel", "-f", "{connection_file}"],
    );
    let mut app = App::new(root.to_path_buf(), settings)
        .with_session_path(root.join("session.toml"))
        .with_jupyter_kernel_dirs(vec![kernels])
        .with_jupyter_connection("fake", info);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// Open `name` with `text` under `root` and put the cursor on `line`.
fn open(app: &mut App, root: &Path, name: &str, text: &str, line: usize) {
    let path = root.join(name);
    std::fs::write(&path, text).must();
    app.open_initial(&path);
    app.editor
        .active_tab_mut()
        .must()
        .editor
        .set_cursor_line(line);
}

fn dock(app: &App) -> Vec<String> {
    app.bottom_dock.visible(100_000).to_vec()
}

/// Tick until `pred` holds, failing after [`WAIT`].
fn pump(app: &mut App, what: &str, pred: impl Fn(&App) -> bool) {
    let end = Instant::now() + WAIT;
    loop {
        app.poll_jupyter();
        if pred(app) {
            return;
        }
        assert!(
            Instant::now() < end,
            "timed out waiting for {what}\ndock: {:#?}\nstatus: {}\nmessages: {:?}",
            dock(app),
            app.status,
            app.messages
                .items
                .iter()
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

/// Run `action` and wait for the request's reply footer in the dock.
fn run_and_wait(app: &mut App, action: &str) {
    // A reply footer starts with `[n] `; the `[*] ` header and an image
    // placeholder do not.
    let footers = |app: &App| {
        dock(app)
            .iter()
            .filter(|l| {
                l.strip_prefix('[')
                    .and_then(|r| r.split_once("] "))
                    .is_some_and(|(n, _)| n.parse::<u64>().is_ok())
            })
            .count()
    };
    let before = footers(app);
    app.run_action(action);
    pump(app, "the reply", |a| footers(a) > before);
}

fn seen(log: &Seen) -> Vec<String> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn shut_down(mut app: App, kernel: thread::JoinHandle<()>) {
    app.run_action("jupyter.shutdown");
    pump(&mut app, "shutdown", |a| a.jupyter_kernels().is_empty());
    kernel.join().must();
}

#[test]
fn nothing_starts_without_an_action() {
    let _guard = lock();
    let root = scratch("idle");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "a.py", "print('hi')\n", 0);
    for _ in 0..20 {
        app.tick();
        thread::sleep(Duration::from_millis(5));
    }
    // Controls with no kernel start nothing either.
    app.run_action("jupyter.interrupt");
    app.run_action("jupyter.restart");
    app.run_action("jupyter.clear_results");
    app.tick();
    assert_eq!(app.jupyter_kernels().len(), 0);
    assert_eq!(seen(&log).len(), 0);
    assert_eq!(app.jupyter_segment(), "");

    // The first explicit Run starts the kernel and sends the line.
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(app.jupyter_kernels().len(), 1);
    assert_eq!(seen(&log), ["print('hi')"]);
    assert!(dock(&app).iter().any(|l| l == "hi"), "{:?}", dock(&app));
    assert!(app.jupyter_segment().starts_with("fake "));
    shut_down(app, kernel);
}

#[test]
fn run_targets_selection_cell_line_and_advance() {
    let _guard = lock();
    let root = scratch("targets");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    let text = "import os\n# %% first\nx = 1\ny = 2\n# %% second\nprint('hi')\n";
    open(&mut app, &root, "cells.py", text, 2);

    // A cell: the lines between the markers. Run and Advance moves to the
    // next marker, so the next Run sends the next cell.
    run_and_wait(&mut app, "jupyter.run_advance");
    assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 4);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(seen(&log), ["x = 1\ny = 2", "print('hi')"]);

    // The lines above the first marker are a cell too.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(0);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(seen(&log)[2], "import os");

    // A selection wins over the cell: `x = 1` is chars 22..27.
    let start = text.find("x = 1").unwrap();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(start, start + 5);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(seen(&log)[3], "x = 1");

    // Without markers: the current line, with its result as an
    // end-of-line hint.
    open(&mut app, &root, "plain.py", "a = 1\n6 * 7\n", 1);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(seen(&log)[4], "6 * 7");
    assert!(dock(&app).iter().any(|l| l == "=6 * 7"));
    assert!(app.editor.active_tab().unwrap().editor.has_inlay_hints());
    app.run_action("jupyter.clear_results");
    assert!(!app.editor.active_tab().unwrap().editor.has_inlay_hints());

    // A blank line runs nothing.
    open(&mut app, &root, "blank.py", "\n\n", 0);
    app.run_action("jupyter.run");
    assert_eq!(seen(&log).len(), 5);
    shut_down(app, kernel);
}

#[test]
fn errors_and_images_reach_the_dock_as_plain_text() {
    let _guard = lock();
    let root = scratch("errors");
    let (info, _log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "e.py", "1/0\nimg\n", 0);
    run_and_wait(&mut app, "jupyter.run");
    let lines = dock(&app);
    assert!(
        lines.contains(&"ZeroDivisionError: division by zero".to_owned()),
        "{lines:?}"
    );
    assert!(lines.iter().all(|l| !l.contains('\u{1b}')), "{lines:?}");

    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(1);
    run_and_wait(&mut app, "jupyter.run");
    let lines = dock(&app);
    assert!(lines.iter().any(|l| l.contains("image/png")), "{lines:?}");
    assert!(lines.contains(&"<Image>".to_owned()));
    shut_down(app, kernel);
}

#[test]
fn an_org_block_asks_first_in_an_untrusted_workspace() {
    let _guard = lock();
    let root = scratch("org");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    let org = "* Notes\n#+begin_src jupyter-python\n40 + 2\n#+end_src\n";
    open(&mut app, &root, "notes.org", org, 2);

    // `n` cancels: nothing starts.
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(!app.jupyter_confirm_open());
    assert_eq!(app.jupyter_kernels().len(), 0);

    // `y` runs the block's body in the Python kernel (`jupyter-python`).
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    pump(&mut app, "the block's result", |a| {
        dock(a).iter().any(|l| l == "=40 + 2")
    });
    assert_eq!(seen(&log), ["40 + 2"]);

    // The same block again runs without asking.
    run_and_wait(&mut app, "jupyter.run");
    assert!(!app.jupyter_confirm_open());
    assert_eq!(seen(&log).len(), 2);
    shut_down(app, kernel);
}

#[test]
fn interrupt_reaches_the_kernel() {
    let _guard = lock();
    let root = scratch("interrupt");
    let (info, _log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "i.py", "x = 1\n", 0);
    run_and_wait(&mut app, "jupyter.run");
    app.run_action("jupyter.interrupt");
    pump(&mut app, "the interrupt", |a| {
        dock(a).iter().any(|l| l == "KeyboardInterrupt")
    });
    shut_down(app, kernel);
}

#[test]
fn start_kernel_opens_the_picker_and_starts_the_choice() {
    let _guard = lock();
    let root = scratch("picker");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    app.run_action("jupyter.start");
    assert_eq!(app.jupyter_picker_rows(), Some(1));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.jupyter_picker_rows(), None);
    pump(&mut app, "the kernel to be ready", |a| {
        a.jupyter_kernels()
            .iter()
            .any(|(_, s)| *s == vix_jupyter::KernelState::Idle)
    });
    assert_eq!(seen(&log).len(), 0, "starting runs no code");
    shut_down(app, kernel);
}

#[test]
fn a_language_without_a_kernelspec_opens_the_picker() {
    let _guard = lock();
    let root = scratch("nolang");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "notes.txt", "hello\n", 0);
    app.run_action("jupyter.run");
    assert_eq!(app.jupyter_picker_rows(), Some(1));
    // Picking the kernel sends the waiting run.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    pump(&mut app, "the waiting run", |a| {
        dock(a).iter().any(|l| l == "=hello")
    });
    assert_eq!(seen(&log), ["hello"]);
    shut_down(app, kernel);
}

#[test]
fn no_kernelspec_installed_warns_and_starts_nothing() {
    let _guard = lock();
    let root = scratch("none");
    let empty = root.join("kernels");
    std::fs::create_dir_all(&empty).unwrap();
    let mut app = App::new(root.clone(), Settings::default())
        .with_session_path(root.join("session.toml"))
        .with_jupyter_kernel_dirs(vec![empty]);
    let warnings = app.messages.items.len();
    app.run_action("jupyter.start");
    assert_eq!(app.jupyter_picker_rows(), None);
    assert_eq!(app.messages.items.len(), warnings + 1);
    assert_eq!(app.jupyter_kernels().len(), 0);
}

#[test]
fn a_missing_kernel_program_is_reported_not_run() {
    let _guard = lock();
    let root = scratch("spawn");
    let kernels = kernels_dir(
        &root,
        "ghost",
        "python",
        &["vix-test-no-such-kernel", "-f", "{connection_file}"],
    );
    let mut app = App::new(root.clone(), Settings::default())
        .with_session_path(root.join("session.toml"))
        .with_jupyter_kernel_dirs(vec![kernels]);
    open(&mut app, &root, "a.py", "1\n", 0);
    let before = app.messages.items.len();
    app.run_action("jupyter.run");
    assert_eq!(app.jupyter_kernels().len(), 0);
    assert_eq!(app.messages.items.len(), before + 1);
}

#[test]
fn a_remote_workspace_is_refused() {
    let _guard = lock();
    let root = scratch("remote");
    let (info, log, _kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "a.py", "print('hi')\n", 0);
    let fake = Arc::new(FakeAuthority::new(root.join("remote")).unwrap());
    let previous = vix_authority::install(fake);
    let before = app.messages.items.len();
    app.run_action("jupyter.start");
    app.run_action("jupyter.run");
    app.run_action("jupyter.run_advance");
    let after = app.messages.items.len();
    let picker = app.jupyter_picker_rows();
    vix_authority::install(previous);
    assert_eq!(after, before + 3, "each action warns");
    assert_eq!(picker, None);
    assert_eq!(app.jupyter_kernels().len(), 0);
    assert_eq!(seen(&log).len(), 0);
}

/// The same flow against a real ipykernel, when `VIX_JUPYTER_PYTHON` names
/// a Python that has it. CI does not set it.
#[test]
fn real_ipykernel_runs_code() {
    let Ok(python) = std::env::var("VIX_JUPYTER_PYTHON") else {
        return;
    };
    let _guard = lock();
    let root = scratch("ipykernel");
    let kernels = kernels_dir(
        &root,
        "py",
        "python",
        &[
            &python,
            "-m",
            "ipykernel_launcher",
            "-f",
            "{connection_file}",
        ],
    );
    let mut app = App::new(root.clone(), Settings::default())
        .with_session_path(root.join("session.toml"))
        .with_jupyter_kernel_dirs(vec![kernels]);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    let code = "for i in range(2):\n    print(i * 21)\n";
    open(&mut app, &root, "r.py", code, 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, code.chars().count());
    app.run_action("jupyter.run");
    pump(&mut app, "ipykernel output", |a| {
        dock(a).iter().any(|l| l == "21")
    });
    app.run_action("jupyter.shutdown");
    pump(&mut app, "ipykernel shutdown", |a| {
        a.jupyter_kernels().is_empty()
    });
}

#[test]
fn ctrl_shift_enter_runs_the_line() {
    let _guard = lock();
    let root = scratch("key");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    open(&mut app, &root, "k.py", "print('hi')\n", 0);
    app.focus = vix::app::Focus::Editor;
    app.on_key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    pump(&mut app, "the keyed run", |a| {
        dock(a).iter().any(|l| l == "hi")
    });
    assert_eq!(seen(&log), ["print('hi')"]);
    // The key ran the code; it did not also type a newline.
    assert_eq!(app.editor.active_tab().unwrap().text(), "print('hi')\n");
    shut_down(app, kernel);
}

// ----- T734e: Org babel -------------------------------------------------------

/// Settings that never ask before an Org block runs.
fn no_prompts() -> Settings {
    let mut settings = Settings::default();
    settings.jupyter_confirm_evaluate = "never".into();
    settings
}

fn text(app: &App) -> String {
    app.editor.active_tab().must().text()
}

/// Replace the active buffer and put the cursor on `line`.
fn set_text(app: &mut App, text: &str, line: usize) {
    let editor = &mut app.editor.active_tab_mut().must().editor;
    editor.set_content(text);
    editor.set_cursor_line(line);
}

#[test]
fn org_results_insert_replace_append_and_silent() {
    let _guard = lock();
    let root = scratch("babel");
    let (info, log, kernel) = fake_kernel();
    let mut app = app_with(&root, info, no_prompts());
    let org = "* Notes\n#+begin_src python\nboth\n#+end_src\n\nAfter.\n";
    open(&mut app, &root, "notes.org", org, 2);

    // Insert: `:results value` (the default) is everything the cell shows.
    run_and_wait(&mut app, "jupyter.run");
    let once =
        "* Notes\n#+begin_src python\nboth\n#+end_src\n\n#+RESULTS:\n: out\n: =both\n\nAfter.\n";
    assert_eq!(text(&app), once);
    assert!(app.editor.active_tab().unwrap().dirty);
    // No end-of-line hint: the results are in the buffer.
    assert!(!app.editor.active_tab().unwrap().editor.has_inlay_hints());

    // Replace: running again leaves one results block, and the same
    // results add no undo step, so one undo takes the first write back.
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(text(&app), once);
    app.run_action("edit.undo");
    assert_eq!(text(&app), org);

    // `C-c C-c` runs the block too; `:results output append` adds only the
    // printed text after the old results.
    let appending = once.replace(
        "#+begin_src python",
        "#+begin_src python :results output append",
    );
    set_text(&mut app, &appending, 2);
    let before = seen(&log).len();
    run_and_wait(&mut app, "org.ctrl_c_ctrl_c");
    assert_eq!(seen(&log).len(), before + 1);
    assert_eq!(
        text(&app),
        appending.replace(": =both\n", ": =both\n: out\n")
    );

    // `:results silent` writes nothing and shows the hint instead.
    let silent = org.replace("#+begin_src python", "#+begin_src python :results silent");
    set_text(&mut app, &silent, 2);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(text(&app), silent);
    assert!(app.editor.active_tab().unwrap().editor.has_inlay_hints());
    shut_down(app, kernel);
}

#[test]
fn org_eval_never_refuses_and_query_always_asks() {
    let _guard = lock();
    let root = scratch("eval");
    let (info, log, kernel) = fake_kernel();
    let mut app = app_with(&root, info, no_prompts());
    let org = "#+begin_src python :eval never\n1\n#+end_src\n";
    open(&mut app, &root, "e.org", org, 1);
    app.run_action("jupyter.run");
    app.run_action("org.ctrl_c_ctrl_c");
    assert!(!app.jupyter_confirm_open());
    assert_eq!(app.jupyter_kernels().len(), 0);
    assert_eq!(seen(&log).len(), 0);
    assert_eq!(text(&app), org);

    // `:eval query` asks even when the setting says never.
    set_text(
        &mut app,
        "#+begin_src python :eval query\n1\n#+end_src\n",
        1,
    );
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    pump(&mut app, "the queried block", |a| {
        text(a).contains("#+RESULTS:\n: =1\n")
    });
    // …and again on the next run.
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(seen(&log), ["1"]);
    shut_down(app, kernel);
}

#[test]
fn clear_results_removes_the_results_block() {
    let _guard = lock();
    let root = scratch("clear");
    let (info, _log, kernel) = fake_kernel();
    let mut app = app_with(&root, info, no_prompts());
    let org = "#+begin_src python\nboth\n#+end_src\nTail.\n";
    open(&mut app, &root, "c.org", org, 1);
    run_and_wait(&mut app, "jupyter.run");
    assert!(text(&app).contains("#+RESULTS:"));
    app.run_action("jupyter.clear_results");
    assert_eq!(text(&app), org);
    // Undo brings the results back in one step.
    app.run_action("edit.undo");
    assert!(text(&app).contains("#+RESULTS:\n: out\n: =both\n"));
    shut_down(app, kernel);
}

// ----- T734f: trust ------------------------------------------------------------

#[test]
fn always_in_this_workspace_is_remembered() {
    let _guard = lock();
    let root = scratch("always");
    let (info, log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    let org = "#+begin_src python\n2\n#+end_src\n";
    open(&mut app, &root, "a.org", org, 1);
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    pump(&mut app, "the trusted block", |a| text(a).contains(": =2"));
    let session = vix_session::Session::load_from(&root.join("session.toml"));
    assert!(
        session
            .workspaces
            .iter()
            .any(|w| w.jupyter_trusted == Some(true)),
        "the answer is persisted"
    );
    shut_down(app, kernel);

    // A new App for the same workspace runs a different block without asking.
    let (info, log2, kernel) = fake_kernel();
    let mut app = app_with(&root, info, Settings::default());
    open(
        &mut app,
        &root,
        "b.org",
        "#+begin_src python\n3\n#+end_src\n",
        1,
    );
    run_and_wait(&mut app, "jupyter.run");
    assert!(!app.jupyter_confirm_open());
    assert_eq!(seen(&log2), ["3"]);

    // A file outside the workspace still asks, and offers no "always".
    let outside = root.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    open(
        &mut app,
        &outside,
        "o.org",
        "#+begin_src python\n4\n#+end_src\n",
        1,
    );
    app.run_action("jupyter.run");
    assert!(app.jupyter_confirm_open());
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(app.jupyter_confirm_open(), "`a` is not offered here");
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(seen(&log2), ["3"]);
    assert_eq!(seen(&log), ["2"]);
    shut_down(app, kernel);
}

#[test]
fn confirm_evaluate_always_asks_every_run() {
    let _guard = lock();
    let root = scratch("ask");
    let (info, log, kernel) = fake_kernel();
    let mut settings = Settings::default();
    settings.jupyter_confirm_evaluate = "always".into();
    let mut app = app_with(&root, info, settings);
    open(
        &mut app,
        &root,
        "k.org",
        "#+begin_src python\n5\n#+end_src\n",
        1,
    );
    for _ in 0..2 {
        app.run_action("jupyter.run");
        assert!(app.jupyter_confirm_open());
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        let n = seen(&log).len();
        pump(&mut app, "the confirmed run", |_| seen(&log).len() > n);
    }
    // A line sent on purpose never asks.
    open(&mut app, &root, "k.py", "6\n", 0);
    run_and_wait(&mut app, "jupyter.run");
    assert!(!app.jupyter_confirm_open());
    shut_down(app, kernel);
}

#[test]
fn project_kernelspecs_need_a_trusted_workspace() {
    let _guard = lock();
    let root = scratch("project");
    let (info, _log, _kernel) = fake_kernel();
    // A kernelspec in the workspace's own `.venv`, and one in a directory
    // inside the workspace passed explicitly.
    write_kernelspec(
        &root.join(".venv").join("share").join("jupyter"),
        "venv",
        "python",
        &["vix-test-no-such-kernel"],
    );
    let inside = write_kernelspec(&root.join("tools"), "inside", "python", &["x"]);
    let mut app = app(&root, info);
    app =
        app.with_jupyter_kernel_dirs(vec![root.with_extension("kernels").join("kernels"), inside]);
    app.run_action("jupyter.start");
    assert_eq!(app.jupyter_picker_rows(), Some(1), "untrusted: only `fake`");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // Trust the workspace (T132's decision) and both project specs appear.
    let key = std::fs::canonicalize(&root)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut session = vix_session::Session::load_from(&root.join("session.toml"));
    session.set_scripts_trusted(&key, Some(true));
    session.save_to(&root.join("session.toml")).unwrap();
    app.run_action("jupyter.start");
    assert_eq!(app.jupyter_picker_rows(), Some(3));
}

// ----- T734g: images -------------------------------------------------------------

#[test]
fn an_image_opens_in_a_tab_and_a_bomb_is_refused() {
    let _guard = lock();
    let root = scratch("image");
    let (info, _log, kernel) = fake_kernel();
    let mut app = app(&root, info);
    app.picker = Some(ratatui_image::picker::Picker::halfblocks());
    open(&mut app, &root, "i.py", "bomb\nimg\n", 0);
    let tabs = app.editor.tabs.len();
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(app.editor.tabs.len(), tabs, "the bomb opened nothing");
    assert!(dock(&app).contains(&"<Image>".to_owned()));

    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(1);
    run_and_wait(&mut app, "jupyter.run");
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert!(app.editor.active_tab().unwrap().is_image());
    shut_down(app, kernel);
}

#[test]
fn an_org_image_is_saved_for_a_file_link() {
    let _guard = lock();
    let root = scratch("orgimage");
    let (info, _log, kernel) = fake_kernel();
    let mut app = app_with(&root, info, no_prompts());
    let tabs_before = app.editor.tabs.len();
    open(
        &mut app,
        &root,
        "plots.org",
        "#+begin_src python :file out/plot.png\nimg\n#+end_src\n",
        1,
    );
    run_and_wait(&mut app, "jupyter.run");
    assert!(text(&app).ends_with("#+RESULTS:\n[[file:out/plot.png]]\n"));
    let saved = std::fs::read(root.join("out").join("plot.png")).unwrap();
    assert!(saved.starts_with(b"\x89PNG"));
    // Saved for the link, not opened in a tab.
    assert_eq!(app.editor.tabs.len(), tabs_before + 1);

    // Without `:file`: `<org-file-stem>-images/<hash>.png`.
    set_text(&mut app, "#+begin_src python\nimg\n#+end_src\n", 1);
    run_and_wait(&mut app, "jupyter.run");
    let images: Vec<_> = std::fs::read_dir(root.join("plots-images"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(images.len(), 1);
    assert!(text(&app).contains(&format!("[[file:plots-images/{}]]", images[0])));

    // A `:file` outside the Org file's folder, and an oversized image, are
    // not written; the results fall back to text.
    set_text(
        &mut app,
        "#+begin_src python :file ../escape.png\nimg\n#+end_src\n",
        1,
    );
    run_and_wait(&mut app, "jupyter.run");
    assert!(!root.with_extension("png").exists());
    assert!(!root.parent().unwrap().join("escape.png").exists());
    assert!(text(&app).contains("#+RESULTS:\n: <Image>\n"));
    set_text(
        &mut app,
        "#+begin_src python :file big.png\nbomb\n#+end_src\n",
        1,
    );
    run_and_wait(&mut app, "jupyter.run");
    assert!(!root.join("big.png").exists());
    assert!(text(&app).contains("#+RESULTS:\n: <Image>\n"));
    shut_down(app, kernel);
}

/// An Org block's `#+RESULTS:` and a `:file` image against a real
/// ipykernel, when `VIX_JUPYTER_PYTHON` names a Python that has it.
#[test]
fn real_ipykernel_writes_org_results() {
    let Ok(python) = std::env::var("VIX_JUPYTER_PYTHON") else {
        return;
    };
    let _guard = lock();
    let root = scratch("ipykernel-org");
    let kernels = kernels_dir(
        &root,
        "py",
        "python",
        &[
            &python,
            "-m",
            "ipykernel_launcher",
            "-f",
            "{connection_file}",
        ],
    );
    let mut app = App::new(root.clone(), no_prompts())
        .with_session_path(root.join("session.toml"))
        .with_jupyter_kernel_dirs(vec![kernels]);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    let org = format!(
        "#+begin_src python :results output\nfor i in range(2):\n    print(i * 21)\n#+end_src\n\n\
         #+begin_src python :file real.png\nimport base64\nfrom IPython.display import Image, display\n\
         display(Image(data=base64.b64decode(\"{PNG}\")))\n#+end_src\n"
    );
    open(&mut app, &root, "real.org", &org, 1);
    app.run_action("jupyter.run");
    pump(&mut app, "the printed results", |a| {
        text(a).contains("#+RESULTS:\n: 0\n: 21\n")
    });
    let line = text(&app)
        .split('\n')
        .position(|l| l.starts_with("import base64"))
        .unwrap();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(line);
    app.run_action("jupyter.run");
    pump(&mut app, "the image link", |a| {
        text(a).contains("[[file:real.png]]")
    });
    assert!(
        std::fs::read(root.join("real.png"))
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    app.run_action("jupyter.shutdown");
    pump(&mut app, "ipykernel shutdown", |a| {
        a.jupyter_kernels().is_empty()
    });
}
