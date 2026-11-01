//! Integrated terminal: a real PTY hosting a shell, parsed by `vt100`.
//!
//! [`Terminal::open`] spawns the user's shell on a pseudo-terminal
//! ([`portable_pty`], so it works on Unix and Windows `ConPTY`). A reader thread
//! feeds the shell's output into a shared `vt100::Parser`, which maintains the
//! screen grid the UI renders. Key events are encoded to terminal byte sequences
//! by [`encode_key`] and written back to the PTY.
//!
//! The parser keeps a bounded scrollback (at most [`MAX_SCROLLBACK`] lines),
//! which [`Terminal::scroll_by`] pages through; a [`Selection`] copies text
//! out of the visible view; [`encode_mouse`] forwards the mouse when the child
//! asked for it; and [`Tabs`] holds several terminals at once (T631).
//!
//! The pure parts (key and mouse encoding, scrolling a bare parser, selection
//! text, the tab set) are unit-tested here without a PTY.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

mod tabs;
pub use tabs::Tabs;

/// Default scrollback, in lines, of a new terminal (the `terminal_scrollback`
/// setting's default).
pub const DEFAULT_SCROLLBACK: usize = 2000;

/// The most scrollback lines a terminal keeps, whatever the setting says. A
/// `vt100` cell is about 36 bytes, so a full 200-column scrollback at this cap
/// is roughly 360 MB; the default is a twenty-fifth of that.
pub const MAX_SCROLLBACK: usize = 50_000;

/// The scrollback a terminal keeps for a configured `lines`: at most
/// [`MAX_SCROLLBACK`] (0 turns scrollback off).
#[must_use]
pub fn clamp_scrollback(lines: usize) -> usize {
    lines.min(MAX_SCROLLBACK)
}

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
    selection: Option<Selection>,
    /// What was spawned, so [`Terminal::restart`] can run it again (T666).
    spawn: SpawnSpec,
    /// The shell's working directory as last announced by OSC 7 (T666).
    osc_cwd: Arc<Mutex<Option<std::path::PathBuf>>>,
    /// Whether editor chords go to the shell (`true`, the default) or stay
    /// with the editor (T666).
    capture: bool,
    /// Whether the shell's exit has been reported (see
    /// [`Terminal::take_exit_notice`]).
    exit_noted: bool,
}

/// The command a [`Terminal`] runs, kept for [`Terminal::restart`].
#[derive(Debug, Clone)]
struct SpawnSpec {
    program: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
    cwd: std::path::PathBuf,
}

/// The live half of a terminal: the PTY and its child.
struct Pty {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    alive: Arc<AtomicBool>,
}

