//! Filesystem helpers for the explorer's copy / cut / paste / delete, plus a
//! crash-safe atomic writer used across the app for saving files.

pub mod dir_edit;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic counter making temp-file names unique within a process without a
/// clock or RNG (both of which are unavailable in some harness contexts).
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Recursively copy a file or directory tree from `tui/src` to `dst`.
///
/// Symlinks are **copied as links**, never dereferenced: without this, a symlink
/// inside a copied directory would cause the target's contents to be duplicated
/// into the destination (data exfiltration when copying an attacker-supplied
/// tree). Uses `symlink_metadata` so the type check itself doesn't follow links.
///
/// # Errors
///
/// Returns an error if any read, directory creation, or copy fails.
pub fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    let ty = meta.file_type();
    if ty.is_symlink() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        let target = fs::read_link(src)?;
        return symlink_portable(&target, dst);
    }
    if ty.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst).map(|_| ())
    }
}

/// Create a symlink at `link` pointing to `target`, on any platform.
#[cfg(unix)]
fn symlink_portable(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Create a symlink at `link` pointing to `target`, on any platform.
#[cfg(windows)]
fn symlink_portable(target: &Path, link: &Path) -> io::Result<()> {
    // Choose the right Windows symlink kind based on the target.
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// Create a symlink at `link` pointing to `target`, on any platform.
#[cfg(not(any(unix, windows)))]
fn symlink_portable(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlinks are not supported on this platform",
    ))
}

/// Move `tui/src` to `dst`, falling back to copy+remove across filesystems.
///
/// # Errors
///
/// Returns an error if the rename and the copy+remove fallback both fail.
pub fn move_path(src: &Path, dst: &Path) -> io::Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    copy_recursive(src, dst)?;
    remove_path(src)
}

/// Delete a file or directory tree.
///
/// A symlink is removed as a link (never followed), so deleting an entry that is
/// a symlink-to-directory cannot recurse into and destroy the link's target.
/// The type is read with `symlink_metadata` and, because a followed `is_dir()`
/// check would be a TOCTOU window, the decision is made from that single stat.
///
/// # Errors
///
/// Returns an error if the file or directory cannot be removed.
pub fn remove_path(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    }
}

/// Find `name` on `PATH` (honouring `PATHEXT` on Windows) and return its
/// absolute path. Never consults the current directory, so a repository that
/// ships its own `git.exe` cannot get it run just by being opened (Windows
/// resolves a bare program name against more than `PATH`).
#[must_use]
pub fn which_on_path(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE".into())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue; // an empty PATH entry means the cwd: never search it
        }
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Move a file or directory tree to the OS trash (Recycle Bin / Trash / etc.)
/// instead of deleting it outright (T209). Unlike [`remove_path`], this can
/// be undone by the user through their file manager's trash/recycle view.
///
/// # Errors
///
/// Returns an error if the platform's trash mechanism rejects the move (no
/// trash available, permission denied, the path doesn't exist, …).
pub fn trash_path(path: &Path) -> io::Result<()> {
    // On macOS the crate's default asks Finder through an `osascript` child
    // process; the `NSFileManager` call does the same job without starting one.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        let mut ctx = trash::TrashContext::default();
        ctx.set_delete_method(DeleteMethod::NsFileManager);
        ctx.delete(path).map_err(io::Error::other)
    }
    #[cfg(not(target_os = "macos"))]
    {
        trash::delete(path).map_err(io::Error::other)
    }
}

/// Atomically write `data` to `path`: write a sibling temp file, flush it to
/// disk, then rename it over the target. A crash or full disk mid-write leaves
/// the original intact rather than a truncated file, and the rename is atomic so
/// readers never observe a half-written file.
///
/// The target is canonicalized first so that when `path` is a symlink the write
/// goes *through* it (preserving an intentional symlink such as a dotfile)
/// rather than replacing the link with a regular file. The temp file is created
/// with `O_EXCL` (`create_new`), so a pre-planted symlink at the temp name
/// causes an error instead of a follow — closing the classic `/tmp`-style
/// temp-file hijack.
///
/// Permissions match a plain write: an existing file keeps its exact mode; a new
/// file gets the process umask default (`0666 & ~umask`), just as `fs::write`
/// would. Being a rename, the atomic path replaces the inode, so when the
/// target has more than one hard link, or its owner or group differs from the
/// new file's (T663), it is skipped and the write is done **in place** instead
/// (keeping the link and owner), after copying the original to the
/// [`recovery_dir`]; a leftover copy marks an interrupted save, see
/// [`pending_recoveries`]. ACLs and xattrs are still not carried over by the
/// rename path.
///
/// The atomic path needs write+execute permission on the *parent directory* (to
/// create the temp and rename it). When only the file itself is writable (a
/// writable file inside a directory you can't write), this falls back to a plain
/// in-place write — losing atomicity but preserving the ability to save, as
/// `fs::write` did before.
///
/// # Errors
///
/// Returns an error only if both the atomic write and the in-place fallback
/// fail (e.g. the file itself is not writable, or the disk is full).
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    match write_atomic_inner(path, data, false) {
        Ok(()) => Ok(()),
        Err(atomic_err) => {
            // Fall back to a truncating in-place write, which needs write
            // permission only on the file (not its directory). The original is
            // copied to the recovery directory first (T663).
            let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            write_in_place_with_recovery(&target, data, recovery_dir().as_deref())
                .map_err(|fallback_err| combine_errors(&atomic_err, &fallback_err))
        }
    }
}

/// Marker error: renaming a temp file over the target would change its
/// identity (hard-link count or owner), so the caller must write in place.
#[derive(Debug)]
struct RenameWouldChangeIdentity;

