//! Integration tests for Vix's terminal-independent logic.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]

#[cfg(feature = "ai")]
mod ai;
mod catalog;
mod common;
mod coverage;
#[cfg(feature = "sql")]
mod db;
mod editing;
mod find;
#[cfg(feature = "git")]
mod git;
mod keybindings;
mod keymaps;
mod lsp;
mod menu;
mod modal;
mod org;
mod palette;
mod panels;
#[cfg(feature = "script")]
mod scripting;
mod structural_replace;
mod themes;
mod tutor;
mod workspace;
