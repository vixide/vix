//! Schema compare and migrations (T785): snapshot one schema on two saved
//! connections, diff them, and write the migration that makes the target
//! look like the source.
//!
//! The pieces are pure and separately testable:
//!
//! - [`snapshot`] — the data model of one schema ([`Snapshot`]).
//! - [`read`] — fills a snapshot from catalog queries (`PostgreSQL` and
//!   `SQLite`; `MySQL` is not supported).
//!
//! The owning spec is `crates/vix-db/spec/index.md` ("Schema compare").

pub mod read;
pub mod snapshot;

pub use snapshot::Snapshot;