impl std::fmt::Display for RenameWouldChangeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an atomic rename would change the file's hard links or owner")
    }
}

impl std::error::Error for RenameWouldChangeIdentity {}

/// Whether a rename of `tmp` (just created by this process) over `target`
/// would split a hard link or change the owner or group. Always `false` on
/// non-Unix platforms and for a target that does not exist yet.
#[cfg(unix)]
fn rename_would_change_identity(target: &Path, tmp: &fs::File) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    let (Ok(old), Ok(new)) = (fs::metadata(target), tmp.metadata()) else {
        return false;
    };
    old.nlink() > 1 || old.uid() != new.uid() || old.gid() != new.gid()
}

/// Non-Unix stub: there is no inode/owner model to preserve, so the atomic
/// rename is always acceptable.
#[cfg(not(unix))]
fn rename_would_change_identity(_target: &Path, _tmp: &fs::File) -> bool {
    false
}

/// Magic first line of a recovery copy.
const RECOVERY_MAGIC: &str = "VIXRECOVER";

/// The directory holding pre-save copies of files that were written in place:
/// `$XDG_STATE_HOME/vix/recovery`, else `~/.local/state/vix/recovery`. `None`
/// when no home directory is known.
#[must_use]
pub fn recovery_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(base.join("vix").join("recovery"))
}

/// Write `data` over `target` in place, first copying the existing contents to
/// `recovery` (when given and the target exists). The copy is deleted once the
/// write has been flushed; if the process dies mid-write it stays behind for
/// [`pending_recoveries`]. A failure to take the copy is not fatal: the save
/// proceeds without one.
///
/// # Errors
///
/// Returns the I/O error of the in-place write.
pub fn write_in_place_with_recovery(
    target: &Path,
    data: &[u8],
    recovery: Option<&Path>,
) -> io::Result<()> {
    use std::io::Write as _;
    // Open (without truncating) before taking the copy, so a target we cannot
    // write leaves no copy behind.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(target)?;
    let backup = recovery.and_then(|dir| make_recovery_copy(target, dir));
    f.set_len(0)?;
    f.write_all(data)?;
    f.sync_all()?;
    if let Some(b) = backup {
        let _ = fs::remove_file(b);
    }
    Ok(())
}

/// Write a pre-save copy of `target` into the recovery directory `dir`, so an
/// interrupted in-place save can be repaired on the next launch. The copy holds
/// the `RECOVERY_MAGIC` line, the original path, then the original bytes. Best
/// effort: any failure yields `None` and the save proceeds without a copy.
/// The file is created `0600` and `create_new` (never clobbers) with a
/// process-id + sequence prefix so concurrent saves cannot collide.
fn make_recovery_copy(target: &Path, dir: &Path) -> Option<PathBuf> {
    use std::io::Write as _;
    let original = fs::read(target).ok()?;
    fs::create_dir_all(dir).ok()?;
    let base = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dest = dir.join(format!("{}-{seq}-{base}.vixrecover", std::process::id()));
    let mut out = format!("{RECOVERY_MAGIC}\n{}\n", target.display()).into_bytes();
    out.extend_from_slice(&original);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(&dest).ok()?;
    f.write_all(&out).ok()?;
    f.sync_all().ok()?;
    Some(dest)
}

/// A leftover copy of a file whose in-place save was interrupted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// The recovery copy on disk.
    pub copy: PathBuf,
    /// The file that was being saved.
    pub original: PathBuf,
}

impl Recovery {
    /// The saved pre-save contents.
    ///
    /// # Errors
    ///
    /// Returns an error if the copy cannot be read or is malformed.
    pub fn contents(&self) -> io::Result<Vec<u8>> {
        let raw = fs::read(&self.copy)?;
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "malformed recovery copy");
        let mut parts = raw.splitn(3, |b| *b == b'\n');
        parts.next().ok_or_else(bad)?;
        parts.next().ok_or_else(bad)?;
        Ok(parts.next().ok_or_else(bad)?.to_vec())
    }

    /// Restore: write the saved contents back over the original and delete the
    /// copy.
    ///
    /// # Errors
    ///
    /// Returns an error if the copy cannot be read or the original written.
    pub fn restore(&self) -> io::Result<()> {
        let data = self.contents()?;
        write_in_place_with_recovery(&self.original, &data, None)?;
        fs::remove_file(&self.copy)
    }

    /// Discard: delete the copy and keep the file as it is on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the copy cannot be deleted.
    pub fn discard(&self) -> io::Result<()> {
        fs::remove_file(&self.copy)
    }
}

/// Leftover recovery copies in `dir`, ordered by file name. Unreadable or
/// malformed entries are skipped.
#[must_use]
pub fn pending_recoveries(dir: &Path) -> Vec<Recovery> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Recovery> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "vixrecover"))
        .filter_map(|copy| {
            let raw = fs::read(&copy).ok()?;
            let mut lines = raw.splitn(3, |b| *b == b'\n');
            if lines.next()? != RECOVERY_MAGIC.as_bytes() {
                return None;
            }
            let original = PathBuf::from(String::from_utf8(lines.next()?.to_vec()).ok()?);
            Some(Recovery { copy, original })
        })
        .collect();
    out.sort_by(|a, b| a.copy.cmp(&b.copy));
    out
}

