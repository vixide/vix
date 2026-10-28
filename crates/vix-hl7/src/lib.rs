//! HL7 v2 (ER7, "pipe-and-hat") message support for Vix: a lenient parser, a
//! numbered field view, an acknowledgement reader, pure MLLP framing and -- behind
//! the `net` feature -- a blocking TCP sender.
//!
//! The core is pure: [`Message::parse`] never fails and never panics, [`mllp`]
//! only moves bytes between buffers, and nothing here logs or writes to disk,
//! because HL7 messages routinely carry protected health information (PHI).
//! The only I/O is the opt-in `net` module, which dials exactly the endpoint
//! the caller names.
//!
//! See `crates/vix-hl7/spec/index.md`.

mod ack;
mod escape;
mod message;
pub mod mllp;
#[cfg(feature = "net")]
pub mod net;
mod panel;
mod path;
mod view;

pub use ack::{Ack, AckCode};
pub use escape::{escape, unescape};
pub use message::{Component, Delimiters, Field, Message, Repetition, Segment};
pub use panel::Panel;
pub use path::{Path, PathError};
pub use view::{Row, RowKind, pretty, rows};
