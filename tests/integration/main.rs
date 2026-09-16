//! Integration tests for Vix's terminal-independent logic.
#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]

mod ai;
mod catalog;
mod common;
mod coverage;
mod db;
mod editing;
mod find;
mod git;
mod keybindings;
mod keymaps;
mod lsp;
mod menu;
mod modal;
mod org;
mod palette;
mod panels;
mod scripting;
mod structural_replace;
mod themes;
mod tutor;
mod workspace;