/// Both `write_atomic`'s attempts failed (T535, Run H): report the
/// **fallback's** own error, not the earlier atomic attempt's — the atomic
/// error is almost always the parent-directory permission issue the fallback
/// exists specifically to route around, so leading with it points the user at
/// a stale, misleading cause (e.g. "permission denied creating temp file"
/// when the real, final reason is "disk full"). The atomic error is kept in
/// the message too, not discarded outright, in case it's still relevant.
fn combine_errors(atomic_err: &io::Error, fallback_err: &io::Error) -> io::Error {
    io::Error::new(
        fallback_err.kind(),
        format!("{fallback_err} (atomic write also failed: {atomic_err})"),
    )
}

/// Same as [`write_atomic`], but a **newly created** file is always 0600
/// (owner-only) regardless of umask, rather than the process default — for
/// persisted files whose content is inherently sensitive (T133), e.g.
/// `vix-undo-store`'s per-file undo histories (a full edit history can
/// carry text that no longer appears in the current buffer at all). An
/// *existing* file's own permissions are still preserved untouched, exactly
/// like [`write_atomic`] — this only changes the very first write.
///
/// # Errors
///
/// Returns an error only if both the atomic write and the in-place fallback
/// fail (e.g. the file itself is not writable, or the disk is full).
pub fn write_atomic_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let existed = path.exists();
    match write_atomic_inner(path, data, true) {
        Ok(()) => Ok(()),
        Err(atomic_err) => {
            let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            write_in_place_with_recovery(&target, data, recovery_dir().as_deref())
                .map_err(|fallback_err| combine_errors(&atomic_err, &fallback_err))?;
            if !existed {
                restrict_to_owner(&target);
            }
            Ok(())
        }
    }
}

/// Best-effort: narrow `path` to owner-only (0600) on Unix, no-op elsewhere
/// and no-op on any error (the file still exists and still holds whatever
/// permissions creation gave it; failing to tighten them further is not
/// worth surfacing to a caller that just wants "as private as we could
/// manage"). For a file `confy` or another writer already created at the
/// process's default (umask) mode — narrowing it after the fact is the only
/// hook available, since neither exposes a way to choose the mode at
/// creation time.
pub fn restrict_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path; // no equivalent notion of "owner-only" to apply here
    }
}

/// The atomic write-temp-then-rename path. Returns an error (which
/// [`write_atomic`]/[`write_atomic_private`] use to trigger the in-place
/// fallback) if the directory can't be written, the temp can't be created,
/// or the rename fails. `force_private_for_new` is what tells the two apart
/// for a file that doesn't exist yet — see [`write_atomic_private`].
fn write_atomic_inner(path: &Path, data: &[u8], force_private_for_new: bool) -> io::Result<()> {
    use std::io::Write as _;

    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let base = target.file_name().and_then(|s| s.to_str()).unwrap_or("out");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{base}.vixtmp-{}-{seq}", std::process::id()));

    // An existing file's mode is known up front so the temp can be created with
    // it (no window where private content is world-readable); a new file is
    // created at the process default so umask is respected exactly like a plain
    // create — creating at a fixed 0600 would make every new file owner-only,
    // unless the caller specifically asked for that via `force_private_for_new`.
    let existing = fs::metadata(&target).ok().map(|m| m.permissions());
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    if existing.is_some() || force_private_for_new {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;

    // T663: a rename would split a hard link or change the owner/group; decline
    // so the caller writes in place (after taking a recovery copy) instead.
    if rename_would_change_identity(&target, &f) {
        drop(f);
        let _ = fs::remove_file(&tmp);
        return Err(io::Error::other(RenameWouldChangeIdentity));
    }

    let write_result = f.write_all(data).and_then(|()| f.sync_all());
    if let Err(e) = write_result {
        drop(f);
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    // Restore the existing file's exact permission bits (the 0600 temp above may
    // have been narrower than the original, e.g. a group-writable file).
    if let Some(perms) = existing {
        let _ = f.set_permissions(perms);
    }
    drop(f);

    if let Err(e) = fs::rename(&tmp, &target) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Write `data` to a freshly created file in the OS temp directory and return
/// its path. The file is created with `O_EXCL` (and 0600 on Unix), so if the
/// name is already present — e.g. a symlink a local attacker planted to redirect
/// the write to a victim file — creation fails and the next name is tried rather
/// than the link being followed. The caller owns the returned path and should
/// delete it when finished.
///
/// # Errors
///
/// Returns an error if no name could be created (after several attempts) or the
/// write fails.
pub fn write_private_temp(prefix: &str, data: &[u8]) -> io::Result<PathBuf> {
    use std::io::Write as _;
    let dir = std::env::temp_dir();
    let mut last_err = None;
    for _ in 0..16 {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{prefix}-{}-{seq}.tmp", std::process::id()));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        match opts.open(&path) {
            Ok(mut f) => {
                f.write_all(data)?;
                f.sync_all()?;
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "temp name unavailable")))
}

/// A name in `dir` derived from `original` that doesn't yet exist, appending
/// " copy", " copy 2", … before the extension. Used for same-directory copies.
#[must_use]
pub fn unique_copy_name(dir: &Path, original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = original
        .extension()
        .map(|e| e.to_string_lossy().into_owned());

    for n in 1.. {
        let suffix = if n == 1 {
            " copy".to_string()
        } else {
            format!(" copy {n}")
        };
        let name = match &ext {
            Some(e) => format!("{stem}{suffix}.{e}"),
            None => format!("{stem}{suffix}"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("the loop returns once a free name is found")
}

/// Whether `path` exists but this user can't write it (T572). The metadata
/// flag covers Windows' read-only attribute and mode `r--`; opening for append
/// (which never changes the file) catches a root-owned `rw-r--r--` file
/// under a normal user.
#[must_use]
pub fn is_unwritable(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) if m.is_file() => {
            m.permissions().readonly() || fs::OpenOptions::new().append(true).open(path).is_err()
        }
        _ => false,
    }
}

/// A list of path globs, such as the `read_only_paths` and `ephemeral_paths`
/// settings (T662). `~/` at the start of a pattern means the home directory
/// (`HOME`, else `USERPROFILE`), `**` spans directories, and an invalid pattern
/// is skipped rather than failing the whole list.
#[derive(Debug, Clone, Default)]
pub struct PathPatterns(globset::GlobSet);

impl PathPatterns {
    /// Compile `patterns`.
    #[must_use]
    pub fn new<S: AsRef<str>>(patterns: &[S]) -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let mut set = globset::GlobSetBuilder::new();
        for pattern in patterns {
            let pattern = pattern.as_ref();
            let expanded = match (pattern.strip_prefix("~/"), &home) {
                (Some(rest), Some(home)) => format!("{}/{rest}", home.to_string_lossy()),
                _ => pattern.to_string(),
            };
            if let Ok(glob) = globset::GlobBuilder::new(&expanded)
                .literal_separator(true)
                .build()
            {
                set.add(glob);
            }
        }
        Self(set.build().unwrap_or_default())
    }

    /// Whether `path` matches any pattern.
    #[must_use]
    pub fn matches(&self, path: &Path) -> bool {
        self.0.is_match(path)
    }
}

/// Largest file [`read_text_guarded`] reads into a text buffer by default.
pub const MAX_TEXT_BYTES: u64 = 50 * 1024 * 1024;

/// The most [`read_text_guarded`] will ever read into memory, even when the user
/// confirms opening a file above their `large_file_mb` limit (T571).
pub const HARD_MAX_TEXT_BYTES: u64 = 1024 * 1024 * 1024;

/// Why [`read_text_guarded`] refused a file (T571). Carried inside an
/// [`io::Error`] of kind `InvalidData`; recover it with
/// [`TextReadError::from_io`] to pick a translated message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextReadError {
    /// A FIFO, socket, device, or directory: reading it could block forever.
    NotRegular,
    /// Larger than the byte limit.
    TooLarge,
    /// Contains NUL bytes: binary, not text.
    Binary,
}

impl std::fmt::Display for TextReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotRegular => "not a regular file",
            Self::TooLarge => "file is too large to open as text",
            Self::Binary => "binary file",
        })
    }
}

