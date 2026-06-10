//! `vix-editor`: Vix's fully-custom terminal code-editor widget.
//!
//! The buffer + Tree-sitter highlighting + undo/redo *engine* (`code`, `history`,
//! `selection`, `utils`, `theme`) and the editing *operations* (`actions`) are
//! reused from Vix's earlier editor crate; the *widget* layer — editor state,
//! input, mouse, and the soft-wrap renderer — is owned here. Grammars are
//! optional behind Cargo features.

// The reused modules keep their upstream style; allow Clippy's stylistic lints
// within them. (The renderer is being rewritten in-house to support soft wrap.)
#[allow(clippy::all)]
pub mod actions;
#[allow(clippy::all)]
pub mod click;
#[allow(clippy::all)]
pub mod code;
#[allow(clippy::all)]
pub mod editor;
#[cfg(feature = "crossterm")]
#[allow(clippy::all)]
pub mod editor_crossterm;
#[allow(clippy::all)]
pub mod history;
#[allow(clippy::all)]
pub mod render;
#[allow(clippy::all)]
pub mod selection;
#[allow(clippy::all)]
pub mod theme;
#[allow(clippy::all)]
pub mod utils;

// Vix-owned modules, held to clippy::pedantic (see each module's inner
// attributes): the soft-wrap layout + renderer, and bracket matching.
mod brackets;
mod wrap;
