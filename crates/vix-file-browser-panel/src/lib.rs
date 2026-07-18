//! The file browser panel's listing, search, sort, and filter state.
//!
//! Vix's File → Open… opens a *file browser*: a recursive listing of the
//! current root directory (walked with `walkdir`) that the user narrows by
//! typing a query, reorders by name / size / date created / date modified, and
//! navigates with the arrow keys (or the mouse). This crate is pure state and
//! logic — walking, matching, ranking, and sorting — so it is tested without a
//! terminal; the host wires keys, renders rows, and opens the chosen file.
//!
//! The query is a whitespace-separated list of tokens, each one of:
//!
//! - `ext:rs` (or `ext:rs,toml`, or a bare `.rs`) — keep files with one of the
//!   extensions;
//! - a glob — a token containing `*` or `?` (e.g. `*.lock`, `src/*test*`) —
//!   matched case-insensitively against the root-relative path;
//! - anything else — fuzzy-matched (subsequence, case-insensitive) against the
//!   root-relative path via `vix_palette::fuzzy_score`.
//!
//! An entry must match every token. While the sort is untouched (name,
//! ascending) and at least one fuzzy token is present, files are ranked by
//! fuzzy score instead, best match first; an explicit sort always wins.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

/// Cap on the number of entries kept from one walk, so an enormous tree cannot
/// stall the UI. When the cap is hit, [`Panel::truncated`] is set and the host
/// shows a marker.
pub const DEFAULT_MAX_ENTRIES: usize = 10_000;

/// One walked file or directory.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Absolute (root-joined) path, used to open the file or re-root the panel.
    pub path: PathBuf,
    /// Path relative to the panel root, `/`-joined; what queries match against.
    pub rel: String,
    /// File name (final component), shown first in the row.
    pub name: String,
    /// Whether this entry is a directory (listed before files, opened by
    /// re-rooting the panel).
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Creation time in seconds since the Unix epoch; `None` where the
    /// filesystem does not record it.
    pub created: Option<i64>,
    /// Last-modified time in seconds since the Unix epoch, or `None`.
    pub modified: Option<i64>,
}

/// The column the listing is ordered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    /// Root-relative path, case-insensitive (the default).
    Name,
    /// Size in bytes.
    Size,
    /// Date created.
    Created,
    /// Date modified.
    Modified,
}

impl SortKey {
    /// The next key in the cycle Name → Size → Created → Modified → Name.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Created,
            SortKey::Created => SortKey::Modified,
            SortKey::Modified => SortKey::Name,
        }
    }

    /// The i18n key for this sort key's display name; the host translates.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            SortKey::Name => "ui.file_browser_sort_name",
            SortKey::Size => "ui.file_browser_sort_size",
            SortKey::Created => "ui.file_browser_sort_created",
            SortKey::Modified => "ui.file_browser_sort_modified",
        }
    }
}

/// One parsed query token (see the crate docs for the syntax).
enum Token {
    /// `ext:rs,toml` or `.rs`: keep files whose extension is listed.
    Ext(Vec<String>),
    /// A token containing `*` or `?`: glob over the relative path.
    Glob(String),
    /// Anything else: fuzzy subsequence over the relative path.
    Fuzzy(String),
}

/// Query + listing + selection state for the file browser overlay. `selected`
/// and `scroll` index into the *filtered* row list (see [`Panel::matches`]).
pub struct Panel {
    /// The directory being browsed; entries are walked from here.
    pub root: PathBuf,
    /// The walked entries, in walk order (filtering and ordering happen in
    /// [`Panel::matches`]).
    pub entries: Vec<Entry>,
    /// The current filter text (see the crate docs for the token syntax).
    pub query: String,
    /// The active sort column.
    pub sort: SortKey,
    /// Sort direction; `true` = ascending.
    pub ascending: bool,
    /// Whether hidden (dot-prefixed) files and directories are listed.
    pub show_hidden: bool,
    /// Index of the highlighted row within the filtered list.
    pub selected: usize,
    /// First visible filtered row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
    /// Whether the last walk stopped at [`Panel::max_entries`].
    pub truncated: bool,
    /// Cap on walked entries (see [`DEFAULT_MAX_ENTRIES`]).
    pub max_entries: usize,
}

