//! Plain data an [`crate::Authority`] hands back: file metadata, directory
//! entries, and walk options. Owned values rather than `std::fs` types, so a
//! remote authority can build them from a `stat` reply.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What kind of filesystem object a path is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (only reported when links are not followed).
    Symlink,
    /// Anything else: a FIFO, socket or device.
    Other,
}

impl FileKind {
    /// The kind of a local `std::fs::FileType`.
    #[must_use]
    pub fn from_std(t: std::fs::FileType) -> Self {
        if t.is_symlink() {
            Self::Symlink
        } else if t.is_dir() {
            Self::Dir
        } else if t.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

/// Metadata for one path: the subset of `std::fs::Metadata` Vix reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileStat {
    /// What the path is.
    pub kind: FileKind,
    /// Size in bytes.
    pub len: u64,
    /// Last modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
    /// Creation time, when the platform reports one.
    pub created: Option<SystemTime>,
    /// Whether the read-only permission/attribute is set.
    pub readonly: bool,
}

impl FileStat {
    /// A stat of `kind` and `len` with no times and not read-only; chain
    /// [`FileStat::with_modified`] and friends to fill in the rest.
    #[must_use]
    pub fn new(kind: FileKind, len: u64) -> Self {
        Self {
            kind,
            len,
            modified: None,
            created: None,
            readonly: false,
        }
    }

    /// This stat with its modification time set.
    #[must_use]
    pub fn with_modified(mut self, t: Option<SystemTime>) -> Self {
        self.modified = t;
        self
    }

    /// This stat with its creation time set.
    #[must_use]
    pub fn with_created(mut self, t: Option<SystemTime>) -> Self {
        self.created = t;
        self
    }

    /// This stat with its read-only flag set.
    #[must_use]
    pub fn with_readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    /// The stat of a local `std::fs::Metadata`.
    #[must_use]
    pub fn from_std(m: &std::fs::Metadata) -> Self {
        Self::new(FileKind::from_std(m.file_type()), m.len())
            .with_modified(m.modified().ok())
            .with_created(m.created().ok())
            .with_readonly(m.permissions().readonly())
    }

    /// Whether this is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == FileKind::Dir
    }

    /// Whether this is a regular file.
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.kind == FileKind::File
    }

    /// Whether this is a symbolic link.
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.kind == FileKind::Symlink
    }
}

/// One entry of [`crate::Authority::read_dir`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DirEntry {
    /// The entry's full path (the listed directory joined with its name).
    pub path: PathBuf,
    /// The entry's own kind, **not** following a symlink (like
    /// `std::fs::DirEntry::file_type`).
    pub kind: FileKind,
    /// Whether the entry is a directory **following** a symlink (like
    /// `Path::is_dir`), so a link to a directory reads as one.
    pub is_dir: bool,
}

impl DirEntry {
    /// An entry at `path` of `kind`, which is a directory (after following
    /// links) when `is_dir`.
    #[must_use]
    pub fn new(path: PathBuf, kind: FileKind, is_dir: bool) -> Self {
        Self { path, kind, is_dir }
    }

    /// The entry's file name (the last component of [`DirEntry::path`]).
    #[must_use]
    pub fn name(&self) -> &OsStr {
        self.path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
    }

    /// Whether the entry itself is a symbolic link.
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.kind == FileKind::Symlink
    }

    /// The entry's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// How [`crate::Authority::walk_files`] walks a project.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WalkOptions {
    /// Deepest level to descend to (`None`: unbounded).
    pub max_depth: Option<usize>,
    /// Skip dotfiles and dot-directories.
    pub skip_hidden: bool,
    /// Honor `.gitignore`, `.ignore`, git's global excludes and
    /// `.git/info/exclude`, even outside a git repository, and in parent
    /// directories.
    pub honor_ignore_files: bool,
    /// Directory names always pruned below the root (`target`,
    /// `node_modules`, …).
    pub prune_dirs: Vec<String>,
    /// Stop after this many files.
    pub limit: usize,
}

impl WalkOptions {
    /// The fuzzy file finder's walk: depth 12, hidden files skipped, ignore
    /// files honored, `target` and `node_modules` pruned, at most `limit`
    /// files.
    #[must_use]
    pub fn file_index(limit: usize) -> Self {
        Self {
            max_depth: Some(12),
            skip_hidden: true,
            honor_ignore_files: true,
            prune_dirs: vec!["target".into(), "node_modules".into()],
            limit,
        }
    }
}
