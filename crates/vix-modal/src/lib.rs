//! A real modal-editing engine: mode state machine, operator × motion
//! composition, counts, registers, text objects, dot-repeat — the pieces
//! the Vi/Spacemacs keymaps' ad hoc `vim_normal_key` binding table (in the
//! App shell) does not have.
//!
//! Landing in slices (improvement plan T112–T115) — see `spec/index.md` for
//! the audit of what existed before this, and the v1 design. T112 (this
//! slice): the `Mode` enum, plus host wiring for Visual/Visual Line entry,
//! exit, and cursor-extending motion, behind the new `Settings::modal_engine`
//! flag (default off). `vim_normal_key`'s existing table keeps handling
//! everything else for now — T113/T114/T115 progressively replace pieces of
//! it with real composable motions/operators, per the spec's § Rollout.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod mode;

pub use mode::Mode;
