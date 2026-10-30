//! [`Transport`]: how a [`super::ShellAuthority`] reaches the `/bin/sh` that
//! runs its scripts (T718). `ssh` is one transport ([`super::SshTransport`]);
//! `docker exec`, `podman exec`, `kubectl exec` and `wsl.exe` are another
//! ([`crate::container::ExecTransport`]). Everything above the transport —
//! the scripts, their quoting, the exit-status error codes, the remote agent —
//! is shared, so every transport has the same semantics.

use std::fmt::Debug;
use std::path::Path;
use std::process::Command;

use super::agent_boot::AgentMode;

/// A way to run a POSIX `sh` script on another machine (or in a container).
pub trait Transport: Send + Sync + Debug {
    /// The authority's [`crate::Authority::name`] (`"ssh"`, `"docker"`, …).
    fn name(&self) -> &'static str;

    /// What to put in front of an error message: the host, container or
    /// distribution (never a secret).
    fn label(&self) -> String;

    /// The command that runs `script` in `/bin/sh` on the other side, its
    /// stdin and stdout connected to the returned command's. `tty` asks for
    /// a pseudo-terminal there (the integrated terminal). `env_names` are
    /// variables the transport passes by *name* from the local command's
    /// environment (only asked of a transport whose
    /// [`Transport::passes_env_by_name`] is `true`); the caller sets their
    /// values on the returned command, so they never appear on its argv.
    fn shell(&self, script: &[u8], tty: bool, env_names: &[&str]) -> Command;

    /// Whether a script that exited with `status` and printed `stderr`
    /// failed because the transport itself did (the connection, the
    /// container or the engine), which the authority reports as
    /// `NotConnected` so the App can offer a reconnect. The scripts' own
    /// statuses are 100–112.
    fn lost(&self, status: i32, stderr: &str) -> bool;

    /// Whether [`Transport::shell`] can pass environment variables by name
    /// (`docker exec -e NAME`), so a process's environment values stay off
    /// every argv. `false`: they are exported by the script, which travels
    /// on argv (as with `ssh`).
    fn passes_env_by_name(&self) -> bool {
        false
    }

    /// The end of the integrated terminal's script: replace the shell with
    /// the user's login shell there.
    fn login_shell(&self) -> &'static str {
        "exec \"${SHELL:-/bin/sh}\" -l\n"
    }

    /// Whether connecting starts the remote agent (T717).
    fn agent_mode(&self) -> AgentMode;

    /// The binary [`AgentMode::Upload`] uploads; `None` is this process's own
    /// executable, when it is `vix`.
    fn agent_binary(&self) -> Option<&Path>;
}
