//! The **authority** slot (T710): one trait that owns the workspace side
//! effects Vix performs — reading, writing, stat-ing, listing, renaming and
//! removing files, walking a project, and spawning processes.
//!
//! Today the only production implementation is [`LocalAuthority`], which does
//! exactly what Vix always did (it delegates to `vix-fileops`' guarded and
//! atomic helpers and to `std`). The point of the trait is that a remote
//! authority (SSH, T712; a remote agent, T717; containers, Kubernetes and
//! WSL, T718) can be installed in the same slot without touching the call
//! sites.
//!
//! The slot is process-wide: [`current`] returns the installed authority (the
//! local one until something calls [`install`]). Configuration, themes,
//! session, undo history and other per-user state deliberately do **not** go
//! through it — they always live on the machine running the UI. See
//! `crates/vix-authority/spec/index.md` for the full list.
//!
//! # Layout
//!
//! - [`types`](crate::FileStat): owned data the trait returns ([`FileStat`],
//!   [`DirEntry`], [`WalkOptions`]).
//! - [`ProcessSpec`]: a process described as data, turned into a
//!   [`std::process::Command`] by [`Authority::command`].
//! - [`LocalAuthority`]: the default; [`FakeAuthority`]: a temp-dir-backed
//!   stand-in for tests.
//! - [`ssh`], [`agent`] and [`container`]: the remote authorities (SSH, the
//!   `vix --remote-agent` protocol, and `docker`/`podman`/`kubectl`/`wsl`
//!   exec). [`remote`] holds what they share; [`conformance`] is the test
//!   suite every implementation must pass.
//!
//! Cross-cutting design: `spec/remote-editing/index.md` at the repo root.
//!

pub mod agent;
pub mod conformance;
pub mod container;
mod fake;
mod local;
mod process;
pub mod remote;
pub mod ssh;
mod types;

pub use fake::{Call, FakeAuthority};
pub use local::LocalAuthority;
pub use process::{Arg, ProcessSpec};
pub use types::{DirEntry, FileKind, FileStat, WalkOptions};
pub use vix_fileops::{HARD_MAX_TEXT_BYTES, MAX_TEXT_BYTES, TextFile, TextReadError, TextStream};

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

    /// Whether this authority's paths are this machine's own and reading a
    /// file is a cheap local call (T715). `false` (the default, so a new
    /// authority is treated as remote) makes the App's project-wide searches
    /// run a search tool (`rg`, else `grep`) through [`Authority::command`]
    /// instead of reading every indexed file one round trip at a time.
    fn is_local(&self) -> bool {
        false
    }

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

    /// [`Authority::read_text_guarded`], handing plain UTF-8 text to `sink`
    /// block by block instead of returning it whole, so opening a large file
    /// needs no file-sized copy besides the buffer built from it
    /// (`vix_fileops::stream_text_guarded`, T749). The default reads the
    /// whole file and returns [`TextStream::Whole`], which every caller
    /// handles; the local authority streams.
    ///
    /// # Errors
    /// As [`Authority::read_text_guarded`].
    fn stream_text_guarded(
        &self,
        path: &Path,
        max_bytes: u64,
        sink: &mut dyn FnMut(&str),
    ) -> io::Result<TextStream> {
        let _ = sink;
        self.read_text_guarded(path, max_bytes)
            .map(TextStream::Whole)
    }

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

    /// [`Authority::write_atomic`], but only if the file is still the version
    /// the caller last saw (T712): `expected` is its stat then, or `None` if
    /// it did not exist. A file that changed length or modification time
    /// (compared in whole seconds, the precision a remote `stat` reports),
    /// appeared, or vanished is left alone and the call fails with a
    /// [`WriteConflict`]. A remote authority checks and writes in one
    /// round trip; the default checks with [`Authority::metadata`] first.
    ///
    /// # Errors
    /// A [`WriteConflict`] (see [`WriteConflict::is`]), or as
    /// [`Authority::write_atomic`].
    fn write_atomic_if_unchanged(
        &self,
        path: &Path,
        data: &[u8],
        expected: Option<&FileStat>,
    ) -> io::Result<()> {
        let now = self.metadata(path).ok();
        if !same_version(expected, now.as_ref()) {
            return Err(WriteConflict::error());
        }
        self.write_atomic(path, data)
    }

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

    /// [`Authority::metadata`] of every path, in order (T717). A remote
    /// authority answers them all in one round trip (the agent's batch, or
    /// one shell script) instead of one per path; the default asks one at a
    /// time. The App's once-a-second check for files changed on disk uses
    /// it.
    fn metadata_batch(&self, paths: &[&Path]) -> Vec<io::Result<FileStat>> {
        paths.iter().map(|p| self.metadata(p)).collect()
    }

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

    /// Whether [`Authority::command`] runs on a POSIX system, so a shell
    /// script for it is `sh -c` rather than Windows' `cmd /C` (T716). The
    /// default is this machine's platform; the SSH authority's remote is
    /// always POSIX, even from a Windows client.
    fn shell_is_posix(&self) -> bool {
        !cfg!(windows)
    }

    /// The process that opens an interactive shell for the integrated
    /// terminal (T716). It runs **on this machine**, on a local
    /// pseudo-terminal: for the local authority it is `shell` itself in
    /// `cwd`; for a remote one it is a client (`ssh -t`) whose far end
    /// starts the remote user's login shell in `cwd` (an authority path).
    /// `cwd` of `None` means the home directory on the authority's side. The
    /// returned spec's own working directory, when set, is a local one.
    ///
    /// The default serves a local authority only (a remote one has nothing
    /// local to put on a PTY), so any other authority must override it.
    ///
    /// # Errors
    /// `Unsupported` when the authority has no interactive shell to offer,
    /// or the spec can't be built (an invalid path).
    fn terminal(&self, shell: &str, cwd: Option<&Path>) -> io::Result<ProcessSpec> {
        if !self.is_local() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this workspace has no interactive shell",
            ));
        }
        let spec = ProcessSpec::new(shell);
        Ok(match cwd {
            Some(dir) => spec.current_dir(dir),
            None => spec,
        })
    }
}

