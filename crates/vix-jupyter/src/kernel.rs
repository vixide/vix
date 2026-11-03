//! The synchronous kernel handle the App holds.

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vix_jupyter_core::ansi::strip_ansi;
use vix_jupyter_core::{ConnectionError, ConnectionInfo, KernelSpec, KeyError, Limits, Transport};

use crate::event::{KernelEvent, RequestId};
use crate::runtime_dir::{RuntimeDir, random_hex};
use crate::worker::{Command, Launched, Worker};

/// How much of a kernel's stderr is kept for [`crate::DeathReason::Exited`].
const STDERR_TAIL_BYTES: usize = 4096;

/// How to launch or connect to a kernel.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// Where the private runtime directory is created (default: the OS
    /// temporary directory).
    pub runtime_base: PathBuf,
    /// The kernel's working directory (default: Vix's).
    pub cwd: Option<PathBuf>,
    /// IPC (the Unix default) or loopback TCP (the Windows default).
    pub transport: Transport,
    /// How long the kernel has to answer `kernel_info` (default 30 s).
    pub start_timeout: Duration,
    /// How long a kernel has to exit after `shutdown_request` before it is
    /// killed (default 2 s).
    pub shutdown_grace: Duration,
    /// The `username` in message headers.
    pub username: String,
    /// Size caps for received messages.
    pub limits: Limits,
}

impl Default for LaunchOptions {
    /// Defaults: private runtime directory under the OS temp dir, Vix's own working directory, IPC sockets on Unix and loopback TCP elsewhere, a 30 s start timeout, a 2 s shutdown grace, and the username `vix`.
    fn default() -> Self {
        Self {
            runtime_base: std::env::temp_dir(),
            cwd: None,
            transport: if cfg!(unix) {
                Transport::Ipc
            } else {
                Transport::Tcp
            },
            start_timeout: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(2),
            username: "vix".to_owned(),
            limits: Limits::default(),
        }
    }
}

/// Why a kernel could not be started.
#[derive(Debug)]
#[non_exhaustive]
pub enum LaunchError {
    /// The runtime directory or connection file could not be written.
    Io(io::Error),
    /// The kernel command could not be run (for example, not installed).
    Spawn(io::Error),
    /// The worker thread or its runtime could not be started.
    Runtime(io::Error),
    /// The connection is unusable.
    Connection(ConnectionError),
    /// The connection points at another machine: v1 refuses remote kernels.
    NotLocal,
}

impl fmt::Display for LaunchError {
    /// Renders a sentence describing which launch step failed, including the underlying I/O error where there is one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot prepare the kernel's runtime directory: {e}"),
            Self::Spawn(e) => write!(f, "cannot start the kernel: {e}"),
            Self::Runtime(e) => write!(f, "cannot start the kernel client: {e}"),
            Self::Connection(e) => write!(f, "{e}"),
            Self::NotLocal => f.write_str("the kernel is not on this machine"),
        }
    }
}

impl std::error::Error for LaunchError {}

impl From<KeyError> for LaunchError {
    /// An empty signing key is the only [`KeyError`] case, and is reported as [`ConnectionError::EmptyKey`] so callers see one error type.
    fn from(_: KeyError) -> Self {
        Self::Connection(ConnectionError::EmptyKey)
    }
}

/// The last bytes a kernel wrote to stderr.
#[derive(Debug, Clone, Default)]
pub(crate) struct StderrTail(Arc<Mutex<Vec<u8>>>);

impl StderrTail {
    /// Append `chunk`, keeping only the last `STDERR_TAIL_BYTES` bytes. A poisoned lock is ignored: losing diagnostics must not take the reader thread down.
    fn push(&self, chunk: &[u8]) {
        if let Ok(mut buf) = self.0.lock() {
            buf.extend_from_slice(chunk);
            let excess = buf.len().saturating_sub(STDERR_TAIL_BYTES);
            buf.drain(..excess);
        }
    }

    /// The tail as text, ANSI codes removed.
    pub(crate) fn text(&self) -> String {
        self.0
            .lock()
            .map(|buf| strip_ansi(&String::from_utf8_lossy(&buf)))
            .unwrap_or_default()
    }
}

/// Run a kernelspec's command with `connection_file`. Its stdin and stdout
/// are closed; its stderr tail is kept. On Unix it gets its own process
/// group, so a terminal's Ctrl+C never reaches it.
pub(crate) fn spawn_kernel(
    spec: &KernelSpec,
    connection_file: &Path,
    cwd: Option<&Path>,
) -> io::Result<(Child, StderrTail)> {
    let argv = spec.argv_for(connection_file);
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty kernel argv"))?;
    let mut cmd = Process::new(program);
    cmd.args(rest)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let tail = StderrTail::default();
    if let Some(mut stderr) = child.stderr.take() {
        let sink = tail.clone();
        // Best effort: if the reader thread cannot start, stderr is dropped
        // (the pipe closes and the kernel's writes fail quietly).
        let _ = thread::Builder::new()
            .name("vix-jupyter-stderr".into())
            .spawn(move || {
                let mut buf = [0u8; 1024];
                while let Ok(n) = stderr.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    sink.push(&buf[..n]);
                }
            });
    }
    Ok((child, tail))
}

