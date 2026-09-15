//! A real modal-editing engine: mode state machine, operator × motion
//! composition, counts, registers, text objects, dot-repeat — the pieces
//! the Vi/Spacemacs keymaps' ad hoc `vim_normal_key` binding table (in the
//! App shell) does not have.
//!
//! Landing in slices (improvement plan T112–T115) — see `spec/index.md` for
//! the audit of what existed before this, and the v1 design. T112: the
//! `Mode` enum, plus host wiring for Visual/Visual Line entry, exit, and
//! cursor-extending motion. T113 (this slice): [`count`]'s numeric-prefix
//! accumulator and [`motion`]'s pure `h j k l w b e 0 ^ $ gg G { } ( ) f t F
//! T` functions, wired into Normal mode (behind the same
//! `Settings::modal_engine` flag, default off). `vim_normal_key`'s existing
//! table keeps handling everything else (`d`/`c`/`y`/`x`/`p`/…) for now —
//! T114/T115 progressively replace more of it, per the spec's § Rollout.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod count;
pub mod mode;
pub mod motion;

pub use count::Count;
pub use mode::Mode;
