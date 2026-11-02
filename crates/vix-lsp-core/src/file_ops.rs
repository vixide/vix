//! File-operation and file-watching messages (T623): `workspace/willRenameFiles`,
//! `workspace/didRenameFiles`, `workspace/didChangeWatchedFiles`, and the
//! glob filters a server uses to say which paths it cares about, either
//! statically (`capabilities.workspace.fileOperations`) or dynamically
//! (`client/registerCapability` with `workspace/didChangeWatchedFiles`).

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Value, json};

/// What happened to a watched file, per `FileChangeType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileChangeType {
    /// The file appeared.
    Created,
    /// The file's content changed.
    Changed,
    /// The file went away.
    Deleted,
}

impl FileChangeType {
    /// The protocol number sent in a `FileEvent`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Created => 1,
            Self::Changed => 2,
            Self::Deleted => 3,
        }
    }

    /// The `WatchKind` bit a watcher sets to receive this change.
    #[must_use]
    pub fn watch_bit(self) -> u8 {
        match self {
            Self::Created => 1,
            Self::Changed => 2,
            Self::Deleted => 4,
        }
    }
}

/// One glob a server registered interest in.
#[derive(Clone, Debug)]
struct Glob {
    matcher: GlobMatcher,
    /// `Some(true)` matches only directories, `Some(false)` only files.
    dirs: Option<bool>,
}

impl Glob {
    /// Parse a glob string; `None` when it is malformed.
    fn new(pattern: &str, ignore_case: bool, dirs: Option<bool>) -> Option<Self> {
        let glob = GlobBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .literal_separator(true)
            .build()
            .ok()?;
        Some(Self {
            matcher: glob.compile_matcher(),
            dirs,
        })
    }

    /// Whether `path` matches, either as-is or relative to one of `roots`
    /// (a pattern without a leading `**/` is relative to a workspace folder).
    fn matches(&self, path: &Path, is_dir: bool, roots: &[PathBuf]) -> bool {
        if self.dirs.is_some_and(|d| d != is_dir) {
            return false;
        }
        if self.matcher.is_match(path) {
            return true;
        }
        roots.iter().any(|root| {
            path.strip_prefix(root)
                .is_ok_and(|rel| self.matcher.is_match(rel))
        })
    }
}

/// The paths a server wants file-operation requests for. An empty filter list
/// (the server named the operation without any filters) matches everything.
#[derive(Clone, Debug, Default)]
pub struct FileOpFilters {
    globs: Vec<Glob>,
    /// Filters existed but none applied to `file:` paths: match nothing.
    none: bool,
}

impl FileOpFilters {
    /// Whether the server wants the operation for `path`.
    #[must_use]
    pub fn matches(&self, path: &Path, is_dir: bool, roots: &[PathBuf]) -> bool {
        if self.none {
            return false;
        }
        self.globs.is_empty() || self.globs.iter().any(|g| g.matches(path, is_dir, roots))
    }
}

