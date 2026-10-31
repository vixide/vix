//! Hot exit: backups of unsaved buffers, so a crash or a forced quit never
//! loses typed text (T629).
//!
//! A [`BackupStore`] is a directory (`<config>/backup/` for a real run) of
//! small JSON files, one per dirty buffer:
//!
//! - a buffer with a file is named by the hash of its *identity* — the
//!   canonical local path, or the `ssh://host/path` form of a remote one, so
//!   two hosts' `/etc/hosts` never share a backup — exactly the path-hash
//!   naming the persistent undo store uses;
//! - an untitled buffer is named `untitled-<workspace hash>-<session id>`,
//!   its stable session id ([`new_session_id`]) kept on the tab, so the next
//!   backup of the same buffer replaces its file and a workspace only offers
//!   back its own untitled buffers.
//!
//! Each file holds the identity, the text, and the text's SHA-256 (the same
//! content hash the undo store tags histories with); [`BackupStore::read`]
//! rejects a file whose hash does not match, so a torn or hand-edited backup
//! is never offered. Nothing here applies a backup: the host only ever
//! offers it, shown as a diff against the file.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sha;

/// Prefix of an untitled buffer's backup file name.
const UNTITLED_PREFIX: &str = "untitled-";

/// Which buffer a backup belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKey<'a> {
    /// A buffer with a file: its identity (see [`local_identity`]).
    File(&'a str),
    /// An untitled buffer of `workspace`, by its session id.
    Untitled {
        /// The workspace's session key.
        workspace: &'a str,
        /// The buffer's session id ([`new_session_id`]).
        id: &'a str,
    },
}

/// A backup read back from the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The identity it was written under (a path, or `untitled:<id>`).
    pub identity: String,
    /// The unsaved text.
    pub text: String,
}

/// Borrowed form written to disk (avoids copying the text).
#[derive(serde::Serialize)]
struct StoredRef<'a> {
    identity: &'a str,
    hash: &'a str,
    text: &'a str,
}

/// Owned form read from disk.
#[derive(serde::Deserialize)]
struct Stored {
    identity: String,
    hash: String,
    text: String,
}

/// The identity of a local file: its canonical path (the path as given when
/// it cannot be canonicalized), the same key the undo store hashes.
#[must_use]
pub fn local_identity(file: &Path) -> String {
    let canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    canon.to_string_lossy().into_owned()
}

/// A fresh session id for an untitled buffer: unique within this process
/// and, through the clock and the process id, across runs. Only ASCII
/// alphanumerics, so it is safe in a file name.
#[must_use]
pub fn new_session_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("{ms:x}p{:x}n{n:x}", std::process::id())
}

/// Whether `id` could have come from [`new_session_id`].
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The first 16 hex digits of the workspace's hash (enough to keep
/// workspaces apart in one directory).
fn workspace_tag(workspace: &str) -> String {
    sha(workspace).chars().take(16).collect()
}