/// The refusal inside the `io::Error` from
/// [`Authority::write_atomic_if_unchanged`]: the file changed since the
/// caller read it.
///
/// It travels as the *source* of an `io::Error` (kind `Other`) so the trait
/// keeps returning plain `io::Result`; test for it with [`WriteConflict::is`]
/// rather than matching on the error kind.
///
/// # Examples
/// ```
/// use vix_authority::WriteConflict;
/// let e = WriteConflict::error();
/// assert!(WriteConflict::is(&e));
/// assert!(!WriteConflict::is(&std::io::Error::other("something else")));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WriteConflict;

impl std::fmt::Display for WriteConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the file changed since it was read")
    }
}

impl std::error::Error for WriteConflict {}

impl WriteConflict {
    /// The `io::Error` (kind `Other`) carrying a conflict.
    #[must_use]
    pub fn error() -> io::Error {
        io::Error::other(Self)
    }

    /// Whether `e` reports a conflict.
    #[must_use]
    pub fn is(e: &io::Error) -> bool {
        e.get_ref()
            .is_some_and(<dyn std::error::Error + Send + Sync>::is::<Self>)
    }
}

/// Whether `program` can be started under `auth` (T716): an executable on
/// its `PATH` ([`Authority::which`], resolved on the authority's side), or,
/// for a name with a `/`, a file at that path. A remote language server or
/// debug adapter is looked up this way before it is spawned, so a missing
/// one fails to start as it would locally rather than as an `ssh` that
/// starts and then exits ("not found" looks like a crash). One round trip
/// over SSH.
#[must_use]
pub fn can_run(auth: &dyn Authority, program: &str) -> bool {
    if program.contains('/') {
        auth.is_file(Path::new(program))
    } else {
        auth.which(program).is_some()
    }
}

/// Whether a file is still the version `expected` describes: both missing,
/// or both present with the same length and the same modification time in
/// whole seconds (a missing time on either side compares equal).
///
/// Whole seconds because that is all a remote `stat` reports; comparing
/// sub-second parts would make every remote save look like a conflict.
///
/// # Examples
/// ```
/// use vix_authority::{same_version, FileKind, FileStat};
/// let a = FileStat::new(FileKind::File, 10);
/// assert!(same_version(Some(&a), Some(&a)));
/// assert!(same_version(None, None));
/// assert!(!same_version(Some(&a), None));
/// assert!(!same_version(Some(&a), Some(&FileStat::new(FileKind::File, 11))));
/// ```
#[must_use]
pub fn same_version(expected: Option<&FileStat>, now: Option<&FileStat>) -> bool {
    match (expected, now) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let secs = |s: &FileStat| {
                s.modified
                    .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
            };
            a.len == b.len
                && match (secs(a), secs(b)) {
                    (Some(x), Some(y)) => x == y,
                    _ => true,
                }
        }
        _ => false,
    }
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

/// The installed authority, if [`install`] was ever called. `None` means
/// "use [`local()`]". A `RwLock` because reads (every file operation) vastly
/// outnumber the one or two installs per process.
static SLOT: RwLock<Option<Arc<dyn Authority>>> = RwLock::new(None);

/// The shared local authority returned while the slot is empty. One lazily
/// created instance, so repeated [`current`] calls hand out the same `Arc`.
fn local() -> Arc<dyn Authority> {
    static LOCAL: OnceLock<Arc<dyn Authority>> = OnceLock::new();
    Arc::clone(LOCAL.get_or_init(|| Arc::new(LocalAuthority)))
}

/// The authority every routed call site uses: whatever [`install`] put in the
/// slot, or [`LocalAuthority`] if nothing has.
#[must_use]
pub fn current() -> Arc<dyn Authority> {
    // A poisoned lock still holds a valid `Option<Arc<..>>`: a panic elsewhere
    // cannot leave it half-written, so recover rather than propagate.
    let installed = SLOT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(Arc::clone);
    installed.unwrap_or_else(local)
}

/// Put `authority` in the process-wide slot and return the one it replaces
/// (the local authority if the slot was empty). The App calls this when a
/// remote workspace is opened (`vix ssh://…`, **File → Open Remote…**, the OC
/// menu) and to put the previous authority back afterwards; see
/// `spec/remote-editing/index.md`.
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
        assert!(current().is_local());
    }

    #[test]
    fn the_terminal_runs_the_local_shell_or_maps_the_fake_directory() {
        let local = LocalAuthority
            .terminal("/bin/zsh", Some(Path::new("/w")))
            .unwrap();
        assert_eq!(local.program(), "/bin/zsh");
        assert_eq!(local.get_args().len(), 0);
        assert_eq!(local.cwd(), Some(Path::new("/w")));
        assert_eq!(LocalAuthority.shell_is_posix(), !cfg!(windows));

        let dir = std::env::temp_dir().join(format!("vix-authority-term-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("proj")).unwrap();
        let fake = FakeAuthority::new(&dir).unwrap();
        let spec = fake.terminal("sh", Some(Path::new("/proj"))).unwrap();
        assert_eq!(spec.cwd(), Some(fake.backing().join("proj").as_path()));
        assert_eq!(
            fake.terminal("sh", None).unwrap().cwd(),
            Some(fake.backing())
        );
        assert!(fake.terminal("sh", Some(Path::new("/../x"))).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
