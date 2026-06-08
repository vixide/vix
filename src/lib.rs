//! STRIDE: Simple Terminal Rust IDE.
//!
//! A keyboard-friendly TUI text editor built on [`ratatui`] and
//! [`ratatui-code-editor`]. The crate is split into focused modules so the
//! editing logic can be unit-tested and reused without a live terminal.
//!
//! ```
//! use std::path::PathBuf;
//! use stride::app::App;
//!
//! // Build an app rooted at a directory. No terminal is required for this.
//! let app = App::new(PathBuf::from("."));
//! assert!(!app.should_quit);
//! ```
//!
//! [`ratatui`]: https://crates.io/crates/ratatui
//! [`ratatui-code-editor`]: https://crates.io/crates/ratatui-code-editor

pub mod app;
pub mod datetime;
pub mod editor;
pub mod explorer;
pub mod fileops;
pub mod menu;
pub mod messages;
pub mod palette;
pub mod query;
pub mod search;
pub mod settings;
pub mod theme;
pub mod ui;
