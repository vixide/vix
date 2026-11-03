//! The shared behavioral suite every [`Authority`] must pass (T711).
//!
//! [`run`] exercises each trait method under a scratch directory and returns
//! the failures as human-readable lines (empty means it conforms). It is a
//! plain function rather than a `#[test]` so each implementation's own test
//! (local, fake, and later SSH) calls it with its own scratch path.
//!
//! The suite is split into one private function per area (`files`,
//! `conflicts`, `listing`, `moves`, `processes`) that share a private `Report`; the
//! order matters because later areas build on files earlier ones leave
//! behind (`listing` expects `a.txt` and `sub/`, `moves` renames `a.txt`).
//! Only behavior common to every authority is asserted, so a new
//! implementation (T718's containers, for one) is held to the same contract.
//! Owning spec: `crates/vix-authority/spec/index.md`.

use std::path::Path;

use crate::{Authority, FileKind, ProcessSpec, WalkOptions, WriteConflict};

/// Collects failures without stopping at the first, so one run lists every
/// non-conformance instead of just the earliest. Holds the messages.
struct Report(Vec<String>);

impl Report {
    /// Record `what` as a failure unless `ok`.
    fn check(&mut self, ok: bool, what: &str) {
        if !ok {
            self.0.push(what.to_string());
        }
    }

    /// Unwrap `r`, recording `what: error` as a failure and returning `None`
    /// on `Err`, so a step's follow-up checks can be skipped via `if let`.
    fn ok<T, E: std::fmt::Display>(&mut self, r: Result<T, E>, what: &str) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.0.push(format!("{what}: {e}"));
                None
            }
        }
    }
}

/// Run the suite against `auth`. `scratch` is a path in `auth`'s own
/// namespace that must not exist yet; the suite creates it, works inside it,
/// and removes it at the end.
///
/// Returns one line per failed expectation; an empty vector means `auth`
/// conforms. Without the `process` feature the process checks instead assert
/// that [`Authority::command`] reports `Unsupported`.
///
/// # Examples
/// ```
/// use vix_authority::{conformance, FakeAuthority};
/// let dir = std::env::temp_dir().join(format!("vix-conf-doc-{}", std::process::id()));
/// let fake = FakeAuthority::new(&dir).unwrap();
/// let failures = conformance::run(&fake, std::path::Path::new("/scratch"));
/// assert!(failures.is_empty(), "{failures:?}");
/// let _ = std::fs::remove_dir_all(&dir);
/// let _ = std::fs::remove_dir_all(fake.trash_dir());
/// ```
#[must_use]
pub fn run(auth: &dyn Authority, scratch: &Path) -> Vec<String> {
    let mut r = Report(Vec::new());
    r.check(!auth.exists(scratch), "scratch must not exist yet");
    r.ok(
        auth.create_dir_all(&scratch.join("sub/deeper")),
        "create_dir_all",
    );
    r.check(
        auth.is_dir(&scratch.join("sub/deeper")),
        "is_dir after create_dir_all",
    );

    files(auth, scratch, &mut r);
    conflicts(auth, scratch, &mut r);
    listing(auth, scratch, &mut r);
    moves(auth, scratch, &mut r);
    processes(auth, scratch, &mut r);

    r.ok(auth.remove_path(scratch), "remove_path(scratch)");
    r.check(!auth.exists(scratch), "scratch gone after remove_path");
    r.0
}