/// Open a PTY of `rows`x`cols` running `spec`, with a reader thread feeding
/// `parser` and tracking OSC 7 into `osc_cwd`.
fn spawn_pty(
    spec: &SpawnSpec,
    rows: u16,
    cols: u16,
    parser: &Arc<Mutex<vt100::Parser>>,
    osc_cwd: &Arc<Mutex<Option<std::path::PathBuf>>>,
) -> std::io::Result<Pty> {
    let pty = portable_pty::native_pty_system();
    let size = portable_pty::PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = pty.openpty(size).map_err(|e| to_io(&e))?;
    let mut cmd = portable_pty::CommandBuilder::new(&spec.program);
    cmd.args(&spec.args);
    cmd.cwd(&spec.cwd);
    // The workspace's activated environment (venv/direnv/mise; T667).
    vix_project_env::apply_to(|key, value| match value {
        Some(value) => cmd.env(key, value),
        None => cmd.env_remove(key),
    });
    cmd.env("TERM", "xterm-256color");
    let child = pair.slave.spawn_command(cmd).map_err(|e| to_io(&e))?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| to_io(&e))?;
    let writer = pair.master.take_writer().map_err(|e| to_io(&e))?;
    let alive = Arc::new(AtomicBool::new(true));
    let reader_parser = Arc::clone(parser);
    let reader_alive = Arc::clone(&alive);
    let reader_cwd = Arc::clone(osc_cwd);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut osc = Osc7Scanner::default();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(dir) = osc.feed(&buf[..n]) {
                        *reader_cwd
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dir);
                    }
                    if let Ok(mut p) = reader_parser.lock() {
                        p.process(&buf[..n]);
                    }
                }
            }
        }
        reader_alive.store(false, Ordering::SeqCst);
    });
    Ok(Pty {
        writer,
        master: pair.master,
        child,
        alive,
    })
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
    /// Spawn `shell` on a new PTY of `rows`×`cols`, rooted at `cwd`, keeping
    /// up to `scrollback` lines (clamped by [`clamp_scrollback`]).
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the shell cannot spawn.
    pub fn open(
        shell: &str,
        cwd: &std::path::Path,
        rows: u16,
        cols: u16,
        scrollback: usize,
    ) -> std::io::Result<Terminal> {
        Terminal::open_command(shell, &[] as &[&str], cwd, rows, cols, scrollback)
    }

    /// Spawn `program` with `args` on a new PTY of `rows`×`cols`, rooted at
    /// `cwd` (the OC menu's `exec -it <container> sh` tab, and a remote
    /// workspace's `ssh -t` session, T716), keeping up to `scrollback` lines
    /// (clamped by [`clamp_scrollback`]).
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the program cannot spawn.
    pub fn open_command<S: AsRef<std::ffi::OsStr>>(
        program: impl AsRef<std::ffi::OsStr>,
        args: &[S],
        cwd: &std::path::Path,
        rows: u16,
        cols: u16,
        scrollback: usize,
    ) -> std::io::Result<Terminal> {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let spec = SpawnSpec {
            program: program.as_ref().to_os_string(),
            args: args.iter().map(|a| a.as_ref().to_os_string()).collect(),
            cwd: cwd.to_path_buf(),
        };
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            rows,
            cols,
            clamp_scrollback(scrollback),
        )));
        let osc_cwd = Arc::new(Mutex::new(None));
        let pty = spawn_pty(&spec, rows, cols, &parser, &osc_cwd)?;
        Ok(Terminal {
            parser,
            writer: pty.writer,
            master: pty.master,
            child: pty.child,
            alive: pty.alive,
            rows,
            cols,
            selection: None,
            spawn: spec,
            osc_cwd,
            capture: true,
            exit_noted: false,
        })
    }

    /// Run the same command again on a fresh PTY, keeping this terminal's
    /// screen and scrollback (T666). The old child is killed first; `marker`
    /// is written into the transcript as a separator line, after the screen
    /// is returned to its normal, mouse-free state (so a shell that died in
    /// `vim` does not leave the new one on the alternate screen).
    ///
    /// # Errors
    /// Returns an error if the new PTY cannot be created, in which case the
    /// terminal is left as it was.
    pub fn restart(&mut self, marker: &str) -> std::io::Result<()> {
        let pty = spawn_pty(
            &self.spawn,
            self.rows,
            self.cols,
            &self.parser,
            &self.osc_cwd,
        )?;
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.selection = None;
        {
            let mut parser = self.lock();
            parser.screen_mut().set_scrollback(0);
            // Leave the alternate screen only if the old shell was on it:
            // doing so from the normal screen would discard the transcript.
            let leave_alt = if parser.screen().alternate_screen() {
                "\x1b[?1049l"
            } else {
                ""
            };
            let reset = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[0m";
            let line = marker.replace(['\r', '\n', '\x1b'], " ");
            parser.process(format!("{leave_alt}{reset}\r\n{line}\r\n").as_bytes());
        }
        self.writer = pty.writer;
        self.master = pty.master;
        self.child = pty.child;
        self.alive = pty.alive;
        self.exit_noted = false;
        *self
            .osc_cwd
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        Ok(())
    }

    /// The shell's working directory: the last one it announced with OSC 7
    /// (`ESC ] 7 ; file://host/path`), else the directory it was started in.
    #[must_use]
    pub fn cwd(&self) -> std::path::PathBuf {
        self.osc_cwd
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| self.spawn.cwd.clone())
    }

    /// `true` once for a shell that has exited: the host reports it, and the
    /// tab stays (so it can be restarted) instead of being dropped.
    pub fn take_exit_notice(&mut self) -> bool {
        if self.alive() || self.exit_noted {
            return false;
        }
        self.exit_noted = true;
        true
    }

    /// Whether editor chords go to the shell (the default) rather than the
    /// editor.
    #[must_use]
    pub fn capture(&self) -> bool {
        self.capture
    }

    /// Choose whether editor chords go to the shell.
    pub fn set_capture(&mut self, capture: bool) {
        self.capture = capture;
    }

    /// Send `text` to the shell as if pasted, then Enter: wrapped in
    /// bracketed-paste markers when the shell asked for them, else with
    /// newlines as carriage returns (T666).
    pub fn send_text(&mut self, text: &str) {
        let bracketed = self.lock().screen().bracketed_paste();
        self.scroll_to_bottom();
        let bytes = paste_bytes(text, bracketed);
        self.send_bytes(&bytes);
    }

    /// The text of view row `row` (0 is the top of what is shown), trailing
    /// blanks trimmed.
    #[must_use]
    pub fn row_text(&self, row: u16) -> String {
        let parser = self.lock();
        let screen = parser.screen();
        let (_, cols) = screen.size();
        screen
            .contents_between(row, 0, row, cols)
            .trim_end()
            .to_string()
    }

    /// The whitespace-delimited word under cell (`row`, `col`) of the view,
    /// with surrounding quotes and brackets trimmed.
    #[must_use]
    pub fn word_at(&self, row: u16, col: u16) -> Option<String> {
        word_in(&self.row_text(row), usize::from(col))
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

    /// Write already-encoded terminal input to the shell (a Cowork guest's
    /// keystrokes on a shared terminal, T697, which the host has checked).
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let _ = self.writer.write_all(bytes);
            let _ = self.writer.flush();
        }
    }

    /// How many lines the view is scrolled back from the live screen (0 when
    /// following the shell).
    #[must_use]
    pub fn scroll_offset(&self) -> usize {
        self.lock().screen().scrollback()
    }

    /// Scroll the view `delta` lines back into the scrollback (negative:
    /// toward the live screen), clamped to what is kept. Clears the selection,
    /// which is in view coordinates. Returns the new offset.
    pub fn scroll_by(&mut self, delta: isize) -> usize {
        self.selection = None;
        scroll_parser(&mut self.lock(), delta)
    }

    /// Back to the live screen (typing does this first).
    pub fn scroll_to_bottom(&mut self) {
        if self.scroll_offset() > 0 {
            self.selection = None;
            self.lock().screen_mut().set_scrollback(0);
        }
    }

    /// The live screen, whatever the view is scrolled to: what a Cowork share
    /// sends (T697), so scrolling back on the host never reaches guests.
    #[must_use]
    pub fn live_screen(&self) -> vt100::Screen {
        let mut screen = self.lock().screen().clone();
        screen.set_scrollback(0);
        screen
    }

    /// The current mouse selection, in view coordinates.
    #[must_use]
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Start, extend or clear the mouse selection.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    /// The selected text, or the whole visible view without a selection.
    #[must_use]
    pub fn copy_text(&self) -> String {
        let parser = self.lock();
        match self.selection {
            Some(sel) if !sel.is_empty() => sel.text(parser.screen()),
            _ => parser.screen().contents(),
        }
    }

    /// Whether the child asked for mouse reports (so clicks, drags and the
    /// wheel go to it rather than selecting or scrolling here).
    #[must_use]
    pub fn wants_mouse(&self) -> bool {
        self.lock().screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None
    }

    /// Forward a mouse event at cell (`col`, `row`) of the screen, encoded the
    /// way the child asked; returns `false` when it asked for nothing (or not
    /// for this kind of event).
    pub fn send_mouse(
        &mut self,
        kind: MouseEventKind,
        col: u16,
        row: u16,
        modifiers: KeyModifiers,
    ) -> bool {
        let bytes = {
            let parser = self.lock();
            let screen = parser.screen();
            encode_mouse(
                screen.mouse_protocol_mode(),
                screen.mouse_protocol_encoding(),
                kind,
                col,
                row,
                modifiers,
            )
        };
        self.send_bytes(&bytes);
        !bytes.is_empty()
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
        self.selection = None;
        let size = portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let _ = self.master.resize(size);
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_size(rows, cols);
        }
    }
}

