//! Vix: Simple Terminal Rust IDE.
//!
//! A keyboard-friendly TUI text editor built on [`ratatui`], with Vix's
//! fully-custom code-editor widget in the `vix_editor_core` crate. Vix is a
//! cargo workspace: each major concept is its own `crates/vix_*` member crate,
//! re-exported here under its familiar module name (`pub use vix_git as git`,
//! etc.). This root `vix` crate holds only the `App` shell (`app`, `ui`,
//! `search`, and the surfaces tangled with them) that cannot be split off. The
//! split lets each concept build and unit-test on its own.
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
//! User-facing text is looked up with the `t!` macro against the locale files
//! in `locales/`. English (`en`) is the fallback; Spanish, French, German, and
//! Welsh are also bundled. The translation table is embedded once, in the
//! `vix_i18n` crate, which every member crate shares; select a language with
//! `rust_i18n::set_locale` (the binary wires this to the `locale` setting and
//! the `--locale` flag).
//!
//! [`ratatui`]: https://crates.io/crates/ratatui

// Always start with high quality coding conventions.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

// The translation table is embedded once, in the `vix_i18n` crate. Bringing it
// in with `#[macro_use]` makes the `t!` and `surface!` macros available
// unqualified throughout the crate, exactly as the former `rust_i18n` import did.
#[macro_use]
extern crate vix_i18n;

// Surface the translation lookup functions at this crate's root so the `t!`
// expansion (which references `crate::_rust_i18n_try_translate`) resolves here.
vix_i18n::surface!();

// The MUSL static build uses mimalloc as its global allocator; that lives in the
// `vix` binary (main.rs) so it applies once to the whole program. Declaring it
// here too would conflict (two `#[global_allocator]` in one binary).

pub mod app;
pub use vix_case as case;
pub use vix_editor::editor;
/// The custom code-editor widget engine (buffer, Tree-sitter highlighting,
/// history, selection, soft-wrap renderer). Reached through
/// [`editor::CodeEditor`] and `crate::editor_core` paths.
pub use vix_editor_core as editor_core;
pub mod explorer;
pub use vix_fileops as fileops;
pub use vix_format_tool as format_tool;
pub use vix_jwt_tool as jwt_tool;
pub use vix_lsp as lsp;
pub use vix_markdown_preview as markdown_preview;
pub use vix_menu as menu;
pub mod messages;
pub use vix_palette as palette;
pub mod workspace_search;
pub use vix_qr_tool as qr_tool;
pub use vix_query as query;
pub use vix_regex_tool as regex_tool;
pub mod search;
pub use vix_session as session;
pub use vix_settings as settings;
pub use vix_snippet_tool as snippet_tool;
pub use vix_snippets as snippets;
pub use vix_tags as tags;
pub use vix_tasks as tasks;
pub use vix_terminal as terminal;
pub use vix_test_runner as test_runner;
pub use vix_textops as textops;
pub use vix_theme as theme;
pub use vix_undo_store as undo_store;
pub mod ui;

// Folded-in modules (formerly separate `vix-*` subcrates). The custom
// code-editor widget lives in `editor_core` (above); its Tree-sitter grammars
// are gated behind this crate's `lang-*` features.
pub use vix_affix as affix;
pub use vix_ai_diff as ai_diff;
pub use vix_ai_panel as ai_panel;
pub use vix_align as align;
pub use vix_ascii_character_picker as ascii_character_picker;
pub use vix_base_tool as base_tool;
pub use vix_base16 as base16;
pub use vix_base64_tool as base64_tool;
pub use vix_bottom_dock as bottom_dock;
pub use vix_calculator_tool as calculator_tool;
pub use vix_calendar_panel as calendar_panel;
pub use vix_checksum_tool as checksum_tool;
pub use vix_clock_panel as clock_panel;
pub use vix_color_converter_tool as color_converter_tool;
pub use vix_conflict_tool as conflict_tool;
pub use vix_contact_panel as contact_panel;
pub use vix_convert_from_csv_into_json_tool as convert_from_csv_into_json_tool;
pub use vix_convert_from_csv_into_tsv_tool as convert_from_csv_into_tsv_tool;
pub use vix_convert_from_html_into_markdown_tool as convert_from_html_into_markdown_tool;
pub use vix_convert_from_json_into_csv_tool as convert_from_json_into_csv_tool;
pub use vix_convert_from_json_into_toml_tool as convert_from_json_into_toml_tool;
pub use vix_convert_from_json_into_tsv_tool as convert_from_json_into_tsv_tool;
pub use vix_convert_from_json_into_yaml_tool as convert_from_json_into_yaml_tool;
pub use vix_convert_from_markdown_into_html_tool as convert_from_markdown_into_html_tool;
pub use vix_convert_from_toml_into_json_tool as convert_from_toml_into_json_tool;
pub use vix_convert_from_tsv_into_csv_tool as convert_from_tsv_into_csv_tool;
pub use vix_convert_from_tsv_into_json_tool as convert_from_tsv_into_json_tool;
pub use vix_convert_from_yaml_into_json_tool as convert_from_yaml_into_json_tool;
pub use vix_convert_tabular as convert_tabular;
pub use vix_dap as dap;
pub use vix_db as db;
pub use vix_diff_view as diff_view;
pub use vix_edit_bytes as edit_bytes;
pub use vix_editor::pane_tree;
pub use vix_editorconfig as editorconfig;
pub use vix_emmet as emmet;
pub use vix_file_information_panel as file_information_panel;
pub use vix_find_panel as find_panel;
pub use vix_git as git;
pub use vix_html_character_picker as html_character_picker;
pub use vix_http_client as http_client;
pub use vix_keyboard_shortcut_panel as keyboard_shortcut_panel;
pub use vix_keymap_model as keymap_model;
pub use vix_left_dock as left_dock;
pub use vix_locale_model as locale_model;
pub use vix_lorem as lorem;
pub use vix_lsp_core as lsp_core;
pub use vix_macros as macros;
pub use vix_media_type as media_type;
pub use vix_nerd_font_picker as nerd_font_picker;
pub use vix_org as org;
pub use vix_org_contacts as org_contacts;
pub use vix_outline_panel as outline_panel;
pub use vix_pomodoro_tool as pomodoro_tool;
pub use vix_right_dock as right_dock;
pub use vix_roam as roam;
pub use vix_spellcheck as spellcheck;
pub use vix_status_bar_panel as status_bar_panel;
pub use vix_system_information_panel as system_information_panel;
pub use vix_workspace as workspace;
pub mod edit_outline;
pub use vix_edit_sql as edit_sql;
pub mod edit_table;
/// The calendar box's month-grid logic; re-exported as `vix::calendar` so the
/// app and tests share one path.
pub use crate::calendar_panel as calendar;
pub use vix_edit_value as edit_value;
pub use vix_text_information_panel as text_information_panel;
pub use vix_theme_model as theme_model;
pub use vix_time_zone_model as time_zone_model;
pub use vix_unit_converter_tool as unit_converter_tool;
pub use vix_url_tool as url_tool;
pub use vix_uuid_tool as uuid_tool;
pub use vix_vcard_panel as vcard_panel;
pub use vix_vcard_parser as vcard_parser;
pub use vix_welcome_panel as welcome_panel;
pub use vix_workspace_dashboard_panel as workspace_dashboard_panel;
pub use vix_x11_color_picker as x11_color_picker;
pub use vix_zid_tool as zid_tool;

/// The clock box's date/time strings; re-exported as `vix::clock` so the app and
/// tests share one path.
pub use crate::clock_panel as clock;
