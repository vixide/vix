//! Writing an executable test script without ETXTBSY (T789).
//!
//! A test that writes a script and then runs it races with every other
//! thread in the test binary that spawns a process: a child forked while this
//! process still holds the script open for writing inherits that handle, and
//! until the child execs, running the script fails with ETXTBSY ("Text file
//! busy"). So the script is staged under another name and copied into place
//! by `cp`: the only write handle to the real file lives in `cp`'s own
//! process, never in this one, so no fork here can inherit it.
//!
//! Shared by the test binaries that include it with
//! `#[path = "support/script.rs"] mod script;`. Each published crate keeps
//! its own copy (`tests/support/script.rs`), since a crate must build from its
//! own files.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

/// Write `contents` to `path` as an executable (0755) script.
///
/// # Errors
///
/// Any I/O error, or `cp` failing.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let staging = path.with_extension("staging");
    std::fs::write(&staging, contents.as_ref())?;
    let copied = std::process::Command::new("cp")
        .arg(&staging)
        .arg(path)
        .status()?;
    if !copied.success() {
        return Err(std::io::Error::other(format!(
            "cp {} {} failed",
            staging.display(),
            path.display()
        )));
    }
    std::fs::remove_file(&staging)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}
