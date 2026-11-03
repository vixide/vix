//! SSH tunnels (M6): forward a local port to the database host over `ssh`.
//!
//! When a [`Connection`] has an `ssh_host`, the workbench opens an
//! `ssh -N -L <local>:<db_host>:<db_port>` tunnel to a free local port, waits
//! for it to accept connections, and points the sqlx URL at
//! `127.0.0.1:<local>` (see [`crate::connect::url_via_local`]). The tunnel
//! child's lifetime is tied to the [`Tunnel`] — dropping it kills `ssh`, so a
//! disconnect tears the forward down. The argument construction is pure and
//! unit-tested; only [`open`] spawns a process.
//!
//! # Security
//!
//! The `ssh` argv is built from saved-connection fields that can arrive from a
//! shared or imported config, so they are untrusted: [`ssh_args`] refuses any
//! value that `ssh` could parse as an option (a leading `-`, e.g.
//! `-oProxyCommand=…`) or that contains whitespace/control characters, and
//! ends options with `--` before the destination. No shell is involved; the
//! arguments are passed to `Command` directly. The child's stdio is all null,
//! so `ssh` can never prompt on (or scribble over) the TUI's terminal -- key
//! or agent authentication is therefore required. Owning spec:
//! `crates/vix-db/spec/index.md`.

use super::connect::{Connection, Kind};
use std::net::TcpListener;
use std::process::Child;
use std::time::{Duration, Instant};

/// How long to wait for the forwarded local port to come up.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// A running `ssh` forward. Dropping it kills the child, closing the tunnel.
#[derive(Debug)]
pub struct Tunnel {
    /// The `ssh -N -L …` child process.
    child: Child,
    /// The local port the database URL should target.
    pub local_port: u16,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Why an SSH tunnel could not be set up.
#[derive(Debug)]
#[non_exhaustive]
pub enum TunnelError {
    /// A connection field could be misread by `ssh` as an option, or holds
    /// whitespace/control characters (the argument-injection guard).
    InvalidField(String),
    /// No local port was free to forward.
    NoPort,
    /// `ssh` could not be found: it isn't installed, or isn't on `PATH`.
    NotInstalled,
    /// `ssh` could not be started for another reason.
    Spawn(std::io::Error),
    /// The forward did not come up within the ten-second ready timeout.
    Timeout,
}

impl std::fmt::Display for TunnelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField(why) => f.write_str(why),
            Self::NoPort => f.write_str(&t!("msg.db_tunnel_no_port")),
            Self::NotInstalled => f.write_str(&t!("msg.db_tunnel_ssh_missing")),
            Self::Spawn(e) => f.write_str(&t!("msg.db_tunnel_spawn", error = e.to_string())),
            Self::Timeout => f.write_str(&t!("msg.db_tunnel_timeout")),
        }
    }
}

impl std::error::Error for TunnelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TunnelError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotInstalled
        } else {
            Self::Spawn(e)
        }
    }
}

/// The `-L` forward spec `local:db_host:db_port`.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     vix_db::tunnel::forward_spec(55000, "db.internal", "5432"),
///     "55000:db.internal:5432"
/// );
/// ```
#[must_use]
pub fn forward_spec(local_port: u16, db_host: &str, db_port: &str) -> String {
    format!("{local_port}:{db_host}:{db_port}")
}

/// Reject a connection value that could be misparsed by `ssh` as an option or
/// smuggle extra tokens. A value beginning with `-` is treated by `ssh` as an
/// option — e.g. an `ssh_host` of `-oProxyCommand=…` yields arbitrary command
/// execution — and whitespace/control characters can split or corrupt the
/// argument. Connections are `Serialize`/`Deserialize` and can arrive from a
/// shared/imported config, so these fields are not fully trusted.
fn reject_option_like(label: &str, value: &str) -> Result<(), TunnelError> {
    let v = value.trim();
    if v.starts_with('-') {
        return Err(TunnelError::InvalidField(format!(
            "{label} must not start with '-': {value:?}"
        )));
    }
    if v.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(TunnelError::InvalidField(format!(
            "{label} must not contain whitespace or control characters"
        )));
    }
    Ok(())
}

