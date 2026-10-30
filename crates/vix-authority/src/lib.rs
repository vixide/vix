//! The **authority** slot (T710): one trait that owns the workspace side
//! effects Vix performs — reading, writing, stat-ing, listing, renaming and
//! removing files, walking a project, and spawning processes.
//!
//! Today the only production implementation is [`LocalAuthority`], which does
//! exactly what Vix always did (it delegates to `vix-fileops`' guarded and
//! atomic helpers and to `std`). The point of the trait is that a remote
//! authority (SSH, T712; a remote agent, T717; containers, T718) can be
//! installed in the same slot later without touching the call sites.
//!
//! The slot is process-wide: [`current`] returns the installed authority (the
//! local one until something calls [`install`]). Configuration, themes,
//! session, undo history and other per-user state deliberately do **not** go
//! through it — they always live on the machine running the UI. See
//! `crates/vix-authority/spec/index.md` for the full list.
//!

pub mod conformance;
mod fake;
mod local;
mod process;
mod types;

pub use fake::{Call, FakeAuthority};
pub use local::LocalAuthority;
pub use process::{Arg, ProcessSpec};
pub use types::{DirEntry, FileKind, FileStat, WalkOptions};
pub use vix_fileops::{HARD_MAX_TEXT_BYTES, MAX_TEXT_BYTES, TextFile, TextReadError};

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

/// Everything Vix does to a workspace's files and processes.
///
/// Every method mirrors one `std::fs`/`vix-fileops` operation the App used to
/// call directly, with the same semantics and the same errors, so routing a
/// call site through an authority is not a behavior change. Paths are the
/// authority's own: for [`LocalAuthority`] they are ordinary local paths.
pub trait Authority: Send + Sync + std::fmt::Debug {
    /// A short, stable, untranslated identifier (`"local"`, `"fake"`, …) for
    /// logs and tests. Never shown to the user.
    fn name(&self) -> &'static str;

    /// Read the whole file (`std::fs::read`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Read at most `limit` bytes from the start of the file, bounding memory
    /// even if the file is huge or grows while being read (`File::open` +
    /// `take(limit)`). A result of exactly `limit` bytes means "at least".
    ///
    /// # Errors
    /// The underlying I/O error.
    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>>;

    /// Read the whole file as UTF-8 (`std::fs::read_to_string`).
    ///
    /// # Errors
    /// The underlying I/O error, or `InvalidData` for non-UTF-8 content.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Read a text file through the T571 guards: refuse a non-regular,
    /// oversized (over `max_bytes`) or binary file, and detect the encoding
    /// (`vix_fileops::read_text_guarded`).
    ///
    /// # Errors
    /// An `InvalidData` error wrapping a [`TextReadError`], or the I/O error.
    fn read_text_guarded(&self, path: &Path, max_bytes: u64) -> io::Result<TextFile>;

    /// [`Authority::read_text_guarded`] at the default [`MAX_TEXT_BYTES`],
    /// returning only the text (`vix_fileops::read_to_string_guarded`).
    ///
    /// # Errors
    /// As [`Authority::read_text_guarded`].
    fn read_to_string_guarded(&self, path: &Path) -> io::Result<String> {
        self.read_text_guarded(path, MAX_TEXT_BYTES).map(|f| f.text)
    }

    /// Plain, truncating write (`std::fs::write`), for the few places that
    /// never used the atomic writer.
    ///
    /// # Errors
    /// The underlying I/O error.
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;

    /// Crash-safe write: temp file, flush, rename over the target, writing
    /// through a symlink and keeping an existing file's mode
    /// (`vix_fileops::write_atomic`).
    ///
    /// # Errors
    /// When both the atomic write and its in-place fallback fail.
    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()>;

    /// Atomically replace a text file's contents in the encoding it already
    /// has (`vix_fileops::write_text_in_file_encoding`).
    ///
    /// # Errors
    /// `InvalidData` for a lossy existing file or unmappable text, or the I/O
    /// error.
    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> io::Result<()>;

    /// Metadata, following symlinks (`std::fs::metadata`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn metadata(&self, path: &Path) -> io::Result<FileStat>;

    /// Metadata of the link itself (`std::fs::symlink_metadata`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileStat>;

