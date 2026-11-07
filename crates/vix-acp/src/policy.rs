//! What an agent may touch: workspace path containment and permission choice.
//!
//! The agent is untrusted code that names file paths in requests such as
//! `fs/read_text_file`. Everything here is the gatekeeper between those
//! requests and the file system; [`crate::client::Client`] and the App call it
//! before any read or write. [`contain`] is purely lexical (cheap, testable);
//! [`contain_on_disk`] adds the symlink check that needs the real file system.
//! Permission helpers deliberately only ever pick a *one-shot* allow, or a
//! reject: a standing grant must be an explicit user decision elsewhere.
//!
//! Owning spec: `crates/vix-acp/spec/index.md` (trust model).

use std::path::{Component, Path, PathBuf};

use crate::client::{OptionKind, PermOption};

/// Why a path the agent named was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// Not an absolute path.
    NotAbsolute,
    /// Contains a `..` component or a NUL byte.
    Traversal,
    /// Outside the workspace root.
    Outside,
}

/// Lexically check that `requested` is an absolute path inside `root`, with no
/// `..`. Pure: touches no file system. See [`contain_on_disk`] for symlinks.
///
/// `starts_with` compares whole path components, so `/w/proj2/x` is *not*
/// inside `/w/proj`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use vix_acp::policy::{contain, PathError};
/// let root = Path::new("/w/proj");
/// assert!(contain(root, "/w/proj/a.rs").is_ok());
/// assert_eq!(contain(root, "/w/proj/../x"), Err(PathError::Traversal));
/// assert_eq!(contain(root, "a.rs"), Err(PathError::NotAbsolute));
/// ```
///
/// # Errors
/// [`PathError`] naming the first rule the path breaks.
pub fn contain(root: &Path, requested: &str) -> Result<PathBuf, PathError> {
    // A NUL byte could truncate the path at the OS boundary and make a
    // checked path differ from the one actually opened.
    if requested.contains('\0') {
        return Err(PathError::Traversal);
    }
    let path = Path::new(requested);
    if !path.is_absolute() {
        return Err(PathError::NotAbsolute);
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PathError::Traversal);
    }
    if path.starts_with(root) {
        Ok(path.to_path_buf())
    } else {
        Err(PathError::Outside)
    }
}

/// [`contain`], and additionally the real path (symlinks resolved) must stay
/// inside the real root. A path that does not exist yet is judged by its
/// nearest existing ancestor.
///
/// # Errors
/// [`PathError`] as for [`contain`], or [`PathError::Outside`] when a symlink
/// leads out of the root.
pub fn contain_on_disk(root: &Path, requested: &str) -> Result<PathBuf, PathError> {
    let path = contain(root, requested)?;
    // Canonicalise the root too: it may itself sit behind a symlink (e.g.
    // macOS `/tmp` -> `/private/tmp`), which would otherwise fail `starts_with`.
    let real_root = root.canonicalize().map_err(|_| PathError::Outside)?;
    // A file the agent is about to create does not exist, so `canonicalize`
    // fails; walk up to the nearest ancestor that does and judge that instead.
    let mut probe = path.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            return if real.starts_with(&real_root) {
                Ok(path)
            } else {
                Err(PathError::Outside)
            };
        }
        match probe.parent() {
            Some(p) => probe = p,
            None => return Err(PathError::Outside),
        }
    }
}

/// The option id that rejects a permission request: `reject_once` first, then
/// `reject_always`. `None` means answer `cancelled`.
#[must_use]
pub fn reject_option(options: &[PermOption]) -> Option<&str> {
    [OptionKind::RejectOnce, OptionKind::RejectAlways]
        .iter()
        .find_map(|k| options.iter().find(|o| o.kind == *k))
        .map(|o| o.id.as_str())
}

/// The option id that allows a request exactly once. Never `allow_always`: a
/// standing grant is not something one `y` should give.
#[must_use]
pub fn allow_once_option(options: &[PermOption]) -> Option<&str> {
    options
        .iter()
        .find(|o| o.kind == OptionKind::AllowOnce)
        .map(|o| o.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str, kind: OptionKind) -> PermOption {
        PermOption {
            id: id.into(),
            name: id.into(),
            kind,
        }
    }

    #[test]
    fn contains_only_absolute_paths_under_root() {
        // `/w` is not absolute on Windows.
        let base = if cfg!(windows) { "C:/w" } else { "/w" };
        let at = |rest: &str| format!("{base}{rest}");
        let root_text = at("/proj");
        let root = Path::new(&root_text);
        assert!(contain(root, &at("/proj/a/b.rs")).is_ok());
        assert_eq!(contain(root, "a/b.rs"), Err(PathError::NotAbsolute));
        assert_eq!(contain(root, &at("/proj/../x")), Err(PathError::Traversal));
        assert_eq!(contain(root, &at("/other/x")), Err(PathError::Outside));
        assert_eq!(contain(root, &at("/proj2/x")), Err(PathError::Outside));
        assert_eq!(contain(root, &at("/proj/a\0b")), Err(PathError::Traversal));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_root_is_refused() {
        let base = std::env::temp_dir().join(format!("vix-acp-policy-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        std::fs::write(root.join("ok"), "o").unwrap();
        let real_root = root.canonicalize().unwrap();
        let link = format!("{}/link/secret", real_root.display());
        assert_eq!(contain_on_disk(&real_root, &link), Err(PathError::Outside));
        let ok = format!("{}/ok", real_root.display());
        assert!(contain_on_disk(&real_root, &ok).is_ok());
        let fresh = format!("{}/new/file", real_root.display());
        assert!(contain_on_disk(&real_root, &fresh).is_ok());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn rejection_prefers_reject_once_and_never_allows() {
        let opts = [
            opt("a", OptionKind::AllowAlways),
            opt("ra", OptionKind::RejectAlways),
            opt("ro", OptionKind::RejectOnce),
        ];
        assert_eq!(reject_option(&opts), Some("ro"));
        assert_eq!(reject_option(&opts[..1]), None);
        assert_eq!(allow_once_option(&opts), None);
    }
}
