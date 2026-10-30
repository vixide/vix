//! [`FakeAuthority`] (T711): a temp-dir-backed authority for tests.
//!
//! It presents a virtual absolute namespace (`/src/main.rs`) backed by an
//! ordinary local directory, the way a remote authority presents the remote
//! machine's paths. A test that installs it sees every routed call site work
//! against paths that don't exist on the local machine, and can assert on the
//! calls that were made.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};

use crate::local::{read_limited_local, walk_local};
use crate::{Authority, DirEntry, FileStat, ProcessSpec, TextFile, WalkOptions};

/// One recorded call on a [`FakeAuthority`]: the operation name (the trait
/// method's name) and the virtual path it was given (the first one, for
/// two-path operations; the program name for `which`/`command`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Call {
    /// The trait method, e.g. `"read_to_string"` or `"write_atomic"`.
    pub op: &'static str,
    /// The virtual path (or program) the call named.
    pub path: PathBuf,
}

/// An authority whose virtual root `/` is a local backing directory.
///
/// - Every path must be rooted (`/a/b`); a relative path, a `..` that climbs
///   above `/`, or a symlink that resolves outside the backing directory is
///   refused with `PermissionDenied`.
/// - Paths it returns (`canonicalize`, `read_dir`, `walk_files`) are virtual.
/// - `trash` moves the path into a sibling trash directory, never the user's
///   real trash.
/// - `command` runs the program locally, with its working directory mapped
///   into the backing directory.
/// - `is_local` is `false` (T715): the App searches it with a tool run
///   through `command`, as it would a remote machine.
/// - Every call is recorded; see [`FakeAuthority::calls`].
#[derive(Debug)]
pub struct FakeAuthority {
    backing: PathBuf,
    trash: PathBuf,
    calls: Mutex<Vec<Call>>,
}

impl FakeAuthority {
    /// A fake rooted at `backing`, which is created if missing (and
    /// canonicalized, so symlink checks compare like with like). The trash
    /// directory is the sibling `<backing>.trash`.
    ///
    /// # Errors
    /// Creating or canonicalizing `backing` failed.
    pub fn new(backing: impl AsRef<Path>) -> io::Result<Self> {
        std::fs::create_dir_all(backing.as_ref())?;
        let backing = std::fs::canonicalize(backing.as_ref())?;
        let mut trash = backing.clone().into_os_string();
        trash.push(".trash");
        Ok(Self {
            backing,
            trash: PathBuf::from(trash),
            calls: Mutex::new(Vec::new()),
        })
    }

    /// The local directory standing in for `/`.
    #[must_use]
    pub fn backing(&self) -> &Path {
        &self.backing
    }

    /// The local directory `trash` moves paths into.
    #[must_use]
    pub fn trash_dir(&self) -> &Path {
        &self.trash
    }

    /// Every call made so far, oldest first.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Forget the recorded calls.
    pub fn clear_calls(&self) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// The local backing path for the virtual `path`, without the symlink
    /// check.
    ///
    /// # Errors
    /// `PermissionDenied` for a relative path or one that climbs above `/`.
    pub fn to_backing(&self, path: &Path) -> io::Result<PathBuf> {
        if !path.has_root() {
            return Err(denied(path));
        }
        let mut rel = PathBuf::new();
        for c in path.components() {
            match c {
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
                Component::ParentDir => {
                    if !rel.pop() {
                        return Err(denied(path));
                    }
                }
                Component::Normal(n) => rel.push(n),
            }
        }
        Ok(self.backing.join(rel))
    }

    /// The virtual path for the local backing `path`, or `None` if it lies
    /// outside the backing directory.
    #[must_use]
    pub fn to_virtual(&self, path: &Path) -> Option<PathBuf> {
        let rel = path.strip_prefix(&self.backing).ok()?;
        let mut out = PathBuf::from("/");
        out.push(rel);
        Some(out)
    }

