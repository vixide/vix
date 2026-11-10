//! The helper process (feature `net`): find the `vix-matrix` program, start
//! it with piped stdin and stdout, and pump both on background threads so
//! the App only ever touches channels and never blocks on the helper.
//!
//! The helper gets no arguments and no extra environment: the password goes
//! over stdin inside [`Request::Login`], never into argv or the environment
//! where other processes could read it. Its stderr is dropped, or appended
//! to a log file when [`SpawnOptions::stderr_log`] is set. Nothing here logs.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::codec::{MAX_LINE, decode_reply, encode_request};
use crate::protocol::{ErrorKey, Reply, Request};

/// The helper's program name, without the platform's executable suffix.
pub const HELPER: &str = "vix-matrix";

/// How long [`Handle::close`] waits for the helper to say
/// [`Reply::Closed`] and exit before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// Where the helper is: `setting` when given (the `im.matrix_helper`
/// setting; `None` when that file does not exist), else `vix-matrix` beside
/// `beside` (the running `vix` executable), else the first `vix-matrix` on
/// `path` (the `PATH` value).
#[must_use]
pub fn find_helper(
    setting: Option<&Path>,
    beside: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(p) = setting {
        return p.is_file().then(|| p.to_path_buf());
    }
    let name = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
    let near = beside
        .and_then(Path::parent)
        .map(|dir| dir.join(&name))
        .filter(|p| p.is_file());
    near.or_else(|| {
        path.into_iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join(&name))
            .find(|p| p.is_file())
    })
}

/// How to start the helper.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SpawnOptions {
    /// The program, usually from [`find_helper`]; `None` when it was not
    /// found, which ends the [`Handle`] at once with
    /// [`ErrorKey::HelperMissing`].
    pub program: Option<PathBuf>,
    /// A file the helper's stderr is appended to; `None` drops it.
    pub stderr_log: Option<PathBuf>,
}

impl SpawnOptions {
    /// Start `program`, dropping its stderr.
    #[must_use]
    pub fn new(program: Option<PathBuf>) -> SpawnOptions {
        SpawnOptions {
            program,
            stderr_log: None,
        }
    }
}

/// The far end of [`Handle::pair`]: what a test sees in place of the helper.
#[derive(Debug)]
pub struct Peer {
    /// Requests the App sent.
    requests: Receiver<Request>,
    /// Replies to the App.
    replies: Sender<Reply>,
    /// Set when this end is dropped, so [`Handle::is_finished`] turns true.
    done: Arc<AtomicBool>,
}

impl Peer {
    /// The next request the App sent, if any (never blocks).
    #[must_use]
    pub fn try_recv(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }

    /// Every request sent so far.
    #[must_use]
    pub fn drain(&self) -> Vec<Request> {
        self.requests.try_iter().collect()
    }

    /// Send a reply as the helper would. `false` once the handle is gone.
    #[must_use = "false means the handle is gone"]
    pub fn reply(&self, reply: Reply) -> bool {
        self.replies.send(reply).is_ok()
    }
}

