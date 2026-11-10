//! The Matrix bridge of Vix's IM feature (`spec/index.md`).
//!
//! Matrix runs out of process, in the `vix-matrix` helper (the `matrix/`
//! workspace), so `matrix-sdk` and its SQLite store never enter Vix's own
//! dependency graph (`spec/instant-message/matrix.md`). This crate is Vix's
//! side of that conversation:
//!
//! - [`protocol`]: the [`Request`]s Vix sends and the [`Reply`]s the helper
//!   sends back. The helper uses the same types.
//! - [`codec`]: one JSON object per line, with a protocol version. Pure;
//!   decoding never panics.
//! - `net` (feature `net`): find and start the helper, and pump its stdin
//!   and stdout on background threads, talking to the App over channels.
//!
//! Like `vix-im-irc`, this crate does not depend on `vix-im`; the App turns
//! replies into `vix-im` messages and sanitizes every received string.

pub mod codec;
#[cfg(feature = "net")]
pub mod net;
pub mod protocol;

pub use codec::{
    DecodeError, MAX_LINE, decode_reply, decode_request, encode_reply, encode_request,
};
pub use protocol::{ErrorKey, MessageKind, PROTOCOL_VERSION, Password, Reply, Request};