/// What [`paste_bytes`] ends a pasted line with: Enter.
const ENTER: u8 = b'\r';

/// The bytes that send `text` to a shell as a pasted command and press
/// Enter: inside bracketed-paste markers when `bracketed`, else with every
/// newline sent as a carriage return (what Enter types).
#[must_use]
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.trim_end_matches(['\r', '\n']);
    let mut out = Vec::new();
    if bracketed {
        out.extend_from_slice(b"\x1b[200~");
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
    } else {
        out.extend_from_slice(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
    }
    out.push(ENTER);
    out
}

/// The whitespace-delimited word of `line` that covers character column
/// `col`, with surrounding quotes, brackets and trailing `,;.` trimmed;
/// `None` on blank space. The pure half of [`Terminal::word_at`].
#[must_use]
pub fn word_in(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.get(col).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let mut start = col;
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = col;
    while end + 1 < chars.len() && !chars[end + 1].is_whitespace() {
        end += 1;
    }
    let word: String = chars[start..=end].iter().collect();
    let word = word.trim_matches(|c| {
        matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';'
        )
    });
    let word = word.trim_end_matches('.');
    (!word.is_empty()).then(|| word.to_string())
}

/// Finds OSC 7 (`ESC ] 7 ; file://host/path` ended by BEL or `ESC \`) in the
/// bytes a shell prints, across chunk boundaries: the way a shell tells its
/// terminal where it is (T666). Everything else passes through untouched; the
/// scanner only watches.
#[derive(Debug, Default)]
pub struct Osc7Scanner {
    carry: Vec<u8>,
}

