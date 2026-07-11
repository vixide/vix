//! JSON snippet files: loading, scope resolution, merging, and the picker's
//! filter state.
//!
//! Snippets are defined in JSON files (the VS Code shape — see
//! `spec/index.md`) gathered from four scopes: bundled, global,
//! media-type, and project. The tabstop syntax in a snippet body is parsed by
//! [`vix_snippet_tool::parse`]; this module handles the files and the merge.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::{Path, PathBuf};

/// Where a snippet came from (used for display and merge precedence).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Built into Vix.
    Bundled,
    /// `<config>/global/snippets/snippets.json`.
    Global,
    /// `<config>/media-types/<type>/snippets/snippets.json` for the given type.
    MediaType(String),
    /// The project's snippet file.
    Project,
}

impl Scope {
    /// A short human label for the picker.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Scope::Bundled => "bundled".to_string(),
            Scope::Global => "global".to_string(),
            Scope::MediaType(m) => m.clone(),
            Scope::Project => "project".to_string(),
        }
    }
}

/// One snippet: a name, zero or more expansion prefixes, the body (with tabstop
/// markers), an optional description, and its source scope.
#[derive(Clone, Debug)]
pub struct Snippet {
    /// Display name (the JSON object key).
    pub name: String,
    /// Expansion prefixes (typed + Tab). Empty means picker-only.
    pub prefixes: Vec<String>,
    /// Body text with `$1`/`${1:…}`/`$0` tabstops.
    pub body: String,
    /// Optional human description.
    pub description: String,
    /// Source scope.
    pub scope: Scope,
}

/// Parse a snippet JSON document into snippets tagged with `scope`. Tolerant: a
/// malformed document yields an empty list; entries missing a `body` are skipped.
#[must_use]
pub fn parse_json(json: &str, scope: &Scope) -> Vec<Snippet> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, val) in map {
        let serde_json::Value::Object(obj) = val else {
            continue;
        };
        let Some(body) = obj.get("body").and_then(json_string_or_lines) else {
            continue;
        };
        let prefixes = obj.get("prefix").map(json_string_list).unwrap_or_default();
        let description = obj
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        out.push(Snippet {
            name,
            prefixes,
            body,
            description,
            scope: scope.clone(),
        });
    }
    // Stable order by name so the picker is deterministic.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A JSON string, or an array of strings joined with newlines.
fn json_string_or_lines(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(a) => Some(
            a.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// A JSON string, or an array of strings, as a `Vec<String>` (empties dropped).
fn json_string_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) if !s.is_empty() => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// The built-in snippets ([`vix_snippet_tool::SNIPPETS`]) as [`Snippet`]s with
/// [`Scope::Bundled`]. They have no expansion prefix (picker-only).
#[must_use]
pub fn bundled() -> Vec<Snippet> {
    vix_snippet_tool::SNIPPETS
        .iter()
        .map(|s| Snippet {
            name: s.name.to_string(),
            prefixes: Vec::new(),
            body: s.body.to_string(),
            description: String::new(),
            scope: Scope::Bundled,
        })
        .collect()
}

/// Vix's config directory (e.g. `~/.config/vix/`), or `None` if undeterminable.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    vix_settings::Settings::config_path().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// The global snippets directory, if the config directory is known
/// (`<config>/global/snippets`).
#[must_use]
pub fn global_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("global").join("snippets"))
}

/// The `media-types/<type>/<subtype>/snippets` relative path for `media_type`
/// (e.g. `text/rust` → `media-types/text/rust/snippets`).
#[must_use]
pub fn media_type_rel(media_type: &str) -> PathBuf {
    let mut p = PathBuf::from("media-types");
    for seg in media_type.split('/') {
        p = p.join(seg);
    }
    p.join("snippets")
}

/// The project snippet file path: `root` joined with the relative `project_rel`.
#[must_use]
pub fn project_file(root: &Path, project_rel: &str) -> PathBuf {
    root.join(project_rel)
}

