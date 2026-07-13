//! Process-wide serialization of system-clipboard access.
//!
//! Platform clipboard backends — notably macOS's Cocoa `NSPasteboard` — are not
//! thread-safe: concurrent `arboard` calls corrupt memory and crash the process.
//! Every crate that touches the clipboard must go through [`set`] / [`get`] so
//! all access is sequential behind one shared lock. In the single-threaded app
//! the lock is uncontended; under parallel tests (or any future background
//! copy) it is what keeps the platform backend from being entered concurrently.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use anyhow::{Result, anyhow};

/// The process-wide clipboard lock. Held for the duration of each `arboard`
/// call so no two clipboard operations run at once, anywhere in the process.
static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Write `text` to the system clipboard, serialized behind the shared lock.
///
/// # Errors
/// Returns the backend error when the clipboard is unavailable.
pub fn set(text: &str) -> Result<()> {
    let _guard = CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| anyhow!(e.to_string()))
}

/// Read the system clipboard text, serialized behind the shared lock.
///
/// # Errors
/// Returns the backend error when the clipboard is unavailable or empty.
pub fn get() -> Result<String> {
    let _guard = CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|e| anyhow!(e.to_string()))
}
