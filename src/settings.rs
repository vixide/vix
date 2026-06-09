//! User settings, persisted with [`confy`].
//!
//! Settings live in the platform configuration directory under the application
//! name `vix` (e.g. `~/.config/vix/config.toml` on Linux). [`confy`] picks the
//! right location per OS and handles (de)serialization, so this module only
//! defines the schema and thin load/save wrappers.
//!
//! ```
//! use vix::settings::Settings;
//!
//! // Defaults are always available even when no config file exists yet.
//! let defaults = Settings::default();
//! assert_eq!(defaults.theme, "dark");
//! assert_eq!(defaults.locale, "en");
//! ```

use serde::{Deserialize, Serialize};

/// Application name used by [`confy`] to locate the config directory.
const APP_NAME: &str = "vix";

/// Config file stem; the on-disk file is `config.<ext>` (e.g. `config.toml`).
const CONFIG_NAME: &str = "config";

/// Persisted user preferences.
///
/// Every field has a default (see [`Settings::default`]); `#[serde(default)]`
/// lets older config files load even when new fields are added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Show the line-number gutter.
    pub line_numbers: bool,
    /// Show the file explorer on startup.
    pub show_explorer: bool,
    /// Show the message drawer on startup.
    pub show_messages: bool,
    /// Open single-clicked / arrow-scanned files in an ephemeral preview tab.
    pub preview_tabs: bool,
    /// Color theme: `"dark"` (default) or `"light"`.
    pub theme: String,
    /// UI language as a locale code (e.g. `"en"`, `"es"`, `"fr"`, `"de"`, `"cy"`).
    /// Used as the default; a `--locale` CLI flag overrides it for one run.
    pub locale: String,
    /// Width (columns) of the left dock (file explorer); drag its right edge to
    /// resize.
    pub explorer_width: u16,
    /// Width (columns) of the right dock (message drawer); drag its left edge to
    /// resize.
    pub messages_width: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            line_numbers: true,
            show_explorer: true,
            show_messages: true,
            preview_tabs: true,
            theme: "dark".to_string(),
            locale: "en".to_string(),
            explorer_width: 30,
            messages_width: 32,
        }
    }
}

impl Settings {
    /// Load settings from the user's config directory, falling back to
    /// [`Settings::default`] on any error (missing file, parse failure, …).
    #[must_use]
    pub fn load() -> Settings {
        confy::load(APP_NAME, Some(CONFIG_NAME)).unwrap_or_default()
    }

    /// Persist settings to the user's config directory.
    ///
    /// # Errors
    ///
    /// Returns a [`confy::ConfyError`] if the config directory cannot be
    /// created or the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(CONFIG_NAME), self)
    }

    /// Directory holding custom JSON themes (`<config dir>/themes/`), or `None`
    /// if the config location cannot be determined.
    #[must_use]
    pub fn themes_dir() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("themes")))
    }
}