impl Panel {
    /// Open a browser rooted at `root` with the default settings, walking the
    /// tree immediately.
    #[must_use]
    pub fn open(root: &Path) -> Self {
        let mut p = Panel {
            root: root.to_path_buf(),
            entries: Vec::new(),
            query: String::new(),
            sort: SortKey::Name,
            ascending: true,
            show_hidden: false,
            selected: 0,
            scroll: 0,
            truncated: false,
            max_entries: DEFAULT_MAX_ENTRIES,
        };
        p.refresh();
        p
    }

    /// Re-walk [`Panel::root`], honoring [`Panel::show_hidden`] and stopping at
    /// [`Panel::max_entries`]. Resets the highlight to the top.
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.truncated = false;
        self.selected = 0;
        self.scroll = 0;
        let show_hidden = self.show_hidden;
        let walker = WalkDir::new(&self.root)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(move |e| show_hidden || !is_hidden(e.file_name()));
        for entry in walker.filter_map(Result::ok) {
            if self.entries.len() >= self.max_entries {
                self.truncated = true;
                break;
            }
            let rel = entry
                .path()
                .strip_prefix(&self.root)
                .unwrap_or_else(|_| entry.path())
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let meta = entry.metadata().ok();
            let is_dir = entry.file_type().is_dir();
            self.entries.push(Entry {
                path: entry.path().to_path_buf(),
                rel,
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir,
                size: if is_dir {
                    0
                } else {
                    meta.as_ref().map_or(0, std::fs::Metadata::len)
                },
                created: meta.as_ref().and_then(|m| unix_secs(m.created().ok())),
                modified: meta.as_ref().and_then(|m| unix_secs(m.modified().ok())),
            });
        }
    }

    /// Indices into [`Panel::entries`] of the rows matching the current query,
    /// in display order: directories first, then per the active sort — or by
    /// fuzzy relevance while the sort is untouched and a fuzzy token is typed.
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        let tokens = parse_query(&self.query);
        let mut scored: Vec<(usize, i32)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| entry_score(e, &tokens).map(|s| (i, s)))
            .collect();
        let has_fuzzy = tokens.iter().any(|t| matches!(t, Token::Fuzzy(_)));
        let by_relevance = has_fuzzy && self.sort == SortKey::Name && self.ascending;
        scored.sort_by(|&(a, sa), &(b, sb)| {
            let (ea, eb) = (&self.entries[a], &self.entries[b]);
            // Directories group before files regardless of the sort.
            eb.is_dir.cmp(&ea.is_dir).then_with(|| {
                if by_relevance {
                    sb.cmp(&sa).then_with(|| cmp_key(ea, eb, SortKey::Name))
                } else {
                    let ord = cmp_key(ea, eb, self.sort);
                    if self.ascending { ord } else { ord.reverse() }
                }
            })
        });
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Number of rows matching the current filter.
    #[must_use]
    pub fn len(&self) -> usize {
        self.matches().len()
    }

    /// Whether the filter matches no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append a character to the filter and reset the highlight to the top.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll = 0;
    }

    /// Remove the last character of the filter and reset the highlight.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < self.len() {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = (self.selected + page.max(1)).min(self.len().saturating_sub(1));
    }

    /// Scroll just enough to keep the highlight inside a `view_h`-row viewport.
    pub fn ensure_visible(&mut self, view_h: usize) {
        let h = view_h.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
    }

    /// Advance to the next sort column (name → size → created → modified),
    /// keeping the direction.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Flip between ascending and descending.
    pub fn toggle_order(&mut self) {
        self.ascending = !self.ascending;
        self.selected = 0;
        self.scroll = 0;
    }

    /// Toggle listing hidden (dot-prefixed) entries, re-walking the tree.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    /// The highlighted entry, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.matches().get(self.selected).map(|&i| &self.entries[i])
    }

    /// Act on the highlighted row: a directory re-roots the panel into it (and
    /// returns `None`); a file returns its path for the host to open.
    pub fn activate(&mut self) -> Option<PathBuf> {
        let entry = self.selected_entry()?;
        if entry.is_dir {
            self.root = entry.path.clone();
            self.query.clear();
            self.refresh();
            None
        } else {
            Some(entry.path.clone())
        }
    }

    /// The `line`/`column` jump target when the query ends a token with
    /// `:line[:col]` (e.g. `main.rs:120`), as the classic Open prompt accepts.
    /// The suffix is also ignored while matching.
    #[must_use]
    pub fn target(&self) -> Option<(usize, usize)> {
        self.query
            .split_whitespace()
            .find_map(|w| vix_palette::parse_path_target(w).1)
    }

    /// Re-root the panel at the parent directory, if there is one.
    pub fn parent(&mut self) {
        if let Some(parent) = self.root.parent().map(Path::to_path_buf) {
            self.root = parent;
            self.query.clear();
            self.refresh();
        }
    }
}

