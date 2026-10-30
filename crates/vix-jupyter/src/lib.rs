//! Jupyter kernel transport for Vix (T734, `spec/jupyter`).
//!
//! - [`discover`] — find kernelspecs in Jupyter's data directories (always
//!   built; reads files, starts nothing).
//! - [`KernelEvent`] — what a running kernel reports to the App.
//! - With the `process` feature: [`KernelHandle`] launches a kernel from its
//!   kernelspec (or connects to a running one) and talks to it over the
//!   pure-Rust `zeromq` crate on a worker thread that runs a
//!   `current_thread` tokio runtime. The App calls the synchronous handle
//!   and polls [`KernelHandle::events`] on its tick.
//!
//! Every launched kernel gets a fresh `0700` runtime directory and a `0600`
//! connection file holding a random key from the OS. The key is never put
//! on a command line or in a log. On Unix the sockets are IPC (Unix-domain
//! sockets in that directory), so no TCP port is opened; on Windows they are
//! TCP on `127.0.0.1`. Every received message's HMAC is verified before it
//! is parsed, and a message that fails is dropped
//! ([`KernelEvent::Rejected`]). The directory is removed at shutdown.

pub mod discover;
mod event;

#[cfg(feature = "process")]
mod kernel;
#[cfg(feature = "process")]
mod runtime_dir;
#[cfg(feature = "process")]
mod worker;

pub use event::{DeathReason, KernelEvent, KernelState, RequestId};
pub use vix_jupyter_core as core;

#[cfg(feature = "process")]
pub use kernel::{KernelHandle, LaunchError, LaunchOptions};
#[cfg(feature = "process")]
pub use runtime_dir::PrivateDir;
