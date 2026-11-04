//! An Agent Client Protocol (ACP) client for Vix (T635).
//!
//! The protocol is pure: [`wire`] frames newline-delimited JSON-RPC,
//! [`client::Client`] is a state machine with no IO, and [`policy`] decides
//! which paths and permission options an agent may have. Only [`process`]
//! (behind the `process` feature) starts the agent. See `spec/index.md` for the
//! trust model.

pub mod client;
pub mod policy;
#[cfg(feature = "process")]
pub mod process;
#[cfg(feature = "process")]
pub mod terminal;
pub mod wire;

pub use client::{Client, Event, OptionKind, PermOption, Phase, PlanEntry, Step, TerminalExit};
