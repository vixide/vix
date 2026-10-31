//! A script's own translations (§ Translations, T735): one string table per
//! locale, read from a plugin's `locales/<code>.toml` files and looked up by
//! the Rhai `tr` function with the same locale and fallback chain as the
//! host's `t!` — the active locale, then its tag truncations (`pt-BR` →
//! `pt`), then English.
//!
//! The files are data, never code: [`crate::discover`] and
//! [`crate::plugins::discover_enabled`] only list a directory's top-level
//! `.rhai` files, so nothing under `locales/` is ever compiled or run.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// The locale every lookup falls back to last, the same as the host's own
/// catalog (`vix-i18n`'s `fallback = "en"`).
pub const FALLBACK_LOCALE: &str = "en";

/// The largest translation file read, in bytes. A bigger one is reported and
/// skipped — a catalog is a few hundred short strings, not a data store.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// One script's (or one plugin's) translations: locale code → key → text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    locales: BTreeMap<String, HashMap<String, String>>,
}

impl Catalog {
    /// An empty catalog: every lookup misses, so `tr(key)` returns `key`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` when no locale has any string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locales.values().all(HashMap::is_empty)
    }

    /// Every locale code with a table, sorted.
    #[must_use]
    pub fn locales(&self) -> Vec<&str> {
        self.locales.keys().map(String::as_str).collect()
    }

    /// Add (or replace) `locale`'s table from TOML `text`: string values,
    /// with nested tables flattened into dotted keys (`[cmd] upper = "…"` is
    /// key `cmd.upper`), the same key shape as the host's own locale files.
    ///
    /// # Errors
    ///
    /// Returns the TOML parse error, or names the first key whose value is
    /// not a string or a table. Nothing is added on error.
    pub fn insert_toml(&mut self, locale: &str, text: &str) -> Result<(), String> {
        let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut strings = HashMap::new();
        flatten("", &table, &mut strings)?;
        self.locales.insert(locale.to_string(), strings);
        Ok(())
    }

    /// Read every `<code>.toml` file directly inside `dir` (not its
    /// subdirectories; symlinks and other non-regular files are skipped,
    /// the same rule as script discovery). A missing directory is an empty
    /// catalog. A file that cannot be read, is larger than
    /// [`MAX_FILE_BYTES`], has an invalid locale code as its stem, or does
    /// not parse is skipped and described in the returned errors (`"<file
    /// name>: <reason>"`); every other file still loads.
    #[must_use]
    pub fn load_dir(dir: &Path) -> (Self, Vec<String>) {
        let mut catalog = Self::new();
        let mut errors = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (catalog, errors);
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
            .collect();
        files.sort();
        for path in files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let code = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !valid_locale_code(code) {
                errors.push(format!("{name}: not a locale code"));
                continue;
            }
            match std::fs::metadata(&path) {
                Ok(m) if m.len() > MAX_FILE_BYTES => {
                    errors.push(format!("{name}: larger than {MAX_FILE_BYTES} bytes"));
                    continue;
                }
                Ok(_) => {}
                Err(e) => {
                    errors.push(format!("{name}: {e}"));
                    continue;
                }
            }
            let result = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| catalog.insert_toml(code, &text));
            if let Err(e) = result {
                errors.push(format!("{name}: {e}"));
            }
        }
        (catalog, errors)
    }

    /// The text for `key` in `locale`, walking the host's fallback chain:
    /// `locale` itself, then each tag truncation (`zh-Hant-TW` → `zh-Hant`
    /// → `zh`), then [`FALLBACK_LOCALE`]. `None` when no table in the chain
    /// has the key.
    #[must_use]
    pub fn lookup(&self, locale: &str, key: &str) -> Option<&str> {
        let mut current = locale;
        loop {
            if let Some(text) = self.locales.get(current).and_then(|t| t.get(key)) {
                return Some(text);
            }
            match current.rfind('-') {
                Some(n) => current = &current[..n],
                None => break,
            }
        }
        self.locales
            .get(FALLBACK_LOCALE)
            .and_then(|t| t.get(key))
            .map(String::as_str)
    }

    /// [`Catalog::lookup`], then replace each `%{name}` with `args`' value
    /// for `name` — the host's `t!` placeholder syntax. A miss returns `key`
    /// itself, so an untranslated string is still readable. A placeholder
    /// with no matching argument is left as written.
    #[must_use]
    pub fn translate(&self, locale: &str, key: &str, args: &[(String, String)]) -> String {
        let mut text = self.lookup(locale, key).unwrap_or(key).to_string();
        for (name, value) in args {
            text = text.replace(&format!("%{{{name}}}"), value);
        }
        text
    }
}