impl std::error::Error for TextReadError {}

impl TextReadError {
    /// The refusal inside `e`, if `e` came from [`read_text_guarded`].
    #[must_use]
    pub fn from_io(e: &io::Error) -> Option<Self> {
        e.get_ref()?.downcast_ref::<Self>().copied()
    }
}

/// A text file read by [`read_text_guarded`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextFile {
    /// The decoded text.
    pub text: String,
    /// The file couldn't be decoded losslessly in any encoding (T661) and was
    /// decoded as UTF-8 with replacements, so the caller should open it
    /// read-only rather than corrupt it on save.
    pub lossy: bool,
    /// The encoding and BOM it was read in, to write it back the same way
    /// (T661). UTF-8 when `lossy`.
    pub encoding: vix_encoding::TextEncoding,
}

/// Read a text file, refusing anything that would hang, exhaust memory, or
/// isn't text (T571). The encoding comes from [`vix_encoding::detect`]: a BOM,
/// then UTF-8, then a lossless legacy guess (T661). A UTF-16 BOM skips the
/// NUL-byte binary check, since UTF-16 text is full of NULs. Text no encoding
/// decodes losslessly is decoded as UTF-8 with replacements, with
/// [`TextFile::lossy`] set.
///
/// # Errors
///
/// An `InvalidData` error wrapping a [`TextReadError`] for a non-regular,
/// oversized, or binary file; otherwise the underlying I/O error.
pub fn read_text_guarded(path: &Path, max_bytes: u64) -> io::Result<TextFile> {
    use io::Read as _;
    let refuse = |why| io::Error::new(io::ErrorKind::InvalidData, why);
    // `metadata` follows symlinks, so a link to a regular file is fine.
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(refuse(TextReadError::NotRegular));
    }
    if meta.len() > max_bytes {
        return Err(refuse(TextReadError::TooLarge));
    }
    // Sized from the stat, so the read is one allocation: `Take` hides the
    // file's size from `read_to_end`, which otherwise grows the buffer by
    // doubling (a 100 MB file cost ~264 MB of allocations, T749). One spare
    // byte lets the read see EOF without growing.
    let capacity = usize::try_from(meta.len().saturating_add(1)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    // `take` bounds the read even if the file grows after the stat.
    fs::File::open(path)?
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    decode_text(bytes, max_bytes)
}

/// What [`stream_text_guarded`] produced.
#[derive(Debug)]
pub enum TextStream {
    /// The file was UTF-8 without a byte order mark — the common case — and
    /// its whole text went to the sink, in order, in chunks. Not lossy.
    Streamed,
    /// The file needed decoding (a BOM, another encoding, or invalid UTF-8):
    /// whatever the sink received is to be discarded, and this is the file,
    /// exactly as [`read_text_guarded`] returns it.
    Whole(TextFile),
}

