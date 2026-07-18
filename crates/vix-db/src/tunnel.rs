//! SSH tunnels (M6): forward a local port to the database host over `ssh`.
//!
//! When a [`Connection`] has an `ssh_host`, the workbench opens an
//! `ssh -N -L <local>:<db_host>:<db_port>` tunnel to a free local port, waits
//! for it to accept connections, and points the sqlx URL at
//! `127.0.0.1:<local>` (see [`crate::connect::url_via_local`]). The tunnel
//! child's lifetime is tied to the [`Tunnel`] — dropping it kills `ssh`, so a
//! disconnect tears the forward down. The argument construction is pure and
//! unit-tested; only [`open`] spawns a process.

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

/// The `-L` forward spec `local:db_host:db_port`.
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
fn reject_option_like(label: &str, value: &str) -> Result<(), String> {
    let v = value.trim();
    if v.starts_with('-') {
        return Err(format!("{label} must not start with '-': {value:?}"));
    }
    if v.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "{label} must not contain whitespace or control characters"
        ));
    }
    Ok(())
}

/// The full `ssh` argument vector for `conn`, forwarding `local_port` to the
/// connection's database host/port. `-N` (no remote command) and
/// `ExitOnForwardFailure=yes` (fail fast if the forward can't bind) are always
/// set; `-p` / `-i` are added when configured.
///
/// # Errors
/// Returns a message when any connection field could be misinterpreted by `ssh`
/// as an option (a leading `-`) or contains whitespace/control characters — the
/// guard against SSH argument injection (`-oProxyCommand=…` → RCE).
pub fn ssh_args(conn: &Connection, local_port: u16) -> Result<Vec<String>, String> {
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
#[must_use]
pub fn wanted(conn: &Connection) -> bool {
    !matches!(conn.kind, Kind::Sqlite) && !conn.ssh_host.trim().is_empty()
}

/// Open a tunnel for `conn`, or `Ok(None)` when none is configured.
///
/// # Errors
///
/// Returns a display-ready message if no local port is free, `ssh` cannot be
/// spawned, or the forward does not come up within [`READY_TIMEOUT`].
pub fn open(conn: &Connection) -> Result<Option<Tunnel>, String> {
    if !wanted(conn) {
        return Ok(None);
    }
    let local_port = free_port().ok_or_else(|| t!("msg.db_tunnel_no_port").to_string())?;
    let args = ssh_args(conn, local_port)?;
    let child = std::process::Command::new("ssh")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| t!("msg.db_tunnel_spawn", error = e.to_string()).to_string())?;
    let mut tunnel = Tunnel { child, local_port };
    if wait_ready(local_port, &mut tunnel.child) {
        Ok(Some(tunnel))
    } else {
        // Drop kills the child; surface the failure.
        let _ = tunnel.child.kill();
        Err(t!("msg.db_tunnel_timeout").to_string())
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
fn wait_ready(local_port: u16, child: &mut Child) -> bool {
    use std::net::TcpStream;
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
