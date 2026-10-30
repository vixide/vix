//! [`LocalAuthority`]: today's behavior, unchanged.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Authority, DirEntry, FileKind, FileStat, ProcessSpec, TextFile, WalkOptions};

/// The machine Vix runs on. Every method is the `std::fs` or `vix-fileops`
/// call the App made directly before T710, so the guards those helpers carry
/// (atomic saves, the T571 read guards, symlink-safe copy and remove, the
/// `PATH`-only program lookup) apply exactly as before.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalAuthority;

impl Authority for LocalAuthority {
    fn name(&self) -> &'static str {
        "local"
    }

    fn is_local(&self) -> bool {
        true
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        read_limited_local(path, limit)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_text_guarded(&self, path: &Path, max_bytes: u64) -> io::Result<TextFile> {
        vix_fileops::read_text_guarded(path, max_bytes)
    }

    fn read_to_string_guarded(&self, path: &Path) -> io::Result<String> {
        vix_fileops::read_to_string_guarded(path)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        std::fs::write(path, data)
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        vix_fileops::write_atomic(path, data)
    }

    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> io::Result<()> {
        vix_fileops::write_text_in_file_encoding(path, text)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileStat> {
        std::fs::metadata(path).map(|m| FileStat::from_std(&m))
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileStat> {
        std::fs::symlink_metadata(path).map(|m| FileStat::from_std(&m))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        Ok(std::fs::read_dir(dir)?
            .flatten()
            .map(|e| {
                let path = e.path();
                let kind = e.file_type().map_or(FileKind::Other, FileKind::from_std);
                let is_dir = match kind {
                    FileKind::Dir => true,
                    FileKind::Symlink => path.is_dir(),
                    _ => false,
                };
                DirEntry::new(path, kind, is_dir)
            })
            .collect())
    }

    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_path(&self, path: &Path) -> io::Result<()> {
        vix_fileops::remove_path(path)
    }

    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()> {
        vix_fileops::move_path(from, to)
    }

    fn copy_recursive(&self, from: &Path, to: &Path) -> io::Result<()> {
        vix_fileops::copy_recursive(from, to)
    }

    fn trash(&self, path: &Path) -> io::Result<()> {
        vix_fileops::trash_path(path)
    }

    fn is_unwritable(&self, path: &Path) -> bool {
        vix_fileops::is_unwritable(path)
    }

    fn walk_files(&self, root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
        walk_local(root, opts)
    }

    fn which(&self, name: &str) -> Option<PathBuf> {
        vix_fileops::which_on_path(name)
    }

    #[cfg(feature = "process")]
    fn command(&self, spec: &ProcessSpec) -> io::Result<Command> {
        Ok(spec.to_local_command())
    }

    #[cfg(not(feature = "process"))]
    fn command(&self, _spec: &ProcessSpec) -> io::Result<Command> {
        Err(crate::process_disabled())
    }
}

/// Read at most `limit` bytes of a local file.
pub(crate) fn read_limited_local(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Walk a local directory with the `ignore` crate (ripgrep's engine) as `opts`
/// says. Shared by [`LocalAuthority`] and the fake, which walks its backing
/// directory the same way.
pub(crate) fn walk_local(root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if opts.limit == 0 {
        return out;
    }
    let prune = opts.prune_dirs.clone();
    let walker = ignore::WalkBuilder::new(root)
        .max_depth(opts.max_depth)
        .hidden(opts.skip_hidden)
        .git_ignore(opts.honor_ignore_files)
        .git_global(opts.honor_ignore_files)
        .git_exclude(opts.honor_ignore_files)
        .ignore(opts.honor_ignore_files)
        .require_git(false) // apply .gitignore/.ignore even outside a git repo
        .parents(opts.honor_ignore_files)
        .filter_entry(move |e| {
            // Prune heavy build/vendor dirs, but never the walk root itself.
            e.depth() == 0
                || !e
                    .file_name()
                    .to_str()
                    .is_some_and(|n| prune.iter().any(|p| p == n))
        })
        .build();
    for entry in walker.flatten() {
        if out.len() >= opts.limit {
            break;
        }
        if entry.file_type().is_some_and(|t| t.is_file()) {
            out.push(entry.into_path());
        }
    }
    out
}
