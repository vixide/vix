//! The conformance suite must say what is wrong with a bad authority, not
//! just pass good ones: it lists every failed expectation, with the error an
//! operation returned when it returned one.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use vix_authority::{
    Authority, DirEntry, FakeAuthority, FileStat, ProcessSpec, TextFile, WalkOptions, conformance,
};

/// A fake with one deliberate defect.
#[derive(Debug)]
enum Defect {
    /// Every write is refused.
    RefusesWrites,
    /// Reads return the wrong bytes.
    LiesOnRead,
}

#[derive(Debug)]
struct Broken {
    inner: FakeAuthority,
    defect: Defect,
}

impl Authority for Broken {
    fn name(&self) -> &'static str {
        "broken"
    }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        match self.defect {
            Defect::LiesOnRead => Ok(b"lie".to_vec()),
            Defect::RefusesWrites => self.inner.read(p),
        }
    }
    fn read_limited(&self, p: &Path, l: u64) -> io::Result<Vec<u8>> {
        self.inner.read_limited(p, l)
    }
    fn read_to_string(&self, p: &Path) -> io::Result<String> {
        self.inner.read_to_string(p)
    }
    fn read_text_guarded(&self, p: &Path, m: u64) -> io::Result<TextFile> {
        self.inner.read_text_guarded(p, m)
    }
    fn write(&self, p: &Path, d: &[u8]) -> io::Result<()> {
        match self.defect {
            Defect::RefusesWrites => Err(io::ErrorKind::PermissionDenied.into()),
            Defect::LiesOnRead => self.inner.write(p, d),
        }
    }
    fn write_atomic(&self, p: &Path, d: &[u8]) -> io::Result<()> {
        self.inner.write_atomic(p, d)
    }
    fn write_text_in_file_encoding(&self, p: &Path, t: &str) -> io::Result<()> {
        self.inner.write_text_in_file_encoding(p, t)
    }
    fn metadata(&self, p: &Path) -> io::Result<FileStat> {
        self.inner.metadata(p)
    }
    fn symlink_metadata(&self, p: &Path) -> io::Result<FileStat> {
        self.inner.symlink_metadata(p)
    }
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        self.inner.canonicalize(p)
    }
    fn read_dir(&self, p: &Path) -> io::Result<Vec<DirEntry>> {
        self.inner.read_dir(p)
    }
    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        self.inner.create_dir_all(p)
    }
    fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.inner.rename(a, b)
    }
    fn remove_file(&self, p: &Path) -> io::Result<()> {
        self.inner.remove_file(p)
    }
    fn remove_path(&self, p: &Path) -> io::Result<()> {
        self.inner.remove_path(p)
    }
    fn move_path(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.inner.move_path(a, b)
    }
    fn copy_recursive(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.inner.copy_recursive(a, b)
    }
    fn trash(&self, p: &Path) -> io::Result<()> {
        self.inner.trash(p)
    }
    fn is_unwritable(&self, p: &Path) -> bool {
        self.inner.is_unwritable(p)
    }
    fn walk_files(&self, r: &Path, o: &WalkOptions) -> Vec<PathBuf> {
        self.inner.walk_files(r, o)
    }
    fn which(&self, n: &str) -> Option<PathBuf> {
        self.inner.which(n)
    }
    fn command(&self, s: &ProcessSpec) -> io::Result<Command> {
        self.inner.command(s)
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-conf-fail-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn an_authority_that_refuses_writes_is_reported_with_the_error() {
    let dir = scratch("refuse");
    let broken = Broken {
        inner: FakeAuthority::new(&dir).unwrap(),
        defect: Defect::RefusesWrites,
    };
    let failures = conformance::run(&broken, Path::new("/work"));
    assert!(
        failures.iter().any(|f| f.starts_with("write: ")),
        "the refused write is named with its error: {failures:?}"
    );
    assert!(
        failures
            .iter()
            .any(|f| f.starts_with("write: ") && f.contains("permission denied")),
        "{failures:?}"
    );
    // Later expectations that depended on the write are reported too, not
    // hidden behind the first failure.
    assert!(failures.len() > 1, "{failures:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(broken.inner.trash_dir());
}

#[test]
fn an_authority_that_returns_the_wrong_bytes_is_reported_by_expectation() {
    let dir = scratch("lie");
    let broken = Broken {
        inner: FakeAuthority::new(&dir).unwrap(),
        defect: Defect::LiesOnRead,
    };
    let failures = conformance::run(&broken, Path::new("/work"));
    assert!(
        failures.contains(&"read returns what write wrote".to_string()),
        "{failures:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(broken.inner.trash_dir());
}

#[test]
fn a_scratch_directory_that_already_exists_is_reported() {
    let dir = scratch("exists");
    let fake = FakeAuthority::new(&dir).unwrap();
    std::fs::create_dir_all(fake.backing().join("work")).unwrap();
    let failures = conformance::run(&fake, Path::new("/work"));
    assert!(
        failures.contains(&"scratch must not exist yet".to_string()),
        "{failures:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(fake.trash_dir());
}