/// The start of an OSC 7 sequence.
const OSC7: &[u8] = b"\x1b]7;";

/// The longest OSC 7 body kept while waiting for its terminator.
const OSC7_MAX: usize = 4096;

impl Osc7Scanner {
    /// Feed the next chunk of output; returns the last directory announced in
    /// it (or completed by it), if any.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<std::path::PathBuf> {
        self.carry.extend_from_slice(bytes);
        let mut found = None;
        let mut pos = 0;
        loop {
            let rest = &self.carry[pos..];
            let Some(at) = find_sub(rest, OSC7) else {
                // Keep a possible start of the marker split across chunks.
                let keep = (1..OSC7.len())
                    .rev()
                    .find(|&k| rest.ends_with(&OSC7[..k]))
                    .unwrap_or(0);
                pos = self.carry.len() - keep;
                break;
            };
            let body_at = pos + at + OSC7.len();
            let body = &self.carry[body_at..];
            if let Some((len, term)) = osc_end(body) {
                if let Some(dir) = parse_file_uri(&body[..len]) {
                    found = Some(dir);
                }
                pos = body_at + len + term;
            } else {
                // Unterminated: wait for more, unless it is hopeless.
                pos = if body.len() > OSC7_MAX {
                    self.carry.len()
                } else {
                    pos + at
                };
                break;
            }
        }
        self.carry.drain(..pos);
        found
    }
}

/// Index of the first occurrence of `needle` in `hay`.
fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Where an OSC body ends: (body length, terminator length), for BEL or `ESC \`.
fn osc_end(body: &[u8]) -> Option<(usize, usize)> {
    body.iter().enumerate().find_map(|(i, &b)| match b {
        0x07 => Some((i, 1)),
        0x1b if body.get(i + 1) == Some(&b'\\') => Some((i, 2)),
        _ => None,
    })
}

/// The directory a `file://host/path` URI names (host ignored, `%XX`
/// decoded), or `None` for another scheme or a relative path.
fn parse_file_uri(uri: &[u8]) -> Option<std::path::PathBuf> {
    let uri = std::str::from_utf8(uri).ok()?;
    let rest = uri.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let mut bytes = Vec::with_capacity(path.len());
    let raw = path.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%'
            && let Some(hex) = path.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            bytes.push(v);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    let mut text = String::from_utf8(bytes).ok()?;
    // `file:///C:/x` is the Windows path `C:/x`.
    let b = text.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        text.remove(0);
    }
    Some(std::path::PathBuf::from(text))
}