/// [`read_text_guarded`] without holding the whole file in memory: UTF-8
/// text is handed to `sink` a block at a time as it is read, so a caller
/// building its own structure (an editor's rope) never needs a second,
/// file-sized copy. Opening a 100 MB file then peaks at the size of the
/// buffer it builds instead of that plus 100 MB (T749). The same guards
/// apply: a non-regular, oversized, or binary file is refused, the binary
/// check looks at the same first 8 KiB, and any file that is not plain UTF-8
/// is read again through [`read_text_guarded`] and returned whole.
///
/// # Errors
///
/// As [`read_text_guarded`].
pub fn stream_text_guarded(
    path: &Path,
    max_bytes: u64,
    sink: &mut dyn FnMut(&str),
) -> io::Result<TextStream> {
    use io::Read as _;
    // Read granularity. A multi-byte character can straddle a block boundary;
    // the trailing partial bytes are carried to the front of the next block.
    const BLOCK: usize = 64 * 1024;
    let refuse = |why| io::Error::new(io::ErrorKind::InvalidData, why);
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(refuse(TextReadError::NotRegular));
    }
    if meta.len() > max_bytes {
        return Err(refuse(TextReadError::TooLarge));
    }
    let whole = || read_text_guarded(path, max_bytes).map(TextStream::Whole);
    // `take(max + 1)`: one byte over the cap lets us detect a file that grew
    // after the metadata check without ever reading unbounded data.
    let mut file = fs::File::open(path)?.take(max_bytes + 1);
    let mut buf = vec![0u8; BLOCK];
    // Bytes of a character split across two blocks, kept at the front.
    let mut carry = 0;
    let mut total: u64 = 0;
    let mut first = true;
    loop {
        // Fill the block (a read may return less than asked).
        let mut filled = carry;
        while filled < buf.len() {
            match file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        let read = filled - carry;
        total += read as u64;
        if total > max_bytes {
            return Err(refuse(TextReadError::TooLarge));
        }
        let block = &buf[..filled];
        if first {
            first = false;
            if vix_encoding::sniff_bom(block).is_some() || block.iter().take(8192).any(|&b| b == 0)
            {
                return whole();
            }
        }
        let valid = match std::str::from_utf8(block) {
            Ok(text) => {
                sink(text);
                block.len()
            }
            // Invalid UTF-8: decode the whole file the usual way.
            Err(e) if e.error_len().is_some() => return whole(),
            // A character cut off by the block's end: keep its bytes.
            Err(e) => {
                let valid = e.valid_up_to();
                if let Ok(text) = std::str::from_utf8(&block[..valid]) {
                    sink(text);
                }
                valid
            }
        };
        carry = filled - valid;
        if read == 0 {
            // End of file: a character still cut off is invalid UTF-8.
            return if carry == 0 {
                Ok(TextStream::Streamed)
            } else {
                whole()
            };
        }
        buf.copy_within(valid..filled, 0);
    }
}

/// The decoding half of [`read_text_guarded`], for bytes read some other way
/// (a remote authority reads them over SSH, T712): refuse more than
/// `max_bytes` or binary content, detect the encoding, and decode.
///
/// # Errors
///
/// An `InvalidData` error wrapping [`TextReadError::TooLarge`] or
/// [`TextReadError::Binary`].
pub fn decode_text(mut bytes: Vec<u8>, max_bytes: u64) -> io::Result<TextFile> {
    let refuse = |why| io::Error::new(io::ErrorKind::InvalidData, why);
    if bytes.len() as u64 > max_bytes {
        return Err(refuse(TextReadError::TooLarge));
    }
    let bom = vix_encoding::sniff_bom(&bytes);
    if bom.is_none() && bytes.iter().take(8192).any(|&b| b == 0) {
        return Err(refuse(TextReadError::Binary));
    }
    let utf8 = vix_encoding::TextEncoding::UTF8;
    if bom.is_none() {
        // The common case, without copying the text.
        match String::from_utf8(bytes) {
            Ok(text) => {
                return Ok(TextFile {
                    text,
                    lossy: false,
                    encoding: utf8,
                });
            }
            Err(e) => bytes = e.into_bytes(),
        }
    }
    let file = match vix_encoding::detect(&bytes) {
        Some((text, encoding)) => TextFile {
            text,
            lossy: false,
            encoding,
        },
        None => TextFile {
            text: match bom {
                Some(enc) => vix_encoding::decode_lossy(&bytes, enc).0,
                None => String::from_utf8_lossy(&bytes).into_owned(),
            },
            lossy: true,
            encoding: utf8,
        },
    };
    // A UTF-16 BOM skipped the NUL check on the raw bytes; a binary file that
    // merely starts with FF FE still has NULs once decoded.
    if file.text.chars().take(4096).any(|c| c == '\0') {
        return Err(refuse(TextReadError::Binary));
    }
    Ok(file)
}

/// Atomically replace a text file's contents with `text`, in the encoding and
/// BOM the file already has (T661), for writes that bypass an editor tab
/// (replace across files). A file that doesn't exist yet is written as UTF-8.
///
/// # Errors
///
/// `InvalidData` when the existing file only decodes lossily (writing would
/// corrupt it) or `text` has a character its encoding can't hold (with the
/// [`vix_encoding::Unmappable`] inside); otherwise the read or write error.
pub fn write_text_in_file_encoding(path: &Path, text: &str) -> io::Result<()> {
    let bytes = encode_like(read_text_guarded(path, HARD_MAX_TEXT_BYTES), text)?;
    write_atomic(path, &bytes)
}

/// The bytes [`write_text_in_file_encoding`] writes: `text` encoded the way
/// `existing` (the guarded read of the file about to be replaced) was, UTF-8
/// when it was not found. Shared with remote authorities (T712).
///
/// # Errors
///
/// `InvalidData` when `existing` was lossy or `text` can't be encoded;
/// `existing`'s own error unless it is `NotFound`.
pub fn encode_like(existing: io::Result<TextFile>, text: &str) -> io::Result<Vec<u8>> {
    let encoding = match existing {
        Ok(file) if file.lossy => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the file's encoding can't be decoded losslessly",
            ));
        }
        Ok(file) => file.encoding,
        Err(e) if e.kind() == io::ErrorKind::NotFound => vix_encoding::TextEncoding::UTF8,
        Err(e) => return Err(e),
    };
    vix_encoding::encode(text, encoding).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Read a user-chosen text file for a feature that only wants its contents
