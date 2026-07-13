//! Integrated terminal: a real PTY hosting a shell, parsed by `vt100`.
//!
//! [`Terminal::open`] spawns the user's shell on a pseudo-terminal
//! ([`portable_pty`], so it works on Unix and Windows `ConPTY`). A reader thread
//! feeds the shell's output into a shared `vt100::Parser`, which maintains the
//! screen grid the UI renders. Key events are encoded to terminal byte sequences
//! by [`encode_key`] and written back to the PTY.
//!
//! Only key encoding is unit-tested here; the live PTY is exercised manually.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A running integrated terminal: the PTY master, the child shell, and the shared
/// `vt100` parser holding the screen state.
pub struct Terminal {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    alive: Arc<AtomicBool>,
    rows: u16,
    cols: u16,
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // `portable_pty::Child` does not kill the shell on drop, and the reader
        // thread holds a cloned master fd, so without this the shell process and
        // its reader thread would leak on every terminal close/reopen. Killing
        // and reaping the child closes the PTY slave, so the reader sees EOF and
        // exits.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Terminal {
    /// Spawn `shell` on a new PTY of `rows`×`cols`, rooted at `cwd`.
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the shell cannot spawn.
    pub fn open(
        shell: &str,
        cwd: &std::path::Path,
        rows: u16,
        cols: u16,
    ) -> std::io::Result<Terminal> {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let pty = portable_pty::native_pty_system();
        let size = portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = pty.openpty(size).map_err(|e| to_io(&e))?;
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).map_err(|e| to_io(&e))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| to_io(&e))?;
        let writer = pair.master.take_writer().map_err(|e| to_io(&e))?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let alive = Arc::new(AtomicBool::new(true));
        let reader_parser = Arc::clone(&parser);
        let reader_alive = Arc::clone(&alive);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut p) = reader_parser.lock() {
                            p.process(&buf[..n]);
                        }
                    }
                }
            }
            reader_alive.store(false, Ordering::SeqCst);
        });

        Ok(Terminal {
            parser,
            writer,
            master: pair.master,
            child,
            alive,
            rows,
            cols,
        })
    }

    /// Whether the shell is still running (the reader thread sets this false on EOF).
    #[must_use]
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Current grid size (rows, cols).
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Lock the parser to read its screen for rendering.
    pub fn lock(&self) -> MutexGuard<'_, vt100::Parser> {
        self.parser
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Forward a key event to the shell (no-op encoding does nothing).
    pub fn send_key(&mut self, key: KeyEvent) {
        let bytes = encode_key(key);
        if !bytes.is_empty() {
            let _ = self.writer.write_all(&bytes);
            let _ = self.writer.flush();
        }
    }

    /// Resize the PTY and parser to `rows`×`cols` if it changed.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if (rows, cols) == (self.rows, self.cols) {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let size = portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let _ = self.master.resize(size);
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
    }
}

/// Map a `portable_pty` error into an [`std::io::Error`].
fn to_io(e: &anyhow::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Encode a crossterm key event into the byte sequence a terminal expects.
#[must_use]
pub fn encode_key(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let mut out: Vec<u8> = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                // Control combos collapse to the low control byte (Ctrl-A = 0x01).
                let b = (c.to_ascii_uppercase() as u8).wrapping_sub(b'@') & 0x7f;
                vec![b]
            } else {
                let mut s = [0u8; 4];
                c.encode_utf8(&mut s).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => function_key(n),
        _ => Vec::new(),
    };
    // Alt prefixes the sequence with ESC, the usual "meta" convention.
    if alt && !out.is_empty() && !matches!(key.code, KeyCode::Esc) {
        let mut prefixed = vec![0x1b];
        prefixed.append(&mut out);
        return prefixed;
    }
    out
}

/// Byte sequence for function key `n` (F1–F12); empty beyond F12.
fn function_key(n: u8) -> Vec<u8> {
    match n {
        1 => b"\x1bOP".to_vec(),
        2 => b"\x1bOQ".to_vec(),
        3 => b"\x1bOR".to_vec(),
        4 => b"\x1bOS".to_vec(),
        5 => b"\x1b[15~".to_vec(),
        6 => b"\x1b[17~".to_vec(),
        7 => b"\x1b[18~".to_vec(),
        8 => b"\x1b[19~".to_vec(),
        9 => b"\x1b[20~".to_vec(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn plain_char_is_utf8() {
        assert_eq!(encode_key(k(KeyCode::Char('a'), KeyModifiers::NONE)), b"a");
        assert_eq!(
            encode_key(k(KeyCode::Char('é'), KeyModifiers::NONE)),
            "é".as_bytes()
        );
    }

    #[test]
    fn ctrl_char_is_control_byte() {
        assert_eq!(
            encode_key(k(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            vec![0x03]
        );
        assert_eq!(
            encode_key(k(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            vec![0x01]
        );
    }

    #[test]
    fn alt_char_is_esc_prefixed() {
        assert_eq!(
            encode_key(k(KeyCode::Char('x'), KeyModifiers::ALT)),
            vec![0x1b, b'x']
        );
    }

    #[test]
    fn special_keys() {
        assert_eq!(encode_key(k(KeyCode::Enter, KeyModifiers::NONE)), b"\r");
        assert_eq!(encode_key(k(KeyCode::Up, KeyModifiers::NONE)), b"\x1b[A");
        assert_eq!(
            encode_key(k(KeyCode::Backspace, KeyModifiers::NONE)),
            vec![0x7f]
        );
    }

    #[test]
    #[cfg(unix)]
    fn dropping_the_terminal_kills_the_shell_and_reader() {
        use std::time::{Duration, Instant};
        let term = Terminal::open("/bin/sh", std::path::Path::new("/"), 24, 80)
            .expect("spawn a shell");
        // Observe the reader thread's liveness flag past the drop.
        let alive = Arc::clone(&term.alive);
        assert!(alive.load(Ordering::SeqCst), "reader should start alive");
        drop(term); // Drop kills+reaps the child → PTY EOF → reader exits.
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !alive.load(Ordering::SeqCst),
            "reader thread never saw EOF — the shell leaked"
        );
    }
}
