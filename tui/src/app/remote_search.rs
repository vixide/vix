//! Project-wide line scans over the file index, local or remote (T715).
//!
//! Workspace search, search-to-dock, the TODO finder and Todo Tree, and
//! go-to-definition all walk the same thing: every line of every indexed
//! file. [`App::scan_index`] is that walk. Under the local authority it reads
//! each file in-process, exactly as before. Under any other authority
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

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

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

impl App {
    /// Visit every line of every indexed file whose root-relative path
    /// `allow` accepts, in index order, until `visit` breaks. `query`
    /// describes what the caller looks for, so a remote authority can
    /// prefilter with a search tool; the caller still checks each line
    /// itself. See the module docs.
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
                    number: i + 1,
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
            let text = tab.text();
            let rel = self.root_rel(path);
            found.extend(text.lines().enumerate().map(|(n, line)| Found {
                order: order[path],
                number: n + 1,
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
                self.done += 1;
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
        self.done + usize::from(self.current_hit && !self.capped)
    }
}
