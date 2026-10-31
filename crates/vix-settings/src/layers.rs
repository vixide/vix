//! Settings layers (T627): the project file `<root>/.vix/settings.toml`
//! deep-merged over the user settings, and per-language overrides resolved
//! per buffer. Precedence is built-in < user < project < language.
//!
//! Everything here works on [`toml::Table`]s: a [`Settings`] is serialized
//! to a table, a layer is merged onto it, and the result is deserialized
//! back, so a layer needs to name only the keys it changes.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::Settings;

/// The settings keys a `[languages.<id>]` table may set: the ones that
/// describe one buffer rather than the whole editor.
pub const LANGUAGE_KEYS: &[&str] = &[
    "indent_style",
    "tab_width",
    "wrap_column",
    "trim_trailing_whitespace",
    "ensure_final_newline",
    "format_on_save",
    "soft_wrap",
    "show_whitespace",
    "line_numbers",
    "relative_line_numbers",
    "auto_pair",
    "rainbow_brackets",
];

/// Where a workspace's project settings live: `<root>/.vix/settings.toml`.
#[must_use]
pub fn project_settings_path(root: &Path) -> PathBuf {
    root.join(".vix").join("settings.toml")
}

/// Why a project settings file could not be used.
#[derive(Debug)]
#[non_exhaustive]
pub enum LayerError {
    /// The file exists but could not be read.
    Io(std::io::Error),
    /// The file is not valid TOML.
    Parse(toml::de::Error),
    /// The merged settings no longer deserialize (a value of the wrong type).
    Merge(toml::de::Error),
    /// The base settings could not be serialized (should not happen).
    Serialize(toml::ser::Error),
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayerError::Io(e) => write!(f, "{e}"),
            LayerError::Parse(e) | LayerError::Merge(e) => write!(f, "{}", e.message()),
            LayerError::Serialize(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LayerError {}

/// Read the project settings file at `path` as a table. A missing file is
/// `Ok(None)`, not an error.
///
/// # Errors
///
/// [`LayerError::Io`] when the file exists but can't be read, and
/// [`LayerError::Parse`] when it isn't valid TOML.
pub fn read_layer(path: &Path) -> Result<Option<toml::Table>, LayerError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LayerError::Io(e)),
    };
    text.parse::<toml::Table>()
        .map(Some)
        .map_err(LayerError::Parse)
}

/// `settings` as a TOML table (every key, flattened groups at the top level).
///
/// # Errors
///
/// [`LayerError::Serialize`] if serialization fails.
fn to_table(settings: &Settings) -> Result<toml::Table, LayerError> {
    toml::Table::try_from(settings).map_err(LayerError::Serialize)
}

