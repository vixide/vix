//! A real modal-editing engine: mode state machine, operator × motion
//! composition, counts, registers, text objects, dot-repeat — the pieces
//! the Vi/Spacemacs keymaps' ad hoc `vim_normal_key` binding table (in the
//! App shell) does not have.
//!
//! Design-only for now (improvement plan T111) — see `spec/index.md` for
//! the audit of what exists today, and the v1 design for what replaces it.
//! The mode engine, motions, operators, text objects, and dot-repeat land
//! in T112–T115.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