/// A running kernel. Requests are sent without blocking; results arrive as
/// [`KernelEvent`]s on [`events`](Self::events). Dropping the handle kills
/// the kernel (if Vix launched it) and removes its runtime directory.
pub struct KernelHandle {
    /// Requests to the worker task. Sends never block, and fail silently once the worker has finished.
    commands: UnboundedSender<Command>,
    /// Everything the worker reports, polled by the App.
    events: Receiver<KernelEvent>,
    /// The random session id placed in every message header (also the prefix of request ids).
    session: String,
    /// Counter for request ids, so [`KernelHandle::execute`] can return an id without waiting for the worker.
    next_id: AtomicU64,
    /// The worker thread; taken when joined on drop.
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for KernelHandle {
    /// Prints only the session id; the channels and join handle carry nothing useful to log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KernelHandle")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl KernelHandle {
    /// Launch `spec` with a fresh connection: a new `0700` runtime
    /// directory, a `0600` connection file and a random key.
    ///
    /// # Errors
    ///
    /// A [`LaunchError`] when the directory, the process or the client
    /// thread cannot be created. A kernel that starts and then fails is
    /// reported as [`KernelEvent::Dead`] instead.
    pub fn launch(spec: &KernelSpec, options: &LaunchOptions) -> Result<Self, LaunchError> {
        let dir = RuntimeDir::create(&options.runtime_base).map_err(LaunchError::Io)?;
        let info = dir
            .new_connection(options.transport, &spec.name)
            .map_err(LaunchError::Io)?;
        let file = dir.write_connection(&info).map_err(LaunchError::Io)?;
        let (child, stderr) =
            spawn_kernel(spec, &file, options.cwd.as_deref()).map_err(LaunchError::Spawn)?;
        let launched = Launched {
            child,
            stderr,
            spec: spec.clone(),
            cwd: options.cwd.clone(),
            dir,
        };
        Self::start(info, Some(launched), options)
    }

    /// Connect to a kernel someone else started, from its connection file.
    /// Vix does not own its process: shutdown sends `shutdown_request` only.
    ///
    /// # Errors
    ///
    /// [`LaunchError::NotLocal`] for a TCP address other than loopback, a
    /// [`LaunchError::Connection`] for an empty key, or
    /// [`LaunchError::Runtime`].
    pub fn connect(info: ConnectionInfo, options: &LaunchOptions) -> Result<Self, LaunchError> {
        if !info.is_local() {
            return Err(LaunchError::NotLocal);
        }
        Self::start(info, None, options)
    }

    /// Shared tail of [`KernelHandle::launch`] and [`KernelHandle::connect`]: derive the signer from the key, mint a random session id, then run the async worker on a dedicated thread with its own single-threaded Tokio runtime, so the App's UI thread never touches an async runtime.
    fn start(
        info: ConnectionInfo,
        launched: Option<Launched>,
        options: &LaunchOptions,
    ) -> Result<Self, LaunchError> {
        let signer = info.signer()?;
        let session = random_hex(16).map_err(LaunchError::Runtime)?;
        let (commands, command_rx) = unbounded_channel();
        let (event_tx, events) = mpsc::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(LaunchError::Runtime)?;
        let worker = Worker::new(info, signer, session.clone(), options, launched, event_tx);
        let thread = thread::Builder::new()
            .name("vix-jupyter".into())
            .spawn(move || runtime.block_on(worker.run(command_rx)))
            .map_err(LaunchError::Runtime)?;
        Ok(Self {
            commands,
            events,
            session,
            next_id: AtomicU64::new(1),
            thread: Some(thread),
        })
    }

    /// Run `code`. Returns the request's id at once; its outputs and reply
    /// follow as events. Sends `allow_stdin: false`.
    pub fn execute(&self, code: &str) -> RequestId {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}_{n}", self.session);
        let _ = self.commands.send(Command::Execute {
            id: id.clone(),
            code: code.to_owned(),
        });
        RequestId(id)
    }

    /// Interrupt the running request: SIGINT, or an `interrupt_request` on
    /// the control channel when the kernelspec asks for `message` (and
    /// always on Windows or for a kernel Vix did not launch).
    pub fn interrupt(&self) {
        let _ = self.commands.send(Command::Interrupt);
    }

    /// Restart the kernel (a launched kernel is stopped and started again
    /// on the same connection). [`KernelEvent::Restarted`] and then
    /// [`KernelEvent::Ready`] follow.
    pub fn restart(&self) {
        let _ = self.commands.send(Command::Restart);
    }

    /// Shut the kernel down: `shutdown_request` on control, then kill it
    /// after the grace period. [`KernelEvent::Dead`] follows.
    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }

    /// The event channel, to poll with `try_recv` on the App tick.
    #[must_use]
    pub fn events(&self) -> &Receiver<KernelEvent> {
        &self.events
    }
}

impl Drop for KernelHandle {
    /// Dropping the handle kills the kernel (when Vix launched it) and joins the worker thread, so no process or thread outlives its owner.
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Kill);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
