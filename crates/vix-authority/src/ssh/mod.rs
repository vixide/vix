//! The shell-based SSH authority (T712) and the remote path names it uses.
//!
//! [`SshTarget`] parses `ssh://user@host:port/path` and TRAMP's
//! `/ssh:user@host#port:/path`; [`quote`] holds the shell quoting every
//! remote command goes through. Both are pure and always built. The
//! authority itself, `SshAuthority`, needs the `ssh` feature.

#[cfg(feature = "ssh")]
mod authority;
pub mod quote;
#[cfg(feature = "ssh")]
mod stat;
mod target;

#[cfg(feature = "ssh")]
pub use authority::{SshAuthority, SshOptions};
pub use target::{SshTarget, TargetError};
