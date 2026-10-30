//! [`RemoteTarget`]: any remote workspace name Vix accepts (T718) — an SSH
//! host ([`SshTarget`]) or a container ([`ContainerTarget`]) — so the App's
//! Open Remote dialog, `vix <name>`, the status bar, the session key and the
//! trust decision handle all of them the same way.

use crate::container::ContainerTarget;
use crate::ssh::{SshTarget, TargetError};

/// A remote workspace: where it is and the path there.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RemoteTarget {
    /// A host over `ssh` (`ssh://…`, `/ssh:…`).
    Ssh(SshTarget),
    /// A container, pod container or WSL distribution (`docker://…`,
    /// `podman://…`, `k8s://…`, `wsl://…`).
    Container(ContainerTarget),
}

impl RemoteTarget {
    /// Whether `name` uses one of the remote forms (it may still fail to
    /// parse).
    #[must_use]
    pub fn is_remote(name: &str) -> bool {
        SshTarget::is_remote(name) || ContainerTarget::is_container(name)
    }

    /// Parse any remote form.
    ///
    /// # Errors
    /// A [`TargetError`] naming the bad part, or
    /// [`TargetError::NotRemote`].
    pub fn parse(name: &str) -> Result<Self, TargetError> {
        if ContainerTarget::is_container(name) {
            ContainerTarget::parse(name).map(Self::Container)
        } else {
            SshTarget::parse(name).map(Self::Ssh)
        }
    }

    /// The canonical name (`ssh://…`, `docker://…`, …); the session key.
    #[must_use]
    pub fn to_url(&self) -> String {
        match self {
            Self::Ssh(t) => t.to_url(),
            Self::Container(t) => t.to_url(),
        }
    }

    /// The path on the remote side.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Ssh(t) => &t.path,
            Self::Container(t) => &t.path,
        }
    }

    /// Replace the path (an already resolved absolute path, or `~`).
    pub fn set_path(&mut self, path: String) {
        match self {
            Self::Ssh(t) => t.path = path,
            Self::Container(t) => t.path = path,
        }
    }

    /// A short label for the status bar and messages: `user@host:port`,
    /// `docker:name`, `k8s:ns/pod/ctr`, `wsl:distro`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Ssh(t) => {
                let mut label = String::new();
                if let Some(user) = &t.user {
                    label.push_str(user);
                    label.push('@');
                }
                label.push_str(&t.host);
                if let Some(port) = t.port {
                    label.push(':');
                    label.push_str(&port.to_string());
                }
                label
            }
            Self::Container(t) => t.label(),
        }
    }

    /// The name used in a "could not connect" message: the SSH host, or the
    /// container label.
    #[must_use]
    pub fn host(&self) -> String {
        match self {
            Self::Ssh(t) => t.host.clone(),
            Self::Container(t) => t.label(),
        }
    }
}

impl std::fmt::Display for RemoteTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_url())
    }
}

impl From<SshTarget> for RemoteTarget {
    fn from(t: SshTarget) -> Self {
        Self::Ssh(t)
    }
}

impl From<ContainerTarget> for RemoteTarget {
    fn from(t: ContainerTarget) -> Self {
        Self::Container(t)
    }
}

#[cfg(feature = "ssh")]
pub use connect::{ConnectOptions, Connection};

#[cfg(feature = "ssh")]
mod connect {
    use std::io;
    use std::sync::Arc;

    use super::RemoteTarget;
    use crate::Authority;
    use crate::container::{ContainerAuthority, ExecOptions};
    use crate::ssh::{AgentMode, AgentStatus, SshAuthority, SshOptions};

    /// How [`RemoteTarget::connect`] runs `ssh` and the container engines.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct ConnectOptions {
        /// For an `ssh://` name.
        pub ssh: SshOptions,
        /// For a container name.
        pub exec: ExecOptions,
    }

    impl ConnectOptions {
        /// Options with the remote agent `mode` for both kinds.
        #[must_use]
        pub fn with_agent(mut self, mode: AgentMode) -> Self {
            self.ssh = self.ssh.with_agent(mode);
            self.exec = self.exec.with_agent(mode);
            self
        }

        /// These options with `ssh` for `ssh://` names.
        #[must_use]
        pub fn with_ssh(mut self, ssh: SshOptions) -> Self {
            self.ssh = ssh;
            self
        }

        /// These options with `exec` for container names.
        #[must_use]
        pub fn with_exec(mut self, exec: ExecOptions) -> Self {
            self.exec = exec;
            self
        }
    }

    /// A connected authority, ready to install.
    #[derive(Debug)]
    #[non_exhaustive]
    pub struct Connection {
        /// The authority.
        pub authority: Arc<dyn Authority>,
        /// What became of the remote agent (T717).
        pub agent: AgentStatus,
    }

    impl RemoteTarget {
        /// Connect: build the authority for this name and check that it
        /// answers (and start the agent, as the options say). Only an SSH
        /// connect with `interactive` may prompt (on the terminal, before
        /// the UI); a container connect never does.
        ///
        /// # Errors
        /// The authority could not be built, or the connection, engine or
        /// container failed.
        pub fn connect(
            &self,
            options: &ConnectOptions,
            interactive: bool,
        ) -> io::Result<Connection> {
            match self {
                Self::Ssh(t) => {
                    let auth = SshAuthority::with_options(t.clone(), options.ssh.clone())?;
                    auth.connect(interactive)?;
                    let agent = auth.agent_status();
                    Ok(Connection {
                        authority: Arc::new(auth),
                        agent,
                    })
                }
                Self::Container(t) => {
                    let auth = ContainerAuthority::new(t.clone(), options.exec.clone());
                    auth.connect()?;
                    let agent = auth.agent_status();
                    Ok(Connection {
                        authority: Arc::new(auth),
                        agent,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_parses_and_round_trips() {
        for name in [
            "ssh://me@box:2222/srv",
            "docker://web/srv",
            "podman://u@api/~",
            "k8s://ns/pod/ctr/app",
            "wsl://Ubuntu/home/me",
        ] {
            let t = RemoteTarget::parse(name).unwrap();
            assert!(RemoteTarget::is_remote(name));
            assert_eq!(RemoteTarget::parse(&t.to_url()).unwrap(), t);
        }
        assert!(!RemoteTarget::is_remote("src/main.rs"));
        assert_eq!(RemoteTarget::parse("x"), Err(TargetError::NotRemote));
    }

    #[test]
    fn labels_and_paths() {
        let mut t = RemoteTarget::parse("ssh://me@box:2222/srv").unwrap();
        assert_eq!(t.label(), "me@box:2222");
        assert_eq!(t.host(), "box");
        t.set_path("/x".into());
        assert_eq!(t.to_url(), "ssh://me@box:2222/x");
        let t = RemoteTarget::parse("docker://web/srv").unwrap();
        assert_eq!((t.label().as_str(), t.path()), ("docker:web", "/srv"));
    }
}