/// The full `ssh` argument vector for `conn`, forwarding `local_port` to the
/// connection's database host/port. `-N` (no remote command) and
/// `ExitOnForwardFailure=yes` (fail fast if the forward can't bind) are always
/// set; `-p` / `-i` are added when configured.
///
/// # Errors
/// Returns [`TunnelError::InvalidField`] when any connection field could be misinterpreted by `ssh`
/// as an option (a leading `-`) or contains whitespace/control characters — the
/// guard against SSH argument injection (`-oProxyCommand=…` → RCE).
pub fn ssh_args(conn: &Connection, local_port: u16) -> Result<Vec<String>, TunnelError> {
    let db_port = if conn.port.is_empty() {
        conn.kind.default_port()
    } else {
        &conn.port
    };
    // Validate every field that reaches the argv as (part of) a positional token
    // or an option value an attacker could hijack.
    reject_option_like("database host", &conn.host)?;
    reject_option_like("database port", db_port)?;
    reject_option_like("ssh host", &conn.ssh_host)?;
    if !conn.ssh_user.trim().is_empty() {
        reject_option_like("ssh user", &conn.ssh_user)?;
    }
    if !conn.ssh_port.trim().is_empty() {
        reject_option_like("ssh port", &conn.ssh_port)?;
    }
    if !conn.ssh_identity.trim().is_empty() {
        reject_option_like("ssh identity", &conn.ssh_identity)?;
    }

    let mut args = vec![
        "-N".to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-L".to_string(),
        forward_spec(local_port, conn.host.trim(), db_port.trim()),
    ];
    if !conn.ssh_port.trim().is_empty() {
        args.push("-p".to_string());
        args.push(conn.ssh_port.trim().to_string());
    }
    if !conn.ssh_identity.trim().is_empty() {
        args.push("-i".to_string());
        args.push(conn.ssh_identity.trim().to_string());
    }
    // End-of-options marker so the destination can never be read as a flag even
    // if a future edit relaxes the leading-dash check above.
    args.push("--".to_string());
    let host = conn.ssh_host.trim();
    args.push(if conn.ssh_user.trim().is_empty() {
        host.to_string()
    } else {
        format!("{}@{host}", conn.ssh_user.trim())
    });
    Ok(args)
}

/// Whether `conn` asks to be tunnelled (a server engine with an `ssh_host`).
/// `SQLite` never tunnels, whatever its `ssh_host` says.
#[must_use]
pub fn wanted(conn: &Connection) -> bool {
    !matches!(conn.kind, Kind::Sqlite) && !conn.ssh_host.trim().is_empty()
}

/// Open a tunnel for `conn`, or `Ok(None)` when none is configured.
///
/// Blocks for up to the ready timeout while waiting for the forward, so the
/// host calls it from the background connect thread, not the UI thread.
///
/// # Errors
///
/// Returns a [`TunnelError`] if no local port is free, `ssh` is missing or
/// cannot be spawned, or the forward does not come up within `READY_TIMEOUT`
/// (10s).
pub fn open(conn: &Connection) -> Result<Option<Tunnel>, TunnelError> {
    if !wanted(conn) {
        return Ok(None);
    }
    // The port is probed free and then released before `ssh` binds it, so
    // another process could grab it in between; `ExitOnForwardFailure=yes`
    // turns that rare race into a prompt `Timeout` instead of a silent hang.
    let local_port = free_port().ok_or(TunnelError::NoPort)?;
    let args = ssh_args(conn, local_port)?;
    let child = std::process::Command::new("ssh")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(TunnelError::from)?;
    let mut tunnel = Tunnel { child, local_port };
    if wait_ready(local_port, &mut tunnel.child) {
        Ok(Some(tunnel))
    } else {
        // Kill explicitly (Drop would too) so the failure path is obvious,
        // then surface the timeout.
        let _ = tunnel.child.kill();
        Err(TunnelError::Timeout)
    }
}

/// Reserve a free local TCP port by binding to `:0` and dropping the listener.
fn free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()
        .map(|addr| addr.port())
}