    /// Whether `path` exists (following symlinks), like `Path::exists`.
    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    /// Whether `path` is a directory (following symlinks), like `Path::is_dir`.
    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).is_ok_and(|m| m.is_dir())
    }

    /// Whether `path` is a regular file (following symlinks), like
    /// `Path::is_file`.
    fn is_file(&self, path: &Path) -> bool {
        self.metadata(path).is_ok_and(|m| m.is_file())
    }

    /// The canonical, absolute form of `path` (`std::fs::canonicalize`).
    ///
    /// # Errors
    /// The underlying I/O error (for example, `path` does not exist).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// The entries directly inside `dir`, in the order the platform returns
    /// them; an entry that errors while being listed is skipped, as
    /// `read_dir(..).flatten()` did.
    ///
    /// # Errors
    /// When `dir` itself can't be listed.
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>>;

    /// Create `dir` and every missing parent (`std::fs::create_dir_all`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn create_dir_all(&self, dir: &Path) -> io::Result<()>;

    /// Rename `from` to `to` (`std::fs::rename`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Remove one file (`std::fs::remove_file`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Remove a file, a symlink (never its target) or a whole directory tree
    /// (`vix_fileops::remove_path`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn remove_path(&self, path: &Path) -> io::Result<()>;

    /// Move a file or tree, falling back to copy-then-remove across devices
    /// (`vix_fileops::move_path`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Copy a file or tree, copying symlinks as links
    /// (`vix_fileops::copy_recursive`).
    ///
    /// # Errors
    /// The underlying I/O error.
    fn copy_recursive(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Move `path` to the trash, undoably (`vix_fileops::trash_path`).
    ///
    /// # Errors
    /// When the platform's trash rejects the move.
    fn trash(&self, path: &Path) -> io::Result<()>;

    /// Whether `path` exists but can't be written by this user
    /// (`vix_fileops::is_unwritable`).
    fn is_unwritable(&self, path: &Path) -> bool;

    /// The regular files under `root`, walked as [`WalkOptions`] says (the
    /// fuzzy file finder's index). Unreadable entries are skipped.
    fn walk_files(&self, root: &Path, opts: &WalkOptions) -> Vec<PathBuf>;

    /// Resolve `name` against the authority's `PATH`, never its current
    /// directory (`vix_fileops::which_on_path`).
    fn which(&self, name: &str) -> Option<PathBuf>;

    /// Build a [`Command`] that runs `spec` under this authority. The caller
    /// still chooses the stdio and spawns it, exactly as before; for the local
    /// authority the result is the `Command` the call site used to build
    /// itself.
    ///
    /// # Errors
    /// `Unsupported` in a build without the `process` feature, or when the
    /// authority can't run processes at all (or, for the fake, when the
    /// working directory escapes its namespace).
    fn command(&self, spec: &ProcessSpec) -> io::Result<Command>;
}

/// The error [`Authority::command`] returns in a build without the `process`
/// feature: that build never starts a process (T595).
#[cfg(not(feature = "process"))]
pub(crate) fn process_disabled() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this build cannot start processes",
    )
}

/// The installed authority, if [`install`] was ever called.
static SLOT: RwLock<Option<Arc<dyn Authority>>> = RwLock::new(None);

/// The shared local authority returned while the slot is empty.
fn local() -> Arc<dyn Authority> {
    static LOCAL: OnceLock<Arc<dyn Authority>> = OnceLock::new();
    Arc::clone(LOCAL.get_or_init(|| Arc::new(LocalAuthority)))
}

/// The authority every routed call site uses: whatever [`install`] put in the
/// slot, or [`LocalAuthority`] if nothing has.
#[must_use]
pub fn current() -> Arc<dyn Authority> {
    let installed = SLOT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(Arc::clone);
    installed.unwrap_or_else(local)
}

/// Put `authority` in the process-wide slot and return the one it replaces
/// (the local authority if the slot was empty). Nothing in Vix calls this yet;
/// it is the hook T712's remote authorities plug into.
pub fn install(authority: Arc<dyn Authority>) -> Arc<dyn Authority> {
    let mut slot = SLOT.write().unwrap_or_else(PoisonError::into_inner);
    slot.replace(authority).unwrap_or_else(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slot_defaults_to_local() {
        assert_eq!(current().name(), "local");
    }
}
