//! A seeded "chaos monkey" for the real `App` (T575): long random streams of
//! key, mouse, paste and resize events driven into the app on a `TestBackend`,
//! each iteration under a watchdog, so a panic or a hang is reported with the
//! seed and the last events and can be replayed exactly.
//!
//! `fuzz/` covers parsers and `tests/integration` covers scripted flows; this
//! covers what neither does — whatever sequence of input a person might produce.
//!
//! ```text
//! cargo run --release --example chaos_monkey -- --iterations 200 --events 400
//! cargo run --release --example chaos_monkey -- --seed 1234 --iterations 1   # replay
//! ```
//!
//! Options: `--seed N` (first iteration's seed; default from the clock, always
//! printed), `--iterations N` (default 50), `--events N` per iteration (default
//! 300), `--timeout SECS` per iteration (default 20), `--last N` events kept for
//! the report (default 40). Iteration `i` uses seed `seed + i`, so the seed
//! printed for a failure replays it with `--iterations 1`.
//!
//! The run re-executes itself in a sandbox: an empty `PATH` (so no random
//! command line can start `sh`, `git` or anything else), and a throwaway `HOME`
//! and config directory (so saved settings, themes and the trash never touch the
//! real ones). The clipboard is the in-memory one the library uses by default.
//! For a network-free run, wrap it: `unshare -rn cargo run --example chaos_monkey`.
//!
//! Exit status: 0 clean, 1 an iteration panicked, 2 an iteration hung.

#![warn(clippy::pedantic)]

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use vix::app::{App, AppFlags};
use vix::settings::Settings;

/// Marks the sandboxed child process.
const SANDBOX_VAR: &str = "VIX_CHAOS_SANDBOX";

/// Command-line options.
struct Options {
    seed: u64,
    iterations: u64,
    events: usize,
    timeout: Duration,
    last: usize,
}

fn parse_options() -> Options {
    let mut o = Options {
        seed: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| u64::try_from(d.as_nanos()).unwrap_or(1)),
        iterations: 50,
        events: 300,
        timeout: Duration::from_secs(20),
        last: 40,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().and_then(|v| v.parse::<u64>().ok());
        match (flag.as_str(), value) {
            ("--seed", Some(v)) => o.seed = v,
            ("--iterations", Some(v)) => o.iterations = v,
            ("--events", Some(v)) => o.events = usize::try_from(v).unwrap_or(300),
            ("--timeout", Some(v)) => o.timeout = Duration::from_secs(v),
            ("--last", Some(v)) => o.last = usize::try_from(v).unwrap_or(40),
            _ => {
                eprintln!(
                    "usage: chaos_monkey [--seed N] [--iterations N] [--events N] [--timeout SECS] [--last N]"
                );
                std::process::exit(64);
            }
        }
    }
    o
}

/// A small deterministic generator (xorshift64*), so a seed replays exactly and
/// no `rand` dependency is needed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Mix the seed so small consecutive seeds give unrelated streams.
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// One generated input.
#[derive(Debug, Clone)]
enum Input {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
}

const NAMED_KEYS: &[KeyCode] = &[
    KeyCode::Enter,
    KeyCode::Esc,
    KeyCode::Esc,
    KeyCode::Tab,
    KeyCode::BackTab,
    KeyCode::Backspace,
    KeyCode::Delete,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Insert,
];

const PASTES: &[&str] = &[
    "hello",
    "line one\nline two\n\nline four",
    "\t\ttabbed\r\ncrlf",
    "héllo wörld — ünïcode ✓ 日本語 🦀",
    "",
    "   ",
    "((([[[{{{",
    "a,b,c\n1,2,3\n",
    "# heading\n- [ ] todo\n",
];