    fn record(&self, op: &'static str, path: &Path) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Call {
                op,
                path: path.to_path_buf(),
            });
    }

    /// Record the call, map `path`, and refuse it if its nearest existing
    /// ancestor resolves (through symlinks) outside the backing directory.
    fn map(&self, op: &'static str, path: &Path) -> io::Result<PathBuf> {
        self.record(op, path);
        self.confine(path)
    }

    /// Map `path` and refuse it if its nearest existing ancestor resolves
    /// (through symlinks) outside the backing directory. Not recorded.
    fn confine(&self, path: &Path) -> io::Result<PathBuf> {
        let backing = self.to_backing(path)?;
        let mut probe = backing.as_path();
        loop {
            if let Ok(real) = std::fs::canonicalize(probe) {
                if real.starts_with(&self.backing) {
                    return Ok(backing);
                }
                return Err(denied(path));
            }
            match probe.parent() {
                Some(p) => probe = p,
                None => return Err(denied(path)),
            }
        }
    }

    /// Map a path that names a link itself (so the link is not followed):
    /// only its parent must stay inside.
    fn map_nofollow(&self, op: &'static str, path: &Path) -> io::Result<PathBuf> {
        self.record(op, path);
        let backing = self.to_backing(path)?;
        if let Some(parent) = path.parent().filter(|p| *p != path) {
            let parent_backing = self.to_backing(parent)?;
            if let Ok(real) = std::fs::canonicalize(&parent_backing)
                && !real.starts_with(&self.backing)
            {
                return Err(denied(path));
            }
        }
        Ok(backing)
    }

    fn virtualize(&self, path: &Path) -> io::Result<PathBuf> {
        self.to_virtual(path).ok_or_else(|| denied(path))
    }
}

fn denied(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{} is outside the fake authority", path.display()),
    )
}

