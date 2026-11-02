//! The agent subprocess: spawn, write lines to its stdin, read lines from its
//! stdout on a thread. This is the only place the crate starts a process; the
//! caller (the App) must have established that the workspace is trusted.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use serde_json::Value;

use crate::wire::{LineDecoder, encode_line};

/// A running ACP agent.
pub struct AgentProcess {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    closed: bool,
}

impl AgentProcess {
    /// Start `argv` (program then arguments, no shell) in `cwd`. Stderr is
    /// discarded.
    ///
    /// # Errors
    /// An empty `argv`, or the OS refusing to start the program.
    pub fn spawn(argv: &[String], cwd: &Path) -> io::Result<Self> {
        let (exe, rest) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty agent command"))?;
        let mut child = Command::new(exe)
            .args(rest)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no agent stdin"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no agent stdout"))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut decoder = LineDecoder::new();
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        decoder.push(&buf[..n]);
                        while let Some(v) = decoder.pop() {
                            if tx.send(v).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            closed: false,
        })
    }

    /// Write one message. A broken pipe marks the agent closed.
    ///
    /// # Errors
    /// The write to the agent's stdin failed.
    pub fn send(&mut self, value: &Value) -> io::Result<()> {
        let r = self
            .stdin
            .write_all(&encode_line(value))
            .and_then(|()| self.stdin.flush());
        if r.is_err() {
            self.closed = true;
        }
        r
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
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
