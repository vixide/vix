//! User scripting: load `.rhai` scripts and run them against the active
//! buffer — register palette commands, bind keys, read/modify the buffer,
//! prompt for input, show messages.
//!
//! Design-only for now (improvement plan T101) — see `spec/index.md` for the
//! engine choice, script discovery, the API v1 surface, error handling, and
//! sandboxing. The engine, host wiring, keybindings, and sample scripts land
//! in T102–T105.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
