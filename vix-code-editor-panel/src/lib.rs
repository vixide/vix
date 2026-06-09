//! `vix-code-editor-panel`: an internal fork of `ratatui-code-editor`.
//!
//! Upstream code is kept close to its original form (to ease tracking upstream)
//! rather than restyled to Vix's conventions, so Clippy's stylistic lints are
//! allowed crate-wide here. Vix-specific additions still aim to be clean.
#![allow(clippy::all)]

pub mod editor;
#[cfg(feature = "crossterm")]
pub mod editor_crossterm;
pub mod code;
pub mod history;
pub mod selection;
pub mod theme;
pub mod utils;
pub mod click;
pub mod actions;
pub mod render;