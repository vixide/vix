//! Where an account's encrypted SQLite store lives, and the session saved
//! in the keyring (`spec/index.md`, "Store and keyring").

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize as _;

/// The environment variable that replaces Vix's platform data directory,
/// for packaging and testing. The store is then
/// `$VIX_MATRIX_DATA_DIR/matrix/<account>/`.
pub const DATA_DIR_ENV: &str = "VIX_MATRIX_DATA_DIR";

/// Vix's directory under the platform data directory, found exactly as
/// `vix_settings::Settings::data_dir` finds it (`etcetera`'s base strategy:
/// `$XDG_DATA_HOME/vix` or `~/.local/share/vix` on Unix and macOS,
/// `%APPDATA%\vix` on Windows), or [`DATA_DIR_ENV`] when it is set and not
/// empty. `None` when neither can be found.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    use etcetera::BaseStrategy as _;
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    etcetera::choose_base_strategy()
        .ok()
        .map(|s| s.data_dir().join("vix"))
}

/// An account name as one safe path component: ASCII letters, digits, `-`,
/// `_`, and `.` (except first) kept, every other byte written as `%XX`. The
/// same escaping as Vix's IM history file names, so no two accounts share
/// a directory and none can name `..`, a separator or an absolute path.
#[must_use]
pub fn escape_account(account: &str) -> String {
    /// Upper-case hex digits for the `%XX` escapes.
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for b in account.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') || (b == b'.' && !out.is_empty()) {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        }
    }
    out
}

/// The store directory for `account` under the data directory `base`:
/// `<base>/matrix/<escaped account>`. `None` for an empty account name,
/// which would otherwise name `<base>/matrix` itself.
#[must_use]
pub fn store_dir(base: &Path, account: &str) -> Option<PathBuf> {
    if account.is_empty() {
        return None;
    }
    Some(base.join("matrix").join(escape_account(account)))
}

/// A fresh random store passphrase: 32 bytes from the OS, as 64 hex digits.
/// `None` when the OS has no randomness to give.
#[must_use]
pub fn new_passphrase() -> Option<String> {
    /// Lower-case hex digits.
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0xF)]));
    }
    bytes.zeroize();
    Some(out)
}

/// What the keyring holds for one account: enough to resume the session
/// without the password, and the passphrase that unlocks its store.
///
/// `Debug` never prints the token or the passphrase, and both are wiped
/// when it is dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSession {
    /// The homeserver as the Login request named it (a URL or a server
    /// name), which a later Login must name again to resume the session.
    pub configured: String,
    /// The homeserver URL the session was made on (after discovery).
    pub homeserver: String,
    /// The full user id.
    pub user_id: String,
    /// This device's id.
    pub device_id: String,
    /// The access token.
    pub access_token: String,
    /// The SQLite store's passphrase.
    pub passphrase: String,
}

impl SavedSession {
    /// Whether this session belongs to `user` (a full user id, or its
    /// local part) on `homeserver`. A session for another user or server is
    /// not resumed: the account's configuration changed.
    #[must_use]
    pub fn matches(&self, homeserver: &str, user: &str) -> bool {
        let user_ok = if user.starts_with('@') {
            self.user_id == user
        } else {
            self.user_id
                .strip_prefix('@')
                .and_then(|rest| rest.split_once(':'))
                .map(|(local, _)| local)
                == Some(user)
        };
        user_ok && same_server(&self.configured, homeserver)
    }

    /// The JSON written to the keyring.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Read the keyring's JSON; `None` when it is not a saved session.
    #[must_use]
    pub fn from_json(json: &str) -> Option<SavedSession> {
        serde_json::from_str(json).ok()
    }
}

/// Whether two homeserver strings name the same server: equal once the
/// scheme and any trailing slash are dropped, ignoring ASCII case.
fn same_server(saved: &str, requested: &str) -> bool {
    fn bare(s: &str) -> String {
        let s = s.trim();
        let s = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
            .unwrap_or(s);
        s.trim_end_matches('/').to_ascii_lowercase()
    }
    bare(saved) == bare(requested)
}

impl fmt::Debug for SavedSession {
    /// Never prints the access token or the passphrase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedSession")
            .field("configured", &self.configured)
            .field("homeserver", &self.homeserver)
            .field("user_id", &self.user_id)
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

impl Drop for SavedSession {
    /// Wipe the secrets so they do not linger in freed memory.
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.passphrase.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use std::path::Component;

    use super::*;

    #[test]
    fn an_account_cannot_escape_the_base_directory() {
        let base = Path::new("/data/vix");
        for account in [
            "..",
            ".",
            "../../etc",
            "/etc/passwd",
            "a/b",
            "a\\b",
            "C:\\x",
            "%2E%2E",
            "\0",
            "~",
            "é",
        ] {
            let dir = store_dir(base, account).unwrap();
            assert_eq!(
                dir.parent(),
                Some(base.join("matrix").as_path()),
                "{account:?} -> {dir:?}"
            );
            let last = dir.components().next_back().unwrap();
            assert!(
                matches!(last, Component::Normal(_)),
                "{account:?} -> {dir:?}"
            );
        }
        assert_eq!(store_dir(base, ""), None);
    }

    #[test]
    fn escaping_keeps_plain_names_and_is_injective_on_examples() {
        assert_eq!(escape_account("work.matrix-1_x"), "work.matrix-1_x");
        assert_eq!(escape_account(".hidden"), "%2Ehidden");
        assert_eq!(escape_account("a/b"), "a%2Fb");
        assert_ne!(escape_account("a/b"), escape_account("a%2Fb"));
    }

    #[test]
    fn a_passphrase_is_64_hex_digits_and_random() {
        let a = new_passphrase().unwrap();
        let b = new_passphrase().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    fn saved() -> SavedSession {
        SavedSession {
            configured: "https://matrix.example.org/".into(),
            homeserver: "https://matrix.example.org/".into(),
            user_id: "@alice:example.org".into(),
            device_id: "DEV".into(),
            access_token: "syt_secret".into(),
            passphrase: "pass-secret".into(),
        }
    }

    #[test]
    fn a_saved_session_never_prints_its_secrets_and_round_trips() {
        let s = saved();
        let shown = format!("{s:?}");
        assert!(
            !shown.contains("syt_secret") && !shown.contains("pass-secret"),
            "{shown}"
        );
        assert_eq!(SavedSession::from_json(&s.to_json()), Some(s));
        assert_eq!(SavedSession::from_json("not json"), None);
    }

    #[test]
    fn a_saved_session_matches_its_user_and_server() {
        let s = saved();
        assert!(s.matches("https://matrix.example.org", "@alice:example.org"));
        assert!(s.matches("matrix.example.org", "alice"));
        assert!(!s.matches("https://matrix.example.org", "@bob:example.org"));
        assert!(!s.matches("https://other.org", "alice"));
    }
}