impl Drop for Peer {
    /// A dropped peer is a helper that exited.
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

/// The App's side of one helper process.
#[derive(Debug)]
pub struct Handle {
    /// Requests to the stdin writer thread; `None` once closing.
    tx: Option<Sender<Request>>,
    /// Replies from the stdout reader thread.
    rx: Receiver<Reply>,
    /// Set by the reader thread when the helper's stdout ends.
    done: Arc<AtomicBool>,
    /// The helper process, when there is one.
    child: Option<Child>,
    /// The two pump threads, joined on close.
    threads: Vec<JoinHandle<()>>,
}

impl Handle {
    /// Start the helper. Always returns a handle: when the program is
    /// missing or will not start, its replies are one
    /// [`Reply::Error`] ([`ErrorKey::HelperMissing`] or
    /// [`ErrorKey::HelperExited`]) and [`Reply::Closed`].
    #[must_use]
    pub fn spawn(options: &SpawnOptions) -> Handle {
        let (reply_tx, rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let ended = |key: ErrorKey, detail: Option<String>| {
            let _ = reply_tx.send(Reply::Error { key, detail });
            let _ = reply_tx.send(Reply::Closed);
            done.store(true, Ordering::SeqCst);
        };
        let Some(program) = &options.program else {
            ended(ErrorKey::HelperMissing, None);
            return Handle::ended(rx, done);
        };
        let stderr = options
            .stderr_log
            .as_ref()
            .and_then(|p| OpenOptions::new().create(true).append(true).open(p).ok())
            .map_or_else(Stdio::null, Stdio::from);
        let spawned = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                ended(ErrorKey::HelperExited, Some(e.kind().to_string()));
                return Handle::ended(rx, done);
            }
        };
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            ended(ErrorKey::HelperExited, None);
            return Handle::ended(rx, done);
        };
        let (tx, requests) = mpsc::channel::<Request>();
        let writer = thread::Builder::new()
            .name("vix-matrix-protocol-in".to_string())
            .spawn(move || {
                for request in requests {
                    let mut line = encode_request(&request);
                    line.push('\n');
                    if stdin
                        .write_all(line.as_bytes())
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
                // Dropping stdin here closes the pipe: the helper reads EOF.
            });
        let flag = Arc::clone(&done);
        let reader = thread::Builder::new()
            .name("vix-matrix-protocol-out".to_string())
            .spawn(move || pump_replies(stdout, &reply_tx, &flag));
        let threads = [writer, reader]
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        Handle {
            tx: Some(tx),
            rx,
            done,
            child: Some(child),
            threads,
        }
    }

    /// A handle with no process behind it, whose replies were already sent.
    fn ended(rx: Receiver<Reply>, done: Arc<AtomicBool>) -> Handle {
        Handle {
            tx: None,
            rx,
            done,
            child: None,
            threads: Vec::new(),
        }
    }

    /// A handle wired to an in-memory [`Peer`] instead of a process, for
    /// tests of the App's side of the bridge.
    #[must_use]
    pub fn pair() -> (Handle, Peer) {
        let (tx, requests) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        let done = Arc::new(AtomicBool::new(false));
        let peer = Peer {
            requests,
            replies,
            done: Arc::clone(&done),
        };
        let handle = Handle {
            tx: Some(tx),
            rx,
            done,
            child: None,
            threads: Vec::new(),
        };
        (handle, peer)
    }

    /// Send a request. `false` when the helper is gone.
    #[must_use = "false means the helper has ended"]
    pub fn send(&self, request: Request) -> bool {
        !self.is_finished() && self.tx.as_ref().is_some_and(|tx| tx.send(request).is_ok())
    }

    /// The next reply, if one has arrived (never blocks).
    #[must_use]
    pub fn try_recv(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }

    /// Whether the helper has ended. Replies it sent before ending are
    /// still waiting in [`Handle::try_recv`].
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    /// Ask the helper to quit, wait briefly for it, kill it if it hangs,
    /// and return the replies not yet received (ending with
    /// [`Reply::Closed`]).
    #[must_use]
    pub fn close(mut self) -> Vec<Reply> {
        self.shut_down();
        self.rx.try_iter().collect()
    }

