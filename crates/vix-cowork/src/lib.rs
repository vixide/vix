//! Pure real-time collaboration core for Vix's Cowork feature.
//!
//! No I/O lives here, so every rule is unit and property tested without a
//! socket or a terminal:
//!
//! - [`SharedDoc`] — one CRDT replica of a buffer, with local-edit capture,
//!   remote updates turned into [`BufferEdit`]s, cursor-stable [`Anchor`]s and
//!   per-peer undo.
//! - [`Peer`], [`PeerId`], [`Permission`], [`Action`] — who is in the session
//!   and what each may do. Everything defaults to view-only.
//! - [`SharedPath`] — the one door a guest-named path goes through.
//! - [`session`] — [`HostSession`] and [`GuestSession`]: the authority and the
//!   mirror, as plain state machines that return the messages to send.
//! - [`wire`] — the application [`Message`]s and their total, panic-free codec.
//! - [`presence`] — peer colors (contrast-checked) and the remote-cursor
//!   overlay model the renderer draws.
//!
//! The transport (`vix-cowork-net`) encrypts and authenticates; this crate
//! decides what an authenticated peer may do. See `spec/cowork`.

pub mod doc;
pub mod error;
pub mod path;
pub mod peer;
pub mod presence;
pub mod session;
pub mod wire;

pub use doc::{Anchor, BufferEdit, DEFAULT_CAP_BYTES, Reverted, SharedDoc, transform_offset};
pub use error::{CoworkError, PathProblem, WireProblem};
pub use path::SharedPath;
pub use peer::{Action, Peer, PeerId, Permission};
pub use session::{GuestEvent, GuestSession, Handled, HostSession, Outbound};
pub use wire::{Channel, ChatKind, ChatLine, ErrorCode, Message, Presence};
