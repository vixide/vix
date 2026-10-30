//! The remote agent (T717): `vix --remote-agent` on the remote machine,
//! speaking a framed binary protocol over the SSH channel's stdin and
//! stdout, so a file operation is one request rather than one `ssh` run, and
//! many stats are one round trip.
//!
//! - [`wire`]: the frame codec and the messages.
//! - [`serve`] / [`serve_stdio`]: the agent side, which answers requests
//!   with [`crate::LocalAuthority`] and starts no process.
//! - [`AgentAuthority`]: an [`crate::Authority`] over a connected agent.
//!   `SshAuthority` (the `ssh` feature) uses the same client when its
//!   agent is running, and its shell scripts otherwise.
//!
//! The handshake requires the agent to be exactly this build's Vix version
//! ([`VERSION`]) and protocol ([`wire::PROTOCOL`]). See
//! `crates/vix-authority/spec/agent/index.md`.

mod authority;
mod client;
mod server;
pub mod wire;

pub use authority::AgentAuthority;
pub use client::Watch;
#[cfg(feature = "ssh")]
pub(crate) use client::{AgentClient, Fail};
pub use server::{serve, serve_stdio};

/// The Vix version an agent must report: this crate's, which is the
/// workspace's (and so the `vix` binary's) version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
