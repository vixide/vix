//! [`ExecTransport`] and [`ContainerAuthority`] (T718): the shell authority's
//! scripts run through `docker exec`, `podman exec`, `kubectl exec` or
//! `wsl.exe --exec` instead of `ssh`.
//!
//! There is no container-specific file logic here: [`ExecTransport`] only
//! builds the engine command line and classifies engine failures, and
//! everything else (scripts, quoting, error codes, the agent) is
//! [`crate::ssh::ShellAuthority`], shared with SSH. Target parsing lives in
//! [`super::target`]. Owning spec: `crates/vix-authority/spec/container/index.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::target::{ContainerTarget, ExecKind};
use crate::ssh::quote::sh_argv;
use crate::ssh::{AgentMode, ShellAuthority, Transport};

/// How a [`ContainerAuthority`] runs its engine. The container, user and
/// path come from the [`ContainerTarget`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExecOptions {
    /// The program to run (`docker`, `podman`, `kubectl`, `wsl.exe`): `None`
    /// is the kind's own name, looked up on `PATH`; tests give an explicit
    /// path to a fake engine.
    pub program: Option<PathBuf>,
    /// Whether connecting starts the remote agent inside (T717); the App
    /// sets it from the `remote_agent` setting.
    pub agent: AgentMode,
    /// The binary [`AgentMode::Upload`] uploads; `None` is this process's own
    /// executable, when it is `vix`.
    pub agent_binary: Option<PathBuf>,
}

impl ExecOptions {
    /// These options running `program` as the engine.
    #[must_use]
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// These options with the remote agent `mode`.
    #[must_use]
    pub fn with_agent(mut self, mode: AgentMode) -> Self {
        self.agent = mode;
        self
    }

    /// These options uploading `binary` as the agent.
    #[must_use]
    pub fn with_agent_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.agent_binary = Some(binary.into());
        self
    }
}

/// The `exec` transport: one running container (pod container, WSL
/// distribution), entered with its engine's `exec` for each script.
#[derive(Debug)]
pub struct ExecTransport {
    /// Which container, engine kind, user and path.
    target: ContainerTarget,
    /// Engine program override and agent settings.
    options: ExecOptions,
}

impl ExecTransport {
    /// The engine command up to (not including) what runs inside: `docker
    /// exec -i [-t] [-u USER] [-e NAME]… CONTAINER`, `kubectl exec -i [-t] -n
    /// NS -c CTR POD --`, `wsl.exe -d DISTRO [-u USER] --exec`. Every value
    /// in it was checked when the target was parsed (nothing option-shaped);
    /// no environment *value* is ever on it.
    #[must_use]
    pub fn prefix(&self, tty: bool, env_names: &[&str]) -> Command {
        let t = &self.target;
        let program = self
            .options
            .program
            .clone()
            .unwrap_or_else(|| PathBuf::from(t.kind.program()));
        let mut cmd = Command::new(program);
        match t.kind {
            ExecKind::Docker | ExecKind::Podman => {
                cmd.args(["exec", "-i"]);
                if tty {
                    cmd.arg("-t");
                }
                if let Some(user) = &t.user {
                    cmd.arg("-u").arg(user);
                }
                for name in env_names {
                    cmd.arg("-e").arg(name);
                }
                cmd.arg(&t.name);
            }
            ExecKind::Kubernetes => {
                cmd.args(["exec", "-i"]);
                if tty {
                    cmd.arg("-t");
                }
                if let Some(ns) = &t.namespace {
                    cmd.arg("-n").arg(ns);
                }
                if let Some(ctr) = &t.container {
                    cmd.arg("-c").arg(ctr);
                }
                cmd.arg(&t.name).arg("--");
            }
            ExecKind::Wsl => {
                cmd.arg("-d").arg(&t.name);
                if let Some(user) = &t.user {
                    cmd.arg("-u").arg(user);
                }
                cmd.arg("--exec");
            }
        }
        cmd
    }

    /// The container this transport enters.
    #[must_use]
    pub fn target(&self) -> &ContainerTarget {
        &self.target
    }
}

/// Engine messages that mean the container (or the engine, the cluster,
/// the distribution) is gone rather than that a script failed.
///
/// Matched as substrings of the engine's stderr by [`Transport::lost`]. The
/// strings are the engines' own (untranslated) English messages, so this list
/// is best-effort: an unrecognized message surfaces as an ordinary script
/// failure rather than a lost connection.
const LOST: &[&str] = &[
    // Docker
    "Error response from daemon",
    "Cannot connect to the Docker daemon",
    "No such container",
    "is not running",
    // Podman
    "no container with name or ID",
    "can only create exec sessions on running containers",
    "unable to connect to Podman",
    "Cannot connect to Podman",
    // kubectl (messages captured from `kubectl exec` against a local kind
    // cluster, T718a; all exit with status 1): a missing pod or namespace
    // is `Error from server (NotFound): ...`, a missing container is
    // `error: container X is not valid for pod P ...`, a refused API
    // connection is `The connection to the server H:P was refused ...`, and
    // an unresolvable or silent one is `Unable to connect to the server: ...`.
    "Error from server",
    "unable to upgrade connection",
    "Unable to connect to the server",
    "The connection to the server",
    "is not valid for pod",
    "error: context \"",
    // WSL
    "There is no distribution with the supplied name",
    "WSL_E_DISTRO_NOT_FOUND",
];