/// Deep-merge `layer` into `base`: a table merges key by key, recursively;
/// any other value (a string, a number, an array) replaces `base`'s whole.
pub fn deep_merge(base: &mut toml::Table, layer: &toml::Table) {
    for (key, value) in layer {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(l)) => deep_merge(b, l),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// `base` with `layer` deep-merged over it ([`deep_merge`]).
///
/// # Errors
///
/// [`LayerError::Merge`] when the merged table is no longer valid settings
/// (a value of the wrong type).
pub fn merge_layer(base: &Settings, layer: &toml::Table) -> Result<Settings, LayerError> {
    let mut table = to_table(base)?;
    deep_merge(&mut table, layer);
    table.try_into().map_err(LayerError::Merge)
}

/// The dotted keys in `layer` that [`Settings`] does not define: top-level
/// keys, and keys inside a table the defaults also hold as a non-empty table
/// (such as `[im]`). Map-valued settings whose keys are free-form
/// (`languages`, `jupyter_kernels`) and arrays are not looked inside.
#[must_use]
pub fn unknown_keys(layer: &toml::Table) -> Vec<String> {
    let Ok(known) = to_table(&Settings::default()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_unknown(layer, &known, "", &mut out);
    out
}

/// [`unknown_keys`]'s recursion over one table level.
fn collect_unknown(layer: &toml::Table, known: &toml::Table, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in layer {
        let path = format!("{prefix}{key}");
        match known.get(key) {
            None if !(prefix.is_empty() && key == "languages") => out.push(path),
            Some(toml::Value::Table(k)) if !k.is_empty() => {
                if let toml::Value::Table(l) = value {
                    collect_unknown(l, k, &format!("{path}."), out);
                }
            }
            _ => {}
        }
    }
}

/// The dotted keys of `settings.languages` that are not per-language
/// settings ([`LANGUAGE_KEYS`]), such as `languages.rust.theme`. They are
/// ignored by [`Settings::for_language`].
#[must_use]
pub fn language_key_problems(settings: &Settings) -> Vec<String> {
    settings
        .languages
        .iter()
        .flat_map(|(lang, table)| {
            table
                .keys()
                .filter(|k| !LANGUAGE_KEYS.contains(&k.as_str()))
                .map(move |k| format!("languages.{lang}.{k}"))
        })
        .collect()
}

/// The user settings to save when the live settings `effective` came from
/// `user` with a project layer merged in as `merged`: `user`, with every
/// top-level key whose value changed between `merged` and `effective` taken
/// from `effective`. In-app changes reach the user file; the project's own
/// values never do.
#[must_use]
pub fn unmerge(user: &Settings, merged: &Settings, effective: &Settings) -> Settings {
    let (Ok(mut out), Ok(merged), Ok(effective)) =
        (to_table(user), to_table(merged), to_table(effective))
    else {
        return effective.clone();
    };
    for (key, value) in effective {
        if merged.get(&key) != Some(&value) {
            out.insert(key, value);
        }
    }
    out.try_into().unwrap_or_else(|_| user.clone())
}

impl Settings {
    /// These settings with `lang`'s `[languages.<lang>]` table applied
    /// (only its [`LANGUAGE_KEYS`]; anything else is ignored). Borrowed
    /// unchanged when there is no table for `lang`, or it sets nothing
    /// usable, or the result would not be valid settings.
    #[must_use]
    pub fn for_language(&self, lang: &str) -> Cow<'_, Settings> {
        let Some(table) = self.languages.get(lang) else {
            return Cow::Borrowed(self);
        };
        let layer: toml::Table = table
            .iter()
            .filter(|(k, _)| LANGUAGE_KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if layer.is_empty() {
            return Cow::Borrowed(self);
        }
        merge_layer(self, &layer).map_or(Cow::Borrowed(self), Cow::Owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    #[test]
    fn precedence_is_built_in_then_user_then_project_then_language() {
        let user = Settings {
            tab_width: 3,
            theme: "light".into(),
            wrap_column: 100,
            ..Settings::default()
        };
        let project = table("tab_width = 2\nwrap_column = 72\n[languages.python]\ntab_width = 8\n");
        let merged = merge_layer(&user, &project).unwrap();
        // Built-in survives where nobody overrides it.
        assert_eq!(merged.locale, "en");
        // User beats built-in.
        assert_eq!(merged.theme, "light");
        // Project beats user.
        assert_eq!(merged.tab_width, 2);
        assert_eq!(merged.wrap_column, 72);
        // Language beats project, for that language only.
        assert_eq!(merged.for_language("python").tab_width, 8);
        assert_eq!(merged.for_language("python").wrap_column, 72);
        assert_eq!(merged.for_language("rust").tab_width, 2);
        assert!(matches!(merged.for_language("rust"), Cow::Borrowed(_)));
    }

    #[test]
    fn nested_tables_merge_key_by_key() {
        let mut user = Settings::default();
        user.languages
            .insert("rust".into(), table("tab_width = 4\nsoft_wrap = true"));
        let merged = merge_layer(&user, &table("[languages.rust]\ntab_width = 2\n")).unwrap();
        let rust = merged.for_language("rust");
        assert_eq!(rust.tab_width, 2);
        assert!(rust.editor_visual.soft_wrap);
    }

    #[test]
    fn a_wrongly_typed_value_is_a_merge_error() {
        let err = merge_layer(&Settings::default(), &table("tab_width = \"wide\"")).unwrap_err();
        assert!(matches!(err, LayerError::Merge(_)));
    }

    #[test]
    fn unknown_keys_are_listed_with_their_dotted_path() {
        let layer = table(
            "tab_width = 2\ntab_widht = 3\n[im]\nnick = \"x\"\nbogus = 1\n\
             [languages.rust]\ntab_width = 2\n[jupyter_kernels]\npython = \"python3\"\n",
        );
        assert_eq!(unknown_keys(&layer), vec!["im.bogus", "tab_widht"]);
    }

    #[test]
    fn non_language_keys_in_a_language_table_are_reported_and_ignored() {
        let mut s = Settings::default();
        s.languages
            .insert("rust".into(), table("theme = \"light\"\ntab_width = 2"));
        assert_eq!(language_key_problems(&s), vec!["languages.rust.theme"]);
        let rust = s.for_language("rust");
        assert_eq!(rust.theme, "dark");
        assert_eq!(rust.tab_width, 2);
    }

    #[test]
    fn unmerge_keeps_project_values_out_of_the_user_file() {
        let user = Settings {
            tab_width: 3,
            ..Settings::default()
        };
        let merged = merge_layer(&user, &table("tab_width = 2\ntheme = \"light\"")).unwrap();
        let mut effective = merged.clone();
        effective.gutter.line_numbers = false;
        effective.theme = "dark-high".into();
        let saved = unmerge(&user, &merged, &effective);
        assert_eq!(saved.tab_width, 3, "the project's value is not saved");
        assert!(!saved.gutter.line_numbers, "an in-app change is saved");
        assert_eq!(saved.theme, "dark-high", "a changed project value is saved");
    }

    #[test]
    fn a_missing_project_file_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("vix-settings-layer-{}", std::process::id()));
        assert!(read_layer(&project_settings_path(&dir)).unwrap().is_none());
        std::fs::create_dir_all(dir.join(".vix")).unwrap();
        std::fs::write(project_settings_path(&dir), "tab_width = [").unwrap();
        assert!(matches!(
            read_layer(&project_settings_path(&dir)),
            Err(LayerError::Parse(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }
}
