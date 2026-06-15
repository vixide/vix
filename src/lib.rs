//! Vix: Simple Terminal Rust IDE.
//!
//! A keyboard-friendly TUI text editor built on [`ratatui`] and `vix-editor`,
//! Vix's fully-custom code-editor widget. The crate is split into focused modules
//! so the editing logic can be unit-tested and reused without a live terminal.
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

// Always start with high quality coding conventions.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
// Intentional exceptions to pedantic: TUI layout/color math casts small `usize`
// counts and `f32` ratios to `u16` cell coordinates (always in range), a few
// dispatch/render functions are necessarily long, and several state structs hold
// many independent boolean flags by design.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value
)]

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
pub mod case;
pub mod editor;
pub mod explorer;
pub mod fileops;
pub mod lsp;
pub mod menu;
pub mod messages;
pub mod palette;
pub mod workspace_search;
pub mod query;
pub mod search;
pub mod session;
pub mod settings;
pub mod theme;
pub mod ui;

/// The calendar box's month-grid logic lives in its own crate; re-export it as
/// `vix::calendar` so the app and tests share one path.
pub use vix_calendar_panel as calendar;

/// The clock box's date/time strings live in their own crate; re-export them as
/// `vix::clock` so the app and tests share one path.
pub use vix_clock_panel as clock;
