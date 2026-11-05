//! File-operation and file-watching messages (T623):
//! `workspace/will{Create,Rename,Delete}Files`, their `did` counterparts,
//! `workspace/didChangeWatchedFiles`, and the glob filters a server uses to say
//! which paths it cares about, either statically
//! (`capabilities.workspace.fileOperations`) or dynamically
//! (`client/registerCapability` with `workspace/didChangeWatchedFiles`), plus
//! the bounded directory scan that lets registered globs see files nobody has
//! open.

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
    /// The compiled glob.
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
    /// The accepted filters; a path matches when any of them does.
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
    /// Which paths the watcher covers.
    glob: Glob,
    /// The `WatchKind` bit set (create 1, change 2, delete 4).
    kind: u8,
}

impl Watcher {
    /// Whether `path` (a file) is one this watcher's glob covers, for any kind
    /// of change; what a directory scan asks before remembering a file.
    #[must_use]
    pub fn covers(&self, path: &Path, roots: &[PathBuf]) -> bool {
        self.glob.matches(path, false, roots)
    }

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

/// One kind of file operation a server can ask to hear about
/// (`capabilities.workspace.fileOperations`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    /// A file or folder is created.
    Create,
    /// A file or folder is renamed or moved.
    Rename,
    /// A file or folder is deleted.
    Delete,
}

impl FileOp {
    /// Every operation.
    pub const ALL: [Self; 3] = [Self::Create, Self::Rename, Self::Delete];

    /// An index into per-operation tables.
    fn index(self) -> usize {
        match self {
            Self::Create => 0,
            Self::Rename => 1,
            Self::Delete => 2,
        }
    }

    /// The `fileOperations` capability key of the request form (`willRename`).
    #[must_use]
    pub fn will_key(self) -> &'static str {
        match self {
            Self::Create => "willCreate",
            Self::Rename => "willRename",
            Self::Delete => "willDelete",
        }
    }

    /// The `fileOperations` capability key of the notification form (`didRename`).
    #[must_use]
    pub fn did_key(self) -> &'static str {
        match self {
            Self::Create => "didCreate",
            Self::Rename => "didRename",
            Self::Delete => "didDelete",
        }
    }

    /// The request method (`workspace/willRenameFiles`).
    #[must_use]
    pub fn will_method(self) -> &'static str {
        match self {
            Self::Create => "workspace/willCreateFiles",
            Self::Rename => "workspace/willRenameFiles",
            Self::Delete => "workspace/willDeleteFiles",
        }
    }

    /// The notification method (`workspace/didRenameFiles`).
    #[must_use]
    pub fn did_method(self) -> &'static str {
        match self {
            Self::Create => "workspace/didCreateFiles",
            Self::Rename => "workspace/didRenameFiles",
            Self::Delete => "workspace/didDeleteFiles",
        }
    }
}

/// One file or folder a file operation applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOpItem {
    /// The path the operation acts on: the created or deleted path, or a
    /// rename's old path.
    pub path: PathBuf,
    /// A rename's new path; `None` for a create or delete.
    pub new_path: Option<PathBuf>,
    /// Whether it is a folder (known by the caller before a delete removes it),
    /// matched against a filter's `matches: "file" | "folder"`.
    pub is_dir: bool,
}

impl FileOpItem {
    /// A created or deleted `path`.
    #[must_use]
    pub fn single(path: &Path, is_dir: bool) -> Self {
        Self {
            path: path.to_path_buf(),
            new_path: None,
            is_dir,
        }
    }

    /// A rename of `old` to `new`.
    #[must_use]
    pub fn renamed(old: &Path, new: &Path, is_dir: bool) -> Self {
        Self {
            path: old.to_path_buf(),
            new_path: Some(new.to_path_buf()),
            is_dir,
        }
    }
}

/// Which file operations one server wants to hear about, read from its
/// `initialize` result.
#[derive(Clone, Debug, Default)]
pub struct FileOpCaps {
    /// `will*` filters, indexed by [`FileOp`].
    will: [Option<FileOpFilters>; 3],
    /// `did*` filters, indexed by [`FileOp`].
    did: [Option<FileOpFilters>; 3],
}

impl FileOpCaps {
    /// Read every `willX`/`didX` entry of an `initialize` result.
    #[must_use]
    pub fn from_initialize(result: &Value) -> Self {
        let mut caps = Self::default();
        for op in FileOp::ALL {
            caps.will[op.index()] = parse_file_operation(result, op.will_key());
            caps.did[op.index()] = parse_file_operation(result, op.did_key());
        }
        caps
    }

