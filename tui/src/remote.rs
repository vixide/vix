//! `vix ssh://user@host/path` (T712): open a remote workspace from the
//! command line, before the UI starts.
//!
//! This is the path that can prompt: `ssh` asks for a password, passphrase
//! or host-key confirmation on the plain terminal before the UI starts.
//! **File → Open Remote…** (T713, `tui/src/app/remote_session.rs`) connects
//! without prompting instead, and `App::adopt_remote` gives both the same
//! status-bar host, connection-lost state and trust prompt.

use std::path::PathBuf;

/// When `files` is exactly one remote name (`ssh://…` or `/ssh:…`), connect
/// to it, install an [`crate::authority::ssh::SshAuthority`] in the
/// authority slot, and return the target whose `path` is the workspace root
/// to open (pass it to `App::adopt_remote` once the App exists). A remote
/// directory becomes the root and `files` is emptied; a remote file's
/// directory becomes the root and `files` holds the file's remote path.
///
/// The first connection is made here, on the plain terminal, so `ssh` can
/// ask for a password, passphrase or host-key confirmation the usual way;
/// every later command reuses it without prompting.
///
/// `Ok(None)` when `files` names no remote path.
///
/// # Errors
/// A translated message: the name doesn't parse, the connection or login
/// failed, or the remote path doesn't exist.
pub fn connect_cli(
    files: &mut Vec<PathBuf>,
) -> Result<Option<crate::authority::ssh::SshTarget>, String> {
    let [only] = files.as_slice() else {
        return Ok(None);
    };
    let name = only.to_string_lossy().into_owned();
    if !crate::authority::ssh::SshTarget::is_remote(&name) {
        return Ok(None);
    }
    let target = crate::authority::ssh::SshTarget::parse(&name)
        .map_err(|e| t!("msg.remote_bad_name", name = name, error = e).to_string())?;
    let mut target = target;
    let (root, file) = connect(target.clone())?;
    *files = file.into_iter().collect();
    target.path = root.to_string_lossy().into_owned();
    Ok(Some(target))
}

#[cfg(feature = "tasks")]
fn connect(target: crate::authority::ssh::SshTarget) -> Result<(PathBuf, Option<PathBuf>), String> {
    use crate::authority::Authority as _;
    let host = target.host.clone();
    let failed =
        |e: std::io::Error| t!("msg.remote_connect_failed", host = host, error = e).to_string();
    let remote_path = PathBuf::from(&target.path);
    let auth = crate::authority::ssh::SshAuthority::new(target).map_err(failed)?;
    auth.connect(true).map_err(failed)?;
    let path = auth.canonicalize(&remote_path).map_err(failed)?;
    let (root, file) = if auth.is_dir(&path) {
        (path, None)
    } else {
        let root = path
            .parent()
            .map_or_else(|| PathBuf::from("/"), std::path::Path::to_path_buf);
        (root, Some(path))
    };
    crate::authority::install(std::sync::Arc::new(auth));
    Ok((root, file))
}

/// Without `tasks` Vix starts no process, so it can't run `ssh`.
#[cfg(not(feature = "tasks"))]
fn connect(
    _target: crate::authority::ssh::SshTarget,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    Err(t!("msg.capability_missing", name = "tasks").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_names_are_left_alone() {
        let mut files = vec![PathBuf::from("src/main.rs")];
        assert_eq!(connect_cli(&mut files), Ok(None));
        let mut two = vec![PathBuf::from("ssh://h/a"), PathBuf::from("b")];
        assert_eq!(connect_cli(&mut two), Ok(None), "only a lone remote name");
        let mut none = Vec::new();
        assert_eq!(connect_cli(&mut none), Ok(None));
    }

    #[test]
    fn a_bad_remote_name_is_an_error_before_any_connection() {
        let mut files = vec![PathBuf::from("ssh://-oProxyCommand=x/y")];
        assert!(connect_cli(&mut files).is_err());
        assert_eq!(files.len(), 1, "the arguments are untouched");
    }
}
