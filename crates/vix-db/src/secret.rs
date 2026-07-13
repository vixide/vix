//! Credential waterfall (M7): resolve a connection's password without
//! prompting, and optionally save a prompted one for next time.
//!
//! The order, tried before the interactive prompt: the connection's
//! `password_command` (any command that prints the secret — `pass`, `op read`,
//! a wrapper script), then the OS keyring. On a supported platform a prompted
//! password can be stored back so later connects skip the prompt.
//!
//! The keyring backend is platform-specific: macOS uses the native Security
//! framework (via the `keyring` crate) so the password never appears as a
//! process argument; Linux uses the `secret-tool` CLI with the secret passed on
//! stdin. Both keep the plaintext out of the process table.
//!
//! sqlx does not read `~/.pgpass` / `~/.my.cnf`, so those are deliberately not
//! part of the waterfall. The command *construction* is pure and unit-tested;
//! only [`run`] and the keyring backends touch the OS.

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

/// The `secret-tool lookup` command reading `conn`'s password (Linux).
#[cfg(all(unix, not(target_os = "macos")))]
fn secret_tool_lookup(conn: &Connection) -> Cmd {
    Cmd {
        program: "secret-tool".into(),
        args: vec![
            "lookup".into(),
            "service".into(),
            SERVICE.into(),
            "account".into(),
            account(conn),
        ],
        stdin: None,
    }
}

/// The `secret-tool store` command saving `password` for `conn` (Linux). The
/// secret travels on stdin, never as an argument.
#[cfg(all(unix, not(target_os = "macos")))]
fn secret_tool_store(conn: &Connection, password: &str) -> Cmd {
    Cmd {
        program: "secret-tool".into(),
        args: vec![
            "store".into(),
            "--label".into(),
            format!("vix-db {}", account(conn)),
            "service".into(),
            SERVICE.into(),
            "account".into(),
            account(conn),
        ],
        stdin: Some(password.to_string()),
    }
}

/// Read `conn`'s password from the OS keyring, or `None` when it is absent or
/// the platform has no supported keyring. On macOS this uses the native
/// Security framework (no secret on the process argument list); on Linux it
/// runs `secret-tool lookup`.
#[must_use]
pub fn keyring_get(conn: &Connection) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        keyring::Entry::new(SERVICE, &account(conn))
            .ok()?
            .get_password()
            .ok()
            .filter(|pw| !pw.is_empty())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(&secret_tool_lookup(conn)).filter(|pw| !pw.is_empty())
    }
    #[cfg(not(unix))]
    {
        let _ = conn;
        None
    }
}

/// Store `password` for `conn` in the OS keyring; `true` on success. macOS uses
/// the native Security framework (the secret is never a process argument);
/// Linux runs `secret-tool store` with the secret on stdin. Unsupported
/// platforms return `false`.
#[must_use]
pub fn keyring_set(conn: &Connection, password: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        keyring::Entry::new(SERVICE, &account(conn))
            .and_then(|entry| entry.set_password(password))
            .is_ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(&secret_tool_store(conn, password)).is_some()
    }
    #[cfg(not(unix))]
    {
        let _ = (conn, password);
        false
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
        .stdin(if cmd.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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
    keyring_get(conn)
}

/// Store `password` for `conn` in the keyring (best effort); `true` on success.
#[must_use]
pub fn store(conn: &Connection, password: &str) -> bool {
    keyring_set(conn, password)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str) -> Connection {
        Connection {
            name: name.into(),
            ..Connection::default()
        }
    }

    // On Linux the keyring store command must carry the secret on stdin, never
    // as a process argument (visible in `ps`). macOS no longer builds a command
    // at all — it uses the native Security framework — so this test is Linux-only.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn secret_tool_store_puts_the_password_on_stdin_not_argv() {
        let c = conn("prod");
        let store = secret_tool_store(&c, "hunter2");
        assert!(
            !store.args.iter().any(|a| a == "hunter2"),
            "password must not be a process argument: {store:?}"
        );
        assert_eq!(store.stdin.as_deref(), Some("hunter2"), "secret on stdin");
        assert!(store.args.contains(&"prod".to_string()), "account present");
        assert!(store.args.contains(&SERVICE.to_string()), "service present");

        // The lookup carries neither stdin nor the secret.
        let lookup = secret_tool_lookup(&c);
        assert!(lookup.stdin.is_none());
        assert!(!lookup.args.iter().any(|a| a == "hunter2"));
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