/// Plain file operations: write/read (and limited/guarded variants), atomic
/// and encoding-preserving writes, metadata (single and batched),
/// `canonicalize`.
fn files(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    let a = scratch.join("a.txt");
    r.ok(auth.write(&a, b"hello\n"), "write");
    r.check(
        auth.exists(&a) && auth.is_file(&a),
        "exists/is_file after write",
    );
    r.check(!auth.is_dir(&a), "a file is not a dir");
    r.check(
        auth.read(&a).ok().as_deref() == Some(b"hello\n".as_slice()),
        "read returns what write wrote",
    );
    r.check(
        auth.read_limited(&a, 3).ok().as_deref() == Some(b"hel".as_slice()),
        "read_limited stops at the limit",
    );
    r.check(
        auth.read_to_string(&a).ok().as_deref() == Some("hello\n"),
        "read_to_string",
    );
    r.check(
        auth.read_to_string_guarded(&a).ok().as_deref() == Some("hello\n"),
        "read_to_string_guarded",
    );
    if let Some(t) = r.ok(auth.read_text_guarded(&a, 1024), "read_text_guarded") {
        r.check(t.text == "hello\n" && !t.lossy, "read_text_guarded text");
    }
    r.check(
        auth.read_text_guarded(&a, 2).is_err(),
        "read_text_guarded refuses a file over max_bytes",
    );
    r.ok(auth.write_atomic(&a, b"atomic\n"), "write_atomic");
    r.check(
        auth.read_to_string(&a).ok().as_deref() == Some("atomic\n"),
        "write_atomic replaces the content",
    );
    r.ok(
        auth.write_text_in_file_encoding(&a, "again\n"),
        "write_text_in_file_encoding",
    );
    r.check(
        auth.read_to_string(&a).ok().as_deref() == Some("again\n"),
        "write_text_in_file_encoding content",
    );
    if let Some(st) = r.ok(auth.metadata(&a), "metadata") {
        r.check(st.is_file() && st.len == 6, "metadata kind and length");
        r.check(st.modified.is_some(), "metadata modified time");
    }
    if let Some(st) = r.ok(auth.symlink_metadata(&a), "symlink_metadata") {
        r.check(st.kind == FileKind::File, "symlink_metadata kind");
    }
    r.check(
        auth.metadata(&scratch.join("missing")).is_err(),
        "metadata of a missing path errors",
    );
    let missing = scratch.join("missing");
    let batch = auth.metadata_batch(&[a.as_path(), missing.as_path(), scratch]);
    r.check(
        batch.len() == 3
            && batch[0].as_ref().is_ok_and(|s| s.is_file() && s.len == 6)
            && batch[1]
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            && batch[2].as_ref().is_ok_and(crate::FileStat::is_dir),
        "metadata_batch answers each path in order",
    );
    r.check(
        auth.read(&scratch.join("missing")).is_err(),
        "read of a missing path errors",
    );
    r.check(!auth.is_unwritable(&a), "a fresh file is writable");
    if let Some(c) = r.ok(
        auth.canonicalize(&scratch.join("sub/../a.txt")),
        "canonicalize",
    ) {
        r.check(
            c.file_name().is_some_and(|n| n == "a.txt") && !c.to_string_lossy().contains(".."),
            "canonicalize resolves ..",
        );
        r.check(
            auth.canonicalize(&c).ok().as_ref() == Some(&c),
            "canonicalize is idempotent",
        );
    }
}

/// [`Authority::write_atomic_if_unchanged`] (T712): writes only over the
/// version the caller saw.
fn conflicts(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    let a = scratch.join("a.txt");
    let fresh = scratch.join("fresh.txt");
    let conflict = |res: std::io::Result<()>| res.is_err_and(|e| WriteConflict::is(&e));
    let Some(seen) = r.ok(auth.metadata(&a), "metadata before a checked write") else {
        return;
    };
    r.ok(
        auth.write_atomic_if_unchanged(&a, b"checked\n", Some(&seen)),
        "write_atomic_if_unchanged over the version seen",
    );
    r.check(
        auth.read_to_string(&a).ok().as_deref() == Some("checked\n"),
        "write_atomic_if_unchanged writes",
    );
    r.check(
        conflict(auth.write_atomic_if_unchanged(&a, b"lost\n", Some(&seen))),
        "write_atomic_if_unchanged refuses a changed file",
    );
    r.check(
        conflict(auth.write_atomic_if_unchanged(&a, b"lost\n", None)),
        "write_atomic_if_unchanged refuses a file that appeared",
    );
    r.check(
        conflict(auth.write_atomic_if_unchanged(&fresh, b"lost\n", Some(&seen))),
        "write_atomic_if_unchanged refuses a file that vanished",
    );
    r.check(
        auth.read_to_string(&a).ok().as_deref() == Some("checked\n"),
        "a refused write changes nothing",
    );
    r.ok(
        auth.write_atomic_if_unchanged(&fresh, b"new\n", None),
        "write_atomic_if_unchanged creates a new file",
    );
    r.check(auth.is_file(&fresh), "the new file exists");
    r.ok(auth.remove_file(&fresh), "remove the new file");
}

