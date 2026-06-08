//! User settings, serialized with serde to `~/.config/vix/settings.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            line_numbers: true,
            show_explorer: true,
            show_messages: true,
            preview_tabs: true,
        }
    }
}

impl Settings {
    fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("vix").join("settings.json"))
    }

    /// Load settings, falling back to defaults on any error.
    pub fn load() -> Settings {
        let Some(path) = Self::config_path() else {
            return Settings::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    /// Persist settings; errors are reported to the caller.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}
