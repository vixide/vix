//! Vix: Simple Terminal Rust IDE.
//!
//! A keyboard-friendly TUI text editor built on [`ratatui`] and `vix-code-editor-panel`
//! (an internal fork of [`ratatui-code-editor`]). The crate is split into focused
//! modules so the editing logic can be unit-tested and reused without a live terminal.
//!
//! ```
//! use std::path::PathBuf;
//! use vix::app::App;
//! use vix::settings::Settings;
//!
//! // Build an app rooted at a directory. No terminal is required for this.
//! let app = App::new(PathBuf::from("."), Settings::default());
//! assert!(!app.should_quit);
//! ```
//!
//! # Internationalization
//!
//! User-facing text is looked up with `rust_i18n`'s `t!` macro against the
//! locale files in `locales/`. English (`en`) is the fallback; Spanish,
//! French, German, and Welsh are also bundled. Select a language with
//! [`rust_i18n::set_locale`] (the binary wires this to the `locale` setting and
//! the `--locale` flag).
//!
//! [`ratatui`]: https://crates.io/crates/ratatui
//! [`ratatui-code-editor`]: https://crates.io/crates/ratatui-code-editor

// Always start with high quality coding conventions.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::clippy::pedantic)]

#[macro_use]
extern crate rust_i18n;

// Load the translations in `locales/`, falling back to English for any key that
// a selected language does not (yet) translate.
i18n!("locales", fallback = "en");

// When we build for MUSL static, use faster memory allocator.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod app;
pub mod editor;
pub mod explorer;
pub mod fileops;
pub mod menu;
pub mod messages;
pub mod palette;
pub mod project_search;
pub mod query;
pub mod search;
pub mod settings;
pub mod theme;
pub mod ui;

/// The calendar box's date/time logic lives in its own crate; re-export it as
/// `vix::calendar` so the app and tests share one path.
pub use vix_date_time_calendar_panel as calendar;