fn key(code: KeyCode, modifiers: KeyModifiers) -> Input {
    Input::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn random_char(rng: &mut Rng) -> char {
    const ALPHABET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyz ABCXYZ0123456789.,;:/\\'\"()[]{}<>-_=+*&^%$#@!?~`|";
    if rng.below(40) == 0 {
        return *rng.pick(&['é', 'ß', '日', '🦀', '\u{200d}', '\u{0301}']);
    }
    char::from(*rng.pick(ALPHABET))
}

/// Generate the next input, weighted toward menu chords and overlay open/close.
fn next_input(rng: &mut Rng, width: u16, height: u16) -> Input {
    match rng.below(100) {
        // Plain typing.
        0..=29 => key(KeyCode::Char(random_char(rng)), KeyModifiers::NONE),
        // Navigation and editing keys; Esc is listed twice on purpose.
        30..=54 => key(*rng.pick(NAMED_KEYS), KeyModifiers::NONE),
        // Control chords.
        55..=69 => key(
            KeyCode::Char(char::from(b'a' + u8::try_from(rng.below(26)).unwrap_or(0))),
            KeyModifiers::CONTROL,
        ),
        // Alt+letter: menu mnemonics, which open the menu bar and its submenus.
        70..=79 => key(
            KeyCode::Char(char::from(b'a' + u8::try_from(rng.below(26)).unwrap_or(0))),
            KeyModifiers::ALT,
        ),
        // Shifted and combined navigation.
        80..=83 => key(
            *rng.pick(NAMED_KEYS),
            *rng.pick(&[
                KeyModifiers::SHIFT,
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ]),
        ),
        84 => key(
            KeyCode::F(u8::try_from(1 + rng.below(12)).unwrap_or(1)),
            KeyModifiers::NONE,
        ),
        // Mouse: clicks, drags, scrolls and hover anywhere on screen.
        85..=96 => {
            let button = *rng.pick(&[MouseButton::Left, MouseButton::Left, MouseButton::Right]);
            let kind = match rng.below(6) {
                0 => MouseEventKind::Down(button),
                1 => MouseEventKind::Up(button),
                2 => MouseEventKind::Drag(button),
                3 => MouseEventKind::ScrollUp,
                4 => MouseEventKind::ScrollDown,
                _ => MouseEventKind::Moved,
            };
            Input::Mouse(MouseEvent {
                kind,
                column: u16::try_from(rng.below(usize::from(width) + 2)).unwrap_or(0),
                row: u16::try_from(rng.below(usize::from(height) + 2)).unwrap_or(0),
                modifiers: KeyModifiers::NONE,
            })
        }
        97..=98 => Input::Paste((*rng.pick(PASTES)).to_string()),
        _ => Input::Resize(
            u16::try_from(1 + rng.below(160)).unwrap_or(80),
            u16::try_from(1 + rng.below(60)).unwrap_or(24),
        ),
    }
}

/// A short, human-readable form of an input for the report.
fn describe(input: &Input) -> String {
    match input {
        Input::Key(k) => format!("key {:?} {:?}", k.code, k.modifiers),
        Input::Mouse(m) => format!("mouse {:?} at ({}, {})", m.kind, m.column, m.row),
        Input::Paste(t) => format!("paste {t:?}"),
        Input::Resize(w, h) => format!("resize {w}x{h}"),
    }
}

/// A small workspace with a few kinds of file, so file-specific features have
/// something to act on.
fn make_workspace(root: &Path) {
    let files: &[(&str, &str)] = &[
        ("notes.txt", "alpha\nbeta\ngamma\n\ndelta epsilon\n"),
        ("main.rs", "fn main() {\n    println!(\"hi\");\n}\n"),
        (
            "README.md",
            "# Title\n\nSome *text* and a [link](x).\n\n- one\n- two\n",
        ),
        ("data.csv", "name,qty\napple,3\npear,5\n"),
        ("sub/nested.txt", "nested file\n"),
        (".gitignore", "target/\n"),
    ];
    for (name, body) in files {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, body);
    }
}

fn new_app(root: &Path, session: &Path) -> App {
    let mut app =
        App::new(root.to_path_buf(), Settings::default()).with_session_path(session.to_path_buf());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(&root.join("notes.txt"));
    app.open_initial(&root.join("main.rs"));
    app
}