/// A hot-exit backup store rooted at a directory.
#[derive(Debug, Clone)]
pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    /// A store rooted at `root` (tests use a temp directory).
    #[must_use]
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The store under the user's config directory (`<config>/backup/`), or
    /// `None` when there is no config directory.
    #[must_use]
    pub fn default_location() -> Option<Self> {
        let dir = vix_settings::Settings::config_path()?
            .parent()?
            .join("backup");
        Some(Self::at(dir))
    }

    /// The directory this store keeps its backups in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The backup file for `key`.
    #[must_use]
    pub fn path_of(&self, key: BackupKey<'_>) -> PathBuf {
        let name = match key {
            BackupKey::File(identity) => format!("{}.json", sha(identity)),
            BackupKey::Untitled { workspace, id } => {
                format!("{UNTITLED_PREFIX}{}-{id}.json", workspace_tag(workspace))
            }
        };
        self.root.join(name)
    }

    /// Whether a backup exists for `key` (a stat, no read).
    #[must_use]
    pub fn exists(&self, key: BackupKey<'_>) -> bool {
        self.path_of(key).is_file()
    }

    /// Write `text` as the backup for `key`, replacing any earlier one. The
    /// write is atomic and a new file is owner-only (a backup is unsaved
    /// text, possibly secret).
    ///
    /// # Errors
    /// Any I/O error creating the directory or writing the file.
    pub fn write(&self, key: BackupKey<'_>, text: &str) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let identity = match key {
            BackupKey::File(identity) => identity.to_owned(),
            BackupKey::Untitled { id, .. } => format!("untitled:{id}"),
        };
        let hash = sha(text);
        let json = serde_json::to_string(&StoredRef {
            identity: &identity,
            hash: &hash,
            text,
        })
        .map_err(io::Error::other)?;
        vix_fileops::write_atomic_private(&self.path_of(key), json.as_bytes())
    }

    /// Read the backup for `key`: `None` when there is none, or when it is
    /// unreadable or its content hash does not match its text.
    #[must_use]
    pub fn read(&self, key: BackupKey<'_>) -> Option<Backup> {
        Self::read_file(&self.path_of(key))
    }

    /// Read and verify one backup file.
    fn read_file(path: &Path) -> Option<Backup> {
        let json = std::fs::read_to_string(path).ok()?;
        let stored: Stored = serde_json::from_str(&json).ok()?;
        (stored.hash == sha(&stored.text)).then_some(Backup {
            identity: stored.identity,
            text: stored.text,
        })
    }

    /// Delete the backup for `key`. Best-effort: a missing file is fine.
    pub fn remove(&self, key: BackupKey<'_>) {
        let _ = std::fs::remove_file(self.path_of(key));
    }

    /// The session ids of `workspace`'s untitled backups, sorted (oldest
    /// first, since ids start with the creation time).
    #[must_use]
    pub fn untitled_ids(&self, workspace: &str) -> Vec<String> {
        let prefix = format!("{UNTITLED_PREFIX}{}-", workspace_tag(workspace));
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name.strip_prefix(&prefix)?.strip_suffix(".json")?;
                valid_id(id).then(|| id.to_owned())
            })
            .collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> BackupStore {
        let dir = std::env::temp_dir().join(format!("vix-hot-exit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        BackupStore::at(dir)
    }

    #[test]
    fn a_file_backup_round_trips_and_is_removed() {
        let s = store("file");
        let key = BackupKey::File("/w/a.txt");
        assert!(!s.exists(key));
        s.write(key, "hello\n").unwrap();
        assert!(s.exists(key));
        let b = s.read(key).unwrap();
        assert_eq!(b.text, "hello\n");
        assert_eq!(b.identity, "/w/a.txt");
        s.write(key, "again").unwrap();
        assert_eq!(s.read(key).unwrap().text, "again");
        s.remove(key);
        assert!(s.read(key).is_none());
        let _ = std::fs::remove_dir_all(s.root());
    }

    #[test]
    fn identities_with_the_same_path_on_different_hosts_differ() {
        let s = store("hosts");
        let local = s.path_of(BackupKey::File("/etc/hosts"));
        let remote = s.path_of(BackupKey::File("ssh://box/etc/hosts"));
        assert_ne!(local, remote);
    }

    #[test]
    fn a_tampered_backup_is_rejected() {
        let s = store("tamper");
        let key = BackupKey::File("/w/b.txt");
        s.write(key, "real").unwrap();
        let path = s.path_of(key);
        let json = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, json.replace("real", "fake")).unwrap();
        assert!(s.read(key).is_none());
        let _ = std::fs::remove_dir_all(s.root());
    }

    #[test]
    fn untitled_backups_are_listed_per_workspace() {
        let s = store("untitled");
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b);
        assert!(valid_id(&a));
        s.write(BackupKey::Untitled { workspace: "/w1", id: &a }, "one").unwrap();
        s.write(BackupKey::Untitled { workspace: "/w2", id: &b }, "two").unwrap();
        s.write(BackupKey::File("/w1/x"), "file").unwrap();
        assert_eq!(s.untitled_ids("/w1"), vec![a.clone()]);
        assert_eq!(s.untitled_ids("/w2"), vec![b]);
        let got = s
            .read(BackupKey::Untitled { workspace: "/w1", id: &a })
            .unwrap();
        assert_eq!(got.text, "one");
        assert!(s.untitled_ids("/none").is_empty());
        let _ = std::fs::remove_dir_all(s.root());
    }

    #[test]
    fn a_missing_directory_lists_nothing() {
        let s = store("missing");
        assert!(s.untitled_ids("/w").is_empty());
        assert!(s.read(BackupKey::File("/w/a")).is_none());
    }
}