/// Read and parse a single snippet file, returning `[]` if missing/unreadable.
#[must_use]
pub fn load_file(path: &Path, scope: &Scope) -> Vec<Snippet> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_json(&text, scope),
        Err(_) => Vec::new(),
    }
}

/// Load every `*.json` file in `dir` (sorted by name), parsing each with `scope`.
/// A missing directory yields `[]`.
#[must_use]
pub fn load_dir(dir: &Path, scope: &Scope) -> Vec<Snippet> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    files.iter().flat_map(|p| load_file(p, scope)).collect()
}

/// Load and merge file snippets for the active context: the global directory,
/// then the buffer's media-type directories (config + project), then the
/// configured project file. Bundled snippets are added by the caller; later
/// scopes shadow earlier ones by name. All `*.json` files in each snippets
/// directory are loaded.
#[must_use]
pub fn load_scoped(
    media_type: Option<&str>,
    project_root: &Path,
    project_rel: &str,
) -> Vec<Snippet> {
    let mut snippets: Vec<Snippet> = Vec::new();
    if let Some(d) = global_dir() {
        snippets.extend(load_dir(&d, &Scope::Global));
    }
    if let Some(mt) = media_type {
        let rel = media_type_rel(mt);
        let scope = Scope::MediaType(mt.to_string());
        // Config dir: <config>/media-types/<type>/snippets.
        if let Some(cfg) = config_dir() {
            snippets.extend(load_dir(&cfg.join(&rel), &scope));
        }
        // Project: <root>/config/media-types/<type>/snippets.
        snippets.extend(load_dir(&project_root.join("config").join(&rel), &scope));
    }
    snippets.extend(load_file(
        &project_file(project_root, project_rel),
        &Scope::Project,
    ));
    snippets
}

/// Merge `extra` onto `base`, shadowing by name (a later snippet of the same name
/// replaces the earlier one), and return the combined library sorted by name.
#[must_use]
pub fn merge(mut base: Vec<Snippet>, extra: Vec<Snippet>) -> Vec<Snippet> {
    for s in extra {
        if let Some(slot) = base.iter_mut().find(|b| b.name == s.name) {
            *slot = s;
        } else {
            base.push(s);
        }
    }
    base.sort_by(|a, b| a.name.cmp(&b.name));
    base
}

/// Find the first snippet whose prefix exactly matches `word`.
#[must_use]
pub fn find_by_prefix<'a>(snippets: &'a [Snippet], word: &str) -> Option<&'a Snippet> {
    snippets
        .iter()
        .find(|s| s.prefixes.iter().any(|p| p == word))
}

/// Filter + selection state for the Snippets picker (indices into a library
/// `&[Snippet]` held by the host).
#[derive(Default)]
pub struct Picker {
    /// Case-insensitive filter over name, prefix, and description.
    pub query: String,
    /// Highlighted row within the filtered list.
    pub selected: usize,
    /// First visible filtered row.
    pub scroll: usize,
}

impl Picker {
    /// A fresh picker with an empty filter.
    #[must_use]
    pub fn new() -> Self {
        Picker::default()
    }

