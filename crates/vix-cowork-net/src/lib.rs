//! Encrypted, authenticated TCP transport for Vix Cowork.
//!
//! - [`Listener`] — the host: listens, authenticates guests with a Noise
//!   handshake bound to a single-use [`Invite`], and hands each to the UI as a
//!   [`Knock`] to approve or decline. Nothing is sent to an unapproved guest.
//! - [`join`] — the guest: connect from an invite string, pin the host's key,
//!   say hello, wait for consent.
//! - [`Link`] and [`LinkReader`] — the two halves of an established
//!   connection, carrying `vix_cowork::wire::Message`s as encrypted frames.
//!
//! Module map: `error` (the typed [`NetError`]), `handshake` (Noise setup and
//! the preamble), `invite` (token minting, parsing and the host's table),
//! `link` (framing, chunking and the reader/writer halves) and `server`
//! (accept loop, consent queue, `join`). The message vocabulary itself lives
//! in `vix_cowork::wire`; this crate only moves its bytes safely.
//!
//! Threading: the host runs one accept thread plus one short-lived thread per
//! handshake; a guest's [`LinkReader`] is typically moved onto its own thread
//! with [`LinkReader::spawn`]. Nothing here is async.
//!
//! There is no plaintext mode and no way to connect without an invite: the
//! handshake is `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s`, so the guest
//! authenticates the host by its pinned key and the host authenticates the
//! guest by the invite's secret. See `spec/cowork` and this crate's spec.

// Private modules; the public surface is the re-exports below.
mod error;
mod handshake;
mod invite;
mod link;
mod server;

pub use error::NetError;
pub use handshake::{WIRE_VERSION, connect};
pub use invite::{Invite, InviteTable, MAX_OUTSTANDING};
pub use link::{Link, LinkEvent, LinkReader};
pub use server::{Joined, Knock, Listener, MAX_HANDSHAKES, MAX_PENDING_KNOCKS, join};
