//! Persistent undo: save a buffer's undo tree to disk and restore it on reopen.
//!
//! Each saved file gets a small JSON file under `<config>/undo/` named by a hash
//! of its absolute path, holding the serialized [`History`] plus a hash of the
//! file content it corresponds to. On open, the history is restored **only** when
//! the stored content hash matches the file's current content, so undo is never
//! replayed onto text it doesn't match.

#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use vix_editor_core::history::History;

/// The directory where per-file undo histories live (`<config>/undo/`).
fn undo_dir() -> Option<PathBuf> {
    Some(
        vix_settings::Settings::config_path()?
            .parent()?
            .join("undo"),
    )
}

/// Hex SHA-256 of `s`.
fn sha(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// The store path for `file` (named by a hash of its absolute path).
fn store_path(file: &Path) -> Option<PathBuf> {
    let canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    Some(undo_dir()?.join(format!("{}.json", sha(&canon.to_string_lossy()))))
}

/// Borrowed form written to disk (avoids cloning the history).
#[derive(serde::Serialize)]
struct StoredRef<'a> {
    hash: &'a str,
    history: &'a History,
}

/// Owned form read from disk.
#[derive(serde::Deserialize)]
struct Stored {
    hash: String,
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
        let _ = vix_fileops::write_atomic(&path, json.as_bytes());
    }
}

/// Load the saved undo history for `file`, but only if its stored content hash
/// matches `content` (otherwise the file changed and the history can't be safely
/// applied). `None` when absent, unreadable, or mismatched.
#[must_use]
pub fn load(file: &Path, content: &str) -> Option<History> {
    let text = std::fs::read_to_string(store_path(file)?).ok()?;
    let stored: Stored = serde_json::from_str(&text).ok()?;
    (stored.hash == sha(content)).then_some(stored.history)
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