impl Transport for ExecTransport {
    fn name(&self) -> &'static str {
        self.target.kind.name()
    }

    fn label(&self) -> String {
        self.target.label()
    }

    fn shell(&self, script: &[u8], tty: bool, env_names: &[&str]) -> Command {
        let mut cmd = self.prefix(tty, env_names);
        cmd.args(sh_argv(script));
        cmd
    }

    /// The engine's own failure: Docker's and Podman's 125, WSL's -1, or
    /// (for an exit status the scripts don't use) one of the engines'
    /// "no such container" / "can't connect" messages.
    fn lost(&self, status: i32, stderr: &str) -> bool {
        if (100..=112).contains(&status) {
            return false;
        }
        let engine_status = match self.target.kind {
            ExecKind::Docker | ExecKind::Podman => status == 125,
            ExecKind::Wsl => status == -1,
            ExecKind::Kubernetes => false,
        };
        engine_status || LOST.iter().any(|m| stderr.contains(m))
    }

    /// Docker and Podman take `-e NAME` and read the value from their own
    /// environment; `kubectl exec` has no environment option, and `wsl.exe`
    /// passes only `WSLENV`-listed variables, so for those the script
    /// exports them (on argv, as over `ssh`).
    fn passes_env_by_name(&self) -> bool {
        matches!(self.target.kind, ExecKind::Docker | ExecKind::Podman)
    }

    /// `$SHELL` is often unset in a container: then `bash` if there is one,
    /// else `/bin/sh`.
    fn login_shell(&self) -> &'static str {
        "s=${SHELL:-}\n\
         [ -n \"$s\" ] || s=$(command -v bash 2>/dev/null) || s=/bin/sh\n\
         exec \"$s\" -l\n"
    }

    fn agent_mode(&self) -> AgentMode {
        self.options.agent
    }

    fn agent_binary(&self) -> Option<&Path> {
        self.options.agent_binary.as_deref()
    }
}

/// A running container (a pod's container, a WSL distribution) as Vix's
/// workspace (T718): a [`ShellAuthority`] whose transport is
/// [`ExecTransport`], so it runs exactly the SSH authority's scripts (and,
/// when there is one, the remote agent) inside the container.
pub type ContainerAuthority = ShellAuthority<ExecTransport>;

impl ShellAuthority<ExecTransport> {
    /// An authority for `target`'s container, run as `options` says.
    /// Nothing runs until the first operation (or
    /// [`ContainerAuthority::connect`]).
    #[must_use]
    pub fn new(target: ContainerTarget, options: ExecOptions) -> Self {
        Self::over(ExecTransport { target, options })
    }

    /// The container this authority works in.
    #[must_use]
    pub fn target(&self) -> &ContainerTarget {
        &self.transport().target
    }

    /// Check that the container is running and has a usable `sh` and
    /// `stat`, and start the agent when the options ask for it. Never
    /// prompts.
    ///
    /// # Errors
    /// The engine failed (`NotConnected`: no such container, not running,
    /// no engine), or the container has no usable `sh`/`stat`.
    pub fn connect(&self) -> io::Result<()> {
        self.probe()
    }

    /// Stop the agent, if one runs; nothing else is held open.
    pub fn disconnect(&self) {
        self.stop_agent();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]
mod tests {
    //! `lost` against the engines' real output. The kubectl texts were
    //! captured from `kubectl exec` against a local `kind` cluster (T718a).

    use super::*;

    fn k8s() -> ExecTransport {
        ExecTransport {
            target: ContainerTarget::kubernetes("default", "vixpod", "vixpod").unwrap(),
            options: ExecOptions::default(),
        }
    }

    #[test]
    fn kubectl_failures_are_a_lost_connection() {
        let t = k8s();
        for text in [
            // missing pod
            "Error from server (NotFound): pods \"nopod\" not found",
            // missing namespace
            "Error from server (NotFound): namespaces \"nons\" not found",
            // missing container
            "error: container nocontainer is not valid for pod vixpod out of: vixpod",
            // API server stopped (the node's container stopped)
            "The connection to the server 127.0.0.1:62793 was refused - did you specify the right host or port?",
            // no kubeconfig at all
            "The connection to the server localhost:8080 was refused - did you specify the right host or port?",
            // unresolvable and blackholed API servers
            "Unable to connect to the server: dial tcp: lookup nosuch.invalid: no such host",
            "Unable to connect to the server: net/http: request canceled while waiting for connection (Client.Timeout exceeded while awaiting headers)",
            // a context that is not in the kubeconfig
            "error: context \"nosuchctx\" does not exist",
        ] {
            assert!(t.lost(1, text), "{text}");
        }
    }

    #[test]
    fn a_script_that_exits_nonzero_under_kubectl_is_not_lost() {
        let t = k8s();
        // `exit 7` and the Vix script codes: kubectl reports the status
        // itself, with this line on stderr.
        assert!(!t.lost(7, "command terminated with exit code 7"));
        assert!(!t.lost(1, "command terminated with exit code 1"));
        assert!(!t.lost(105, "command terminated with exit code 105"));
        assert!(!t.lost(1, "grep: nothing"));
        // The pod deleted or the node stopped mid-exec ends the session with
        // the container process's signal status (137, 143) and only that
        // line: indistinguishable from a killed script, so the next
        // operation (NotFound / connection refused) is what reports it lost.
        assert!(!t.lost(137, "command terminated with exit code 137"));
        assert!(!t.lost(143, "command terminated with exit code 143"));
    }
}