/// `read_dir` and `walk_files`: listing shape, directory flags, ordering-free
/// name sets, and the walk limit.
fn listing(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    r.ok(
        auth.write(&scratch.join("sub/deeper/b.rs"), b"fn main() {}\n"),
        "write nested",
    );
    if let Some(entries) = r.ok(auth.read_dir(scratch), "read_dir") {
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        r.check(
            names == ["a.txt", "sub"],
            "read_dir lists the direct children",
        );
        r.check(
            entries
                .iter()
                .all(|e| e.path.starts_with(scratch) || e.path.ends_with(e.name())),
            "read_dir paths are in the authority's namespace",
        );
        r.check(
            entries
                .iter()
                .any(|e| e.name() == "sub" && e.is_dir && e.kind == FileKind::Dir),
            "read_dir marks directories",
        );
    }
    let walked = auth.walk_files(scratch, &WalkOptions::file_index(100));
    let mut names: Vec<_> = walked
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    names.sort();
    r.check(names == ["a.txt", "b.rs"], "walk_files finds every file");
    r.check(
        auth.walk_files(scratch, &WalkOptions::file_index(1)).len() == 1,
        "walk_files honors the limit",
    );
}

/// `rename`, `copy_recursive`, `move_path`, `remove_file` and `remove_path`.
fn moves(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    let a = scratch.join("a.txt");
    let renamed = scratch.join("renamed.txt");
    r.ok(auth.rename(&a, &renamed), "rename");
    r.check(
        !auth.exists(&a) && auth.exists(&renamed),
        "rename moves the file",
    );
    let copy = scratch.join("copy");
    r.ok(
        auth.copy_recursive(&scratch.join("sub"), &copy),
        "copy_recursive",
    );
    r.check(
        auth.is_file(&copy.join("deeper/b.rs")),
        "copy_recursive copies the tree",
    );
    let moved = scratch.join("moved");
    r.ok(auth.move_path(&copy, &moved), "move_path");
    r.check(
        !auth.exists(&copy) && auth.is_dir(&moved),
        "move_path moves the tree",
    );
    r.ok(auth.remove_file(&renamed), "remove_file");
    r.check(!auth.exists(&renamed), "remove_file removes");
    r.ok(auth.remove_path(&moved), "remove_path on a tree");
    r.check(!auth.exists(&moved), "remove_path removes the tree");
}

/// `which` and `command`: the platform shell must be found and run, and an
/// unknown program must not resolve.
#[cfg(feature = "process")]
fn processes(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    let (program, args): (&str, &[&str]) = if cfg!(windows) {
        ("cmd", &["/C", "echo ok"])
    } else {
        ("sh", &["-c", "echo ok"])
    };
    r.check(
        auth.which(program).is_some(),
        "which finds the platform shell",
    );
    r.check(
        auth.which("vix-no-such-program-xyz").is_none(),
        "which misses an unknown program",
    );
    let spec = ProcessSpec::new(program).args(args).current_dir(scratch);
    if let Some(mut cmd) = r.ok(auth.command(&spec), "command")
        && let Some(out) = r.ok(cmd.output(), "spawn the command")
    {
        r.check(
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "ok",
            "the command runs and its output comes back",
        );
    }
}

/// Without the `process` feature no authority may start a process.
#[cfg(not(feature = "process"))]
fn processes(auth: &dyn Authority, scratch: &Path, r: &mut Report) {
    r.check(
        auth.command(&ProcessSpec::new("sh").current_dir(scratch))
            .is_err_and(|e| e.kind() == std::io::ErrorKind::Unsupported),
        "command is Unsupported without the process feature",
    );
}