impl Authority for FakeAuthority {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(self.map("read", path)?)
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        read_limited_local(&self.map("read_limited", path)?, limit)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(self.map("read_to_string", path)?)
    }

    fn read_text_guarded(&self, path: &Path, max_bytes: u64) -> io::Result<TextFile> {
        vix_fileops::read_text_guarded(&self.map("read_text_guarded", path)?, max_bytes)
    }

    fn read_to_string_guarded(&self, path: &Path) -> io::Result<String> {
        vix_fileops::read_to_string_guarded(&self.map("read_to_string_guarded", path)?)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        std::fs::write(self.map("write", path)?, data)
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        vix_fileops::write_atomic(&self.map("write_atomic", path)?, data)
    }

    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> io::Result<()> {
        vix_fileops::write_text_in_file_encoding(
            &self.map("write_text_in_file_encoding", path)?,
            text,
        )
    }

    fn metadata(&self, path: &Path) -> io::Result<FileStat> {
        std::fs::metadata(self.map("metadata", path)?).map(|m| FileStat::from_std(&m))
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileStat> {
        std::fs::symlink_metadata(self.map_nofollow("symlink_metadata", path)?)
            .map(|m| FileStat::from_std(&m))
    }

    fn exists(&self, path: &Path) -> bool {
        self.map("exists", path).is_ok_and(|p| p.exists())
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.map("is_dir", path).is_ok_and(|p| p.is_dir())
    }

    fn is_file(&self, path: &Path) -> bool {
        self.map("is_file", path).is_ok_and(|p| p.is_file())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let real = std::fs::canonicalize(self.map("canonicalize", path)?)?;
        self.virtualize(&real)
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let local = crate::LocalAuthority.read_dir(&self.map("read_dir", dir)?)?;
        local
            .into_iter()
            .map(|e| {
                let kind = e.kind;
                let is_dir = e.is_dir;
                Ok(DirEntry::new(self.virtualize(&e.path)?, kind, is_dir))
            })
            .collect()
    }

    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(self.map("create_dir_all", dir)?)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from = self.map_nofollow("rename", from)?;
        let to = self.confine(to)?;
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(self.map_nofollow("remove_file", path)?)
    }

    fn remove_path(&self, path: &Path) -> io::Result<()> {
        vix_fileops::remove_path(&self.map_nofollow("remove_path", path)?)
    }

    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from = self.map_nofollow("move_path", from)?;
        let to = self.confine(to)?;
        vix_fileops::move_path(&from, &to)
    }

    fn copy_recursive(&self, from: &Path, to: &Path) -> io::Result<()> {
        let from = self.map_nofollow("copy_recursive", from)?;
        let to = self.confine(to)?;
        vix_fileops::copy_recursive(&from, &to)
    }

    fn trash(&self, path: &Path) -> io::Result<()> {
        let from = self.map_nofollow("trash", path)?;
        std::fs::create_dir_all(&self.trash)?;
        let name = from.file_name().ok_or_else(|| denied(path))?.to_os_string();
        let mut to = self.trash.join(&name);
        let mut n = 1u32;
        while to.symlink_metadata().is_ok() {
            let mut numbered = name.clone();
            numbered.push(format!(".{n}"));
            to = self.trash.join(numbered);
            n += 1;
        }
        vix_fileops::move_path(&from, &to)
    }

    fn is_unwritable(&self, path: &Path) -> bool {
        self.map("is_unwritable", path)
            .is_ok_and(|p| vix_fileops::is_unwritable(&p))
    }

    fn walk_files(&self, root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
        let Ok(local) = self.map("walk_files", root) else {
            return Vec::new();
        };
        walk_local(&local, opts)
            .iter()
            .filter_map(|p| self.to_virtual(p))
            .collect()
    }

    fn which(&self, name: &str) -> Option<PathBuf> {
        self.record("which", Path::new(name));
        vix_fileops::which_on_path(name)
    }

    fn command(&self, spec: &ProcessSpec) -> io::Result<Command> {
        self.record("command", Path::new(spec.program()));
        let cwd = match spec.cwd() {
            Some(dir) => Some(self.to_backing(dir)?),
            None => None,
        };
        crate::LocalAuthority.command(&spec.clone().with_cwd(cwd))
    }

    /// The local `shell` in `cwd`'s backing directory (the backing root,
    /// standing in for a home directory, when `cwd` is `None`).
    fn terminal(&self, shell: &str, cwd: Option<&Path>) -> io::Result<ProcessSpec> {
        self.record("terminal", cwd.unwrap_or(Path::new("/")));
        let dir = match cwd {
            Some(dir) => self.to_backing(dir)?,
            None => self.backing().to_path_buf(),
        };
        Ok(ProcessSpec::new(shell).current_dir(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vix-authority-fake-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn paths_map_into_the_backing_directory_and_back() {
        let dir = scratch("map");
        let fake = FakeAuthority::new(&dir).unwrap();
        // The fake stands in for a remote machine (T715).
        assert!(!fake.is_local());
        fake.write(Path::new("/a.txt"), b"hi").unwrap();
        assert_eq!(std::fs::read(fake.backing().join("a.txt")).unwrap(), b"hi");
        assert_eq!(
            fake.canonicalize(Path::new("/./a.txt")).unwrap(),
            PathBuf::from("/a.txt")
        );
        let names: Vec<_> = fake
            .read_dir(Path::new("/"))
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(names, [PathBuf::from("/a.txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escapes_are_refused() {
        let dir = scratch("escape");
        let fake = FakeAuthority::new(dir.join("root")).unwrap();
        let denied =
            |r: io::Result<String>| r.is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied);
        assert!(denied(fake.read_to_string(Path::new("relative.txt"))));
        assert!(denied(fake.read_to_string(Path::new("/../outside.txt"))));
        std::fs::write(dir.join("outside.txt"), "secret").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("outside.txt"), fake.backing().join("link"))
                .unwrap();
            assert!(denied(fake.read_to_string(Path::new("/link"))));
            // The link itself can still be removed: that doesn't follow it.
            fake.remove_file(Path::new("/link")).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn calls_are_recorded_and_trash_stays_local_to_the_fake() {
        let dir = scratch("calls");
        let fake = FakeAuthority::new(dir.join("root")).unwrap();
        fake.write(Path::new("/gone.txt"), b"x").unwrap();
        fake.trash(Path::new("/gone.txt")).unwrap();
        assert!(fake.trash_dir().join("gone.txt").is_file());
        assert!(!fake.exists(Path::new("/gone.txt")));
        let ops: Vec<_> = fake.calls().into_iter().map(|c| c.op).collect();
        assert_eq!(ops, ["write", "trash", "exists"]);
        fake.clear_calls();
        assert_eq!(fake.calls(), Vec::new());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
