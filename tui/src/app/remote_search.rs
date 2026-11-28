//! Project-wide line scans over the file index, local or remote (T715).
//!
//! Workspace search, search-to-dock, the TODO finder and Todo Tree, and
//! go-to-definition all walk the same thing: every line of every indexed
//! file. [`App::scan_index`] is that walk. Under the local authority it reads
//! the files in-process, in batches on worker threads that drop files with
//! no candidate line, remembering those misses for a narrower next query
//! ([`ScanMemo`], T830). Under any other authority
//! ([`Authority::is_local`] is `false`: SSH, the test fake) it runs one search
//! tool there through [`Authority::command`] — `rg --json` when the remote
//! has `rg`, else `grep -rnF` for a literal query — so the search costs one
//! round trip instead of one per file. The tool is a prefilter: every line it
//! reports goes through the caller's own regex, and only files the index
//! holds count. Open buffers are scanned in-process from their (possibly
//! unsaved) text, as the local search does. A query no tool can run (a regex
//! without `rg`), or a tool that fails, falls back to the in-process walk.
//!
//! [`Authority::is_local`]: crate::authority::Authority::is_local
//! [`Authority::command`]: crate::authority::Authority::command

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::App;
use crate::workspace_search::tool::{Tool, ToolLine, ToolQuery};

/// Where a scan reads a file's text from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    /// An open buffer's text when one shows the file (so unsaved edits are
    /// searched), else the file, skipping files over 2 MB
    /// ([`App::current_text`]).
    Buffers,
    /// The file itself (`read_to_string_guarded`), ignoring open buffers.
    Disk,
}

/// One line a scan visits.
pub(super) struct IndexLine<'a> {
    /// The indexed file (an authority path).
    pub path: &'a Path,
    /// `path` relative to the workspace root, as results display it.
    pub rel: &'a str,
    /// 1-based line number.
    pub number: usize,
    /// The line without its line ending.
    pub text: &'a str,
}

/// Lines a tool may report before the scan stops reading (and kills it);
/// every caller caps its own results well below this.
#[cfg(feature = "tasks")]
const MAX_TOOL_LINES: usize = 50_000;

/// The largest file [`Source::Buffers`] reads, as [`App::current_text`].
const BUFFERS_MAX_BYTES: u64 = 2_000_000;

/// A line found by a remote tool or an open buffer, before ordering.
struct Found {
    /// Position of the file in the index (the in-process scan's order).
    order: usize,
    number: usize,
    path: PathBuf,
    rel: String,
    text: String,
}

/// Files the in-process scan reads at a time (T830): enough to keep every
/// worker busy, few enough that the kept texts of a batch stay small and a
/// result cap stops the scan soon after it is reached.
const SCAN_BATCH: usize = 2_048;

/// Files a worker takes at a time: neighbors in the index, so most share
/// the directory whose canonical path [`DirCache`] keeps.
const SCAN_GRAB: usize = 32;

/// One worker's last directory and its canonical path.
#[derive(Default)]
struct DirCache(Option<(PathBuf, PathBuf)>);

/// A file's size and modification time, as one `lstat` reports them.
type Stamp = (u64, SystemTime);

/// A file modified this recently is never remembered as a miss: it may
/// change again within its timestamp's granularity without its size or
/// time changing (git's "racy" rule).
const RACY: Duration = Duration::from_secs(2);

/// What the last in-process scan learned (T830): the files in which no line
/// matched its prefilter, with the stamp each had. A later scan whose
/// prefilter can only match lines the remembered one matched (the same
/// regex, or a literal query containing the remembered literal with the
/// same case rule, as typing a query further makes) skips those files while
/// their stamp is unchanged, at the cost of one `lstat` instead of a read.
#[derive(Default)]
pub(crate) struct ScanMemo {
    /// The remembered prefilter.
    regex: String,
    /// The remembered literal and whether it ignores case, for a literal
    /// query.
    literal: Option<(String, bool)>,
    /// Files with no candidate line, by index path.
    misses: HashMap<PathBuf, Stamp>,
}

impl ScanMemo {
    /// The literal of a query with one fixed string.
    fn literal_of(query: &ToolQuery) -> Option<(String, bool)> {
        match query.fixed.as_deref() {
            Some([word]) => Some((word.clone(), query.ignore_case)),
            _ => None,
        }
    }