    /// The items of `items` this server wants `op` for (the request form when
    /// `will`, else the notification form), honoring its filters. A rename's
    /// request is matched against the old path and its notification against
    /// the new one.
    #[must_use]
    pub fn wanted<'a>(
        &self,
        op: FileOp,
        will: bool,
        items: &'a [FileOpItem],
        roots: &[PathBuf],
    ) -> Vec<&'a FileOpItem> {
        let filters = if will {
            &self.will[op.index()]
        } else {
            &self.did[op.index()]
        };
        let Some(filters) = filters else {
            return Vec::new();
        };
        items
            .iter()
            .filter(|item| {
                let path = match (&item.new_path, will) {
                    (Some(new), false) => new,
                    _ => &item.path,
                };
                filters.matches(path, item.is_dir, roots)
            })
            .collect()
    }
}

/// Params for any `workspace/{will,did}{Create,Rename,Delete}Files`: one
/// `(uri, new_uri)` pair per file; the second is `Some` only for a rename.
#[must_use]
pub fn file_operation_params(files: &[(String, Option<String>)]) -> Value {
    let files: Vec<Value> = files
        .iter()
        .map(|(uri, new)| match new {
            Some(new) => json!({ "oldUri": uri, "newUri": new }),
            None => json!({ "uri": uri }),
        })
        .collect();
    json!({ "files": files })
}

/// Directories a watcher scan never enters.
const SCAN_SKIP: [&str; 5] = [".git", ".hg", ".svn", "target", "node_modules"];

/// How many directory entries one [`scan_watched`] may visit.
pub const SCAN_MAX_ENTRIES: usize = 10_000;

/// How deep [`scan_watched`] descends below a root.
pub const SCAN_MAX_DEPTH: usize = 16;

/// A file's `(mtime in nanoseconds since the epoch, length)`.
type Stamp = (u128, u64);

/// The covered files found by one [`scan_watched`].
#[derive(Clone, Debug, Default)]
pub struct WatchSnapshot {
    /// Each covered file and its stamp.
    files: std::collections::BTreeMap<PathBuf, Stamp>,
    /// Whether the walk finished within its budget; when not, files it did
    /// not reach are not reported as deleted.
    complete: bool,
}

impl WatchSnapshot {
    /// How many covered files were found.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether no covered file was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Whether the walk finished within its entry budget.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

/// Walk `roots` for the files `covers` accepts, without ever visiting more than
/// `max_entries` directory entries or descending past [`SCAN_MAX_DEPTH`], never
/// following symlinks and skipping version-control and build directories
/// (`.git`, `target`, `node_modules`, ...). Each directory is read in sorted
/// order, so a truncated walk reaches the same files every time.
#[must_use]
pub fn scan_watched(
    roots: &[PathBuf],
    covers: &dyn Fn(&Path) -> bool,
    max_entries: usize,
) -> WatchSnapshot {
    let mut files = std::collections::BTreeMap::new();
    let mut budget = max_entries;
    let mut complete = true;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().rev().map(|r| (r.clone(), 0)).collect();
    'walk: while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        let mut subdirs = Vec::new();
        for entry in entries {
            if budget == 0 {
                complete = false;
                break 'walk;
            }
            budget -= 1;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                let skipped = entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| SCAN_SKIP.contains(&n));
                if !skipped && depth < SCAN_MAX_DEPTH {
                    subdirs.push((path, depth + 1));
                }
            } else if kind.is_file()
                && covers(&path)
                && let Ok(meta) = entry.metadata()
            {
                let nanos = meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                files.insert(path, (nanos, meta.len()));
            }
        }
        stack.extend(subdirs.into_iter().rev());
    }
    WatchSnapshot { files, complete }
}

