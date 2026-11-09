//! Credential waterfall (M7): resolve a connection's password without
//! prompting, and optionally save a prompted one for next time.
//!
//! The order, tried before the interactive prompt: the connection's
//! `password_command` (any command that prints the secret — `pass`, `op read`,
//! a wrapper script), then the OS keyring. On a supported platform a prompted
//! password can be stored back so later connects skip the prompt.
//!
//! The keyring backend is platform-specific: macOS uses the native Security
//! framework and Windows the native Credential Manager (both via
//! `keyring-core`'s process-default store, which the App shell installs at
//! startup, T790) so the password never appears as a process argument;
//! Linux uses the `secret-tool` CLI with the secret passed on stdin. All
//! three keep the plaintext out of the process table. Any other platform has
//! no supported backend (Run H, T548: Windows support added — the `keyring`
//! crate already had it, just not enabled).
//!
//! sqlx does not read `~/.pgpass` / `~/.my.cnf`, so those are deliberately not
//! part of the waterfall. The command *construction* is pure and unit-tested;
//! only [`run`] and the keyring backends touch the OS.
//!
//! **The OS keyring is opt-in** (T639). Until [`use_system_keyring`] is
//! called, [`keyring_get`] finds nothing and [`keyring_set`] stores nothing,
//! so a server connection falls through to the password prompt. The binaries
//! opt in at startup; the test suite does not, so a test never waits on the
//! host's keyring (a Keychain lookup can take seconds) or reads a real
//! credential.
//!
//! Consumers: [`crate::connect`] builds the URL once a password is known;
//! the connect path in `lifecycle` calls [`resolve`] on a background thread
//! (T531) because a `password_command` or Keychain lookup can block for
//! seconds. Owning spec: `crates/vix-db/spec/index.md` ("Secure" section).

use std::sync::atomic::{AtomicBool, Ordering};

use super::connect::Connection;

/// The keyring service name under which Vix stores database passwords.
const SERVICE: &str = "vix-db";

/// Whether [`keyring_get`] / [`keyring_set`] reach the OS keyring. Off until
/// [`use_system_keyring`] turns it on. A process-global flag (rather than a
/// parameter) because the opt-in is a once-per-process decision made by the
/// binary; `Relaxed` ordering suffices since no other data is published with it.
static SYSTEM_KEYRING: AtomicBool = AtomicBool::new(false);

/// Route [`keyring_get`] / [`keyring_set`] to the OS keyring for the rest of
/// the process. The `vix` binary and the desktop GUIs call this once at
/// startup, through `vix::use_system_keyring`, which also installs the macOS
/// or Windows credential store as `keyring-core`'s default (T790); anything
/// that does not (the test suite above all) never touches the keyring.
pub fn use_system_keyring() {
    SYSTEM_KEYRING.store(true, Ordering::Relaxed);
}

/// Whether the OS keyring is in use (see [`use_system_keyring`]).
#[must_use]
pub fn is_system_keyring() -> bool {
    SYSTEM_KEYRING.load(Ordering::Relaxed)
}

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
///
/// It is the connection *name*, so renaming a saved connection orphans its
/// stored password (the user is simply prompted again and re-saves it).
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

/// Read `conn`'s password from the OS keyring, or `None` when it is absent,
/// the platform has no supported keyring, or the keyring is not opted into
/// ([`use_system_keyring`]). On macOS this uses the native
/// Security framework, on Windows the native Credential Manager (no secret on
/// the process argument list either way); on Linux it runs `secret-tool
/// lookup`.
#[must_use]
pub fn keyring_get(conn: &Connection) -> Option<String> {
    if !is_system_keyring() {
        return None;
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        keyring_core::Entry::new(SERVICE, &account(conn))
            .ok()?
            .get_password()
            .ok()
            .filter(|pw| !pw.is_empty())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(&secret_tool_lookup(conn)).filter(|pw| !pw.is_empty())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = conn;
        None
    }
}

/// Store `password` for `conn` in the OS keyring; `true` on success. macOS
/// and Windows use their native credential stores (the secret is never a
/// process argument); Linux runs `secret-tool store` with the secret on
/// stdin. Unsupported platforms, and a process that has not opted in with
/// [`use_system_keyring`], return `false`.
#[must_use]
pub fn keyring_set(conn: &Connection, password: &str) -> bool {
    if !is_system_keyring() {
        return false;
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        keyring_core::Entry::new(SERVICE, &account(conn))
            .and_then(|entry| entry.set_password(password))
            .is_ok()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(&secret_tool_store(conn, password)).is_some()
    }
    #[cfg(not(any(unix, windows)))]
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
    // stdout is captured; stderr is discarded so a failing helper cannot leak
    // fragments of a secret (or noise) into the TUI's terminal.
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
        // `take()` drops the handle on return, closing the pipe so the child
        // sees EOF on its stdin and can finish.
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
///
/// The `password_command` is run through `sh -c`, so it may use pipes and
/// quoting; it is user-authored configuration, not untrusted input. An empty
/// output counts as failure so a misconfigured command falls through to the
/// keyring instead of connecting with an empty password.
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
///
/// Called only when the connection has `store_keyring` set; returns `false`
/// (and stores nothing) unless [`use_system_keyring`] was called.
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

    // Tests never opt into the OS keyring (T639), so the keyring is never
    // read or written and a lookup can't wait on the host's Keychain.
    #[test]
    fn keyring_is_off_until_opted_into() {
        assert!(!is_system_keyring());
        let c = conn("vix-test-t639-not-opted-in");
        assert_eq!(keyring_get(&c), None);
        assert!(!keyring_set(&c, "hunter2"));
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
        assert_eq!(c.password_command, "");
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
