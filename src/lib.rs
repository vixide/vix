//! Vix: Simple Terminal Rust IDE.
//!
//! A keyboard-friendly TUI text editor built on [`ratatui`], with Vix's
//! fully-custom code-editor widget in the `editor_core` module. The crate is
//! split into focused modules
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

#[macro_use]
extern crate rust_i18n;

// Load the translations in `locales/`, falling back to English for any key that
// a selected language does not (yet) translate.
i18n!("locales", fallback = "en");

// The MUSL static build uses mimalloc as its global allocator; that lives in the
// `vix` binary (main.rs) so it applies once to the whole program. Declaring it
// here too would conflict (two `#[global_allocator]` in one binary).

pub mod app;
pub mod case;
pub mod editor;
/// The custom code-editor widget engine (buffer, Tree-sitter highlighting,
/// history, selection, soft-wrap renderer). Reached through
/// [`editor::CodeEditor`] and `crate::editor_core` paths.
pub mod editor_core;
pub mod explorer;
pub mod fileops;
pub mod format_tool;
pub mod jwt_tool;
pub mod lsp;
pub mod markdown_preview;
pub mod menu;
pub mod messages;
pub mod palette;
pub mod workspace_search;
pub mod query;
pub mod regex_tool;
pub mod search;
pub mod session;
pub mod settings;
pub mod snippet_tool;
pub mod theme;
pub mod ui;

// Folded-in modules (formerly separate `vix-*` subcrates). The custom
// code-editor widget lives in `editor_core` (above); its Tree-sitter grammars
// are gated behind this crate's `lang-*` features.
pub mod ascii_character_picker;
pub mod base64_tool;
pub mod base_tool;
pub mod bottom_dock;
pub mod calculator_tool;
pub mod calendar_panel;
pub mod checksum_tool;
pub mod conflict_tool;
pub mod clock_panel;
pub mod color_converter_tool;
pub mod contact_panel;
pub mod convert_from_csv_into_json_tool;
pub mod convert_from_csv_into_tsv_tool;
pub mod convert_from_html_into_markdown_tool;
pub mod convert_from_json_into_csv_tool;
pub mod convert_from_json_into_toml_tool;
pub mod convert_from_json_into_tsv_tool;
pub mod convert_from_json_into_yaml_tool;
pub mod convert_from_markdown_into_html_tool;
pub mod convert_from_toml_into_json_tool;
pub mod convert_from_tsv_into_csv_tool;
pub mod convert_from_tsv_into_json_tool;
pub mod convert_from_yaml_into_json_tool;
pub mod convert_tabular;
pub mod file_information_panel;
pub mod find_panel;
pub mod git;
pub mod html_character_picker;
pub mod keyboard_shortcut_panel;
pub mod keymap_model;
pub mod left_dock;
pub mod locale_model;
pub mod lsp_core;
pub mod nerd_font_picker;
pub mod outline_panel;
pub mod pomodoro_tool;
pub mod right_dock;
pub mod spellcheck;
pub mod status_bar_panel;
pub mod system_information_panel;
pub mod text_information_panel;
pub mod theme_model;
pub mod time_zone_model;
pub mod unit_converter_tool;
pub mod url_tool;
pub mod uuid_tool;
pub mod vcard_panel;
pub mod vcard_parser;
pub mod welcome_panel;
pub mod workspace_dashboard_panel;
pub mod x11_color_picker;
pub mod zid_tool;

/// The calendar box's month-grid logic; re-exported as `vix::calendar` so the
/// app and tests share one path.
pub use crate::calendar_panel as calendar;

/// The clock box's date/time strings; re-exported as `vix::clock` so the app and
/// tests share one path.
pub use crate::clock_panel as clock;
