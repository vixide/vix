//! The agent subprocess: spawn, write lines to its stdin, read lines from its
//! stdout on a thread. This is the only place the crate starts a process; the
//! caller (the App) must have established that the workspace is trusted.
//!
//! Compiled only with the `process` feature so the protocol layers
//! ([`crate::wire`], [`crate::client`], [`crate::policy`]) stay IO-free and
//! unit-testable. Reading happens on a helper thread feeding an `mpsc` channel
//! so the UI loop can poll with [`AgentProcess::drain`] without ever blocking.
//! The last [`STDERR_TAIL_BYTES`] of the agent's stderr are kept by another
//! thread for error messages ([`AgentProcess::stderr_tail`]).
//!
//! Owning spec: `crates/vix-acp/spec/index.md`.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::wire::{LineDecoder, encode_line};

/// How much of the agent's stderr is kept: the newest bytes, at most this many.
pub const STDERR_TAIL_BYTES: usize = 8 * 1024;

/// Where [`AgentProcess::send`] writes.
enum Writer {
    /// Straight to the pipe, on the caller's thread.
    Direct(ChildStdin),
    /// To a writer thread, so a full pipe never blocks the caller. The flag
    /// turns true once a write failed (the agent stopped reading).
    Thread(Sender<Vec<u8>>, Arc<AtomicBool>),
    /// Stdin was closed with [`AgentProcess::close_stdin`].
    Closed,
}

/// A running ACP agent. Dropping it kills and reaps the subprocess.
pub struct AgentProcess {
    /// The agent subprocess handle, kept to kill and reap it on drop.
    child: Child,
    /// Where newline-delimited requests go.
    stdin: Writer,
    /// Decoded messages from the reader thread.
    rx: Receiver<Value>,
    /// Set once the pipe broke or the reader thread ended (agent exited).
    closed: bool,
    /// The newest bytes of the agent's stderr.
    stderr: Arc<Mutex<Vec<u8>>>,
    /// Whether the agent leads its own process group, killed as a whole.
    group: bool,
}

impl AgentProcess {
    /// Start `argv` (program then arguments, no shell) in `cwd`. Writes
    /// happen on the caller's thread. Stderr is read on a thread; its tail
    /// is kept.
    ///
    /// # Errors
    /// An empty `argv`, or the OS refusing to start the program.
    pub fn spawn(argv: &[String], cwd: &Path) -> io::Result<Self> {
        Self::start(argv, cwd, false)
    }

    /// Like [`Self::spawn`], but writes go through a writer thread, so
    /// [`Self::send`] never blocks on a full pipe, and on Unix the agent
    /// leads a new process group: dropping it kills the whole group, so the
    /// tool processes it started do not outlive it.
    ///
    /// # Errors
    /// An empty `argv`, or the OS refusing to start the program.
    pub fn spawn_isolated(argv: &[String], cwd: &Path) -> io::Result<Self> {
        Self::start(argv, cwd, true)
    }

    fn start(argv: &[String], cwd: &Path, isolated: bool) -> io::Result<Self> {
        let (exe, rest) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty agent command"))?;
        let mut command = Command::new(exe);
        command
            .args(rest)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        if isolated {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no agent stdin"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no agent stdout"))?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(pipe) = child.stderr.take() {
            keep_tail(pipe, Arc::clone(&stderr));
        }
        let (tx, rx) = mpsc::channel();
        // Detached reader: it ends on EOF/read error (agent exited) or when the
        // receiver is dropped (`send` fails), so no join handle is needed.
        std::thread::spawn(move || {
            let mut decoder = LineDecoder::new();
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        #[expect(
                            clippy::indexing_slicing,
                            reason = "RCA-EXC-2: read returns at most the buffer length, so n is at most buf.len()"
                        )]
                        let received = &buf[..n];
                        decoder.push(received);
                        while let Some(v) = decoder.pop() {
                            if tx.send(v).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        let stdin = if isolated {
            write_on_thread(stdin)
        } else {
            Writer::Direct(stdin)
        };
        Ok(Self {
            child,
            stdin,
            rx,
            closed: false,
            stderr,
            group: isolated,
        })
    }

