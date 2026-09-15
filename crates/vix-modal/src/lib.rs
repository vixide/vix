//! A real modal-editing engine: mode state machine, operator × motion
//! composition, counts, registers, text objects, dot-repeat — the pieces
//! the Vi/Spacemacs keymaps' ad hoc `vim_normal_key` binding table (in the
//! App shell) does not have.
//!
//! Landing in slices (improvement plan T112–T115) — see `spec/index.md` for
//! the audit of what existed before this, and the v1 design. T112: the
//! `Mode` enum, plus host wiring for Visual/Visual Line entry, exit, and
//! cursor-extending motion. T113: [`count`]'s numeric-prefix accumulator and
//! [`motion`]'s pure `h j k l w b e 0 ^ $ gg G { } ( ) f t F T` functions,
//! wired into Normal mode. T114: [`operator`]'s `d`/`c`/`y` composition with
//! any T113 motion (via [`motion::MotionKind`]), `x` as sugar for `d` + one
//! right motion, `p`/`P` reading a register, and [`register`]'s named `a`-`z`
//! map. T115 (this slice): [`text_object`]'s `iw aw i( a( i" a"` (+
//! bracket/quote siblings) pure functions, composing with `d`/`c`/`y` the
//! same way a motion does; dot-repeat (keystroke replay of the last change)
//! lives in the host (`src/app/modal.rs`), not here — there's no pure
//! function to write for "record and replay the exact keys that already
//! went through this same dispatch". Everything behind the same
//! `Settings::modal_engine` flag, default off.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod count;
pub mod mode;
pub mod motion;
pub mod operator;
pub mod register;
pub mod text_object;

pub use count::Count;
pub use mode::Mode;