    /// Send `quit`, close stdin, wait up to [`CLOSE_GRACE`], then kill.
    /// Safe to call twice.
    fn shut_down(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Request::Quit);
        }
        let deadline = Instant::now() + CLOSE_GRACE;
        while self.child.is_some() && !self.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if let Some(mut child) = self.child.take() {
            if !self.is_finished() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for Handle {
    /// Dropping a handle closes it, so no helper outlives its handle.
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Read reply lines until the helper's stdout ends. A line that does not
/// decode becomes an [`ErrorKey::Protocol`] error; an overlong line is
/// skipped with one. When stdout ends without [`Reply::Closed`], an
/// [`ErrorKey::HelperExited`] error and a `Closed` are added.
fn pump_replies(stdout: ChildStdout, out: &Sender<Reply>, done: &AtomicBool) {
    let mut reader = BufReader::new(stdout);
    let mut closed = false;
    let mut buf = Vec::new();
    let limit = u64::try_from(MAX_LINE).unwrap_or(u64::MAX) + 1;
    loop {
        buf.clear();
        match (&mut reader).take(limit).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') && buf.len() > MAX_LINE {
            let more = skip_line(&mut reader);
            let _ = out.send(Reply::error_with(ErrorKey::Protocol, "line too long"));
            if more {
                continue;
            }
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        if line.trim().is_empty() {
            continue;
        }
        let reply = match decode_reply(&line) {
            Ok(reply) => reply,
            Err(e) => Reply::error_with(ErrorKey::Protocol, e.to_string()),
        };
        closed |= reply == Reply::Closed;
        if out.send(reply).is_err() {
            break;
        }
    }
    if !closed {
        let _ = out.send(Reply::error(ErrorKey::HelperExited));
        let _ = out.send(Reply::Closed);
    }
    done.store(true, Ordering::SeqCst);
}

/// Discard the rest of an overlong line. `false` when the stream ended.
fn skip_line(reader: &mut BufReader<ChildStdout>) -> bool {
    loop {
        let (found, used) = match reader.fill_buf() {
            Ok([]) | Err(_) => return false,
            Ok(bytes) => match bytes.iter().position(|b| *b == b'\n') {
                Some(at) => (true, at + 1),
                None => (false, bytes.len()),
            },
        };
        reader.consume(used);
        if found {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_helper_ends_at_once() {
        let handle = Handle::spawn(&SpawnOptions::new(None));
        assert!(handle.is_finished());
        assert!(!handle.send(Request::Rooms));
        assert_eq!(
            handle.close(),
            vec![Reply::error(ErrorKey::HelperMissing), Reply::Closed]
        );
    }

    #[test]
    fn a_program_that_will_not_start_ends_with_helper_exited() {
        let missing = std::env::temp_dir()
            .join("no-such-dir-vix")
            .join("vix-matrix-test");
        let handle = Handle::spawn(&SpawnOptions::new(Some(missing)));
        let replies = handle.close();
        assert!(matches!(
            replies.first(),
            Some(Reply::Error {
                key: ErrorKey::HelperExited,
                ..
            })
        ));
        assert_eq!(replies.last(), Some(&Reply::Closed));
    }

    #[test]
    fn a_pair_carries_both_ways_and_ends_when_the_peer_drops() {
        let (handle, peer) = Handle::pair();
        assert!(handle.send(Request::Rooms));
        assert_eq!(peer.drain(), vec![Request::Rooms]);
        assert!(peer.reply(Reply::Closed));
        assert_eq!(handle.try_recv(), Some(Reply::Closed));
        assert!(!handle.is_finished());
        drop(peer);
        assert!(handle.is_finished());
        assert!(!handle.send(Request::Rooms));
    }

    #[test]
    fn the_setting_wins_and_must_exist() {
        let dir = std::env::temp_dir();
        assert_eq!(
            find_helper(Some(&dir.join("no-such-vix-matrix")), None, None),
            None
        );
        let exe = std::env::current_exe().ok();
        assert_eq!(find_helper(exe.as_deref(), None, None), exe);
    }

    #[test]
    fn the_helper_is_found_beside_vix_then_on_path() {
        let base =
            std::env::temp_dir().join(format!("vix-matrix-protocol-find-{}", std::process::id()));
        let (near, far) = (base.join("bin"), base.join("path"));
        let name = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
        for d in [&near, &far] {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(far.join(&name), b"");
        let path = std::env::join_paths([&near, &far]).ok();
        let vix = near.join("vix");
        assert_eq!(
            find_helper(None, Some(&vix), path.as_deref()),
            Some(far.join(&name))
        );
        let _ = std::fs::write(near.join(&name), b"");
        assert_eq!(
            find_helper(None, Some(&vix), path.as_deref()),
            Some(near.join(&name))
        );
        assert_eq!(find_helper(None, None, None), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