    /// Write one message. A broken pipe marks the agent closed. With
    /// [`Self::spawn_isolated`] the write is queued, and a failed earlier
    /// write is reported by the next call.
    ///
    /// # Errors
    /// The write to the agent's stdin failed, or stdin was closed.
    pub fn send(&mut self, value: &Value) -> io::Result<()> {
        let r = match &mut self.stdin {
            Writer::Direct(stdin) => stdin
                .write_all(&encode_line(value))
                .and_then(|()| stdin.flush()),
            Writer::Thread(tx, failed) => {
                if failed.load(Ordering::Acquire) {
                    Err(io::ErrorKind::BrokenPipe.into())
                } else {
                    tx.send(encode_line(value))
                        .map_err(|_| io::ErrorKind::BrokenPipe.into())
                }
            }
            Writer::Closed => Err(io::ErrorKind::BrokenPipe.into()),
        };
        if r.is_err() {
            self.closed = true;
        }
        r
    }

    /// Close the agent's stdin (after any queued writes), telling it no more
    /// input comes. Later [`Self::send`] calls fail.
    pub fn close_stdin(&mut self) {
        self.stdin = Writer::Closed;
    }

    /// Every message that has arrived, without blocking. Once the agent's
    /// stdout closes, [`Self::is_closed`] turns true.
    pub fn drain(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(v) => out.push(v),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
        out
    }

    /// Whether the agent exited or its pipe broke.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The agent's exit status, if it has exited, without blocking.
    ///
    /// # Errors
    /// The OS could not report on the process.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// The newest (at most [`STDERR_TAIL_BYTES`]) bytes of the agent's
    /// stderr so far, decoded lossily. Not sanitized: pass it through
    /// [`crate::display::sanitize`] before showing it.
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .map_or_default(|buf| String::from_utf8_lossy(&buf).into_owned())
    }
}

/// Keep the newest [`STDERR_TAIL_BYTES`] of `pipe` in `sink`, on a detached
/// thread that ends when the pipe closes. The pipe is always read, so the
/// agent never blocks on a full stderr.
fn keep_tail(mut pipe: impl Read + Send + 'static, sink: Arc<Mutex<Vec<u8>>>) {
    // Best effort: if the thread cannot start, the pipe is dropped and the
    // agent's stderr writes fail quietly.
    let _ = std::thread::Builder::new()
        .name("vix-acp-stderr".into())
        .spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                #[expect(
                    clippy::indexing_slicing,
                    reason = "RCA-EXC-2: read returns at most the buffer length, so n is at most buf.len()"
                )]
                let received = &buf[..n];
                if let Ok(mut tail) = sink.lock() {
                    tail.extend_from_slice(received);
                    // Below the cap there is nothing to drop.
                    if let Some(excess) = tail.len().checked_sub(STDERR_TAIL_BYTES) {
                        tail.drain(..excess);
                    }
                }
            }
        });
}

/// Move `stdin` to a writer thread. It ends, closing the pipe, when every
/// sender is dropped or a write fails.
fn write_on_thread(mut stdin: ChildStdin) -> Writer {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let failed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&failed);
    let started = std::thread::Builder::new()
        .name("vix-acp-writer".into())
        .spawn(move || {
            for line in rx {
                if stdin.write_all(&line).and_then(|()| stdin.flush()).is_err() {
                    flag.store(true, Ordering::Release);
                    return;
                }
            }
        });
    if started.is_err() {
        failed.store(true, Ordering::Release);
    }
    Writer::Thread(tx, failed)
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.group
            && let Ok(pid) = i32::try_from(self.child.id())
        {
            // Every process still in the agent's group, tools included.
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        // Best effort: the child may already have exited. `wait` reaps it so
        // no zombie is left behind.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
