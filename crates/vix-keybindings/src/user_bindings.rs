//! Persisted user key binding overrides: `keybindings.toml`, one
//! `[[binding]]` table per rebound key. Mirrors `crates/vix-macros`'
//! `macros.toml` pattern exactly — plain `toml` + `std::fs`, not
//! `confy::load`/`.save()` (that's for `Settings` itself; `confy` is used
//! only to *locate* the config directory, via
//! `vix_settings::Settings::keybindings_path`).
//!
//! ```toml
//! [[binding]]
//! key_token = "C-S-k"
//! action_id = "edit.duplicate_line"
//! ```
//!
//! T104h scope: the file format and load/save round trip
//! (`load`/`upsert`/`remove`). Checking a loaded override against a live
//! `KeyEvent`, conflict detection against other overrides, and reporting
//! a shadowed built-in live in [`crate::overrides`] (T104i) — this module
//! doesn't reference [`crate::lookup`] at all. `remove` was added in
//! T204 for the keybinding editor's "Reset to default".

#![warn(clippy::pedantic)]

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One persisted key binding override: `key_token` (`vix-macros` grammar)
/// rebound to `action_id`. Always the top-level (`""`) context — neither
/// this format nor `vix-script`'s `bind_key` has any notion of a chord.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserBinding {
    /// The key token being rebound, in `vix-macros`' grammar.
    pub key_token: String,
    /// The `App::run_action`-dispatchable id this key should run instead.
    pub action_id: String,
}

/// The `keybindings.toml` schema: a list of `[[binding]]` tables.
#[derive(Debug, Default, Deserialize, Serialize)]
struct KeyBindingsFile {
    #[serde(default, rename = "binding")]
    bindings: Vec<UserBinding>,
}

/// Load all saved key binding overrides from `path` (empty when missing
/// or unparseable).
#[must_use]
pub fn load(path: &Path) -> Vec<UserBinding> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<KeyBindingsFile>(&text).ok())
        .map(|f| f.bindings)
        .unwrap_or_default()
}

/// Insert or replace `binding` (by `key_token`) in `path`, creating the
/// file if needed.
///
/// # Errors
/// Returns an error if the file cannot be written or serialized.
pub fn upsert(path: &Path, binding: UserBinding) -> std::io::Result<()> {
    let mut bindings = load(path);
    if let Some(existing) = bindings
        .iter_mut()
        .find(|b| b.key_token == binding.key_token)
    {
        *existing = binding;
    } else {
        bindings.push(binding);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = toml::to_string(&KeyBindingsFile { bindings })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, body)
}

/// Remove the override for `key_token` from `path`, if any (added for the
/// T204 keybinding editor's "Reset to default"). A missing file, or a
/// token with no override, is not an error — both leave nothing to
/// remove.
///
/// # Errors
/// Returns an error if the file exists but cannot be written or
/// serialized.
pub fn remove(path: &Path, key_token: &str) -> std::io::Result<()> {
    let mut bindings = load(path);
    let before = bindings.len();
    bindings.retain(|b| b.key_token != key_token);
    if bindings.len() == before {
        return Ok(());
    }
    let body = toml::to_string(&KeyBindingsFile { bindings })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_of_a_missing_file_is_empty() {
        let path = std::env::temp_dir().join(format!(
            "vix-keybindings-missing-{}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert!(load(&path).is_empty());
    }

    #[test]
    fn upsert_writes_and_replaces_by_key_token() {
        let path =
            std::env::temp_dir().join(format!("vix-keybindings-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        upsert(
            &path,
            UserBinding {
                key_token: "C-S-k".into(),
                action_id: "edit.duplicate_line".into(),
            },
        )
        .unwrap();
        upsert(
            &path,
            UserBinding {
                key_token: "C-j".into(),
                action_id: "edit.join_lines".into(),
            },
        )
        .unwrap();
        // Re-saving "C-S-k" replaces rather than duplicates.
        upsert(
            &path,
            UserBinding {
                key_token: "C-S-k".into(),
                action_id: "edit.select_line".into(),
            },
        )
        .unwrap();
        let bindings = load(&path);
        assert_eq!(bindings.len(), 2);
        let b = bindings.iter().find(|b| b.key_token == "C-S-k").unwrap();
        assert_eq!(b.action_id, "edit.select_line");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_deletes_only_the_matching_token() {
        let path =
            std::env::temp_dir().join(format!("vix-keybindings-rm-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        upsert(
            &path,
            UserBinding {
                key_token: "C-S-k".into(),
                action_id: "edit.duplicate_line".into(),
            },
        )
        .unwrap();
        upsert(
            &path,
            UserBinding {
                key_token: "C-j".into(),
                action_id: "edit.join_lines".into(),
            },
        )
        .unwrap();
        remove(&path, "C-S-k").unwrap();
        let bindings = load(&path);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].key_token, "C-j");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_of_a_missing_file_or_unknown_token_is_a_no_op() {
        let path = std::env::temp_dir().join(format!(
            "vix-keybindings-rm-missing-{}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert!(remove(&path, "C-S-k").is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn load_ignores_unparseable_content() {
        let path = std::env::temp_dir().join(format!(
            "vix-keybindings-garbage-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, "not valid toml {{{").unwrap();
        assert!(load(&path).is_empty());
        std::fs::remove_file(&path).ok();
    }
}
