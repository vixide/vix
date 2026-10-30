//! The shell-based SSH authority (T712) and the remote path names it uses.
//!
//! [`SshTarget`] parses `ssh://user@host:port/path` and TRAMP's
//! `/ssh:user@host#port:/path`; [`quote`] holds the shell quoting every
//! remote command goes through. Both are pure and always built. The
//! authority itself, `SshAuthority`, needs the `ssh` feature; with it, the
//! authority can also find, upload and start the remote agent (T717,
//! `AgentMode`).
//!
//! `SshAuthority` is a `ShellAuthority` over the `ssh` `Transport` (T718):
//! the scripts, their quoting and error codes, and the remote agent are
//! shared with the container authorities in [`crate::container`], which run
//! the same scripts through `docker exec`, `podman exec`, `kubectl exec` or
//! `wsl.exe` instead.

#[cfg(feature = "ssh")]
mod agent_boot;
#[cfg(feature = "ssh")]
mod authority;
pub mod quote;
#[cfg(feature = "ssh")]
mod stat;
pub(crate) mod target;
#[cfg(feature = "ssh")]
mod transport;

#[cfg(feature = "ssh")]
pub use agent_boot::{AgentMode, AgentStatus, same_platform};
#[cfg(feature = "ssh")]
pub use authority::{ShellAuthority, SshAuthority, SshOptions, SshTransport};
pub use target::{SshTarget, TargetError};
#[cfg(feature = "ssh")]
pub use transport::Transport;