/// Flatten `table` into `out` under `prefix`, joining nested keys with `.`.
fn flatten(
    prefix: &str,
    table: &toml::Table,
    out: &mut HashMap<String, String>,
) -> Result<(), String> {
    for (key, value) in table {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::String(s) => {
                out.insert(full, s.clone());
            }
            toml::Value::Table(t) => flatten(&full, t, out)?,
            _ => return Err(format!("{full}: not a string")),
        }
    }
    Ok(())
}

/// A locale code as a file stem: 1–35 ASCII letters, digits, `-` or `_`,
/// starting with a letter (`en`, `pt-BR`, `zh-Hant`).
fn valid_locale_code(code: &str) -> bool {
    (1..=35).contains(&code.len())
        && code.starts_with(|c: char| c.is_ascii_alphabetic())
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.insert_toml(
            "en",
            "hello = \"Hello\"\n[cmd]\nupper = \"Uppercase\"\nonly_en = \"English only\"\n",
        )
        .unwrap();
        c.insert_toml("pt", "hello = \"Olá\"\n[cmd]\nupper = \"Maiúsculas\"\n")
            .unwrap();
        c.insert_toml("pt-BR", "hello = \"Oi\"\n").unwrap();
        c
    }

    #[test]
    fn nested_tables_flatten_to_dotted_keys() {
        assert_eq!(catalog().lookup("en", "cmd.upper"), Some("Uppercase"));
    }

    #[test]
    fn lookup_walks_truncation_then_english() {
        let c = catalog();
        assert_eq!(c.lookup("pt-BR", "hello"), Some("Oi"));
        assert_eq!(c.lookup("pt-BR", "cmd.upper"), Some("Maiúsculas"));
        assert_eq!(c.lookup("pt-BR", "cmd.only_en"), Some("English only"));
        assert_eq!(c.lookup("fr", "hello"), Some("Hello"));
        assert_eq!(c.lookup("fr", "missing"), None);
    }

    #[test]
    fn translate_fills_placeholders_and_falls_back_to_the_key() {
        let mut c = Catalog::new();
        c.insert_toml("en", "greet = \"Hi %{name}, %{name}! %{other}\"")
            .unwrap();
        let args = [("name".to_string(), "Ada".to_string())];
        assert_eq!(c.translate("en", "greet", &args), "Hi Ada, Ada! %{other}");
        assert_eq!(c.translate("en", "nope", &args), "nope");
    }

    #[test]
    fn non_string_value_is_rejected_whole() {
        let mut c = Catalog::new();
        let err = c
            .insert_toml("en", "a = \"ok\"\n[cmd]\nn = 3\n")
            .unwrap_err();
        assert!(err.contains("cmd.n"), "{err}");
        assert!(c.is_empty());
    }

    #[test]
    fn locale_codes_are_validated() {
        assert!(valid_locale_code("en"));
        assert!(valid_locale_code("pt-BR"));
        assert!(valid_locale_code("zh_Hant"));
        assert!(!valid_locale_code(""));
        assert!(!valid_locale_code("-en"));
        assert!(!valid_locale_code("../en"));
        assert!(!valid_locale_code("e n"));
    }
}