/// Run one iteration: `events` random inputs, drawing a frame after each.
fn run_iteration(seed: u64, events: usize, log: &Mutex<VecDeque<String>>, keep: usize) {
    let base = std::env::temp_dir().join(format!("vix-chaos-{}-{seed}", std::process::id()));
    let root = base.join("ws");
    let _ = std::fs::remove_dir_all(&base);
    make_workspace(&root);
    let session = base.join("session.toml");

    let mut rng = Rng::new(seed);
    let (mut width, mut height) = (80u16, 24u16);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    let mut app = new_app(&root, &session);

    for n in 0..events {
        let input = next_input(&mut rng, width, height);
        {
            let mut log = log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if log.len() >= keep {
                log.pop_front();
            }
            log.push_back(format!("#{n} {}", describe(&input)));
        }
        match input {
            Input::Key(k) => app.on_key(k),
            Input::Mouse(m) => app.on_mouse(m),
            Input::Paste(t) => app.on_paste(&t),
            Input::Resize(w, h) => {
                width = w;
                height = h;
                terminal.backend_mut().resize(w, h);
                terminal.resize(Rect::new(0, 0, w, h)).expect("resize");
            }
        }
        app.poll_command();
        check_invariants(&app);
        terminal
            .draw(|frame| vix::ui::draw(&mut app, frame))
            .expect("draw a frame");
        // A quit request is just the end of that session: start another.
        if app.flags.contains(AppFlags::SHOULD_QUIT) {
            app = new_app(&root, &session);
        }
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// Invariants that must hold after every event, checked so a bug is caught at
/// the event that causes it rather than at a later draw: the cursor and every
/// selection lie inside the text. A violation panics like any other failure.
fn check_invariants(app: &App) {
    let Some(tab) = app.editor.active_tab() else {
        return;
    };
    let len = tab.editor.code_ref().len_chars();
    let cursor = tab.editor.get_cursor();
    assert!(
        cursor <= len,
        "cursor {cursor} is past the end of the text ({len} chars)"
    );
    for pos in tab.editor.caret_positions() {
        assert!(
            pos <= len,
            "caret {pos} is past the end of the text ({len} chars)"
        );
    }
    if let Some((start, end)) = tab.editor.selection_span() {
        assert!(
            end <= len,
            "selection span {start}..{end} is past the end of the text ({len} chars)"
        );
    }
    for (start, end) in tab.editor.caret_selections() {
        assert!(
            start <= len && end <= len,
            "selection {start}..{end} is past the end of the text ({len} chars)"
        );
    }
}

/// Print a failure with everything needed to replay it.
fn report(kind: &str, seed: u64, message: &str, log: &Mutex<VecDeque<String>>) {
    let mut out = String::new();
    let _ = writeln!(out, "\n=== chaos monkey: {kind} ===");
    let _ = writeln!(out, "seed: {seed}");
    if !message.is_empty() {
        let _ = writeln!(out, "message: {message}");
    }
    let _ = writeln!(
        out,
        "replay: cargo run --release --example chaos_monkey -- --seed {seed} --iterations 1"
    );
    let _ = writeln!(out, "last events (oldest first):");
    for line in log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
    {
        let _ = writeln!(out, "  {line}");
    }
    eprint!("{out}");
}

/// Re-run this example inside the sandbox described in the module docs.
fn run_in_sandbox() -> ! {
    let base = std::env::temp_dir().join(format!("vix-chaos-sandbox-{}", std::process::id()));
    let empty_path: PathBuf = base.join("empty-path");
    let home = base.join("home");
    let config = base.join("config");
    for dir in [&empty_path, &home, &config] {
        let _ = std::fs::create_dir_all(dir);
    }
    let status = Command::new(std::env::current_exe().expect("current exe"))
        .args(std::env::args().skip(1))
        .env(SANDBOX_VAR, "1")
        .env("PATH", &empty_path)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", home.join("data"))
        .status();
    let _ = std::fs::remove_dir_all(&base);
    std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
}

fn main() {
    if std::env::var_os(SANDBOX_VAR).is_none() {
        run_in_sandbox();
    }
    let o = parse_options();
    println!(
        "chaos monkey: seed {}, {} iterations x {} events",
        o.seed, o.iterations, o.events
    );

    let panic_message = Arc::new(Mutex::new(String::new()));
    {
        let slot = Arc::clone(&panic_message);
        std::panic::set_hook(Box::new(move |info| {
            // The location and message, then the frames inside Vix itself.
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let frames: Vec<&str> = backtrace
                .lines()
                .filter(|l| l.contains("vix") && !l.contains("chaos_monkey"))
                .take(14)
                .collect();
            *slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                format!("{info}\n{}", frames.join("\n"));
        }));
    }

    for i in 0..o.iterations {
        let seed = o.seed.wrapping_add(i);
        let log = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx) = channel();
        let worker_log = Arc::clone(&log);
        let (events, keep) = (o.events, o.last);
        // A generous stack: the renderer and menus recurse a little, and a debug
        // build uses more than a release one.
        let spawned = std::thread::Builder::new()
            .name(format!("chaos-{seed}"))
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                run_iteration(seed, events, &worker_log, keep);
                let _ = tx.send(());
            })
            .expect("spawn iteration thread");
        match rx.recv_timeout(o.timeout) {
            Ok(()) => {
                let _ = spawned.join();
            }
            Err(RecvTimeoutError::Timeout) => {
                report("HANG (watchdog timeout)", seed, "", &log);
                std::process::exit(2);
            }
            Err(RecvTimeoutError::Disconnected) => {
                let message = panic_message
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                report("PANIC", seed, &message, &log);
                std::process::exit(1);
            }
        }
    }
    println!("chaos monkey: {} iterations clean", o.iterations);
}
