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
mod big_buffer;
mod catalog;
mod common;
mod compare;
mod coverage;
#[cfg(feature = "cowork")]
mod cowork;
#[cfg(feature = "sql")]
mod db;
mod editing;
#[cfg(feature = "http")]
mod fhir;
mod find;
#[cfg(feature = "git")]
mod git;
mod hl7;
mod hot_exit;
mod ide_parity;
mod im;
mod keybindings;
mod keymaps;
mod large_file;
mod lsp;
mod mcp;
mod menu;
mod modal;
mod org;
mod palette;
mod panels;
mod project_settings;
mod save;
#[cfg(feature = "script")]
mod scripting;
mod structural_replace;
mod themes;
mod tutor;
mod undo_tree;
mod workspace;