/// What changed between two scans: files only in `new` are created, files in
/// both with a different stamp are changed, and files only in `old` are
/// deleted (only when `new` finished its walk, since an unfinished one cannot
/// say a file is gone).
#[must_use]
pub fn diff_snapshots(old: &WatchSnapshot, new: &WatchSnapshot) -> Vec<(PathBuf, FileChangeType)> {
    let mut out = Vec::new();
    for (path, stamp) in &new.files {
        match old.files.get(path) {
            None => out.push((path.clone(), FileChangeType::Created)),
            Some(prev) if prev != stamp => out.push((path.clone(), FileChangeType::Changed)),
            Some(_) => {}
        }
    }
    if new.complete {
        for path in old.files.keys() {
            if !new.files.contains_key(path) {
                out.push((path.clone(), FileChangeType::Deleted));
            }
        }
    }
    out
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
    fn every_operation_has_its_own_filters_and_folder_kind() {
        let result = json!({ "capabilities": { "workspace": { "fileOperations": {
            "willCreate": { "filters": [ { "pattern": { "glob": "**/*.ts", "matches": "file" } } ] },
            "didRename": { "filters": [ { "pattern": { "glob": "**/src", "matches": "folder" } } ] },
            "willDelete": {}
        } } } });
        let caps = FileOpCaps::from_initialize(&result);
        let items = [
            FileOpItem::single(Path::new("/w/a.ts"), false),
            FileOpItem::single(Path::new("/w/dir.ts"), true),
            FileOpItem::single(Path::new("/w/a.rs"), false),
        ];
        let got = caps.wanted(FileOp::Create, true, &items, &[]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, Path::new("/w/a.ts"));
        // A server that did not name the operation is never sent it.
        assert_eq!(caps.wanted(FileOp::Create, false, &items, &[]).len(), 0);
        assert_eq!(caps.wanted(FileOp::Rename, true, &items, &[]).len(), 0);
        // No filters: everything, files and folders alike.
        assert_eq!(caps.wanted(FileOp::Delete, true, &items, &[]).len(), 3);
        // A folder filter matches a folder rename (by its new path), not a file.
        let renames = [
            FileOpItem::renamed(Path::new("/w/old"), Path::new("/w/src"), true),
            FileOpItem::renamed(Path::new("/w/x"), Path::new("/w/src"), false),
        ];
        let got = caps.wanted(FileOp::Rename, false, &renames, &[]);
        assert_eq!(got.len(), 1);
        assert!(got[0].is_dir);
    }

    #[test]
    fn operation_params_have_the_protocol_shape() {
        let created = file_operation_params(&[("file:///a".into(), None)]);
        assert_eq!(created["files"][0]["uri"], "file:///a");
        let renamed = file_operation_params(&[("file:///a".into(), Some("file:///b".into()))]);
        assert_eq!(renamed["files"][0]["newUri"], "file:///b");
        assert_eq!(FileOp::Delete.will_method(), "workspace/willDeleteFiles");
        assert_eq!(FileOp::Create.did_method(), "workspace/didCreateFiles");
    }

    #[test]
    fn scan_is_recursive_bounded_and_diffs_into_events() {
        let root = std::env::temp_dir().join(format!("vix-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("a/b/x.toml"), "1").unwrap();
        std::fs::write(root.join("a/y.toml"), "1").unwrap();
        std::fs::write(root.join("a/skip.txt"), "1").unwrap();
        std::fs::write(root.join("target/z.toml"), "1").unwrap();
        let roots = [root.clone()];
        let covers = |p: &Path| p.extension().is_some_and(|e| e == "toml");
        let first = scan_watched(&roots, &covers, SCAN_MAX_ENTRIES);
        assert_eq!(first.len(), 2, "target/ is skipped, txt is not covered");
        assert!(first.is_complete());

        std::fs::write(root.join("a/b/x.toml"), "changed!").unwrap();
        std::fs::remove_file(root.join("a/y.toml")).unwrap();
        std::fs::write(root.join("a/b/new.toml"), "1").unwrap();
        let second = scan_watched(&roots, &covers, SCAN_MAX_ENTRIES);
        let mut events = diff_snapshots(&first, &second);
        events.sort_by_key(|(p, _)| p.clone());
        assert_eq!(
            events,
            vec![
                (root.join("a/b/new.toml"), FileChangeType::Created),
                (root.join("a/b/x.toml"), FileChangeType::Changed),
                (root.join("a/y.toml"), FileChangeType::Deleted),
            ]
        );

        // A tiny budget stops the walk and so reports no deletions.
        let cut = scan_watched(&roots, &covers, 2);
        assert!(!cut.is_complete());
        assert!(
            diff_snapshots(&second, &cut)
                .iter()
                .all(|(_, c)| *c != FileChangeType::Deleted)
        );
        let _ = std::fs::remove_dir_all(&root);
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
