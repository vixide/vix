//! Schema compare and migrations (T785): snapshot one schema on two saved
//! connections, diff them, and write the migration that makes the target
//! look like the source.
//!
//! The pieces are pure and separately testable:
//!
//! - [`snapshot`] — the data model of one schema ([`Snapshot`]).
//! - [`read`] — fills a snapshot from catalog queries (`PostgreSQL` and
//!   `SQLite`; `MySQL` is not supported).
//! - [`diff`] — the changes between two snapshots, each with its
//!   migration statements and destructive/risky flags.
//! - [`plan`] — orders the chosen changes' statements so the migration
//!   runs, rebuilding views that block a change.
//! - [`review`] — the workbench screens: pick the target, review, copy,
//!   save, open or run the migration.
//!
//! The owning spec is `crates/vix-db/spec/index.md` ("Schema compare").

pub mod diff;
pub mod plan;
pub mod read;
pub mod review;
pub mod snapshot;

pub use snapshot::Snapshot;
