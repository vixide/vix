//! Filesystem helpers for the explorer's copy / cut / paste / delete.

#![warn(clippy::pedantic)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Recursively copy a file or directory tree from `src` to `dst`.
///
/// # Errors
///
/// Returns an error if any read, directory creation, or copy fails.
pub fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
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
/// # Errors
///
/// Returns an error if the file or directory cannot be removed.
pub fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// A name in `dir` derived from `original` that doesn't yet exist, appending
/// " copy", " copy 2", … before the extension. Used for same-directory copies.
#[must_use] 
pub fn unique_copy_name(dir: &Path, original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = original.extension().map(|e| e.to_string_lossy().into_owned());

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
