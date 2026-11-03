//! Persistent undo: save a buffer's undo tree to disk and restore it on reopen.
//!
//! Each saved file gets a small JSON file under `<config>/undo/` named by a hash
//! of its absolute path, holding the serialized [`History`] plus a hash of the
//! file content it corresponds to. On open, the history is restored **only** when
//! the stored content hash matches the file's current content, so undo is never
//! replayed onto text it doesn't match.
//!
//! The [`local_history`] module adds timestamped whole-file snapshots on save
//! (Local History), with labels, diffs and range revert, and the
//! [`hot_exit`] module keeps backups of unsaved buffers for crash recovery.
//!
//! All three stores share conventions: they live under the user's config
//! directory, are keyed by a SHA-256 of a path/identity, write atomically and
//! owner-only (they may hold text no longer in the buffer), and treat every
//! failure as non-fatal -- persistence is a convenience, never a blocker.
//!
//! Owning spec: `crates/vix-undo-store/spec/index.md`.

pub mod hot_exit;
pub mod local_history;

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use vix_editor_core::history::History;

/// The directory where per-file undo histories live (`<config>/undo/`).
/// `None` when the platform has no config directory (persistence is skipped).
fn undo_dir() -> Option<PathBuf> {
    Some(
        vix_settings::Settings::config_path()?
            .parent()?
            .join("undo"),
    )
}

/// Hex SHA-256 of `s`.
pub(crate) fn sha(s: &str) -> String {
    use std::fmt::Write as _;
    // Not a security boundary: the hash only names files and detects that
    // a file changed since its history was saved.
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    // digest 0.11's `Output` no longer implements `LowerHex` directly.
    h.finalize().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The store path for `file` (named by a hash of its absolute path).
/// Canonicalising first means a symlink and its target share one history; a
/// file that does not exist yet falls back to the path as given.
fn store_path(file: &Path) -> Option<PathBuf> {
    let canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    Some(undo_dir()?.join(format!("{}.json", sha(&canon.to_string_lossy()))))
}

/// Borrowed form written to disk (avoids cloning the history).
#[derive(serde::Serialize)]
struct StoredRef<'a> {
    /// SHA-256 of the file content the history applies to.
    hash: &'a str,
    /// The undo tree.
    history: &'a History,
}

/// Owned form read from disk.
#[derive(serde::Deserialize)]
struct Stored {
    /// SHA-256 of the file content the history applies to.
    hash: String,
    /// The undo tree.
    history: History,
}

/// Persist `history` for `file`, tagged with a hash of `content`. Best-effort:
/// failures are silently ignored (undo persistence is non-essential).
pub fn save(file: &Path, content: &str, history: &History) {
    let Some(path) = store_path(file) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let hash = sha(content);
    if let Ok(json) = serde_json::to_string(&StoredRef {
        hash: &hash,
        history,
    }) {
        // Atomic write: two editor instances share the same hash-named store, so
        // a plain truncating write could let a reader observe a half-written file
        // (load then silently drops the history). Write-then-rename avoids the tear.
        // `_private`: a full undo tree can carry text that no longer appears in
        // the current buffer at all (T133), so a brand-new store file is
        // created owner-only rather than at the process's default mode.
        let _ = vix_fileops::write_atomic_private(&path, json.as_bytes());
    }
}

/// Load the saved undo history for `file`, but only if its stored content hash
/// matches `content` (otherwise the file changed and the history can't be safely
/// applied). `None` when absent, unreadable, or mismatched.
#[must_use]
pub fn load(file: &Path, content: &str) -> Option<History> {
    load_with(file, || content.to_owned())
}

/// [`load`], asking for the buffer's text only when a saved history exists
/// to compare it with. Opening a file that was never edited with
/// persistent undo on — the common case — then costs no copy of its text
/// (on a 100 MB file, 100 MB, T749).
#[must_use]
pub fn load_with(file: &Path, content: impl FnOnce() -> String) -> Option<History> {
    let text = std::fs::read_to_string(store_path(file)?).ok()?;
    let stored: Stored = serde_json::from_str(&text).ok()?;
    (stored.hash == sha(&content())).then_some(stored.history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_is_stable_and_distinct() {
        assert_eq!(sha("abc"), sha("abc"));
        assert_ne!(sha("abc"), sha("abd"));
    }
}