/// (insert, revert, compare): [`read_text_guarded`] at the default limit, so
/// legacy encodings decode too (T661) and undecodable text decodes lossily. The same refusals apply, so a FIFO or a
/// multi-GB file cannot freeze or exhaust Vix from any of those paths (T571).
///
/// # Errors
///
/// As [`read_text_guarded`].
pub fn read_to_string_guarded(path: &Path) -> io::Result<String> {
    read_text_guarded(path, MAX_TEXT_BYTES).map(|f| f.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory for one test, created fresh.
    fn scratch(tag: &str) -> PathBuf {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vix-fileops-{tag}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Stream `path`, returning the outcome and the text the sink received.
    fn stream(path: &Path, max: u64) -> (io::Result<TextStream>, String) {
        let mut got = String::new();
        let out = stream_text_guarded(path, max, &mut |s| got.push_str(s));
        (out, got)
    }

    /// T749: plain UTF-8 streams in blocks, a character split across a
    /// block boundary included, and matches what `read_text_guarded` reads.
    #[test]
    fn stream_text_guarded_streams_utf8_in_blocks() {
        let dir = scratch("stream-utf8");
        let path = dir.join("big.txt");
        // 64 KiB blocks: an odd-length ASCII prefix puts 2- and 3-byte
        // characters across every boundary.
        let text = format!("x{}\n", "é日".repeat(60_000));
        fs::write(&path, &text).unwrap();
        let (out, got) = stream(&path, MAX_TEXT_BYTES);
        assert!(matches!(out, Ok(TextStream::Streamed)), "{out:?}");
        assert_eq!(got, text);
        assert_eq!(read_text_guarded(&path, MAX_TEXT_BYTES).unwrap().text, got);

        fs::write(&path, "").unwrap();
        let (out, got) = stream(&path, MAX_TEXT_BYTES);
        assert!(matches!(out, Ok(TextStream::Streamed)));
        assert_eq!(got, "");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Anything that is not plain UTF-8 comes back whole, decoded exactly as
    /// `read_text_guarded` decodes it; the guards refuse as they do there.
    #[test]
    fn stream_text_guarded_falls_back_and_refuses_like_read_text_guarded() {
        let dir = scratch("stream-whole");
        let path = dir.join("f.txt");
        // A BOM; Latin-1 past the first block; a character cut off at EOF.
        let mut late_latin1 = "a".repeat(100_000).into_bytes();
        late_latin1.push(0xE9);
        for bytes in [
            b"\xEF\xBB\xBFbom".to_vec(),
            late_latin1,
            b"cut \xE6\x97".to_vec(),
        ] {
            fs::write(&path, &bytes).unwrap();
            let (out, _) = stream(&path, MAX_TEXT_BYTES);
            let Ok(TextStream::Whole(file)) = out else {
                panic!("{out:?}");
            };
            let want = read_text_guarded(&path, MAX_TEXT_BYTES).unwrap();
            assert_eq!(
                (file.text, file.lossy, file.encoding),
                (want.text, want.lossy, want.encoding)
            );
        }
        let refused =
            |out: io::Result<TextStream>| out.err().and_then(|e| TextReadError::from_io(&e));
        fs::write(&path, b"bin\0ary").unwrap();
        assert_eq!(
            refused(stream(&path, MAX_TEXT_BYTES).0),
            Some(TextReadError::Binary)
        );
        fs::write(&path, "0123456789").unwrap();
        assert_eq!(refused(stream(&path, 5).0), Some(TextReadError::TooLarge));
        assert_eq!(
            refused(stream(&dir, MAX_TEXT_BYTES).0),
            Some(TextReadError::NotRegular)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_keeps_hard_links() {
        let dir = scratch("hardlink");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, b"old").unwrap();
        fs::hard_link(&a, &b).unwrap();
        write_atomic(&a, b"new").unwrap();
        assert_eq!(fs::read(&b).unwrap(), b"new", "link was split");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_place_write_leaves_recovery_copy_only_if_interrupted() {
        let dir = scratch("recover");
        let rec = dir.join("rec");
        let f = dir.join("f.txt");
        fs::write(&f, b"original").unwrap();
        // A completed save removes its copy.
        write_in_place_with_recovery(&f, b"done", Some(&rec)).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"done");
        assert_eq!(pending_recoveries(&rec).len(), 0);
        // Simulate a crash: take the copy and never finish.
        let copy = make_recovery_copy(&f, &rec).unwrap();
        let pending = pending_recoveries(&rec);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].copy, copy);
        assert_eq!(pending[0].original, f);
        assert_eq!(pending[0].contents().unwrap(), b"done");
        fs::write(&f, b"tru").unwrap();
        pending[0].restore().unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"done");
        assert_eq!(pending_recoveries(&rec).len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discard_removes_copy_and_keeps_file() {
        let dir = scratch("discard");
        let rec = dir.join("rec");
        let f = dir.join("f.txt");
        fs::write(&f, b"x").unwrap();
        make_recovery_copy(&f, &rec).unwrap();
        fs::write(&f, b"y").unwrap();
        pending_recoveries(&rec)[0].discard().unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"y");
        assert_eq!(pending_recoveries(&rec).len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_content_and_is_readable() {
        let dir = scratch("atomic");
        let path = dir.join("file.txt");
        write_atomic(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        // No temp leftovers in the directory.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("vixtmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_new_file_matches_a_plain_write_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        // A brand-new file must get the umask-default mode, exactly like
        // `fs::write` — not a fixed owner-only 0600.
        let dir = scratch("atomic-newmode");
        let a = dir.join("via_atomic.txt");
        write_atomic(&a, b"x").unwrap();
        let b = dir.join("via_plain.txt");
        fs::write(&b, b"x").unwrap();
        let ma = fs::metadata(&a).unwrap().permissions().mode() & 0o777;
        let mb = fs::metadata(&b).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            ma, mb,
            "new-file mode must match a plain write (umask-respected)"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_preserves_an_existing_files_exact_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = scratch("atomic-preserve");
        let p = dir.join("f.txt");
        fs::write(&p, b"old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&p, b"new").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640,
            "existing file's exact mode must be preserved"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_private_new_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = scratch("atomic-private-new");
        let p = dir.join("secret.json");
        write_atomic_private(&p, b"x").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"x");
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600,
            "a brand-new file must be owner-only, unlike write_atomic"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_private_preserves_an_existing_files_exact_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        // Same "don't silently narrow an already-wider-open file" guarantee
        // as write_atomic -- the "private" behavior only ever applies to a
        // file's very first write.
        let dir = scratch("atomic-private-preserve");
        let p = dir.join("f.txt");
        fs::write(&p, b"old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic_private(&p, b"new").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o644,
            "an existing file's mode must be preserved, not forced private"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn restrict_to_owner_narrows_an_existing_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = scratch("restrict");
        let p = dir.join("f.txt");
        fs::write(&p, b"x").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        restrict_to_owner(&p);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restrict_to_owner_is_a_silent_no_op_on_a_missing_path() {
        // Best-effort: a caller that races a deletion (or just made a typo)
        // gets no panic and no error to handle.
        restrict_to_owner(Path::new("/does/not/exist/at/all"));
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_saves_a_writable_file_in_an_unwritable_directory() {
        use std::os::unix::fs::PermissionsExt as _;
        // The file is writable but its directory is not: the atomic temp+rename
        // can't run, so the save must fall back to an in-place write (as root the
        // atomic path just succeeds — either way the save works).
        let dir = scratch("atomic-rodir");
        let p = dir.join("f.txt");
        fs::write(&p, b"old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let result = write_atomic(&p, b"new");
        // Restore dir perms first so cleanup can proceed no matter what.
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
        result.expect("save must succeed via the in-place fallback");
        assert_eq!(fs::read(&p).unwrap(), b"new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_reports_the_fallback_error_when_both_attempts_fail() {
        use std::os::unix::fs::PermissionsExt as _;
        // T535 (Run H): when the atomic write AND its in-place fallback both
        // fail, the returned error must reflect the *fallback's* own failure
        // (the one that actually explains why nothing was saved), not the
        // earlier atomic attempt's — a prior version of this function
        // discarded the fallback's error entirely via `.map_err(|_| atomic_err)`.
        // Block the atomic path (unwritable directory) *and* the fallback
        // (read-only file itself), so both genuinely fail.
        let dir = scratch("atomic-both-fail");
        let p = dir.join("f.txt");
        fs::write(&p, b"old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o444)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let result = write_atomic(&p, b"new");
        // Restore perms first so cleanup can proceed no matter what.
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o644));
        let Err(e) = result else {
            // Running as root: chmod doesn't actually block anything, so both
            // attempts genuinely succeed — nothing to assert, and the file's
            // content is already proven correct by the sibling test above.
            return;
        };
        assert_eq!(
            e.kind(),
            io::ErrorKind::PermissionDenied,
            "the reported kind matches the fallback's own failure (a direct \
             fs::write to the read-only file has this same kind), not some \
             other, earlier failure: {e}"
        );
        let msg = e.to_string();
        assert!(
            msg.contains("atomic write also failed"),
            "the atomic attempt's error is kept too, not silently dropped: {msg:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_writes_through_a_symlink_preserving_it() {
        use std::os::unix::fs::symlink;
        let dir = scratch("atomic-sym");
        let real = dir.join("real.txt");
        fs::write(&real, b"orig").unwrap();
        let link = dir.join("link.txt");
        symlink(&real, &link).unwrap();
        // Writing "through" the link updates the real file and keeps the link.
        write_atomic(&link, b"updated").unwrap();
        assert_eq!(fs::read(&real).unwrap(), b"updated");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the symlink must be preserved, not replaced by a regular file"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn copy_recursive_does_not_dereference_symlinks() {
        use std::os::unix::fs::symlink;
        let base = scratch("copy-sym");
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        let secret = base.join("secret.txt");
        fs::write(&secret, b"TOPSECRET").unwrap();
        symlink(&secret, src.join("link")).unwrap();
        let dst = base.join("dst");
        copy_recursive(&src, &dst).unwrap();
        let copied = dst.join("link");
        assert!(
            fs::symlink_metadata(&copied)
                .unwrap()
                .file_type()
                .is_symlink(),
            "must copy the link itself, not the secret's contents"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    #[cfg(unix)]
    fn remove_path_on_symlink_to_dir_does_not_delete_the_target() {
        use std::os::unix::fs::symlink;
        let base = scratch("rm-sym");
        let real_dir = base.join("real_dir");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(real_dir.join("keep.txt"), b"keep").unwrap();
        let link = base.join("link_to_dir");
        symlink(&real_dir, &link).unwrap();
        remove_path(&link).unwrap();
        // The link is gone but the target directory and its contents remain.
        assert!(!link.exists());
        assert!(
            real_dir.join("keep.txt").exists(),
            "target dir was destroyed via the link"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_private_temp_creates_a_unique_file() {
        let a = write_private_temp("vix-test", b"one").unwrap();
        let b = write_private_temp("vix-test", b"two").unwrap();
        assert_ne!(a, b, "each call gets a distinct name");
        assert_eq!(fs::read(&a).unwrap(), b"one");
        assert_eq!(fs::read(&b).unwrap(), b"two");
        let _ = fs::remove_file(&a);
        let _ = fs::remove_file(&b);
    }

    #[test]
    #[cfg(unix)]
    fn write_private_temp_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let p = write_private_temp("vix-perm", b"secret").unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "temp file must not be group/world readable");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn trash_path_moves_a_file_out_of_its_original_location() {
        // A unique name so a re-run (or a parallel test) never collides with
        // a leftover from a previous pass; nothing here cleans the trash
        // itself back up, matching what a real "move to trash" should do.
        let base = std::env::temp_dir();
        let target = base.join(format!(
            "vix-trash-test-{}-{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&target, b"trash me").unwrap();
        trash_path(&target).expect("the OS trash mechanism is available");
        assert!(
            !target.exists(),
            "file should be gone from its original location"
        );
    }

    #[test]
    fn read_text_guarded_refuses_non_text_and_decodes_lossily() {
        let dir = std::env::temp_dir().join(format!("vix-guard-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let why = |p: &Path, max| {
            TextReadError::from_io(&read_text_guarded(p, max).unwrap_err()).unwrap()
        };
        assert_eq!(why(&dir, 100), TextReadError::NotRegular);
        let big = dir.join("big");
        fs::write(&big, vec![b'a'; 200]).unwrap();
        assert_eq!(why(&big, 100), TextReadError::TooLarge);
        let bin = dir.join("bin");
        fs::write(&bin, b"ab\0cd").unwrap();
        assert_eq!(why(&bin, 100), TextReadError::Binary);
        let read = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            fs::write(&p, bytes).unwrap();
            let f = read_text_guarded(&p, 100).unwrap();
            (f.text, f.lossy, f.encoding.to_string())
        };
        // T661: legacy text decodes losslessly in its own encoding now.
        assert_eq!(
            read("latin", b"caf\xe9"),
            ("café".into(), false, "windows-1252".into())
        );
        assert_eq!(
            read("ok", "héllo".as_bytes()),
            ("héllo".into(), false, "UTF-8".into())
        );
        // UTF-16's NULs are text, not a sign of a binary file...
        assert_eq!(
            read("utf16", b"\xFF\xFEh\0i\0"),
            ("hi".into(), false, "UTF-16LE BOM".into())
        );
        // ...unless they survive decoding.
        let fake = dir.join("fake-utf16");
        fs::write(&fake, b"\xFF\xFE\0\0\0\0").unwrap();
        assert_eq!(why(&fake, 100), TextReadError::Binary);
        // A BOM over bytes its encoding can't decode is the lossy, read-only case.
        assert_eq!(
            read("bad-bom", b"\xEF\xBB\xBFcaf\xe9"),
            ("caf\u{fffd}".into(), true, "UTF-8".into())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn is_unwritable_detects_a_read_only_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let p = std::env::temp_dir().join(format!("vix-ro-{}", std::process::id()));
        fs::write(&p, b"x").unwrap();
        assert!(!is_unwritable(&p));
        fs::set_permissions(&p, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(is_unwritable(&p));
        assert!(!is_unwritable(&p.with_extension("missing")));
        let _ = fs::remove_file(&p);
    }

    /// T662: `**` spans directories, `~/` is the home directory, and an
    /// invalid pattern is skipped without losing the others.
    #[test]
    fn path_patterns_match_globs_and_home() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .expect("tests run with a home directory");
        let p = PathPatterns::new(&[
            "**/node_modules/**",
            "~/.cargo/registry/**",
            "a[",
            "/nix/store/**",
        ]);
        assert!(p.matches(Path::new("/w/app/node_modules/left-pad/index.js")));
        assert!(p.matches(&home.join(".cargo/registry/src/x/lib.rs")));
        assert!(p.matches(Path::new("/nix/store/abc-hello/bin/hello")));
        assert!(!p.matches(Path::new("/w/app/src/main.rs")));
        assert!(!p.matches(Path::new("/w/node_modules_notes.md")));
        assert!(!PathPatterns::default().matches(Path::new("/nix/store/x")));
    }

    /// T661: a write that bypasses a tab keeps the file's encoding, and
    /// refuses a file that only decodes lossily.
    #[test]
    fn write_text_in_file_encoding_keeps_the_encoding() {
        let dir = std::env::temp_dir().join(format!("vix-wenc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let utf16 = dir.join("utf16");
        fs::write(&utf16, b"\xFF\xFEa\0").unwrap();
        write_text_in_file_encoding(&utf16, "bc").unwrap();
        assert_eq!(fs::read(&utf16).unwrap(), b"\xFF\xFEb\0c\0");
        let broken = dir.join("broken");
        fs::write(&broken, b"\xEF\xBB\xBF\xe9").unwrap();
        assert!(write_text_in_file_encoding(&broken, "x").is_err());
        assert_eq!(fs::read(&broken).unwrap(), b"\xEF\xBB\xBF\xe9");
        let new = dir.join("new");
        write_text_in_file_encoding(&new, "é").unwrap();
        assert_eq!(fs::read(&new).unwrap(), "é".as_bytes());
        let _ = fs::remove_dir_all(&dir);
    }
}
