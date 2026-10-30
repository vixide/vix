//! The pure core of Vix's Jupyter kernel client (T734, `spec/jupyter`).
//!
//! Nothing here does I/O: no sockets, no processes, no files. The transport
//! crate `vix-jupyter` moves the bytes; this crate decides what they mean.
//!
//! - [`wire`] — the Jupyter wire protocol (v5.3) over ZeroMQ multipart
//!   frames: identities, the `<IDS|MSG>` delimiter, the HMAC-SHA256
//!   signature, and the four JSON parts. [`wire::decode`] checks size caps
//!   and the signature *before* it parses any JSON.
//! - [`message`] — the header and the typed contents of the message types
//!   Vix uses (`kernel_info`, `execute_*`, `stream`, `display_data`,
//!   `error`, `status`, `interrupt_*`, `shutdown_*`).
//! - [`connection`] — the connection file (`kernel-*.json`) model.
//! - [`kernelspec`] — `kernel.json` parsing, the `{connection_file}` argv
//!   substitution, and the kernelspec search path as a pure function of the
//!   environment.
//! - [`output`] — the `Output` model and MIME selection.
//! - [`ansi`] — ANSI escape stripping for tracebacks.
//! - [`cell`] — `# %%` code cells in a plain source file.
//! - [`org`] — the Org `#+RESULTS:` formatter (insert, replace, append,
//!   clear).
//! - [`babel`] — the Org header arguments a run honors (`:results`,
//!   `:file`, `:eval`) and the result text of a run's outputs.
//! - [`image`] — base64 `image/png` and `image/jpeg` decoding with size
//!   caps checked before any pixel is decoded.

pub mod ansi;
pub mod babel;
pub mod cell;
pub mod connection;
pub mod image;
pub mod kernelspec;
pub mod message;
pub mod org;
pub mod output;
pub mod wire;

pub use connection::{Channel, ConnectionError, ConnectionInfo, Transport};
pub use kernelspec::{InterruptMode, KernelSpec, Os, PathEnv, SpecError};
pub use message::{Content, Header, Message, PROTOCOL_VERSION};
pub use output::{Choice, MimeBundle, Output, Payload, StreamName};
pub use wire::{DecodeError, KeyError, Limits, Signer};
