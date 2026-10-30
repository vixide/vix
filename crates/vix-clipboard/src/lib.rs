//! Process-wide serialization of system-clipboard access.
//!
//! Platform clipboard backends — notably macOS's Cocoa `NSPasteboard` — are not
//! thread-safe: concurrent `arboard` calls corrupt memory and crash the process.
//! Every crate that touches the clipboard must go through [`set`] / [`get`] so
//! all access is sequential behind one shared lock. In the single-threaded app
//! the lock is uncontended; under parallel tests (or any future background
//! copy) it is what keeps the platform backend from being entered concurrently.
//!
//! **The platform clipboard is opt-in.** Until [`use_system`] is called, [`set`]
//! and [`get`] read and write a thread-local in-memory clipboard instead. The
//! `vix` binary opts in once at startup; everything else — the test suite above
//! all — keeps the in-memory clipboard, so running `cargo test` can never
//! overwrite whatever the developer had copied. (It did: a keymap test cut the
//! line `doomed` from a scratch buffer, and that landed on the real macOS
//! pasteboard.)
//!
//! **OSC 52 (T625).** Over SSH there is no local platform clipboard to reach,
//! so [`configure`] can route writes through the terminal instead: an OSC 52
//! escape sequence ([`encode_osc52`]) that the terminal — wherever it runs —
//! turns into a clipboard write. Reads under OSC 52 return the internal
//! register (the same thread-local store as the in-memory clipboard), since
//! terminals rarely answer OSC 52 queries. See [`Provider`] and
//! [`select_backend`].

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::cell::RefCell;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// The process-wide clipboard lock. Held for the duration of each `arboard`
/// call so no two clipboard operations run at once, anywhere in the process.
static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

/// Whether [`set`] / [`get`] reach the platform clipboard. Off until
/// [`use_system`] turns it on.
static SYSTEM: AtomicBool = AtomicBool::new(false);

/// The configured [`Provider`], as its tag (see `Provider::tag`). `auto`
/// until [`configure`] says otherwise.
static PROVIDER: AtomicU8 = AtomicU8::new(0);

/// Whether the process runs over SSH, as recorded by [`configure`].
static ENV_SSH: AtomicBool = AtomicBool::new(false);

/// Whether the process runs inside tmux, as recorded by [`configure`].
static ENV_TMUX: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The in-memory clipboard used until [`use_system`] is called — and the
    /// internal register OSC 52 reads fall back to (T625). Per thread,
    /// not process-wide: parallel tests each get their own, so one test's copy
    /// can never be pasted by another (T596). The app only touches the
    /// clipboard from its UI thread, so nothing real is split by this.
    static MEMORY: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// How the clipboard is reached once [`use_system`] is on: the
/// `clipboard_provider` setting (T625).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Provider {
    /// OSC 52 over SSH; otherwise the platform clipboard, falling back to
    /// OSC 52 (writes) and the internal register (reads) when it errors.
    #[default]
    Auto,
    /// The platform clipboard (`arboard`) only.
    System,
    /// OSC 52 writes always; reads come from the internal register.
    Osc52,
}

impl Provider {
    /// Parse a `clipboard_provider` setting value (`"auto"`, `"system"`,
    /// `"osc52"`, case-insensitive). Anything else is [`Provider::Auto`].
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Self::System,
            "osc52" => Self::Osc52,
            _ => Self::Auto,
        }
    }

    /// The tag stored in [`PROVIDER`].
    fn tag(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::System => 1,
            Self::Osc52 => 2,
        }
    }

    /// The inverse of `tag`; unknown tags are `Auto`.
    fn from_tag(tag: u8) -> Self {
        match tag {
            1 => Self::System,
            2 => Self::Osc52,
            _ => Self::Auto,
        }
    }
}

/// The parts of the process environment that pick a clipboard backend, as
/// plain data — so selection is testable from a fake environment without
/// ever setting real environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Environment {
    /// `SSH_TTY` or `SSH_CONNECTION` is set (non-empty).
    pub ssh: bool,
    /// `TMUX` is set (non-empty).
    pub tmux: bool,
}

impl Environment {
    /// Build from a variable lookup (`name -> value`). An empty value counts
    /// as unset.
    #[must_use]
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let set = |name: &str| lookup(name).is_some_and(|v| !v.is_empty());
        Self {
            ssh: set("SSH_TTY") || set("SSH_CONNECTION"),
            tmux: set("TMUX"),
        }
    }

    /// Read the real process environment (reading only — never setting).
    #[must_use]
    pub fn current() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }
}

/// Where a clipboard write goes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    /// The platform clipboard, via `arboard`.
    System,
    /// An OSC 52 escape sequence written to the terminal.
    Osc52,
}

