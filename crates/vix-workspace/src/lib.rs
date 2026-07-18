//! A *workspace*: a named set of project folders plus the files to reopen, saved
//! to and loaded from a standalone file (the VS Code "workspace file" idea).
//!
//! Unlike the per-directory [`crate::session`] (which auto-restores the last
//! state for a single root), a workspace is an explicit, portable file the user
//! saves and opens by name. It can gather several folders so the fuzzy file
//! finder and search span all of them, and records the open files so reopening
//! the workspace restores them.
//!
//! The on-disk format is TOML:
//!
//! ```toml
//! folders = ["/home/me/proj", "/home/me/lib"]
//! files = ["/home/me/proj/src/main.rs"]
//! ```

#![warn(clippy::pedantic)]

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Lexically normalize a path: resolve `.` and `..` components textually
/// (without touching the filesystem, so it never follows a symlink or requires
/// the path to exist). Used for containment checks on untrusted workspace paths.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                // Pop a normal component; keep `..` that would escape the root.
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve `path` to a real path for containment comparison: canonicalize it if
/// it exists (resolving symlinks and `..`), otherwise fall back to lexical
/// normalization so non-existent paths are still handled deterministically.
fn resolve(path: &str) -> PathBuf {
    let p = Path::new(path);
    std::fs::canonicalize(p).unwrap_or_else(|_| normalize_lexical(p))
}

/// The conventional extension for a Vix workspace file.
pub const EXTENSION: &str = "vix-workspace";

/// A saved workspace: its folders (first is the primary root) and the files to
/// reopen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// Absolute folder paths; the first is treated as the primary root.
    pub folders: Vec<String>,
    /// Absolute file paths to reopen, in tab order.
    pub files: Vec<String>,
}

impl Workspace {
    /// Serialize to TOML text.
    ///
    /// # Errors
    /// Returns an error if serialization fails (practically never for this shape).
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Parse a workspace from TOML text.
    ///
    /// # Errors
    /// Returns an error if `text` is not valid workspace TOML.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Whether any folder is a filesystem root (`/`, a bare drive) or empty.
    ///
    /// A workspace file is portable and may be attacker-supplied. Re-rooting the
    /// fuzzy finder / project search at `/` would trigger a recursive walk of the
    /// entire filesystem (a denial-of-service and an information-disclosure of
    /// every path); no legitimate workspace does this, so callers should refuse
    /// to open such a workspace.
    #[must_use]
    pub fn has_root_or_empty_folder(&self) -> bool {
        self.folders.iter().any(|f| {
            let norm = normalize_lexical(Path::new(f));
            // A folder that names nothing concrete — a filesystem root (`/`,
            // `C:\`), the empty string, or a path that `..`-escapes above the
            // root (`/..`) — has no `Normal` component. Such a "folder" can only
            // re-root the index at or above `/`.
            !norm.components().any(|c| matches!(c, Component::Normal(_)))
        })
    }

    /// Whether `file` lies within one of this workspace's `folders`. Used to
    /// avoid auto-opening arbitrary absolute paths a malicious workspace file
    /// smuggled in (e.g. `/etc/passwd`, `~/.ssh/id_rsa`) that are unrelated to
    /// the workspace's own folders.
    ///
    /// Paths that exist are canonicalized before comparison — this resolves
    /// symlinked prefixes (so a canonical file path still matches a non-canonical
    /// folder, and vice versa) and, being a real-path check, also defeats
    /// symlink escapes. Non-existent paths fall back to lexical normalization.
    #[must_use]
    pub fn file_within_folders(&self, file: &str) -> bool {
        let file = resolve(file);
        self.folders.iter().any(|folder| {
            let folder = resolve(folder);
            !folder.as_os_str().is_empty() && file.starts_with(&folder)
        })
    }

    /// The subset of `files` that are **not** contained within any workspace
    /// folder — the entries a caller should decline to auto-open without an
    /// explicit user confirmation.
    #[must_use]
    pub fn external_files(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|f| !self.file_within_folders(f))
            .cloned()
            .collect()
    }

    /// Read a workspace file from `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Write this workspace to `path` as TOML.
    ///
    /// # Errors
    /// Returns an error if serialization or the file write fails.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = self
            .to_toml()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        vix_fileops::write_atomic(path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let ws = Workspace {
            folders: vec!["/a".into(), "/b".into()],
            files: vec!["/a/main.rs".into()],
        };
        let text = ws.to_toml().unwrap();
        assert_eq!(Workspace::from_toml(&text).unwrap(), ws);
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let ws = Workspace::from_toml("folders = [\"/x\"]\n").unwrap();
        assert_eq!(ws.folders, vec!["/x".to_string()]);
        assert!(ws.files.is_empty());
    }

    #[test]
    fn rejects_root_folder_reroot() {
        assert!(
            Workspace {
                folders: vec!["/".into()],
                files: vec![],
            }
            .has_root_or_empty_folder()
        );
        assert!(
            Workspace {
                folders: vec![String::new()],
                files: vec![],
            }
            .has_root_or_empty_folder()
        );
        // A `..`-laden path that collapses to root is also caught.
        assert!(
            Workspace {
                folders: vec!["/home/me/../../..".into()],
                files: vec![],
            }
            .has_root_or_empty_folder()
        );
        assert!(
            !Workspace {
                folders: vec!["/home/me/proj".into()],
                files: vec![],
            }
            .has_root_or_empty_folder()
        );
    }

    #[test]
    fn external_files_are_flagged() {
        let ws = Workspace {
            folders: vec!["/home/me/proj".into()],
            files: vec![
                "/home/me/proj/src/main.rs".into(), // inside — fine
                "/etc/passwd".into(),               // outside — flagged
                "/home/me/proj/../secret".into(),   // escapes via `..` — flagged
            ],
        };
        assert!(ws.file_within_folders("/home/me/proj/src/main.rs"));
        assert!(!ws.file_within_folders("/etc/passwd"));
        assert_eq!(
            ws.external_files(),
            vec![
                "/etc/passwd".to_string(),
                "/home/me/proj/../secret".to_string()
            ]
        );
    }
}