    /// Whether a line `query`'s regex matches is always one the remembered
    /// prefilter matched, so a remembered miss is a miss for `query` too.
    fn covers(&self, query: &ToolQuery) -> bool {
        if self.misses.is_empty() {
            return false;
        }
        if query.regex == self.regex {
            return true;
        }
        // Case folding is per character, so text matching `new` holds a
        // run matching any substring of it under the same rule.
        match (&self.literal, Self::literal_of(query)) {
            (Some((old, old_fold)), Some((new, new_fold))) => {
                !old.is_empty() && *old_fold == new_fold && new.contains(old.as_str())
            }
            _ => false,
        }
    }
}

/// What a worker found for one indexed file.
enum Text {
    /// Unreadable or too large: nothing to visit.
    Skip,
    /// Read, and no line can match; the stamp when it may be remembered.
    Miss(Option<Stamp>),
    /// A remembered miss whose stamp is unchanged: not read again.
    Unchanged(Stamp),
    /// The file's text, with a line the prefilter matches.
    Read(String),
    /// An open buffer shows the file (at this canonical path): the scanning
    /// thread reads the buffer, since tabs stay on it.
    Buffer(PathBuf),
}

/// How the in-process scan's workers read a file.
struct Reader<'a> {
    auth: &'a dyn crate::authority::Authority,
    source: Source,
    /// Paths open tabs show (empty for [`Source::Disk`]).
    tabs: &'a HashSet<PathBuf>,
    /// The caller's prefilter; `None` keeps every readable file.
    prefilter: Option<&'a regex::Regex>,
    /// The last scan's misses, when they hold for this query.
    memo: Option<&'a HashMap<PathBuf, Stamp>>,
    /// When this scan started, for [`RACY`].
    started: SystemTime,
    /// Read on worker threads: only under the local authority, so a remote
    /// one without a search tool sees the one request at a time it always
    /// did.
    parallel: bool,
}

impl Reader<'_> {
    /// One file, as [`App::current_text`] (for [`Source::Buffers`]) or
    /// `read_to_string_guarded` (for [`Source::Disk`]) reads it.
    fn read(&self, path: &Path, dirs: &mut DirCache) -> Text {
        let quick = self.quick_stat(path, dirs);
        if self.source == Source::Buffers {
            let canon = match &quick {
                Some((canon, _)) => canon.clone(),
                None => self
                    .auth
                    .canonicalize(path)
                    .unwrap_or_else(|_| path.to_path_buf()),
            };
            if self.tabs.contains(&canon) {
                return Text::Buffer(canon);
            }
            let len = match &quick {
                Some((_, stat)) => Some(stat.len),
                None => self.auth.metadata(path).ok().map(|m| m.len),
            };
            if len.is_none_or(|len| len > BUFFERS_MAX_BYTES) {
                return Text::Skip;
            }
        }
        let stamp = quick
            .as_ref()
            .and_then(|(_, stat)| Some((stat.len, stat.modified?)));
        if let (Some(stamp), Some(memo)) = (stamp, self.memo)
            && memo.get(path) == Some(&stamp)
        {
            return Text::Unchanged(stamp);
        }
        let Ok(text) = self.auth.read_to_string_guarded(path) else {
            return Text::Skip;
        };
        let Some(re) = self.prefilter else {
            return Text::Read(text);
        };
        if text.lines().any(|line| re.is_match(line)) {
            return Text::Read(text);
        }
        let settled = |(_, modified): &Stamp| {
            self.started
                .duration_since(*modified)
                .is_ok_and(|age| age >= RACY)
        };
        Text::Miss(stamp.filter(settled))
    }

    /// The canonical path and stat of a file that is not itself a symlink,
    /// from one `lstat` and its directory's canonical path (cached in
    /// `dirs`): what `canonicalize` and `metadata` would say, without
    /// `canonicalize`'s walk of every component, which costs about four
    /// times a stat and does not get faster on more threads (T830).
    /// `None` for a symlink, a path without a parent, any error, or a
    /// remote authority (`parallel` off), so the caller asks the slow way.
    fn quick_stat(
        &self,
        path: &Path,
        dirs: &mut DirCache,
    ) -> Option<(PathBuf, crate::authority::FileStat)> {
        if !self.parallel {
            return None;
        }
        let meta = self.auth.symlink_metadata(path).ok()?;
        if meta.kind == crate::authority::FileKind::Symlink {
            return None;
        }
        let (parent, name) = (path.parent()?, path.file_name()?);
        if dirs.0.as_ref().is_none_or(|(p, _)| p != parent) {
            let canon = self.auth.canonicalize(parent).ok()?;
            dirs.0 = Some((parent.to_path_buf(), canon));
        }
        let (_, canon) = dirs.0.as_ref()?;
        Some((canon.join(name), meta))
    }

    /// Every file of `files`, in order, read on up to
    /// `available_parallelism` worker threads that take [`SCAN_GRAB`]
    /// neighboring files at a time (so most share a directory).
    fn read_all(&self, files: &[(&Path, String)]) -> Vec<Text> {
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(files.len().div_ceil(16));
        if threads <= 1 || !self.parallel {
            let mut dirs = DirCache::default();
            return files.iter().map(|(p, _)| self.read(p, &mut dirs)).collect();
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut found: Vec<(usize, Text)> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    s.spawn(|| {
                        let (mut mine, mut dirs) = (Vec::new(), DirCache::default());
                        loop {
                            let from =
                                next.fetch_add(SCAN_GRAB, std::sync::atomic::Ordering::Relaxed);
                            let Some(grab) = files.get(from..from.plus(SCAN_GRAB).min(files.len()))
                            else {
                                break;
                            };
                            if grab.is_empty() {
                                break;
                            }
                            for (offset, (path, _)) in grab.iter().enumerate() {
                                mine.push((from.plus(offset), self.read(path, &mut dirs)));
                            }
                        }
                        mine
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| w.join().unwrap_or_default())
                .collect()
        });
        found.sort_unstable_by_key(|(i, _)| *i);
        // A worker that panicked lost its files: those read as skipped.
        let mut out: Vec<Text> = (0..files.len()).map(|_| Text::Skip).collect();
        for (i, text) in found {
            if let Some(slot) = out.get_mut(i) {
                *slot = text;
            }
        }
        out
    }
}