/// Scroll `parser`'s view `delta` lines back (negative: forward), clamped to
/// its scrollback; returns the new offset. The pure half of
/// [`Terminal::scroll_by`].
pub fn scroll_parser(parser: &mut vt100::Parser, delta: isize) -> usize {
    let now = parser.screen().scrollback();
    parser
        .screen_mut()
        .set_scrollback(now.saturating_add_signed(delta));
    parser.screen().scrollback()
}

/// A mouse selection over a terminal's visible view: where the drag started
/// and where it is now, as (row, col) cells. Either may come first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the drag started (row, col).
    pub anchor: (u16, u16),
    /// Where the pointer is now (row, col).
    pub head: (u16, u16),
}

impl Selection {
    /// A selection that starts and ends at `cell` (row, col).
    #[must_use]
    pub fn at(cell: (u16, u16)) -> Selection {
        Selection {
            anchor: cell,
            head: cell,
        }
    }

    /// The two ends in reading order: (start, end), end inclusive.
    #[must_use]
    pub fn ordered(&self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Whether nothing was dragged over (a plain click).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Whether cell (`row`, `col`) is selected (in reading order, like a
    /// terminal: whole rows between the two ends).
    #[must_use]
    pub fn contains(&self, row: u16, col: u16) -> bool {
        let (start, end) = self.ordered();
        (row, col) >= start && (row, col) <= end
    }

    /// The selected text of `screen`'s visible view, end cell included.
    #[must_use]
    pub fn text(&self, screen: &vt100::Screen) -> String {
        let ((r0, c0), (r1, c1)) = self.ordered();
        screen.contents_between(r0, c0, r1, c1.saturating_add(1))
    }
}

/// Encode a mouse event at screen cell (`col`, `row`) for a child that set
/// `mode` and `encoding` (xterm's protocol: the legacy bytes, UTF-8, or SGR).
/// Empty when the mode does not report this kind of event.
#[must_use]
pub fn encode_mouse(
    mode: vt100::MouseProtocolMode,
    encoding: vt100::MouseProtocolEncoding,
    kind: MouseEventKind,
    col: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> Vec<u8> {
    use vt100::MouseProtocolMode as Mode;
    let button = |b: MouseButton| match b {
        MouseButton::Left => 0u32,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    // (button code, whether it is a release)
    let (mut code, release) = match (mode, kind) {
        (Mode::None, _) => return Vec::new(),
        (_, MouseEventKind::Down(b)) => (button(b), false),
        (_, MouseEventKind::ScrollUp) => (64, false),
        (_, MouseEventKind::ScrollDown) => (65, false),
        (Mode::Press, _) => return Vec::new(),
        (_, MouseEventKind::Up(b)) => (button(b), true),
        (Mode::ButtonMotion | Mode::AnyMotion, MouseEventKind::Drag(b)) => (button(b) + 32, false),
        (Mode::AnyMotion, MouseEventKind::Moved) => (3 + 32, false),
        _ => return Vec::new(),
    };
    if modifiers.contains(KeyModifiers::SHIFT) {
        code += 4;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        code += 8;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        code += 16;
    }
    let (x, y) = (u32::from(col) + 1, u32::from(row) + 1);
    if encoding == vt100::MouseProtocolEncoding::Sgr {
        let end = if release { 'm' } else { 'M' };
        return format!("\x1b[<{code};{x};{y}{end}").into_bytes();
    }
    // The legacy encodings report any release as button 3.
    if release {
        code |= 3;
    }
    let mut out = b"\x1b[M".to_vec();
    for v in [code, x, y] {
        let v = v + 32;
        if encoding == vt100::MouseProtocolEncoding::Utf8 {
            let c = char::from_u32(v.min(2047)).unwrap_or(' ');
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        } else {
            out.push(u8::try_from(v).unwrap_or(u8::MAX));
        }
    }
    out
}

/// Map a `portable_pty` error into an [`std::io::Error`].
fn to_io(e: &anyhow::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Encode a key event into the byte sequence a terminal expects.
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
    use vt100::{MouseProtocolEncoding as Enc, MouseProtocolMode as Mode};

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

    /// A 5-row parser keeping `cap` scrollback lines, fed `lines` numbered
    /// lines.
    fn fed(cap: usize, lines: usize) -> vt100::Parser {
        let mut p = vt100::Parser::new(5, 20, cap);
        for i in 1..=lines {
            p.process(format!("line {i}\r\n").as_bytes());
        }
        p
    }

    #[test]
    fn scrolling_pages_through_the_captured_scrollback() {
        let mut p = fed(100, 30);
        // The live screen shows the last lines.
        assert!(p.screen().contents().contains("line 30"));
        assert_eq!(p.screen().scrollback(), 0);
        assert_eq!(scroll_parser(&mut p, 4), 4);
        assert!(p.screen().contents().contains("line 26"));
        assert!(!p.screen().contents().contains("line 30"));
        assert_eq!(scroll_parser(&mut p, -1), 3);
        // Forward past the live screen stops at 0.
        assert_eq!(scroll_parser(&mut p, -100), 0);
        assert!(p.screen().contents().contains("line 30"));
    }

    #[test]
    fn scrollback_is_capped_at_the_configured_lines() {
        // 30 lines through a 5-row screen push 26 rows off the top (the
        // trailing newline leaves the last row empty); only 10 are kept.
        let mut p = fed(10, 30);
        assert_eq!(scroll_parser(&mut p, isize::MAX), 10);
        let top = p.screen().contents();
        assert!(top.contains("line 17"), "{top}");
        assert!(!top.contains("line 16"), "{top}");
        // No scrollback at all: the view never moves.
        let mut none = fed(0, 30);
        assert_eq!(scroll_parser(&mut none, 5), 0);
    }

    #[test]
    fn the_setting_is_clamped() {
        assert_eq!(clamp_scrollback(0), 0);
        assert_eq!(clamp_scrollback(1234), 1234);
        assert_eq!(clamp_scrollback(usize::MAX), MAX_SCROLLBACK);
        const { assert!(DEFAULT_SCROLLBACK <= MAX_SCROLLBACK) };
    }

    #[test]
    fn a_selection_copies_in_reading_order() {
        let mut p = vt100::Parser::new(3, 10, 0);
        p.process(b"hello\r\nworld");
        let sel = Selection {
            anchor: (1, 2),
            head: (0, 1),
        };
        assert_eq!(sel.ordered(), ((0, 1), (1, 2)));
        assert_eq!(sel.text(p.screen()), "ello\nwor");
        assert!(sel.contains(0, 9));
        assert!(sel.contains(1, 0));
        assert!(!sel.contains(1, 3));
        assert!(Selection::at((2, 2)).is_empty());
        let one_row = Selection {
            anchor: (0, 0),
            head: (0, 3),
        };
        assert_eq!(one_row.text(p.screen()), "hell");
    }

    #[test]
    fn the_mouse_is_forwarded_only_when_asked() {
        const NOTHING: Vec<u8> = Vec::new();
        let down = MouseEventKind::Down(MouseButton::Left);
        let none = KeyModifiers::NONE;
        assert_eq!(
            encode_mouse(Mode::None, Enc::Default, down, 0, 0, none),
            NOTHING
        );
        // Legacy bytes: ESC [ M, then 32 + button, 32 + x + 1, 32 + y + 1.
        assert_eq!(
            encode_mouse(Mode::PressRelease, Enc::Default, down, 4, 2, none),
            vec![0x1b, b'[', b'M', 32, 37, 35]
        );
        // A release is button 3 in the legacy encoding.
        let up = MouseEventKind::Up(MouseButton::Left);
        assert_eq!(
            encode_mouse(Mode::PressRelease, Enc::Default, up, 0, 0, none),
            vec![0x1b, b'[', b'M', 35, 33, 33]
        );
        // X10 mode reports presses only.
        assert_eq!(
            encode_mouse(Mode::Press, Enc::Default, up, 0, 0, none),
            NOTHING
        );
        // SGR: the button, 1-based coordinates, M for press and m for release.
        assert_eq!(
            encode_mouse(Mode::PressRelease, Enc::Sgr, up, 9, 19, none),
            b"\x1b[<0;10;20m"
        );
        assert_eq!(
            encode_mouse(
                Mode::PressRelease,
                Enc::Sgr,
                MouseEventKind::ScrollUp,
                0,
                0,
                KeyModifiers::CONTROL
            ),
            b"\x1b[<80;1;1M"
        );
        // Motion only in the motion modes.
        let drag = MouseEventKind::Drag(MouseButton::Left);
        assert_eq!(
            encode_mouse(Mode::PressRelease, Enc::Sgr, drag, 0, 0, none),
            NOTHING
        );
        assert_eq!(
            encode_mouse(Mode::ButtonMotion, Enc::Sgr, drag, 0, 0, none),
            b"\x1b[<32;1;1M"
        );
        assert_eq!(
            encode_mouse(
                Mode::ButtonMotion,
                Enc::Sgr,
                MouseEventKind::Moved,
                0,
                0,
                none
            ),
            NOTHING
        );
        // UTF-8 encodes a far column as a multi-byte character.
        let far = encode_mouse(Mode::PressRelease, Enc::Utf8, down, 300, 0, none);
        assert_eq!(&far[..4], &[0x1b, b'[', b'M', 32]);
        assert_eq!(std::str::from_utf8(&far[4..]).unwrap(), "\u{14d}!");
    }

    #[test]
    #[cfg(unix)]
    fn dropping_the_terminal_kills_the_shell_and_reader() {
        use std::time::{Duration, Instant};
        let term =
            Terminal::open("/bin/sh", std::path::Path::new("/"), 24, 80, 0).expect("spawn a shell");
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

    #[test]
    fn osc7_reads_a_directory_with_either_terminator() {
        let mut s = Osc7Scanner::default();
        assert_eq!(
            s.feed(b"ls\x1b]7;file://host/home/me/proj\x07$ "),
            Some(std::path::PathBuf::from("/home/me/proj"))
        );
        assert_eq!(
            s.feed(b"\x1b]7;file:///tmp/a%20b\x1b\\"),
            Some(std::path::PathBuf::from("/tmp/a b"))
        );
        assert_eq!(s.feed(b"plain output"), None);
    }

    #[test]
    fn osc7_survives_chunk_boundaries_and_keeps_the_last() {
        let mut s = Osc7Scanner::default();
        assert_eq!(s.feed(b"x\x1b]"), None);
        assert_eq!(s.feed(b"7;file:///usr/"), None);
        assert_eq!(
            s.feed(b"local\x07 \x1b]7;file:///etc\x07"),
            Some(std::path::PathBuf::from("/etc"))
        );
        // A non-file URI and a runaway body are ignored, not fatal.
        assert_eq!(s.feed(b"\x1b]7;https://x/y\x07"), None);
        let mut big = b"\x1b]7;file:///".to_vec();
        big.extend(std::iter::repeat_n(b'a', OSC7_MAX + 10));
        assert_eq!(s.feed(&big), None);
        assert_eq!(
            s.feed(b"\x1b]7;file:///ok\x07"),
            Some(std::path::PathBuf::from("/ok"))
        );
    }

    #[test]
    fn osc7_windows_drive_path() {
        let mut s = Osc7Scanner::default();
        assert_eq!(
            s.feed(b"\x1b]7;file:///C:/work\x07"),
            Some(std::path::PathBuf::from("C:/work"))
        );
    }

    #[test]
    fn word_in_picks_the_word_under_the_column() {
        let line = "error at (src/main.rs:12:5), see";
        assert_eq!(word_in(line, 12).as_deref(), Some("src/main.rs:12:5"));
        assert_eq!(word_in(line, 6), Some("at".to_string()));
        assert_eq!(word_in(line, 5), None);
        assert_eq!(word_in(line, 500), None);
    }

    #[test]
    fn paste_bytes_brackets_or_converts_newlines() {
        assert_eq!(paste_bytes("ls\n", false), b"ls\r");
        assert_eq!(paste_bytes("a\nb\r\nc", false), b"a\rb\rc\r");
        assert_eq!(paste_bytes("a\nb", true), b"\x1b[200~a\nb\x1b[201~\r");
    }
}