/// Poll the local port until it accepts a connection (the forward is up) or the
/// timeout elapses. Returns early with `false` if `child` (the `ssh` process)
/// exits first — with `ExitOnForwardFailure=yes` a failed forward exits almost
/// immediately, so this avoids blocking the caller for the full timeout.
#[expect(
    clippy::expect_used,
    reason = "invariant established earlier in this function"
)]
fn wait_ready(local_port: u16, child: &mut Child) -> bool {
    use std::net::TcpStream;
    // Poll rather than sleep once: `ssh` gives no readiness signal, and the
    // only observable "up" state is the local port accepting a connection.
    let deadline = Instant::now() + READY_TIMEOUT;
    let addr = format!("127.0.0.1:{local_port}");
    while Instant::now() < deadline {
        // If ssh has already exited the forward will never come up.
        if matches!(child.try_wait(), Ok(Some(_))) {
            return false;
        }
        if TcpStream::connect_timeout(
            &addr.parse().expect("valid loopback addr"),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(ssh_user: &str, ssh_port: &str, identity: &str) -> Connection {
        Connection {
            kind: Kind::Postgres,
            host: "db.internal".into(),
            port: String::new(), // default 5432
            ssh_host: "bastion.example.com".into(),
            ssh_user: ssh_user.into(),
            ssh_port: ssh_port.into(),
            ssh_identity: identity.into(),
            ..Connection::default()
        }
    }

    #[test]
    fn forward_spec_is_local_host_port() {
        assert_eq!(
            forward_spec(55000, "db.internal", "5432"),
            "55000:db.internal:5432"
        );
    }

    #[test]
    fn ssh_args_forward_to_the_db_host_and_default_port() {
        let args = ssh_args(&pg("joel", "", ""), 55000).unwrap();
        assert!(args.contains(&"-N".to_string()));
        assert!(
            args.windows(2)
                .any(|w| w == ["-L", "55000:db.internal:5432"]),
            "{args:?}"
        );
        assert_eq!(args.last().unwrap(), "joel@bastion.example.com");
        // The destination is separated by an end-of-options marker.
        assert_eq!(args[args.len() - 2], "--", "{args:?}");
        assert!(
            !args.contains(&"-p".to_string()),
            "no -p without an ssh port"
        );
        assert!(
            !args.contains(&"-i".to_string()),
            "no -i without an identity"
        );
    }

    #[test]
    fn ssh_args_add_port_identity_and_bare_host() {
        let args = ssh_args(&pg("", "2222", "/home/j/.ssh/id"), 6000).unwrap();
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]), "{args:?}");
        assert!(
            args.windows(2).any(|w| w == ["-i", "/home/j/.ssh/id"]),
            "{args:?}"
        );
        assert_eq!(
            args.last().unwrap(),
            "bastion.example.com",
            "no user@ prefix when unset"
        );
    }

    #[test]
    fn missing_ssh_binary_is_a_distinct_error() {
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(matches!(
            TunnelError::from(missing),
            TunnelError::NotInstalled
        ));
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(matches!(TunnelError::from(denied), TunnelError::Spawn(_)));
    }

    #[test]
    fn option_injection_is_an_invalid_field() {
        let mut c = pg("joel", "", "");
        c.ssh_host = "-oProxyCommand=evil".into();
        assert!(matches!(
            ssh_args(&c, 5000),
            Err(TunnelError::InvalidField(_))
        ));
    }

    #[test]
    fn ssh_args_reject_option_injection() {
        // A host/user/identity that ssh would parse as an option (ProxyCommand
        // → RCE) is refused before any process is spawned.
        let mut host = pg("joel", "", "");
        host.ssh_host = "-oProxyCommand=touch /tmp/pwned".into();
        assert!(
            ssh_args(&host, 5000).is_err(),
            "ssh_host option must be rejected"
        );

        let mut user = pg("", "", "");
        user.ssh_user = "-oProxyCommand=x".into();
        assert!(
            ssh_args(&user, 5000).is_err(),
            "ssh_user option must be rejected"
        );

        let mut ident = pg("joel", "", "");
        ident.ssh_identity = "-oProxyCommand=x".into();
        assert!(
            ssh_args(&ident, 5000).is_err(),
            "ssh_identity option must be rejected"
        );

        let mut dbhost = pg("joel", "", "");
        dbhost.host = "-oProxyCommand=x".into();
        assert!(
            ssh_args(&dbhost, 5000).is_err(),
            "db host option must be rejected"
        );

        // A benign connection still succeeds and never emits an attacker option.
        let args = ssh_args(&pg("joel", "", ""), 5000).unwrap();
        assert!(!args.iter().any(|a| a.contains("ProxyCommand")), "{args:?}");
    }

    #[test]
    fn ssh_args_reject_whitespace_and_control_chars() {
        let mut c = pg("joel", "", "");
        c.ssh_host = "bastion.example.com evil".into();
        assert!(
            ssh_args(&c, 5000).is_err(),
            "embedded space must be rejected"
        );
        c.ssh_host = "bastion\nHost *".into();
        assert!(ssh_args(&c, 5000).is_err(), "newline must be rejected");
    }

    #[test]
    fn wanted_only_for_server_engines_with_a_host() {
        assert!(wanted(&pg("j", "", "")));
        let mut no_host = pg("j", "", "");
        no_host.ssh_host.clear();
        assert!(!wanted(&no_host));
        let mut sqlite = pg("j", "", "");
        sqlite.kind = Kind::Sqlite;
        assert!(!wanted(&sqlite), "sqlite is a file, never tunnelled");
    }
}
