//! Vix's fully-custom terminal code-editor widget.
//!
//! The buffer + Tree-sitter highlighting + undo/redo *engine* (`code`, `history`,
//! `selection`, `utils`) and the editing *operations* (`actions`) are reused from
//! Vix's earlier editor crate; the *widget* layer — editor state, input, mouse,
//! and the soft-wrap renderer — is owned here. Grammars are optional behind the
//! crate's `lang-*` Cargo features. Lint configuration (pedantic + the targeted
//! exceptions) is inherited from the crate root (`lib.rs`).

pub mod actions;
pub mod click;
pub mod code;
pub mod editor;
// Always compiled: the main crate always uses the crossterm backend.
pub mod editor_crossterm;
pub mod multicursor;
pub mod named;
pub mod history;
pub mod render;
pub mod selection;
pub mod utils;

// Vix-owned modules, held to crate-wide `clippy::pedantic`: the soft-wrap layout
// + renderer, bracket matching, and line ops.
mod brackets;
mod lines;
mod wrap;