/// Compare two entries under one sort key (ascending). Times missing from the
/// filesystem sort as oldest; name breaks size/date ties.
fn cmp_key(a: &Entry, b: &Entry, key: SortKey) -> std::cmp::Ordering {
    let by_name = |x: &Entry, y: &Entry| {
        x.rel
            .to_lowercase()
            .cmp(&y.rel.to_lowercase())
            .then_with(|| x.rel.cmp(&y.rel))
    };
    match key {
        SortKey::Name => by_name(a, b),
        SortKey::Size => a.size.cmp(&b.size).then_with(|| by_name(a, b)),
        SortKey::Created => a
            .created
            .unwrap_or(i64::MIN)
            .cmp(&b.created.unwrap_or(i64::MIN))
            .then_with(|| by_name(a, b)),
        SortKey::Modified => a
            .modified
            .unwrap_or(i64::MIN)
            .cmp(&b.modified.unwrap_or(i64::MIN))
            .then_with(|| by_name(a, b)),
    }
}

/// Whether `entry` passes every token; `Some(total fuzzy score)` when it does.
/// Directories are exempt from extension filters (they keep their contents
/// reachable) but must still pass glob and fuzzy tokens.
fn entry_score(entry: &Entry, tokens: &[Token]) -> Option<i32> {
    let mut score = 0i32;
    for token in tokens {
        match token {
            Token::Ext(exts) => {
                if entry.is_dir {
                    continue;
                }
                let ext = Path::new(&entry.name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !exts.contains(&ext) {
                    return None;
                }
            }
            Token::Glob(pat) => {
                if !glob_match(pat, &entry.rel.to_lowercase()) {
                    return None;
                }
            }
            Token::Fuzzy(text) => {
                score = score.saturating_add(vix_palette::fuzzy_score(&entry.rel, text)?);
            }
        }
    }
    Some(score)
}

/// Split `query` into whitespace-separated [`Token`]s (see the crate docs).
fn parse_query(query: &str) -> Vec<Token> {
    query
        .split_whitespace()
        .filter_map(|word| {
            if let Some(list) = word.strip_prefix("ext:") {
                let exts: Vec<String> = list
                    .split(',')
                    .map(|e| e.trim().trim_start_matches('.').to_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect();
                return (!exts.is_empty()).then_some(Token::Ext(exts));
            }
            if word.contains(['*', '?']) {
                return Some(Token::Glob(word.to_lowercase()));
            }
            // A bare ".rs"-style token reads as an extension filter.
            if let Some(rest) = word.strip_prefix('.')
                && !rest.is_empty()
                && !rest.contains(['.', '/'])
            {
                return Some(Token::Ext(vec![rest.to_lowercase()]));
            }
            // A trailing ":line[:col]" jump target is not part of the name.
            let (base, target) = vix_palette::parse_path_target(word);
            if target.is_some() && !base.is_empty() {
                return Some(Token::Fuzzy(base));
            }
            Some(Token::Fuzzy(word.to_string()))
        })
        .collect()
}

/// Case-sensitive glob match of `pattern` against `text`, supporting `*` (any
/// run of characters, including `/`) and `?` (any one character). Callers pass
/// both sides lowercased for the case-insensitive behavior the panel documents.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let hay: Vec<char> = text.chars().collect();
    // Two-pointer match with one backtrack point per `*` (classic wildcard walk).
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while t < hay.len() {
        if p < pat.len() && (pat[p] == '?' || pat[p] == hay[t]) {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star = p;
            mark = t;
            p += 1;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

/// A human-readable size such as `973 B`, `4.1 KB`, `12 MB`.
#[must_use]
pub fn size_label(size: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // display only; fine beyond 2^52 bytes
    let mut value = size as f64;
    let mut unit = "B";
    for next in ["KB", "MB", "GB", "TB"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    if unit == "B" {
        format!("{size} {unit}")
    } else if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

/// Whether a file name is hidden (dot-prefixed).
fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// A `SystemTime` as whole seconds since the Unix epoch, or `None`. Shared
/// with other file-listing surfaces (e.g. the recent-files chooser).
#[must_use]
pub fn unix_secs(t: Option<std::time::SystemTime>) -> Option<i64> {
    let t = t?;
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok(),
        // Pre-epoch timestamps (rare, but valid) count downward.
        Err(e) => i64::try_from(e.duration().as_secs()).ok().map(|s| -s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file entry with the given relative path and stats.
    fn file(rel: &str, size: u64, created: i64, modified: i64) -> Entry {
        Entry {
            path: PathBuf::from(format!("/root/{rel}")),
            rel: rel.to_string(),
            name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
            is_dir: false,
            size,
            created: Some(created),
            modified: Some(modified),
        }
    }

    /// A directory entry with the given relative path.
    fn dir(rel: &str) -> Entry {
        Entry {
            is_dir: true,
            size: 0,
            ..file(rel, 0, 0, 0)
        }
    }

    /// A panel over hand-built entries (no filesystem).
    fn panel(entries: Vec<Entry>) -> Panel {
        Panel {
            root: PathBuf::from("/root"),
            entries,
            query: String::new(),
            sort: SortKey::Name,
            ascending: true,
            show_hidden: false,
            selected: 0,
            scroll: 0,
            truncated: false,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    fn rels(p: &Panel) -> Vec<&str> {
        p.matches()
            .into_iter()
            .map(|i| p.entries[i].rel.as_str())
            .collect()
    }

    #[test]
    fn glob_match_wildcards() {
        assert!(glob_match("*.rs", "src/lib.rs"));
        assert!(glob_match("src/*", "src/deep/lib.rs"));
        assert!(glob_match("?ain.rs", "main.rs"));
        assert!(glob_match("*ma*in*", "domain/binary"));
        assert!(!glob_match("*.rs", "src/lib.rs.bak"));
        assert!(!glob_match("?.rs", "ab.rs"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("?", ""));
    }

    #[test]
    fn name_sort_groups_directories_first() {
        let p = panel(vec![
            file("zz.txt", 1, 1, 1),
            dir("aa"),
            file("mm.txt", 1, 1, 1),
        ]);
        assert_eq!(rels(&p), ["aa", "mm.txt", "zz.txt"]);
    }

    #[test]
    fn size_and_date_sorts_follow_direction() {
        let mut p = panel(vec![
            file("small", 1, 30, 300),
            file("big", 900, 10, 100),
            file("mid", 50, 20, 200),
        ]);
        p.sort = SortKey::Size;
        assert_eq!(rels(&p), ["small", "mid", "big"]);
        p.ascending = false;
        assert_eq!(rels(&p), ["big", "mid", "small"]);
        p.sort = SortKey::Created;
        p.ascending = true;
        assert_eq!(rels(&p), ["big", "mid", "small"]);
        p.sort = SortKey::Modified;
        p.ascending = false;
        assert_eq!(rels(&p), ["small", "mid", "big"]);
    }

    #[test]
    fn fuzzy_query_filters_and_ranks_by_relevance() {
        let mut p = panel(vec![
            file("notes/robots.txt", 1, 1, 1),
            file("src/main.rs", 1, 1, 1),
            file("README.md", 1, 1, 1),
        ]);
        p.query = "main".to_string();
        // "main" is a subsequence of src/main.rs only.
        assert_eq!(rels(&p), ["src/main.rs"]);
        // A looser query matches more, ranked best-first (contiguous run wins).
        p.query = "rob".to_string();
        assert_eq!(rels(&p)[0], "notes/robots.txt");
    }

    #[test]
    fn explicit_sort_overrides_fuzzy_ranking() {
        let mut p = panel(vec![
            file("b_match.txt", 5, 1, 1),
            file("a_match.txt", 9, 1, 1),
        ]);
        p.query = "match".to_string();
        p.sort = SortKey::Size;
        assert_eq!(rels(&p), ["b_match.txt", "a_match.txt"]);
    }

    #[test]
    fn ext_and_glob_tokens_filter() {
        let mut p = panel(vec![
            dir("src"),
            file("src/lib.rs", 1, 1, 1),
            file("Cargo.toml", 1, 1, 1),
            file("README.md", 1, 1, 1),
        ]);
        p.query = "ext:rs,toml".to_string();
        // Directories stay reachable under an extension filter.
        assert_eq!(rels(&p), ["src", "Cargo.toml", "src/lib.rs"]);
        p.query = ".md".to_string();
        assert_eq!(rels(&p), ["src", "README.md"]);
        p.query = "*.RS".to_string();
        assert_eq!(rels(&p), ["src/lib.rs"]);
        p.query = "src/* ext:rs".to_string();
        assert_eq!(rels(&p), ["src/lib.rs"]);
    }

    #[test]
    fn walk_respects_hidden_toggle_and_truncation() {
        let root = std::env::temp_dir().join(format!("vix-fb-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), "aaa").unwrap();
        std::fs::write(root.join("sub/b.rs"), "bb").unwrap();
        std::fs::write(root.join(".hidden"), "h").unwrap();

        let mut p = Panel::open(&root);
        assert_eq!(rels(&p), ["sub", "a.txt", "sub/b.rs"]);
        assert!(!p.truncated);
        p.toggle_hidden();
        assert_eq!(rels(&p), ["sub", ".hidden", "a.txt", "sub/b.rs"]);

        p.max_entries = 2;
        p.refresh();
        assert!(p.truncated);
        assert_eq!(p.entries.len(), 2);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn activate_opens_files_and_enters_directories() {
        let root = std::env::temp_dir().join(format!("vix-fb-nav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/inner.txt"), "x").unwrap();

        let mut p = Panel::open(&root);
        // "sub" sorts first; entering it re-roots the walk.
        assert!(p.activate().is_none());
        assert!(p.root.ends_with("sub"));
        assert_eq!(rels(&p), ["inner.txt"]);
        let opened = p.activate().expect("a file returns its path");
        assert!(opened.ends_with("sub/inner.txt"));
        p.parent();
        assert_eq!(rels(&p), ["sub", "sub/inner.txt"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn line_target_suffix_is_parsed_and_ignored_by_matching() {
        let mut p = panel(vec![file("src/main.rs", 1, 1, 1), file("lib.rs", 1, 1, 1)]);
        p.query = "main.rs:12:3".to_string();
        assert_eq!(rels(&p), ["src/main.rs"]);
        assert_eq!(p.target(), Some((12, 3)));
        p.query = "main.rs:12".to_string();
        assert_eq!(p.target(), Some((12, 1)));
        p.query = "main.rs".to_string();
        assert_eq!(p.target(), None);
    }

    #[test]
    fn size_labels_scale() {
        assert_eq!(size_label(0), "0 B");
        assert_eq!(size_label(973), "973 B");
        assert_eq!(size_label(4200), "4.1 KB");
        assert_eq!(size_label(12 * 1024 * 1024), "12 MB");
    }
}
