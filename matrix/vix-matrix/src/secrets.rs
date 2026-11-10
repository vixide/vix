//! Where the saved session lives: the OS keyring (`spec/index.md`, "Store
//! and keyring").
//!
//! One entry per account, under the service [`SERVICE`] with the account
//! name as the keyring user: the [`crate::store::SavedSession`] JSON, which
//! holds the access token and the store passphrase. macOS uses the login
//! Keychain and Windows the Credential Manager, through `keyring-core`, so
//! the secret is never a process argument; Linux runs the `secret-tool` CLI
//! with the secret on stdin (argv only, no shell), as `vix` does.
//!
//! The request loop only sees the [`SecretStore`] trait, so the tests run
//! it over an in-memory store and never reach the real keyring.

/// The keyring service name for every entry the helper writes.
pub const SERVICE: &str = "vix-matrix";

/// The keyring could not be read or written. Carries no detail: a backend's
/// message could quote the secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyringError;

/// A store of one secret string per account.
pub trait SecretStore: Send + Sync {
    /// The secret saved for `account`, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// [`KeyringError`] when the keyring cannot be read.
    fn get(&self, account: &str) -> Result<Option<String>, KeyringError>;

    /// Save `secret` for `account`, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// [`KeyringError`] when the keyring refuses the write.
    fn set(&self, account: &str, secret: &str) -> Result<(), KeyringError>;

    /// Delete `account`'s secret; deleting a missing one succeeds.
    ///
    /// # Errors
    ///
    /// [`KeyringError`] when the keyring refuses the delete.
    fn delete(&self, account: &str) -> Result<(), KeyringError>;
}

/// The OS keyring. Only `main` makes one; tests never do.
#[derive(Debug)]
pub struct OsKeyring;

impl OsKeyring {
    /// Open the platform's keyring. On macOS and Windows this installs the
    /// native credential store as `keyring-core`'s process default (a store
    /// that fails to open is skipped, so every access then fails with
    /// [`KeyringError`]).
    #[must_use]
    pub fn new() -> OsKeyring {
        #[cfg(target_os = "macos")]
        if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
            keyring_core::set_default_store(store);
        }
        #[cfg(windows)]
        if let Ok(store) = windows_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
        OsKeyring
    }
}

