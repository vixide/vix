//! Vix's fully-custom terminal code-editor widget.
//!
//! The buffer + Tree-sitter highlighting + undo/redo *engine* (`code`, `history`,
//! `selection`, `utils`) and the editing *operations* (`actions`) are reused from
//! Vix's earlier editor crate; the *widget* layer — editor state, input, mouse,
//! and the soft-wrap renderer — is owned here. Grammars are optional behind the
//! crate's `lang-*` Cargo features. Lint configuration (pedantic + the targeted
//! exceptions) is inherited from the crate root (`lib.rs`).

#![warn(clippy::pedantic)]

/// Editing operations (cursor movement, insert, delete, indent, clipboard, undo).
pub mod actions;
/// Mouse-click tracking for single/double/triple click detection.
pub mod click;
/// The text buffer, Tree-sitter highlighting, and undo/redo engine.
pub mod code;
/// The editor widget: state, cursor, selection, and input handling.
pub mod editor;
// Always compiled: the main crate always uses the crossterm backend.
/// Crossterm-backed input and event handling for the editor.
pub mod editor_crossterm;
/// Undo/redo history of edit batches.
pub mod history;
/// Multiple cursors ("carets") and the operations applied across them.
pub mod multicursor;
pub mod named;
/// Rendering of the editor, including soft-wrap layout.
pub mod render;
/// Text selection ranges and selection-snapping modes.
pub mod selection;
/// Language detection and indentation, comment, color, and text helpers.
pub mod utils;

// Vix-owned modules, held to crate-wide `clippy::pedantic`: the soft-wrap layout
// + renderer, bracket matching, and line ops.
mod brackets;
mod lines;
mod wrap;
