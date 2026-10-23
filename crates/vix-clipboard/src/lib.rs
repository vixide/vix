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

use anyhow::{Result, anyhow};
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The process-wide clipboard lock. Held for the duration of each `arboard`
/// call so no two clipboard operations run at once, anywhere in the process.
static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

/// Whether [`set`] / [`get`] reach the platform clipboard. Off until
/// [`use_system`] turns it on.
static SYSTEM: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The in-memory clipboard used until [`use_system`] is called. Per thread,
    /// not process-wide: parallel tests each get their own, so one test's copy
    /// can never be pasted by another (T596). The app only touches the
    /// clipboard from its UI thread, so nothing real is split by this.
    static MEMORY: RefCell<Option<String>> = const { RefCell::new(None) };
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

/// Write `text` to the clipboard, serialized behind the shared lock.
///
/// # Errors
/// Returns the backend error when the platform clipboard is unavailable. The
/// in-memory clipboard never fails.
pub fn set(text: &str) -> Result<()> {
    let _guard = lock(&CLIPBOARD_LOCK);
    if !is_system() {
        MEMORY.with_borrow_mut(|m| *m = Some(text.to_string()));
        return Ok(());
    }
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        // T542 (Run H): keep the real `arboard::Error` as the source instead
        // of collapsing it to a plain string via `anyhow!(e.to_string())` --
        // a future caller that wants to distinguish e.g. `ClipboardOccupied`
        // (transient, worth retrying) from `ClipboardNotSupported` can
        // `downcast_ref::<arboard::Error>()`; today's callers still just
        // `Display` it, unaffected either way.
        .map_err(anyhow::Error::from)
}

/// Read the clipboard text, serialized behind the shared lock.
///
/// # Errors
/// Returns the backend error when the platform clipboard is unavailable, or an
/// error when the in-memory clipboard is empty.
pub fn get() -> Result<String> {
    let _guard = lock(&CLIPBOARD_LOCK);
    if !is_system() {
        return MEMORY
            .with_borrow(Clone::clone)
            .ok_or_else(|| anyhow!("clipboard is empty"));
    }
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        // See the matching comment in `set` (T542, Run H).
        .map_err(anyhow::Error::from)
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
}
