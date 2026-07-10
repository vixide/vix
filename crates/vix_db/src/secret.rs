//! Credential waterfall (M7): resolve a connection's password without
//! prompting, and optionally save a prompted one for next time.
//!
//! The order, tried before the interactive prompt: the connection's
//! `password_command` (any command that prints the secret — `pass`, `op read`,
//! a wrapper script), then the OS keyring via its CLI (`security` on macOS,
//! `secret-tool` on Linux). On a supported platform a prompted password can be
//! stored back so later connects skip the prompt.
//!
//! sqlx does not read `~/.pgpass` / `~/.my.cnf`, so those are deliberately not
//! part of the waterfall. The command *construction* is pure and unit-tested;
//! only [`run`] touches the process table.

use super::connect::Connection;

/// The keyring service name under which Vix stores database passwords.
const SERVICE: &str = "vix-db";

/// One external command to run: the program, its arguments, and an optional
/// value piped to its stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    /// Executable name (resolved via `PATH`).
    pub program: String,
    /// Arguments, in order.
    pub args: Vec<String>,
    /// Text written to the command's stdin (e.g. the password for a store).
    pub stdin: Option<String>,
}

/// The keyring account key for `conn` — stable per saved connection.
fn account(conn: &Connection) -> String {
    conn.name.clone()
}

/// The command that reads `conn`'s password from the OS keyring, or `None` on
/// a platform without a supported keyring CLI.
#[must_use]
pub fn keyring_lookup(conn: &Connection) -> Option<Cmd> {
    let account = account(conn);
    if cfg!(target_os = "macos") {
        Some(Cmd {
            program: "security".into(),
            args: vec![
                "find-generic-password".into(),
                "-w".into(),
                "-s".into(),
                SERVICE.into(),
                "-a".into(),
                account,
            ],
            stdin: None,
        })
    } else if cfg!(target_os = "linux") {
        Some(Cmd {
            program: "secret-tool".into(),
            args: vec!["lookup".into(), "service".into(), SERVICE.into(), "account".into(), account],
            stdin: None,
        })
    } else {
        None
    }
}

/// The command that stores `password` for `conn` in the OS keyring, or `None`
/// on an unsupported platform. macOS takes the secret as an argument; Linux's
/// `secret-tool` reads it from stdin.
#[must_use]
pub fn keyring_store(conn: &Connection, password: &str) -> Option<Cmd> {
    let account = account(conn);
    if cfg!(target_os = "macos") {
        Some(Cmd {
            program: "security".into(),
            args: vec![
                "add-generic-password".into(),
                "-U".into(), // update if it already exists
                "-s".into(),
                SERVICE.into(),
                "-a".into(),
                account,
                "-w".into(),
                password.into(),
            ],
            stdin: None,
        })
    } else if cfg!(target_os = "linux") {
        Some(Cmd {
            program: "secret-tool".into(),
            args: vec![
                "store".into(),
                "--label".into(),
                format!("vix-db {account}"),
                "service".into(),
                SERVICE.into(),
                "account".into(),
                account,
            ],
            stdin: Some(password.to_string()),
        })
    } else {
        None
    }
}

/// Run `cmd`, returning its trimmed stdout on a zero exit, or `None` on any
/// failure (missing program, non-zero exit, unreadable output).
#[must_use]
pub fn run(cmd: &Cmd) -> Option<String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new(&cmd.program)
        .args(&cmd.args)
        .stdin(if cmd.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(input) = &cmd.stdin {
        child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolve `conn`'s password non-interactively: try its `password_command`,
/// then the OS keyring. `None` means "fall back to prompting".
#[must_use]
pub fn resolve(conn: &Connection) -> Option<String> {
    let command = conn.password_command.trim();
    if !command.is_empty() {
        let cmd = Cmd {
            program: "sh".into(),
            args: vec!["-c".into(), command.to_string()],
            stdin: None,
        };
        if let Some(pw) = run(&cmd).filter(|pw| !pw.is_empty()) {
            return Some(pw);
        }
    }
    keyring_lookup(conn).and_then(|cmd| run(&cmd)).filter(|pw| !pw.is_empty())
}

/// Store `password` for `conn` in the keyring (best effort); `true` on success.
#[must_use]
pub fn store(conn: &Connection, password: &str) -> bool {
    keyring_store(conn, password).is_some_and(|cmd| run(&cmd).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str) -> Connection {
        Connection { name: name.into(), ..Connection::default() }
    }

    #[test]
    fn lookup_and_store_commands_are_well_formed_per_platform() {
        let c = conn("prod");
        match (keyring_lookup(&c), keyring_store(&c, "hunter2")) {
            (Some(lookup), Some(store)) => {
                assert!(lookup.args.contains(&"prod".to_string()), "account in lookup: {lookup:?}");
                assert!(lookup.args.contains(&SERVICE.to_string()), "service in lookup");
                assert!(lookup.stdin.is_none(), "lookup takes no stdin");
                // The secret reaches the store either as an arg (macOS) or on
                // stdin (Linux), but never leaks into the lookup.
                let via_arg = store.args.iter().any(|a| a == "hunter2");
                let via_stdin = store.stdin.as_deref() == Some("hunter2");
                assert!(via_arg || via_stdin, "store carries the password: {store:?}");
                assert!(!lookup.args.iter().any(|a| a == "hunter2"));
            }
            (None, None) => { /* unsupported platform — both absent, consistent */ }
            other => panic!("lookup/store availability should match: {other:?}"),
        }
    }

    #[test]
    fn resolve_skips_an_empty_password_command() {
        // No password_command and (very likely) no keyring entry for this name.
        let c = conn("vix-test-nonexistent-xyz");
        assert!(c.password_command.is_empty());
        // resolve may hit the keyring CLI but must not find this bogus account.
        assert_eq!(resolve(&c), None);
    }

    #[test]
    fn password_command_stdout_is_used_and_trimmed() {
        let c = Connection {
            name: "echo".into(),
            password_command: "printf '  s3cret\\n'".into(),
            ..Connection::default()
        };
        assert_eq!(resolve(&c).as_deref(), Some("s3cret"));
    }
}