/// Read `capabilities.workspace.fileOperations.<op>` (`willRename` or
/// `didRename`) from an `initialize` result. `None` when the server does not
/// offer the operation at all.
#[must_use]
pub fn parse_file_operation(result: &Value, op: &str) -> Option<FileOpFilters> {
    let entry = result
        .get("capabilities")?
        .get("workspace")?
        .get("fileOperations")?
        .get(op)?;
    if entry.is_null() {
        return None;
    }
    let mut out = FileOpFilters::default();
    if let Some(filters) = entry.get("filters").and_then(Value::as_array) {
        for filter in filters {
            if filter
                .get("scheme")
                .and_then(Value::as_str)
                .is_some_and(|s| s != "file")
            {
                continue;
            }
            let Some(pattern) = filter.get("pattern") else {
                continue;
            };
            let Some(glob) = pattern.get("glob").and_then(Value::as_str) else {
                continue;
            };
            let dirs = match pattern.get("matches").and_then(Value::as_str) {
                Some("file") => Some(false),
                Some("folder") => Some(true),
                _ => None,
            };
            let ignore_case = pattern
                .get("options")
                .and_then(|o| o.get("ignoreCase"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Some(g) = Glob::new(glob, ignore_case, dirs) {
                out.globs.push(g);
            }
        }
        out.none = out.globs.is_empty() && !filters.is_empty();
    }
    Some(out)
}

/// One `FileSystemWatcher` a server registered dynamically.
#[derive(Clone, Debug)]
pub struct Watcher {
    glob: Glob,
    /// The `WatchKind` bit set (create 1, change 2, delete 4).
    kind: u8,
}

impl Watcher {
    /// Whether this watcher wants a `change` to `path`.
    #[must_use]
    pub fn wants(&self, path: &Path, change: FileChangeType, roots: &[PathBuf]) -> bool {
        self.kind & change.watch_bit() != 0 && self.glob.matches(path, false, roots)
    }
}

/// A `client/registerCapability` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    /// The registration id, quoted back by `client/unregisterCapability`.
    pub id: String,
    /// The registered method.
    pub method: String,
    /// The raw `registerOptions`.
    pub options: Value,
}

/// Parse the `registrations` of a `client/registerCapability` request.
#[must_use]
pub fn parse_registrations(params: &Value) -> Vec<Registration> {
    params
        .get("registrations")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|r| {
                    Some(Registration {
                        id: r.get("id")?.as_str()?.to_string(),
                        method: r.get("method")?.as_str()?.to_string(),
                        options: r.get("registerOptions").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the `unregisterations` (the protocol's own spelling) of a
/// `client/unregisterCapability` request into registration ids.
#[must_use]
pub fn parse_unregistrations(params: &Value) -> Vec<String> {
    params
        .get("unregisterations")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|r| r.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The watchers in a `workspace/didChangeWatchedFiles` registration's options.
/// A glob may be a plain string or a `RelativePattern` (`{baseUri, pattern}`;
/// the base is dropped and the pattern matched against workspace-relative and
/// absolute paths alike).
#[must_use]
pub fn parse_watchers(options: &Value) -> Vec<Watcher> {
    options
        .get("watchers")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|w| {
                    let g = w.get("globPattern")?;
                    let pattern = g.as_str().or_else(|| g.get("pattern")?.as_str())?;
                    let kind = w
                        .get("kind")
                        .and_then(Value::as_u64)
                        .and_then(|k| u8::try_from(k).ok())
                        .unwrap_or(7);
                    Some(Watcher {
                        glob: Glob::new(pattern, false, Some(false))?,
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Params for `workspace/willRenameFiles` and `workspace/didRenameFiles`.
#[must_use]
pub fn rename_files_params(old_uri: &str, new_uri: &str) -> Value {
    json!({ "files": [{ "oldUri": old_uri, "newUri": new_uri }] })
}

/// Params for `workspace/didChangeWatchedFiles`: `(uri, change)` pairs.
#[must_use]
pub fn watched_files_params(changes: &[(String, FileChangeType)]) -> Value {
    json!({
        "changes": changes
            .iter()
            .map(|(uri, c)| json!({ "uri": uri, "type": c.code() }))
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn will_rename_filters_follow_the_initialize_result() {
        let result = json!({ "capabilities": { "workspace": { "fileOperations": {
            "willRename": { "filters": [
                { "pattern": { "glob": "**/*.{ts,tsx}", "matches": "file" } },
                { "scheme": "ftp", "pattern": { "glob": "**/*.rs" } }
            ] },
            "didRename": {}
        } } } });
        let will = parse_file_operation(&result, "willRename").unwrap();
        assert!(will.matches(Path::new("/w/src/a.ts"), false, &[]));
        assert!(!will.matches(Path::new("/w/src/a.rs"), false, &[]));
        assert!(!will.matches(Path::new("/w/src/a.ts"), true, &[]));
        let did = parse_file_operation(&result, "didRename").unwrap();
        assert!(did.matches(Path::new("/anything"), false, &[]));
        assert!(parse_file_operation(&result, "willCreate").is_none());
        assert!(parse_file_operation(&json!({}), "willRename").is_none());
    }

    #[test]
    fn other_scheme_only_filters_match_nothing() {
        let result = json!({ "capabilities": { "workspace": { "fileOperations": {
            "willRename": { "filters": [
                { "scheme": "ftp", "pattern": { "glob": "**" } }
            ] }
        } } } });
        let f = parse_file_operation(&result, "willRename").unwrap();
        assert!(!f.matches(Path::new("/w/a.rs"), false, &[]));
    }

    #[test]
    fn relative_globs_match_against_workspace_roots() {
        let result = json!({ "capabilities": { "workspace": { "fileOperations": {
            "didRename": { "filters": [ { "pattern": { "glob": "src/*.rs" } } ] }
        } } } });
        let f = parse_file_operation(&result, "didRename").unwrap();
        let roots = [PathBuf::from("/w")];
        assert!(f.matches(Path::new("/w/src/a.rs"), false, &roots));
        assert!(!f.matches(Path::new("/w/src/deep/a.rs"), false, &roots));
    }

    #[test]
    fn registrations_and_watchers_parse() {
        let params = json!({ "registrations": [
            { "id": "1", "method": "workspace/didChangeWatchedFiles",
              "registerOptions": { "watchers": [
                { "globPattern": "**/Cargo.toml", "kind": 2 },
                { "globPattern": { "baseUri": "file:///w", "pattern": "package.json" } }
              ] } },
            { "id": "2", "method": "textDocument/formatting" }
        ] });
        let regs = parse_registrations(&params);
        assert_eq!(regs.len(), 2);
        let ws = parse_watchers(&regs[0].options);
        assert_eq!(ws.len(), 2);
        let toml = Path::new("/w/crates/a/Cargo.toml");
        assert!(ws[0].wants(toml, FileChangeType::Changed, &[]));
        assert!(!ws[0].wants(toml, FileChangeType::Created, &[]));
        let roots = [PathBuf::from("/w")];
        assert!(ws[1].wants(
            Path::new("/w/package.json"),
            FileChangeType::Deleted,
            &roots
        ));
        let un = json!({ "unregisterations": [ { "id": "1", "method": "x" } ] });
        assert_eq!(parse_unregistrations(&un), vec!["1".to_string()]);
    }

    #[test]
    fn params_have_the_protocol_shape() {
        let p = rename_files_params("file:///a", "file:///b");
        assert_eq!(p["files"][0]["oldUri"], "file:///a");
        assert_eq!(p["files"][0]["newUri"], "file:///b");
        let w = watched_files_params(&[("file:///a".into(), FileChangeType::Deleted)]);
        assert_eq!(w["changes"][0]["type"], 3);
    }
}