#[cfg(any(target_os = "macos", windows))]
impl SecretStore for OsKeyring {
    fn get(&self, account: &str) -> Result<Option<String>, KeyringError> {
        let entry = keyring_core::Entry::new(SERVICE, account).map_err(|_| KeyringError)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret).filter(|s| !s.is_empty())),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => Err(KeyringError),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), KeyringError> {
        keyring_core::Entry::new(SERVICE, account)
            .and_then(|entry| entry.set_password(secret))
            .map_err(|_| KeyringError)
    }

    fn delete(&self, account: &str) -> Result<(), KeyringError> {
        let entry = keyring_core::Entry::new(SERVICE, account).map_err(|_| KeyringError)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err(KeyringError),
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl SecretStore for OsKeyring {
    fn get(&self, account: &str) -> Result<Option<String>, KeyringError> {
        // `secret-tool lookup` exits non-zero when nothing matches.
        let output = linux::run(&linux::lookup(account))?;
        Ok(output.filter(|s| !s.is_empty()))
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), KeyringError> {
        linux::run(&linux::store(account, secret))?
            .map(|_| ())
            .ok_or(KeyringError)
    }

    fn delete(&self, account: &str) -> Result<(), KeyringError> {
        // `secret-tool clear` succeeds whether or not an entry matched.
        linux::run(&linux::clear(account))?
            .map(|_| ())
            .ok_or(KeyringError)
    }
}

#[cfg(not(any(unix, windows)))]
impl SecretStore for OsKeyring {
    fn get(&self, _account: &str) -> Result<Option<String>, KeyringError> {
        Err(KeyringError)
    }

    fn set(&self, _account: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(KeyringError)
    }

    fn delete(&self, _account: &str) -> Result<(), KeyringError> {
        Err(KeyringError)
    }
}

/// The `secret-tool` commands (Linux). Built as plain data so the tests
/// can check that the secret never lands on the argument list.
#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    use super::{KeyringError, SERVICE};

    /// One `secret-tool` run: its arguments and what goes to its stdin.
    #[derive(Debug)]
    pub struct Cmd {
        /// The arguments after `secret-tool`.
        pub args: Vec<String>,
        /// Written to stdin (the secret, for `store`).
        pub stdin: Option<String>,
    }

    fn attributes(account: &str) -> [String; 4] {
        [
            "service".into(),
            SERVICE.into(),
            "account".into(),
            account.into(),
        ]
    }

    /// `secret-tool lookup service vix-matrix account <account>`.
    pub fn lookup(account: &str) -> Cmd {
        let mut args = vec!["lookup".to_string()];
        args.extend(attributes(account));
        Cmd { args, stdin: None }
    }

    /// `secret-tool store --label … service vix-matrix account <account>`,
    /// the secret on stdin.
    pub fn store(account: &str, secret: &str) -> Cmd {
        let mut args = vec![
            "store".to_string(),
            "--label".into(),
            format!("{SERVICE} {account}"),
        ];
        args.extend(attributes(account));
        Cmd {
            args,
            stdin: Some(secret.to_string()),
        }
    }

    /// `secret-tool clear service vix-matrix account <account>`.
    pub fn clear(account: &str) -> Cmd {
        let mut args = vec!["clear".to_string()];
        args.extend(attributes(account));
        Cmd { args, stdin: None }
    }

    /// Run `cmd`: `Err` when `secret-tool` cannot be started, `Ok(None)` on
    /// a non-zero exit, else its stdout (trailing line break removed).
    /// stderr is discarded so nothing a backend prints can leak.
    pub fn run(cmd: &Cmd) -> Result<Option<String>, KeyringError> {
        let mut child = Command::new("secret-tool")
            .args(&cmd.args)
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| KeyringError)?;
        if let Some(input) = &cmd.stdin {
            // Dropping the handle closes the pipe, so the child sees EOF.
            let mut stdin = child.stdin.take().ok_or(KeyringError)?;
            stdin
                .write_all(input.as_bytes())
                .map_err(|_| KeyringError)?;
        }
        let output = child.wait_with_output().map_err(|_| KeyringError)?;
        if !output.status.success() {
            return Ok(None);
        }
        let text = String::from_utf8(output.stdout).map_err(|_| KeyringError)?;
        Ok(Some(text.trim_end_matches(['\r', '\n']).to_string()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_secret_goes_on_stdin_never_argv() {
            let cmd = store("home", "s3cret-token");
            assert!(!cmd.args.iter().any(|a| a.contains("s3cret")));
            assert_eq!(cmd.stdin.as_deref(), Some("s3cret-token"));
            assert!(cmd.args.contains(&"home".to_string()));
            assert!(cmd.args.contains(&SERVICE.to_string()));
            for cmd in [lookup("home"), clear("home")] {
                assert!(cmd.stdin.is_none());
            }
        }
    }
}

/// An in-memory [`SecretStore`] for the tests, and one that always fails.
#[cfg(test)]
pub mod memory {
    use std::collections::HashMap;
    use std::sync::{Mutex, PoisonError};

    use super::{KeyringError, SecretStore};

    /// Secrets in a map; never touches the OS.
    #[derive(Debug, Default)]
    pub struct MemoryKeyring(Mutex<HashMap<String, String>>);

    impl SecretStore for MemoryKeyring {
        fn get(&self, account: &str) -> Result<Option<String>, KeyringError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(account)
                .cloned())
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), KeyringError> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), KeyringError> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(account);
            Ok(())
        }
    }

    /// A keyring that refuses everything.
    #[derive(Debug, Default)]
    pub struct BrokenKeyring;

    impl SecretStore for BrokenKeyring {
        fn get(&self, _account: &str) -> Result<Option<String>, KeyringError> {
            Err(KeyringError)
        }

        fn set(&self, _account: &str, _secret: &str) -> Result<(), KeyringError> {
            Err(KeyringError)
        }

        fn delete(&self, _account: &str) -> Result<(), KeyringError> {
            Err(KeyringError)
        }
    }
}
