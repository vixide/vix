//! Paths named by a guest, confined to the shared root.
//!
//! A guest never sends an absolute path: it names a file relative to the
//! host's project root, with `/` separators. [`SharedPath::parse`] is the one
//! door every such string goes through, and it is purely lexical, so it can
//! be tested without a file system. The host still resolves the result
//! against its root and must not follow a symlink that leaves it; that check
//! needs I/O and lives with the workspace-sharing code (T696).

use crate::error::{CoworkError, PathProblem};
use std::path::PathBuf;

/// The longest accepted path, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;

/// A validated, normalized, root-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SharedPath(String);

impl SharedPath {
    /// Validate and normalize `raw`.
    ///
    /// Rejects: empty paths, absolute paths (a leading `/` or `\`, a drive
    /// letter such as `C:`), any `..` component, control characters
    /// (including NUL), and paths over [`MAX_PATH_BYTES`]. `.` components and
    /// repeated or trailing separators are dropped; `\` is treated as a
    /// separator so a Windows-style escape cannot slip through.
    ///
    /// # Errors
    ///
    /// [`CoworkError::BadPath`] naming what is wrong.
    pub fn parse(raw: &str) -> Result<Self, CoworkError> {
        if raw.len() > MAX_PATH_BYTES {
            return Err(PathProblem::TooLong.into());
        }
        if raw.chars().any(char::is_control) {
            return Err(PathProblem::Control.into());
        }
        if raw.starts_with(['/', '\\']) || has_drive_prefix(raw) {
            return Err(PathProblem::Absolute.into());
        }
        let mut parts: Vec<&str> = Vec::new();
        for part in raw.split(['/', '\\']) {
            match part {
                "" | "." => {}
                ".." => return Err(PathProblem::ParentEscape.into()),
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(PathProblem::Empty.into());
        }
        Ok(Self(parts.join("/")))
    }

    /// The normalized `/`-separated form (the wire and map-key form).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// This path joined under `root`, using the platform separator.
    #[must_use]
    pub fn under(&self, root: &std::path::Path) -> PathBuf {
        let mut out = root.to_path_buf();
        out.extend(self.0.split('/'));
        out
    }
}

fn has_drive_prefix(raw: &str) -> bool {
    let b = raw.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::path::Path;

    fn problem(raw: &str) -> PathProblem {
        match SharedPath::parse(raw) {
            Err(CoworkError::BadPath(p)) => p,
            other => panic!("expected a path problem for {raw:?}, got {other:?}"),
        }
    }

    #[test]
    fn plain_relative_paths_are_normalized() {
        assert_eq!(
            SharedPath::parse("src/main.rs").unwrap().as_str(),
            "src/main.rs"
        );
        assert_eq!(
            SharedPath::parse("./src//main.rs/").unwrap().as_str(),
            "src/main.rs"
        );
        assert_eq!(SharedPath::parse("a\\b").unwrap().as_str(), "a/b");
    }

    #[test]
    fn escapes_are_refused() {
        assert_eq!(problem("../etc/passwd"), PathProblem::ParentEscape);
        assert_eq!(problem("a/../../b"), PathProblem::ParentEscape);
        assert_eq!(problem("a\\..\\b"), PathProblem::ParentEscape);
        assert_eq!(problem("/etc/passwd"), PathProblem::Absolute);
        assert_eq!(problem("\\\\server\\share"), PathProblem::Absolute);
        assert_eq!(problem("C:\\Windows"), PathProblem::Absolute);
        assert_eq!(problem("c:foo"), PathProblem::Absolute);
    }

    #[test]
    fn empty_control_and_long_are_refused() {
        assert_eq!(problem(""), PathProblem::Empty);
        assert_eq!(problem("./"), PathProblem::Empty);
        assert_eq!(problem("a\0b"), PathProblem::Control);
        assert_eq!(problem("a\nb"), PathProblem::Control);
        assert_eq!(
            problem(&"a".repeat(MAX_PATH_BYTES + 1)),
            PathProblem::TooLong
        );
    }

    #[test]
    fn under_stays_inside_the_root() {
        let p = SharedPath::parse("a/b.txt").unwrap();
        let joined = p.under(Path::new("/proj"));
        assert!(joined.starts_with("/proj"));
        assert!(joined.ends_with(Path::new("a").join("b.txt")));
    }

    proptest! {
        #[test]
        fn any_accepted_path_is_relative_and_escape_free(raw in "\\PC{0,40}") {
            if let Ok(p) = SharedPath::parse(&raw) {
                prop_assert!(!p.as_str().starts_with('/'));
                prop_assert!(p.as_str().split('/').all(|c| !c.is_empty() && c != ".." && c != "."));
                let joined = p.under(Path::new("/root"));
                prop_assert!(joined.starts_with("/root"));
                prop_assert!(!joined.components().any(|c| matches!(c, std::path::Component::ParentDir)));
            }
        }

        #[test]
        fn parse_is_idempotent(raw in "[a-z./\\\\]{0,30}") {
            if let Ok(p) = SharedPath::parse(&raw) {
                prop_assert_eq!(SharedPath::parse(p.as_str()).unwrap(), p);
            }
        }
    }
}