    /// Indices into `library` matching the current query.
    #[must_use]
    pub fn matches(&self, library: &[Snippet]) -> Vec<usize> {
        let q = self.query.to_ascii_lowercase();
        library
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                q.is_empty()
                    || s.name.to_ascii_lowercase().contains(&q)
                    || s.description.to_ascii_lowercase().contains(&q)
                    || s.prefixes
                        .iter()
                        .any(|p| p.to_ascii_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of rows matching the current filter.
    #[must_use]
    pub fn len(&self, library: &[Snippet]) -> usize {
        self.matches(library).len()
    }

    /// Append to the filter, resetting the highlight.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll = 0;
    }

    /// Delete the last filter character, resetting the highlight.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move the highlight up `n`, clamped.
    pub fn up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n.max(1));
    }

    /// Move the highlight down `n`, clamped to the filtered length.
    pub fn down(&mut self, n: usize, library: &[Snippet]) {
        let last = self.len(library).saturating_sub(1);
        self.selected = (self.selected + n.max(1)).min(last);
    }

    /// Select a filtered row directly; returns whether it was real.
    pub fn select_index(&mut self, idx: usize, library: &[Snippet]) -> bool {
        if idx < self.len(library) {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Keep the highlight within a window of `height` rows.
    pub fn ensure_visible(&mut self, height: usize, library: &[Snippet]) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.len(library).saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The library index of the highlighted row, if any.
    #[must_use]
    pub fn selected_library_index(&self, library: &[Snippet]) -> Option<usize> {
        self.matches(library).get(self.selected).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_array_fields() {
        let json = r#"{
            "Func": { "prefix": "fn", "body": ["fn ${1:n}() {", "\t$0", "}"], "description": "f" },
            "Print": { "prefix": ["pr", "println"], "body": "println!($0);" },
            "NoBody": { "prefix": "x" }
        }"#;
        let s = parse_json(json, &Scope::Global);
        // NoBody is skipped (no body); the rest parse, sorted by name.
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "Func");
        assert_eq!(s[0].body, "fn ${1:n}() {\n\t$0\n}");
        assert_eq!(s[0].prefixes, vec!["fn"]);
        assert_eq!(s[0].description, "f");
        assert_eq!(s[1].prefixes, vec!["pr", "println"]);
    }

    #[test]
    fn malformed_json_yields_nothing() {
        assert!(parse_json("{ not json", &Scope::Global).is_empty());
        assert!(parse_json("[]", &Scope::Global).is_empty());
    }

    #[test]
    fn media_type_rel_splits_into_path_segments() {
        let rel = media_type_rel("text/rust");
        assert_eq!(rel, std::path::Path::new("media-types/text/rust/snippets"));
        let rel = media_type_rel("application/sql");
        assert_eq!(
            rel,
            std::path::Path::new("media-types/application/sql/snippets")
        );
    }

    #[test]
    fn load_dir_reads_all_json_files() {
        let dir = std::env::temp_dir().join(format!("vix-snip-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("a.json"), r#"{"A": {"body": "1"}}"#).unwrap();
        std::fs::write(dir.join("examples.json"), r#"{"B": {"body": "2"}}"#).unwrap();
        std::fs::write(dir.join("ignore.txt"), "not json").unwrap();
        let snips = load_dir(&dir, &Scope::Project);
        assert_eq!(snips.len(), 2, "both JSON files load, the .txt is ignored");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_shadows_by_name() {
        let base = parse_json(
            r#"{"A": {"body": "1"}, "B": {"body": "2"}}"#,
            &Scope::Global,
        );
        let extra = parse_json(
            r#"{"A": {"body": "override"}, "C": {"body": "3"}}"#,
            &Scope::Project,
        );
        let merged = merge(base, extra);
        assert_eq!(merged.len(), 3);
        let a = merged.iter().find(|s| s.name == "A").unwrap();
        assert_eq!(a.body, "override");
        assert_eq!(a.scope, Scope::Project);
    }

    #[test]
    fn find_by_prefix_matches_any_listed_prefix() {
        let s = parse_json(
            r#"{"P": {"prefix": ["pr", "println"], "body": "x"}}"#,
            &Scope::Global,
        );
        assert!(find_by_prefix(&s, "println").is_some());
        assert!(find_by_prefix(&s, "pr").is_some());
        assert!(find_by_prefix(&s, "nope").is_none());
    }

    #[test]
    fn picker_filters_by_query() {
        let lib = parse_json(
            r#"{"Alpha": {"prefix":"al","body":"x"}, "Beta": {"body":"y","description":"second"}}"#,
            &Scope::Global,
        );
        let mut p = Picker::new();
        assert_eq!(p.len(&lib), 2);
        p.push('a');
        p.push('l');
        assert_eq!(p.len(&lib), 1);
        assert_eq!(
            p.selected_library_index(&lib).map(|i| lib[i].name.clone()),
            Some("Alpha".to_string())
        );
    }
}
