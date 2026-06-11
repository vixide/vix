//! `vix-editor`: Vix's fully-custom terminal code-editor widget.
//!
//! The buffer + Tree-sitter highlighting + undo/redo *engine* (`code`, `history`,
//! `selection`, `utils`, `theme`) and the editing *operations* (`actions`) are
//! reused from Vix's earlier editor crate; the *widget* layer — editor state,
//! input, mouse, and the soft-wrap renderer — is owned here. Grammars are
//! optional behind Cargo features.

#![warn(clippy::pedantic)]
// Intentional exceptions to pedantic, matching the main `vix` crate: TUI
// layout/grapheme math casts small `usize` counts and `f32` ratios to `u16` cell
// coordinates (always in range), and a few render/dispatch functions are
// necessarily long.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

// The reused engine modules keep their upstream style; allow Clippy's stylistic
// and pedantic lints within them (only the Vix-owned modules below are held to
// pedantic). `clippy::all` does not include `pedantic`, so both are listed.
#[allow(clippy::all, clippy::pedantic)]
pub mod actions;
#[allow(clippy::all, clippy::pedantic)]
pub mod click;
#[allow(clippy::all, clippy::pedantic)]
pub mod code;
#[allow(clippy::all, clippy::pedantic)]
pub mod editor;
#[cfg(feature = "crossterm")]
#[allow(clippy::all, clippy::pedantic)]
pub mod editor_crossterm;
#[allow(clippy::all, clippy::pedantic)]
pub mod history;
#[allow(clippy::all, clippy::pedantic)]
pub mod render;
#[allow(clippy::all, clippy::pedantic)]
pub mod selection;
#[allow(clippy::all, clippy::pedantic)]
pub mod theme;
#[allow(clippy::all, clippy::pedantic)]
pub mod utils;

// Vix-owned modules, held to crate-wide `clippy::pedantic`: the soft-wrap layout
// + renderer, bracket matching, and line ops.
mod brackets;
mod lines;
mod wrap;
