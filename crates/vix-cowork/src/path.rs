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

/// The spelling of [`SharedPath::session_notes`].
const SESSION_NOTES: &str = ".vix/cowork/notes.md";

/// A validated, normalized, root-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SharedPath(String);

impl SharedPath {
    /// Validate and normalize `raw`.
    ///
    /// Rejects: empty paths, absolute paths (a leading `/` or `\`, a drive
    /// letter such as `C:`, at the start or as any later component), any `..`
    /// component, control characters
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
                // Not only at the start: on Windows a later `C:` component is
                // a drive prefix too, and `under` would leave the root for it
                // (`a/C:` → `C:`, found by the proptest on Windows, T772).
                other if has_drive_prefix(other) => {
                    return Err(PathProblem::Absolute.into());
                }
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(PathProblem::Empty.into());
        }
        Ok(Self(parts.join("/")))
    }

    /// The session's shared Markdown notes (T759): one scratch buffer per
    /// session, never a file on disk. It lives under `.vix/`, which
    /// [`is_private`](Self::is_private) refuses, so no real file at that path
    /// can be shared, listed or opened in its place, and a guest cannot ask
    /// for it as a file.
    #[must_use]
    pub fn session_notes() -> Self {
        Self(SESSION_NOTES.to_string())
    }

    /// Whether this is [`session_notes`](Self::session_notes).
    #[must_use]
    pub fn is_session_notes(&self) -> bool {
        self.0 == SESSION_NOTES
    }

    /// The normalized `/`-separated form (the wire and map-key form).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this path is never shared, whatever the host or a guest asks:
    /// secrets, keys, credentials, version-control internals and Vix's own
    /// project settings (`spec/cowork`, "Never share"). Matched on each
    /// component, ignoring ASCII case:
    ///
    /// - directories `.git`, `.ssh`, `.gnupg`, `.aws`, `.azure`, `.kube`,
    ///   `.docker`, `.vix` (trusted environment, HTTP environments, FHIR
    ///   profiles);
    /// - any name starting `.env` (`.env`, `.env.local`, `.envrc`);
    /// - `.netrc`, `.npmrc`, `.pypirc`, `.pgpass`, `credentials`,
    ///   `credentials.json`, and SSH keys (`id_rsa`, `id_dsa`, `id_ecdsa`,
    ///   `id_ed25519`, with or without `.pub`);
    /// - key and keystore extensions `.pem`, `.key`, `.p12`, `.pfx`, `.jks`,
    ///   `.keystore`, `.kdbx`, `.gpg`.
    #[must_use]
    pub fn is_private(&self) -> bool {
        // Directories whose entire contents are private (credentials, VCS
        // internals, Vix's own state). Matching is case-insensitive because the
        // path parts are lowercased below.
        const DIRS: &[&str] = &[
            ".git", ".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker", ".vix",
        ];
        // Exact file names that conventionally hold secrets.
        const NAMES: &[&str] = &[
            ".netrc",
            ".npmrc",
            ".pypirc",
            ".pgpass",
            "credentials",
            "credentials.json",
        ];
        // SSH private-key base names; the `.pub` public half is hidden too.
        const KEYS: &[&str] = &["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];
        // Key / keystore / vault file extensions.
        const EXTS: &[&str] = &[
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            ".jks",
            ".keystore",
            ".kdbx",
            ".gpg",
        ];
        let parts: Vec<String> = self.0.split('/').map(str::to_ascii_lowercase).collect();
        let Some((name, dirs)) = parts.split_last() else {
            return false;
        };
        dirs.iter().any(|d| DIRS.contains(&d.as_str()))
            || DIRS.contains(&name.as_str())
            || name.starts_with(".env")
            || NAMES.contains(&name.as_str())
            || KEYS
                .iter()
                .any(|k| name == k || name.strip_prefix(k) == Some(".pub"))
            || EXTS.iter().any(|e| name.ends_with(e))
    }

    /// This path joined under `root`, using the platform separator.
    #[must_use]
    pub fn under(&self, root: &std::path::Path) -> PathBuf {
        let mut out = root.to_path_buf();
        out.extend(self.0.split('/'));
        out
    }
}

/// Whether `raw` starts with a Windows drive prefix such as `C:`. Rejected on
/// every platform: a path that is relative on Unix but absolute on Windows would
/// let a guest escape the share root when the host runs on Windows.
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

    /// T759: the notes path parses as itself and is private, so it can never
    /// stand for a real file.
    #[test]
    fn session_notes_is_a_valid_private_path() {
        let notes = SharedPath::session_notes();
        assert_eq!(SharedPath::parse(notes.as_str()).unwrap(), notes);
        assert!(notes.is_private());
        assert!(notes.is_session_notes());
        assert!(!SharedPath::parse("notes.md").unwrap().is_session_notes());
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
    fn a_drive_component_anywhere_is_refused() {
        for raw in ["a/C:", "a\\c:x", "\u{ae}\\a:", "x/y/Z:/z"] {
            assert!(SharedPath::parse(raw).is_err(), "{raw:?}");
        }
        // A colon later in a name is not a drive.
        assert!(SharedPath::parse("a/bb:c").is_ok());
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

    #[test]
    fn secrets_and_settings_are_private() {
        for p in [
            ".env",
            ".ENV.production",
            "web/.envrc",
            ".git/config",
            "sub/.git/HEAD",
            ".vix/http-env.json",
            ".ssh/id_ed25519",
            "id_rsa.pub",
            "deploy/server.PEM",
            "tls/site.key",
            "home/.netrc",
            ".aws/credentials",
            "vault.kdbx",
        ] {
            assert!(SharedPath::parse(p).unwrap().is_private(), "{p}");
        }
        for p in [
            "src/main.rs",
            "environment.md",
            "docs/keys.md",
            "id_rsa_notes.txt",
            "gitignore.txt",
            "monkey.rs",
        ] {
            assert!(!SharedPath::parse(p).unwrap().is_private(), "{p}");
        }
    }
}
