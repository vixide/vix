//! A *workspace*: a named set of project folders plus the files to reopen, saved
//! to and loaded from a standalone file (the VS Code "workspace file" idea).
//!
//! Unlike the per-directory [`crate::session`] (which auto-restores the last
//! state for a single root), a workspace is an explicit, portable file the user
//! saves and opens by name. It can gather several folders so the fuzzy file
//! finder and search span all of them, and records the open files so reopening
//! the workspace restores them.
//!
//! The on-disk format is TOML:
//!
//! ```toml
//! folders = ["/home/me/proj", "/home/me/lib"]
//! files = ["/home/me/proj/src/main.rs"]
//! ```

#![warn(clippy::pedantic)]

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The conventional extension for a Vix workspace file.
pub const EXTENSION: &str = "vix-workspace";

/// A saved workspace: its folders (first is the primary root) and the files to
/// reopen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// Absolute folder paths; the first is treated as the primary root.
    pub folders: Vec<String>,
    /// Absolute file paths to reopen, in tab order.
    pub files: Vec<String>,
}

impl Workspace {
    /// Serialize to TOML text.
    ///
    /// # Errors
    /// Returns an error if serialization fails (practically never for this shape).
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Parse a workspace from TOML text.
    ///
    /// # Errors
    /// Returns an error if `text` is not valid workspace TOML.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Read a workspace file from `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Write this workspace to `path` as TOML.
    ///
    /// # Errors
    /// Returns an error if serialization or the file write fails.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = self
            .to_toml()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let ws = Workspace {
            folders: vec!["/a".into(), "/b".into()],
            files: vec!["/a/main.rs".into()],
        };
        let text = ws.to_toml().unwrap();
        assert_eq!(Workspace::from_toml(&text).unwrap(), ws);
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let ws = Workspace::from_toml("folders = [\"/x\"]\n").unwrap();
        assert_eq!(ws.folders, vec!["/x".to_string()]);
        assert!(ws.files.is_empty());
    }
}