impl App {
    /// Visit every line of every indexed file whose root-relative path
    /// `allow` accepts, in index order, until `visit` breaks. `query`
    /// describes what the caller looks for, so a remote authority can
    /// prefilter with a search tool, and the in-process scan skips files in
    /// which no line matches it (T830); the caller still checks each line
    /// itself, and `query`'s regex must match every line it wants. See the
    /// module docs.
    pub(super) fn scan_index(
        &self,
        query: &ToolQuery,
        source: Source,
        allow: &dyn Fn(&str) -> bool,
        mut visit: impl FnMut(&IndexLine<'_>) -> ControlFlow<()>,
    ) {
        if let Some(found) = self.tool_scan(query, source, allow) {
            for f in &found {
                let line = IndexLine {
                    path: &f.path,
                    rel: &f.rel,
                    number: f.number,
                    text: &f.text,
                };
                if visit(&line).is_break() {
                    return;
                }
            }
            return;
        }
        self.local_scan(query, source, allow, &mut visit);
    }

    /// The in-process scan (T830): the index is read in batches of
    /// [`SCAN_BATCH`] files, in index order. Worker threads read a batch's
    /// files and drop each one in which no line matches `query`'s regex (the
    /// prefilter every caller's own check is a subset of, as for the remote
    /// tool); then this thread visits the kept files' lines in index order, so
    /// `visit` sees what the one-file-at-a-time walk showed it, less lines
    /// that could not match. Files an open buffer shows are read from the
    /// buffer here. A `query` whose regex does not compile keeps every file.
    fn local_scan(
        &self,
        query: &ToolQuery,
        source: Source,
        allow: &dyn Fn(&str) -> bool,
        visit: &mut dyn FnMut(&IndexLine<'_>) -> ControlFlow<()>,
    ) {
        let auth = crate::authority::current();
        let prefilter = regex::Regex::new(&query.regex).ok();
        let tabs: HashSet<PathBuf> = if source == Source::Buffers {
            self.editor
                .tabs
                .iter()
                .filter_map(|t| t.path.clone())
                .collect()
        } else {
            HashSet::new()
        };
        let mut memo = self
            .scan_memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let old = std::mem::take(&mut *memo);
        let reuse = prefilter.is_some() && old.covers(query);
        let reader = Reader {
            auth: &*auth,
            source,
            tabs: &tabs,
            prefilter: prefilter.as_ref(),
            parallel: auth.is_local(),
            memo: reuse.then_some(&old.misses),
            started: SystemTime::now(),
        };
        let mut misses = HashMap::new();
        'scan: for batch in self.file_index.chunks(SCAN_BATCH) {
            let wanted: Vec<(&Path, String)> = batch
                .iter()
                .map(|p| (p.as_path(), self.root_rel(p)))
                .filter(|(_, rel)| allow(rel))
                .collect();
            let texts = reader.read_all(&wanted);
            for ((path, rel), text) in wanted.iter().zip(texts) {
                let content = match text {
                    Text::Skip | Text::Miss(None) => continue,
                    Text::Miss(Some(stamp)) | Text::Unchanged(stamp) => {
                        misses.insert(path.to_path_buf(), stamp);
                        continue;
                    }
                    Text::Read(s) => s,
                    Text::Buffer(canon) => match self
                        .editor
                        .tabs
                        .iter()
                        .find(|t| t.path.as_deref() == Some(canon.as_path()))
                    {
                        Some(tab) => tab.text(),
                        None => continue,
                    },
                };
                for (i, text) in content.lines().enumerate() {
                    let line = IndexLine {
                        path,
                        rel,
                        number: i.plus(1),
                        text,
                    };
                    if visit(&line).is_break() {
                        break 'scan;
                    }
                }
            }
        }
        if prefilter.is_some() {
            *memo = ScanMemo {
                regex: query.regex.clone(),
                literal: ScanMemo::literal_of(query),
                misses,
            };
        }
    }

    /// The scan as it was before T830, one file at a time with no
    /// prefilter: the oracle the batched scan is tested against, and the
    /// complete scan a remote workspace's Replace in Files scope uses (its
    /// search tool stops after [`MAX_TOOL_LINES`], T864).
    pub(super) fn scan_index_sequential(
        &self,
        source: Source,
        allow: &dyn Fn(&str) -> bool,
        mut visit: impl FnMut(&IndexLine<'_>) -> ControlFlow<()>,
    ) {
        let auth = crate::authority::current();
        for path in &self.file_index {
            let rel = self.root_rel(path);
            if !allow(&rel) {
                continue;
            }
            let content = match source {
                Source::Buffers => self.current_text(path),
                Source::Disk => auth.read_to_string_guarded(path).ok(),
            };
            let Some(content) = content else {
                continue;
            };
            for (i, text) in content.lines().enumerate() {
                let line = IndexLine {
                    path,
                    rel: &rel,
                    number: i.plus(1),
                    text,
                };
                if visit(&line).is_break() {
                    return;
                }
            }
        }
    }

    /// `path` relative to the workspace root, lossily, as results show it.
    fn root_rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// The tool-backed scan, or `None` to scan in-process: the authority is
    /// local, no tool can run `query`, or the tool failed.
    fn tool_scan(
        &self,
        query: &ToolQuery,
        source: Source,
        allow: &dyn Fn(&str) -> bool,
    ) -> Option<Vec<Found>> {
        let auth = crate::authority::current();
        if auth.is_local() {
            return None;
        }
        let (tool, program, args) = pick_tool(&*auth, query)?;
        let order: HashMap<&Path, usize> = self
            .file_index
            .iter()
            .enumerate()
            .filter(|(_, p)| allow(&self.root_rel(p)))
            .map(|(i, p)| (p.as_path(), i))
            .collect();
        // Files an open buffer shows are scanned from the buffer instead.
        let buffered: HashSet<&Path> = if source == Source::Buffers {
            self.editor
                .tabs
                .iter()
                .filter_map(|t| t.path.as_deref())
                .filter(|p| order.contains_key(p))
                .collect()
        } else {
            HashSet::new()
        };
        let mut found = Vec::new();
        for folder in &self.workspace_folders {
            let known = |rel: &str| {
                let p = folder.join(rel);
                order.contains_key(p.as_path()) && !buffered.contains(p.as_path())
            };
            for t in run_tool(&*auth, tool, &program, &args, folder, &known)? {
                let path = folder.join(&t.rel);
                let Some(&i) = order.get(path.as_path()) else {
                    continue;
                };
                if buffered.contains(path.as_path()) {
                    continue;
                }
                found.push(Found {
                    order: i,
                    number: t.line,
                    rel: self.root_rel(&path),
                    path,
                    text: t.text,
                });
            }
        }
        for path in buffered {
            let Some(tab) = self
                .editor
                .tabs
                .iter()
                .find(|t| t.path.as_deref() == Some(path))
            else {
                continue;
            };
            let Some(&file_order) = order.get(path) else {
                continue;
            };
            let text = tab.text();
            let rel = self.root_rel(path);
            found.extend(text.lines().enumerate().map(|(n, line)| Found {
                order: file_order,
                number: n.plus(1),
                path: path.to_path_buf(),
                rel: rel.clone(),
                text: line.to_string(),
            }));
        }
        found.sort_by_key(|f| (f.order, f.number));
        // Nested workspace folders can report one line twice.
        found.dedup_by(|a, b| a.order == b.order && a.number == b.number);
        Some(found)
    }
}

/// The tool for `query` on `auth` (`rg` first, else `grep` when the query is
/// fixed strings), resolved on its `PATH`, with its arguments.
fn pick_tool(
    auth: &dyn crate::authority::Authority,
    query: &ToolQuery,
) -> Option<(Tool, PathBuf, Vec<String>)> {
    [Tool::Rg, Tool::Grep].into_iter().find_map(|tool| {
        let args = tool.args(query)?;
        let program = auth.which(tool.program())?;
        Some((tool, program, args))
    })
}

/// The [`Source::Buffers`] size limit, for a query over that source.
pub(super) fn buffers_query(query: ToolQuery) -> ToolQuery {
    query.max_filesize(BUFFERS_MAX_BYTES)
}

/// Run `program` with `args` in `dir` under `auth` and parse its output.
/// `None` when it can't start, or failed (exit status other than 0 or 1)
/// without reporting a line; partial results from a tool that hit an
/// unreadable file are kept.
#[cfg(feature = "tasks")]
fn run_tool(
    auth: &dyn crate::authority::Authority,
    tool: Tool,
    program: &Path,
    args: &[String],
    dir: &Path,
    known: &dyn Fn(&str) -> bool,
) -> Option<Vec<ToolLine>> {
    use std::io::BufRead as _;
    let spec = crate::authority::ProcessSpec::new(program)
        .args(args)
        .current_dir(dir);
    let mut command = auth.command(&spec).ok()?;
    super::detach_from_tty(&mut command);
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut lines = Vec::new();
    let mut capped = false;
    for raw in std::io::BufReader::new(stdout).split(b'\n') {
        let Ok(raw) = raw else { break };
        let Ok(text) = std::str::from_utf8(&raw) else {
            continue;
        };
        if let Some(line) = tool.parse(text, known) {
            lines.push(line);
            if lines.len() >= MAX_TOOL_LINES {
                capped = true;
                let _ = child.kill();
                break;
            }
        }
    }
    let status = child.wait().ok()?;
    let ok = capped || matches!(status.code(), Some(0 | 1)) || !lines.is_empty();
    ok.then_some(lines)
}

/// Without `tasks` no authority runs processes: always scan in-process.
#[cfg(not(feature = "tasks"))]
fn run_tool(
    _auth: &dyn crate::authority::Authority,
    _tool: Tool,
    _program: &Path,
    _args: &[String],
    _dir: &Path,
    _known: &dyn Fn(&str) -> bool,
) -> Option<Vec<ToolLine>> {
    None
}

/// Counts the files with at least one hit the way the in-process scans
/// always did: a file counts once the scan moves past it, and the file a
/// result cap stops in counts only unless [`FileCount::cap`] was called.
#[derive(Default)]
pub(super) struct FileCount {
    current: Option<PathBuf>,
    current_hit: bool,
    done: usize,
    capped: bool,
}

impl FileCount {
    /// The scan is at a line of `path`.
    pub(super) fn enter(&mut self, path: &Path) {
        if self.current.as_deref() != Some(path) {
            if self.current_hit {
                self.done = self.done.plus(1);
            }
            self.current = Some(path.to_path_buf());
            self.current_hit = false;
        }
    }

    /// The current file has a hit.
    pub(super) fn hit(&mut self) {
        self.current_hit = true;
    }

    /// The scan stopped at its result cap; don't count the current file.
    pub(super) fn cap(&mut self) {
        self.capped = true;
    }

    /// The files with hits.
    pub(super) fn total(&self) -> usize {
        self.done
            .plus(usize::from(self.current_hit && !self.capped))
    }
}

#[cfg(test)]
mod tests {
    //! The batched, prefiltered scan (T830) against the one-file-at-a-time
    //! scan it replaced, on generated trees.
    use super::{FileCount, SCAN_BATCH, Source};
    use crate::app::App;
    use crate::settings::Settings;
    use crate::workspace_search::tool::ToolQuery;
    use regex::Regex;
    use std::ops::ControlFlow;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    /// One visited match: rel, line, column, text.
    type Row = (String, usize, usize, String);

    /// A deterministic tree of `n` files over subdirectories, mixing CRLF
    /// and LF files, case variants, comment tags, empty and non-UTF-8 files
    /// and one file over the 2 MB buffer-source limit.
    fn tree(tag: &str, n: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..n {
            let sub = dir.join(format!("d{}", i / 300));
            if i % 300 == 0 {
                std::fs::create_dir_all(&sub).unwrap();
            }
            let body: Vec<u8> = match i {
                5 => Vec::new(),
                6 => vec![0xff, 0xfe, b'n', 0, b'e', 0xc3],
                _ if i % 7 == 0 => {
                    format!("needle at start {i}\r\nmiddle\r\nends with needle\r\n").into_bytes()
                }
                _ if i % 11 == 0 => format!("x Needle {i}\ny needle\n").into_bytes(),
                _ if i % 13 == 0 => {
                    format!("  // TODO: fix {i}\nfn needle_fn() {{}}\n").into_bytes()
                }
                _ if i % 17 == 0 => "caf\u{e9} \u{c9}t\u{e9}\n".as_bytes().to_vec(),
                _ => format!("plain line {i}\nanother\n").into_bytes(),
            };
            std::fs::write(sub.join(format!("f{i}.rs")), body).unwrap();
        }
        let mut big = "filler line\n".repeat(200_000);
        big.push_str("needle in a big file\n");
        std::fs::write(dir.join("big.txt"), big).unwrap();
        age(&dir);
        dir
    }

    /// Set every file under `dir` an hour old, past the racy window, so
    /// scans may remember them.
    fn age(dir: &Path) {
        let then = SystemTime::now() - Duration::from_secs(3_600);
        for entry in walk(dir) {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&entry)
                .unwrap();
            file.set_modified(then).unwrap();
        }
    }

    /// Every regular file under `dir`.
    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                out.extend(walk(&entry.path()));
            } else if kind.is_file() {
                out.push(entry.path());
            }
        }
        out
    }

    /// An App over `dir` with its index built, two open buffers with
    /// unsaved edits (one gains matches, one loses them) and a symlink to
    /// one of them.
    fn app_over(dir: &Path) -> App {
        let mut app = App::new(dir.to_path_buf(), Settings::default());
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("d0/f1.rs"), dir.join("link.rs")).unwrap();
        for (rel, text) in [
            ("d0/f1.rs", "Needle added\nTODO later\nfn needle_fn() {}\n"),
            ("d0/f7.rs", "nothing here\n"),
        ] {
            app.open_path(&dir.join(rel), false);
            let tab = app.editor.active_tab_mut().unwrap();
            tab.editor.set_content(text);
            tab.dirty = true;
        }
        app.build_file_index();
        assert!(!app.file_index_busy(), "the test tree fits the first pass");
        app
    }

    /// What a workspace search collects (lines `re` matches, capped at
    /// `cap` hits), and its file count, over either scan.
    fn search(
        app: &App,
        query: &ToolQuery,
        source: Source,
        re: &Regex,
        cap: usize,
        sequential: bool,
    ) -> (Vec<Row>, usize) {
        let mut rows = Vec::new();
        let mut files = FileCount::default();
        let mut visit = |line: &super::IndexLine<'_>| {
            files.enter(line.path);
            let Some(m) = re.find(line.text) else {
                return ControlFlow::Continue(());
            };
            files.hit();
            rows.push((
                line.rel.to_string(),
                line.number,
                m.start(),
                line.text.to_string(),
            ));
            if rows.len() >= cap {
                files.cap();
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        if sequential {
            app.scan_index_sequential(source, &|_| true, &mut visit);
        } else {
            app.scan_index(query, source, &|_| true, &mut visit);
        }
        (rows, files.total())
    }

    #[test]
    fn the_batched_scan_finds_what_the_sequential_scan_found() {
        let n = SCAN_BATCH + 700; // more than one batch
        let dir = tree("same", n);
        let app = app_over(&dir);
        assert!(app.file_index_len() > SCAN_BATCH);
        let cases: &[(&str, bool, bool)] = &[
            ("needle", false, false),
            ("Needle", false, true),
            ("^needle", true, false),
            ("needle$", true, true),
            (r"\Aneedle", true, true),
            (r"needle\z", true, true),
            (r"\bneedle_\w+", true, true),
            ("caf\u{e9}", false, false),
            ("\u{e9}t\u{c9}", false, false),
            ("not anywhere", false, false),
            (".", true, false),
            ("", true, false),
        ];
        for &(q, regex, case) in cases {
            let tool = super::buffers_query(ToolQuery::search(q, regex, case));
            let re = Regex::new(&tool.regex).unwrap();
            for cap in [5_000, 3] {
                let new = search(&app, &tool, Source::Buffers, &re, cap, false);
                let old = search(&app, &tool, Source::Buffers, &re, cap, true);
                assert_eq!(new, old, "{q:?} regex={regex} case={case} cap={cap}");
            }
        }
        let some = search(
            &app,
            &super::buffers_query(ToolQuery::search("needle", false, false)),
            Source::Buffers,
            &Regex::new("(?i)needle").unwrap(),
            5_000,
            false,
        );
        assert!(
            some.0
                .iter()
                .any(|r| r.0 == "d0/f1.rs" && r.3 == "Needle added")
        );
        assert!(
            !some.0.iter().any(|r| r.0 == "d0/f7.rs"),
            "unsaved edit wins"
        );
        assert!(!some.0.iter().any(|r| r.0 == "big.txt"), "over 2 MB");

        // The TODO scan reads files from disk, ignoring buffers.
        let tags = crate::todo_tree::DEFAULT_TAGS;
        let todo = |sequential: bool| {
            let query = ToolQuery::any_of(tags);
            let mut items = Vec::new();
            let mut visit = |line: &super::IndexLine<'_>| {
                items.extend(
                    crate::todo_tree::scan(line.rel, line.text, tags)
                        .into_iter()
                        .map(|it| (it.file, line.number, it.col, it.text)),
                );
                ControlFlow::Continue(())
            };
            if sequential {
                app.scan_index_sequential(Source::Disk, &|_| true, &mut visit);
            } else {
                app.scan_index(&query, Source::Disk, &|_| true, &mut visit);
            }
            items
        };
        let found = todo(false);
        assert_ne!(found.len(), 0);
        assert_eq!(found, todo(true));

        // A path filter applies before either scan reads a file.
        let tool = super::buffers_query(ToolQuery::search("needle", false, false));
        let re = Regex::new(&tool.regex).unwrap();
        let only = |rel: &str| rel.starts_with("d1/");
        let mut new = Vec::new();
        app.scan_index(&tool, Source::Buffers, &only, |l| {
            if re.is_match(l.text) {
                new.push((l.rel.to_string(), l.number));
            }
            ControlFlow::Continue(())
        });
        let mut old = Vec::new();
        app.scan_index_sequential(Source::Buffers, &only, |l| {
            if re.is_match(l.text) {
                old.push((l.rel.to_string(), l.number));
            }
            ControlFlow::Continue(())
        });
        assert_ne!(new.len(), 0);
        assert_eq!(new, old);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_workspace_search_panel_reports_the_same_hits() {
        let dir = tree("panel", 400);
        let mut app = app_over(&dir);
        app.run_action("search.workspace");
        for c in "needle".chars() {
            app.on_key(vix_input::KeyEvent::new(
                vix_input::KeyCode::Char(c),
                vix_input::KeyModifiers::NONE,
            ));
        }
        let tool = super::buffers_query(ToolQuery::search("needle", false, false));
        let re = Regex::new(&tool.regex).unwrap();
        let (rows, files) = search(&app, &tool, Source::Buffers, &re, 5_000, true);
        let hits = app.workspace_search_hits();
        assert_eq!(hits.len(), rows.len());
        for (hit, row) in hits.iter().zip(&rows) {
            assert_eq!((hit.rel.as_str(), hit.line), (row.0.as_str(), row.1));
        }
        let status = &app.workspace_search.as_ref().unwrap().status;
        assert!(status.contains(&files.to_string()), "{status}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A workspace search for literal `q` (case-insensitive) over the
    /// batched scan, and the sequential oracle's answer.
    fn both(app: &App, q: &str) -> ((Vec<Row>, usize), (Vec<Row>, usize)) {
        let tool = super::buffers_query(ToolQuery::search(q, false, false));
        let re = Regex::new(&tool.regex).unwrap();
        (
            search(app, &tool, Source::Buffers, &re, 5_000, false),
            search(app, &tool, Source::Buffers, &re, 5_000, true),
        )
    }

    fn misses(app: &App) -> usize {
        app.scan_memo.lock().unwrap().misses.len()
    }

    #[test]
    fn a_narrower_query_skips_remembered_misses_until_they_change() {
        let dir = tree("memo", 600);
        let app = app_over(&dir);
        let (new, old) = both(&app, "need");
        assert_eq!(new, old);
        let remembered = misses(&app);
        assert!(remembered > 300, "{remembered}");
        assert!(
            app.scan_memo
                .lock()
                .unwrap()
                .covers(&ToolQuery::search("needle", false, false))
        );
        assert!(
            !app.scan_memo
                .lock()
                .unwrap()
                .covers(&ToolQuery::search("needle", false, true))
        );
        assert!(
            !app.scan_memo
                .lock()
                .unwrap()
                .covers(&ToolQuery::search("ne", false, false))
        );
        assert!(
            !app.scan_memo
                .lock()
                .unwrap()
                .covers(&ToolQuery::search("needle", true, false))
        );

        // A missed file that grows a match, and one rewritten with a new
        // time but the same size, are read again.
        let grown = dir.join("d0/f2.rs");
        std::fs::write(&grown, "plain line 2\nanother needle\n").unwrap();
        let same_size = dir.join("d0/f3.rs");
        std::fs::write(&same_size, "plain needl 3\nanother\n").unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&same_size)
            .unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(60))
            .unwrap();
        let (new, old) = both(&app, "needl");
        assert_eq!(new, old);
        for rel in ["d0/f2.rs", "d0/f3.rs"] {
            assert!(new.0.iter().any(|r| r.0 == rel), "{rel} found again");
        }
        // The just-written file is inside the racy window: not remembered.
        // A stamp ahead of the scan keeps it there however long a loaded
        // machine takes to reach the scan (a 2 s window raced under load).
        std::fs::OpenOptions::new()
            .write(true)
            .open(&grown)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(3_600))
            .unwrap();
        let (new, _) = both(&app, "needlx");
        assert_eq!(new.0.len(), 0);
        assert!(!app.scan_memo.lock().unwrap().misses.contains_key(&grown));

        // A rewrite that keeps both size and time is not read again: the
        // memo trusts the stamp, as `make` and git do.
        let quiet = dir.join("d0/f4.rs");
        let stamp = std::fs::metadata(&quiet).unwrap().modified().unwrap();
        let (new, _) = both(&app, "needle");
        assert!(!new.0.iter().any(|r| r.0 == "d0/f4.rs"));
        std::fs::write(&quiet, "plain needle\nanother\n").unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&quiet)
            .unwrap();
        file.set_modified(stamp).unwrap();
        let (new, old) = both(&app, "needle");
        assert!(!new.0.iter().any(|r| r.0 == "d0/f4.rs"), "skipped");
        assert!(old.0.iter().any(|r| r.0 == "d0/f4.rs"));
        // Another query that the memo does not cover reads it.
        let (new, old) = both(&app, "plain");
        assert_eq!(new, old);
        assert!(new.0.iter().any(|r| r.0 == "d0/f4.rs"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
