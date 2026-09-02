//! An exhaustive, queryable registry of every built-in keybinding, and the
//! user/script override layer built on top of it.
//!
//! Design-only for now (improvement plan T104) — see `spec/index.md` for the
//! audit of why no such registry exists today, the schema this crate will
//! own, and the staged plan (one keymap conversion per task) that gets there
//! without a single high-risk, all-at-once rewrite of `src/app.rs`'s key
//! dispatch. The registry, the per-keymap conversions, the persisted
//! override file, and the `on_key` choke point land in T104a–T104j.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
