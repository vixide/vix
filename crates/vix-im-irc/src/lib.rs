//! An IRCv3 client for Vix's IM feature (`spec/index.md`).
//!
//! - [`codec`]: parse and write IRC lines (RFC 1459/2812 framing, IRCv3
//!   message tags, `server-time`). Pure; the parser never panics.
//! - [`ctcp`]: CTCP `ACTION` (`/me`).
//! - [`isupport`]: `CASEMAPPING`, `NICKLEN`, `CHANTYPES` from `005`.
//! - [`split`]: name checks, body cleaning, and UTF-8-safe splitting of long
//!   messages under the 512-byte line limit.
//! - [`client`]: the registration / `CAP` / SASL PLAIN / `PING` state
//!   machine, turning server lines into [`Event`]s and App [`Command`]s into
//!   lines. Pure.
//! - `net` (feature `net`): a background thread with TCP + TLS (`rustls`,
//!   `webpki-roots`) driving a [`Client`] and talking to the App over
//!   channels.
//!
//! This crate has its own event types and does not depend on `vix-im`; the
//! App converts between the two. Received text is passed on raw: sanitizing
//! it for display is the App's (and `vix-im`'s) job.

pub mod client;
pub mod codec;
pub mod ctcp;
pub mod isupport;
#[cfg(feature = "net")]
pub mod net;
pub mod split;

pub use client::{Client, Command, Config, ErrorKind, Event, Password, Sasl, Step};
pub use codec::{Line, Prefix, Tag};
pub use isupport::{CaseMapping, Isupport};
pub use split::TextError;
