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
//!
//! Owning spec: `crates/vix-undo-store/spec/index.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sha2::{Digest as _, Sha256};

use crate::sha;

/// [`sha`] of the text the pieces join to, without joining them.
fn sha_chunks<'t>(chunks: impl Iterator<Item = &'t str>) -> String {
    use std::fmt::Write as _;
    let mut h = Sha256::new();
    for chunk in chunks {
        h.update(chunk.as_bytes());
    }
    h.finalize().iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

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

/// The file's JSON shape, borrowed: what [`BackupStore::write_chunks`]
/// writes by hand, kept so a test checks the two agree byte for byte.
#[cfg(test)]
#[derive(serde::Serialize)]
struct StoredRef<'a> {
    /// Which buffer this backup is for (path, `ssh://` form, or `untitled:<id>`).
    identity: &'a str,
    /// SHA-256 of `text`, verified on read to reject torn/edited files.
    hash: &'a str,
    /// The unsaved buffer text.
    text: &'a str,
}

/// Owned form read from disk.
#[derive(serde::Deserialize)]
struct Stored {
    /// See [`StoredRef::identity`].
    identity: String,
    /// See [`StoredRef::hash`].
    hash: String,
    /// See [`StoredRef::text`].
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
    // Distinguishes ids minted in the same millisecond by this process; the
    // clock and pid separate runs and concurrent editor instances.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("{ms:x}p{:x}n{n:x}", std::process::id())
}

/// Whether `id` could have come from [`new_session_id`]. Guards
/// [`BackupStore::untitled_ids`] against stray or hostile file names in the
/// directory leaking into a path.
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
    /// Directory holding the backup JSON files (created on first write).
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
        self.write_chunks(key, std::iter::once(text), &AtomicBool::new(false))
    }

    /// [`BackupStore::write`] for text held in pieces, such as a rope's
    /// chunks, without joining them (T859: a worker thread streams a large
    /// buffer's backup from a rope snapshot). `chunks` is walked twice, once
    /// for the hash and once to write, so it is cloned. The file's bytes are
    /// exactly those [`BackupStore::write`] writes for the joined text, with
    /// the same atomic, owner-only write.
    ///
    /// `cancel` is checked before the write and between chunks: once it is
    /// set, the write stops, the earlier backup (if any) stays as it was,
    /// and the result is an `Interrupted` error. A newer backup of the same
    /// buffer cancels an older one still being written this way.
    ///
    /// # Errors
    /// `Interrupted` when cancelled; any I/O error creating the directory or
    /// writing the file.
    pub fn write_chunks<'t, I>(
        &self,
        key: BackupKey<'_>,
        chunks: I,
        cancel: &AtomicBool,
    ) -> io::Result<()>
    where
        I: Iterator<Item = &'t str> + Clone,
    {
        let cancelled = || io::Error::new(io::ErrorKind::Interrupted, "backup superseded");
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        std::fs::create_dir_all(&self.root)?;
        let identity = match key {
            BackupKey::File(identity) => identity.to_owned(),
            BackupKey::Untitled { id, .. } => format!("untitled:{id}"),
        };
        let hash = sha_chunks(chunks.clone());
        // The same bytes as `serde_json::to_string(&StoredRef { .. })`: the
        // fields in declaration order, then the text escaped one chunk at a
        // time (JSON escapes characters one by one, and a chunk never splits
        // a character, so the pieces join to the whole text's escaping).
        let head = format!(
            "{{\"identity\":{},\"hash\":{},\"text\":\"",
            serde_json::to_string(&identity).map_err(io::Error::other)?,
            serde_json::to_string(&hash).map_err(io::Error::other)?,
        );
        // Owned here: the fallback write may call the closure a second time.
        let source = chunks;
        vix_fileops::write_atomic_private_with(&self.path_of(key), |w| {
            w.write_all(head.as_bytes())?;
            let mut escaped = Vec::new();
            for chunk in source.clone() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(cancelled());
                }
                escaped.clear();
                serde_json::to_writer(&mut escaped, chunk).map_err(io::Error::other)?;
                // Drop the quotes `to_writer` puts around one string.
                let inner = escaped
                    .get(1..escaped.len().saturating_sub(1))
                    .unwrap_or_default();
                w.write_all(inner)?;
            }
            w.write_all(b"\"}")
        })
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

    /// T859: the streamed write is byte for byte the old one-copy encoding,
    /// for text that needs escaping and splits into many pieces.
    #[test]
    fn chunked_writes_match_the_serialized_form() {
        let store = store("chunks");
        let text = "quote \" back \\ tab \t nl \n ctl \u{1} é 日本 \u{7f} end";
        let identity = "/tmp/a \"b\".txt";
        let expected = serde_json::to_string(&StoredRef {
            identity,
            hash: &sha(text),
            text,
        })
        .unwrap();
        let pieces: Vec<&str> = text
            .char_indices()
            .map(|(i, c)| &text[i..i + c.len_utf8()])
            .collect();
        let key = BackupKey::File(identity);
        store
            .write_chunks(key, pieces.iter().copied(), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(store.path_of(key)).unwrap(),
            expected
        );
        store.write(key, text).unwrap();
        assert_eq!(
            std::fs::read_to_string(store.path_of(key)).unwrap(),
            expected
        );
        assert_eq!(store.read(key).unwrap().text, text);
        assert_eq!(sha_chunks(pieces.iter().copied()), sha(text));
    }

    /// T859: a cancelled write leaves the earlier backup as it was.
    #[test]
    fn a_cancelled_chunked_write_keeps_the_earlier_backup() {
        let store = store("cancel");
        let key = BackupKey::File("/tmp/c.txt");
        store.write(key, "first").unwrap();
        let cancel = AtomicBool::new(false);
        let pieces = ["second", " and more"].into_iter().inspect(|_| {
            cancel.store(true, Ordering::Relaxed);
        });
        let err = store.write_chunks(key, pieces, &cancel).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(store.read(key).unwrap().text, "first");
        let err = store
            .write_chunks(key, std::iter::once("third"), &cancel)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted, "cancelled before");
        assert_eq!(store.read(key).unwrap().text, "first");
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
        s.write(
            BackupKey::Untitled {
                workspace: "/w1",
                id: &a,
            },
            "one",
        )
        .unwrap();
        s.write(
            BackupKey::Untitled {
                workspace: "/w2",
                id: &b,
            },
            "two",
        )
        .unwrap();
        s.write(BackupKey::File("/w1/x"), "file").unwrap();
        assert_eq!(s.untitled_ids("/w1"), vec![a.clone()]);
        assert_eq!(s.untitled_ids("/w2"), vec![b]);
        let got = s
            .read(BackupKey::Untitled {
                workspace: "/w1",
                id: &a,
            })
            .unwrap();
        assert_eq!(got.text, "one");
        assert_eq!(s.untitled_ids("/none"), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(s.root());
    }

    #[test]
    fn a_missing_directory_lists_nothing() {
        let s = store("missing");
        assert_eq!(s.untitled_ids("/w"), Vec::<String>::new());
        assert!(s.read(BackupKey::File("/w/a")).is_none());
    }
}
