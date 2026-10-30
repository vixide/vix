//! Container authorities (T718): a running Docker or Podman container, a
//! container in a Kubernetes pod, or a WSL distribution as the workspace.
//!
//! [`ContainerTarget`] parses `docker://[user@]container/path`,
//! `podman://…`, `k8s://namespace/pod/container/path` and
//! `wsl://[user@]distribution/path`; it is pure and always built.
//! `ContainerAuthority` (the `ssh` feature) is the SSH authority's
//! `ShellAuthority` over an `ExecTransport`: the same scripts, quoting,
//! error codes and remote agent, run through the engine's `exec` instead of
//! `ssh`. See `crates/vix-authority/spec/container/index.md`.

#[cfg(feature = "ssh")]
mod exec;
mod target;

#[cfg(feature = "ssh")]
pub use exec::{ContainerAuthority, ExecOptions, ExecTransport};
pub use target::{ContainerTarget, ExecKind};
