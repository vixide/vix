//! Filesystem helpers for the explorer's copy / cut / paste / delete, plus a
//! crash-safe atomic writer used across the app for saving files.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic counter making temp-file names unique within a process without a
/// clock or RNG (both of which are unavailable in some harness contexts).
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Recursively copy a file or directory tree from `src` to `dst`.
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

/// Move `src` to `dst`, falling back to copy+remove across filesystems.
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
/// would. Note that, being a rename, this replaces the inode — hard links,
/// ownership, ACLs, and xattrs are not carried over (only the Unix mode is).
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
    match write_atomic_inner(path, data) {
        Ok(()) => Ok(()),
        Err(atomic_err) => {
            // Fall back to a truncating in-place write, which needs write
            // permission only on the file (not its directory).
            let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            fs::write(&target, data).map_err(|_| atomic_err)
        }
    }
}

/// The atomic write-temp-then-rename path. Returns an error (which
/// [`write_atomic`] uses to trigger the in-place fallback) if the directory
/// can't be written, the temp can't be created, or the rename fails.
fn write_atomic_inner(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write as _;

    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let base = target.file_name().and_then(|s| s.to_str()).unwrap_or("out");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{base}.vixtmp-{}-{seq}", std::process::id()));

    // An existing file's mode is known up front so the temp can be created with
    // it (no window where private content is world-readable); a new file is
    // created at the process default so umask is respected exactly like a plain
    // create — creating at a fixed 0600 would make every new file owner-only.
    let existing = fs::metadata(&target).ok().map(|m| m.permissions());
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    if existing.is_some() {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;

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
}