/// Pick the backend for `provider` in `env`: `auto` means OSC 52 over SSH
/// and the platform clipboard otherwise. (Under `auto`, the fall-back to
/// OSC 52 when `arboard` errors happens at write time, in [`set`].)
#[must_use]
pub fn select_backend(provider: Provider, env: &Environment) -> Backend {
    match provider {
        Provider::Osc52 => Backend::Osc52,
        Provider::Auto if env.ssh => Backend::Osc52,
        Provider::System | Provider::Auto => Backend::System,
    }
}

/// The OSC 52 "set clipboard" sequence for `text`:
/// `ESC ] 52 ; c ; <base64> BEL`. With `tmux`, it is wrapped in tmux's DCS
/// passthrough (`ESC P tmux; … ESC \`, every inner `ESC` doubled) so tmux
/// forwards it to the outer terminal.
#[must_use]
pub fn encode_osc52(text: &str, tmux: bool) -> String {
    let osc = format!("\x1b]52;c;{}\x07", STANDARD.encode(text.as_bytes()));
    if tmux {
        format!("\x1bPtmux;{}\x1b\\", osc.replace('\x1b', "\x1b\x1b"))
    } else {
        osc
    }
}

/// Record the `clipboard_provider` setting and the environment it runs in,
/// process-wide. `tui/src/main.rs` calls this once at startup; without it
/// the provider is `auto` in an empty environment (plain `arboard`). It has
/// no effect until [`use_system`] is on.
pub fn configure(provider: Provider, env: Environment) {
    PROVIDER.store(provider.tag(), Ordering::Relaxed);
    ENV_SSH.store(env.ssh, Ordering::Relaxed);
    ENV_TMUX.store(env.tmux, Ordering::Relaxed);
}

/// The provider and environment recorded by [`configure`].
fn configured() -> (Provider, Environment) {
    (
        Provider::from_tag(PROVIDER.load(Ordering::Relaxed)),
        Environment {
            ssh: ENV_SSH.load(Ordering::Relaxed),
            tmux: ENV_TMUX.load(Ordering::Relaxed),
        },
    )
}

/// Route [`set`] / [`get`] to the platform clipboard for the rest of the
/// process. The `vix` binary calls this once at startup; anything that does not
/// — the test suite, and any embedder that would rather stay self-contained —
/// keeps the in-memory clipboard, so a test run cannot clobber the real one.
pub fn use_system() {
    SYSTEM.store(true, Ordering::Relaxed);
}

/// Whether the platform clipboard is in use (see [`use_system`]).
#[must_use]
pub fn is_system() -> bool {
    SYSTEM.load(Ordering::Relaxed)
}

/// Lock the shared clipboard mutex, ignoring poisoning: a panic in another
/// thread's clipboard call leaves no state of ours to corrupt.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Store `text` in the internal register.
fn set_memory(text: &str) {
    MEMORY.with_borrow_mut(|m| *m = Some(text.to_string()));
}

/// Read the internal register.
fn get_memory() -> Result<String> {
    MEMORY
        .with_borrow(Clone::clone)
        .ok_or_else(|| anyhow!("clipboard is empty"))
}

/// Write `text` to the terminal as OSC 52 and keep it in the internal
/// register, so a later [`get`] can return it.
fn set_osc52(text: &str, tmux: bool) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(encode_osc52(text, tmux).as_bytes())?;
    out.flush()?;
    set_memory(text);
    Ok(())
}

/// Write `text` to the clipboard, serialized behind the shared lock. With the
/// platform clipboard on, the configured [`Provider`] decides between
/// `arboard` and OSC 52 (see [`select_backend`]).
///
/// # Errors
/// Returns the backend error when the platform clipboard is unavailable (and
/// the provider is `system`), or when writing OSC 52 to the terminal fails.
/// The in-memory clipboard never fails.
pub fn set(text: &str) -> Result<()> {
    let _guard = lock(&CLIPBOARD_LOCK);
    if !is_system() {
        set_memory(text);
        return Ok(());
    }
    let (provider, env) = configured();
    if select_backend(provider, &env) == Backend::Osc52 {
        return set_osc52(text, env.tmux);
    }
    let result = arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        // T542 (Run H): keep the real `arboard::Error` as the source instead
        // of collapsing it to a plain string via `anyhow!(e.to_string())` --
        // a future caller that wants to distinguish e.g. `ClipboardOccupied`
        // (transient, worth retrying) from `ClipboardNotSupported` can
        // `downcast_ref::<arboard::Error>()`; today's callers still just
        // `Display` it, unaffected either way.
        .map_err(anyhow::Error::from);
    match result {
        Err(_) if provider == Provider::Auto => set_osc52(text, env.tmux),
        other => other,
    }
}

/// Read the clipboard text, serialized behind the shared lock. Under OSC 52
/// (which terminals rarely let a program read) this is the internal register
/// holding the last copy; under `auto`, a failing `arboard` falls back to it.
///
/// # Errors
/// Returns the backend error when the platform clipboard is unavailable (and
/// no fallback holds text), or an error when the internal register is empty.
pub fn get() -> Result<String> {
    let _guard = lock(&CLIPBOARD_LOCK);
    if !is_system() {
        return get_memory();
    }
    let (provider, env) = configured();
    if select_backend(provider, &env) == Backend::Osc52 {
        return get_memory();
    }
    let result = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        // See the matching comment in `set` (T542, Run H).
        .map_err(anyhow::Error::from);
    match result {
        Err(e) if provider == Provider::Auto => get_memory().map_err(|_| e),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is the in-memory clipboard: text round-trips through it, and
    /// reading before anything was written is an error rather than a panic.
    /// These tests never touch the platform clipboard — that is the point.
    #[test]
    fn in_memory_by_default_and_round_trips() {
        assert!(!is_system(), "the platform clipboard is opt-in");
        assert!(get().is_err(), "nothing copied yet");
        set("hello").unwrap();
        assert_eq!(get().unwrap(), "hello");
        set("goodbye").unwrap();
        assert_eq!(
            get().unwrap(),
            "goodbye",
            "a second copy replaces the first"
        );
    }

    /// The in-memory clipboard belongs to the calling thread, so parallel tests
    /// (each on its own thread) cannot overwrite one another's copy. It used to
    /// be a process-wide static, which made `y`/`p` tests flake under
    /// `cargo test`'s default parallelism (T596).
    #[test]
    fn in_memory_clipboard_is_per_thread() {
        use std::sync::{Arc, Barrier};
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|mine| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    set(mine).unwrap();
                    // Both threads have written before either reads back.
                    barrier.wait();
                    get().unwrap()
                })
            })
            .collect();
        let got: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(got, ["one", "two"]);
    }

    /// A fake environment: only the named variables are set. Tests never
    /// touch the real process environment (T625).
    fn env(vars: &[(&str, &str)]) -> Environment {
        Environment::from_lookup(|name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn osc52_encodes_base64_between_osc_and_bel() {
        assert_eq!(encode_osc52("hello", false), "\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(encode_osc52("", false), "\x1b]52;c;\x07");
        // The UTF-8 bytes are encoded, not the chars.
        assert_eq!(encode_osc52("é", false), "\x1b]52;c;w6k=\x07");
    }

    #[test]
    fn osc52_under_tmux_is_wrapped_in_dcs_passthrough() {
        assert_eq!(
            encode_osc52("hello", true),
            "\x1bPtmux;\x1b\x1b]52;c;aGVsbG8=\x07\x1b\\"
        );
    }

    #[test]
    fn environment_reads_ssh_and_tmux_from_a_fake_lookup() {
        assert_eq!(env(&[]), Environment::default());
        assert!(env(&[("SSH_TTY", "/dev/pts/1")]).ssh);
        assert!(env(&[("SSH_CONNECTION", "10.0.0.1 22 10.0.0.2 22")]).ssh);
        assert!(!env(&[("SSH_TTY", "")]).ssh, "empty counts as unset");
        assert!(!env(&[("SSH_AUTH_SOCK", "/tmp/agent")]).ssh);
        assert!(env(&[("TMUX", "/tmp/tmux-501/default,1,0")]).tmux);
        assert!(!env(&[("SSH_TTY", "/dev/pts/1")]).tmux);
    }

    #[test]
    fn provider_selection_from_a_fake_environment() {
        let local = env(&[]);
        let ssh = env(&[("SSH_CONNECTION", "10.0.0.1 22 10.0.0.2 22")]);
        assert_eq!(select_backend(Provider::Auto, &local), Backend::System);
        assert_eq!(select_backend(Provider::Auto, &ssh), Backend::Osc52);
        assert_eq!(select_backend(Provider::System, &ssh), Backend::System);
        assert_eq!(select_backend(Provider::Osc52, &local), Backend::Osc52);
    }

    #[test]
    fn provider_parses_setting_values_and_defaults_to_auto() {
        assert_eq!(Provider::parse("auto"), Provider::Auto);
        assert_eq!(Provider::parse("system"), Provider::System);
        assert_eq!(Provider::parse(" OSC52 "), Provider::Osc52);
        assert_eq!(Provider::parse("bogus"), Provider::Auto);
        assert_eq!(Provider::parse(""), Provider::Auto);
        for p in [Provider::Auto, Provider::System, Provider::Osc52] {
            assert_eq!(Provider::from_tag(p.tag()), p);
        }
    }
}
