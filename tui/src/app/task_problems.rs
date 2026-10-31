//! Problem matchers over command output, and Go → Next/Previous Error (T632).
//!
//! Every command run through [`App::run_command_in`] streams its lines through
//! a [`Scanner`] as they reach the bottom dock; when it ends, the problems
//! become the [`ProblemList`] that `go.next_error`/`go.prev_error` walk, the
//! Problems panel lists, and the active buffer shows inline. The matching is
//! pure (`vix_tasks::matcher`); a matched file is opened only when the user
//! picks it, and the inline text only touches buffers already open.

use std::path::{Path, PathBuf};

use crate::editor_core::virtual_text::{EolText, VirtualTextLayer};
use crate::lsp::inline::InlineMode;
use crate::tasks::matcher::{self, MatcherSpec, ProblemList, Scanner, Severity};
use crate::workspace_search::Hit;

use super::{App, Focus};

/// The scan of the command now running: its working directory and scanner.
pub(super) struct TaskScan {
    /// Where the command runs; its problems' paths resolve against it.
    cwd: PathBuf,
    /// The matcher state.
    scanner: Scanner,
}

/// The inline color for a problem's severity, matching the LSP colors.
fn severity_color(severity: Severity) -> ratatui::style::Color {
    use ratatui::style::Color;
    match severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Cyan,
    }
}

/// The translated status-line text for a problem's severity and message.
fn severity_text(severity: Severity, message: &str) -> String {
    match severity {
        Severity::Error => t!("status.diagnostic_error", message = message),
        Severity::Warning => t!("status.diagnostic_warning", message = message),
        Severity::Info => t!("status.diagnostic_info", message = message),
    }
    .to_string()
}

impl App {
    /// Use a `tasks.toml` task's `matcher` value for the next command run. An
    /// invalid value is reported, and the run uses `auto`.
    pub(super) fn set_next_matcher(&mut self, task: &str, value: Option<&str>) {
        let spec = match value.map(MatcherSpec::parse) {
            None => MatcherSpec::Auto,
            Some(Ok(spec)) => spec,
            Some(Err(e)) => {
                self.messages.warn(
                    t!(
                        "msg.task_matcher_invalid",
                        task = task,
                        error = e.to_string()
                    )
                    .to_string(),
                );
                MatcherSpec::Auto
            }
        };
        self.pending_matcher = Some(spec);
    }

    /// Start scanning a command that runs in `cwd`: clear the last run's
    /// problems and use `spec` (`None` means `auto`).
    pub(super) fn begin_task_scan(&mut self, cwd: &Path, spec: Option<MatcherSpec>) {
        let spec = spec.unwrap_or_default();
        // Tabs hold canonical paths; canonicalizing the cwd once makes the
        // problems' joined paths compare equal to an open tab's.
        let cwd = crate::authority::current()
            .canonicalize(cwd)
            .unwrap_or_else(|_| cwd.to_path_buf());
        self.task_problems = ProblemList::default();
        self.task_scan = Some(TaskScan {
            cwd,
            scanner: Scanner::new(spec),
        });
    }

    /// Match one line of the running command's output.
    pub(super) fn scan_task_line(&mut self, line: &str) {
        if let Some(scan) = self.task_scan.as_mut() {
            scan.scanner.push_line(line);
        }
    }

    /// The running command ended: keep its problems and return how many there
    /// are (0 when the command was not scanned).
    pub(super) fn finish_task_scan(&mut self) -> usize {
        let Some(scan) = self.task_scan.take() else {
            return 0;
        };
        self.task_problems = ProblemList::new(scan.cwd, scan.scanner.into_problems());
        self.task_problems.len()
    }

    /// Problem `i`'s file: resolved against its run's cwd, or against the
    /// workspace root when that is not a file but the root-relative one is
    /// (cargo prints workspace-relative paths from a member's directory).
    fn task_problem_path(&self, i: usize) -> Option<PathBuf> {
        let path = self.task_problems.path(i)?.to_path_buf();
        let auth = crate::authority::current();
        if auth.is_file(&path) || self.task_problems.cwd() == self.root {
            return Some(path);
        }
        let file = &self.task_problems.problems().get(i)?.file;
        let from_root = matcher::resolve_path(&self.root, file);
        Some(if auth.is_file(&from_root) {
            from_root
        } else {
            path
        })
    }

    /// Go → Next Error (`forward`) / Previous Error: visit the next or previous
    /// problem from the last scanned run, wrapping around, opening its file at
    /// the problem's position.
    pub(super) fn goto_task_problem(&mut self, forward: bool) {
        let Some(step) = self.task_problems.step(forward) else {
            self.status = t!("status.no_task_problems").to_string();
            return;
        };
        let Some(problem) = self.task_problems.problems().get(step.index).cloned() else {
            return;
        };
        let Some(path) = self.task_problem_path(step.index) else {
            return;
        };
        if !crate::authority::current().is_file(&path) {
            self.status = t!("status.task_problem_missing", path = path.display()).to_string();
            return;
        }
        self.with_jump(|s| {
            s.open_path(&path, false);
            let area = s.editor_view();
            s.editor.goto(problem.line, problem.column, area);
            s.focus = Focus::Editor;
        });
        let text = severity_text(problem.severity, &problem.message);
        self.status = t!(
            "status.task_problem_nav",
            index = step.index + 1,
            total = self.task_problems.len(),
            text = text
        )
        .to_string();
    }

    /// The last run's problems as Problems-panel rows (after the language
    /// servers' diagnostics): `rel:line: [severity] message (matcher)`.
    pub(super) fn task_problem_hits(&self) -> Vec<Hit> {
        if self.task_problems.is_empty() {
            return Vec::new();
        }
        // The problems' paths are under the canonical cwd (see
        // `begin_task_scan`); strip whichever spelling of the root matches.
        let canonical_root = crate::authority::current()
            .canonicalize(&self.root)
            .unwrap_or_else(|_| self.root.clone());
        (0..self.task_problems.len())
            .filter_map(|i| {
                let p = self.task_problems.problems().get(i)?;
                let path = self.task_problem_path(i)?;
                let rel = path
                    .strip_prefix(&canonical_root)
                    .or_else(|_| path.strip_prefix(&self.root))
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                let msg: String = matcher_first_line(&p.message).chars().take(100).collect();
                Some(Hit {
                    display: format!(
                        "{rel}:{}: [{}] {msg} ({})",
                        p.line,
                        p.severity.tag(),
                        p.source.name()
                    ),
                    path,
                    rel,
                    line: p.line,
                    col: p.column.unwrap_or(1),
                    text: msg,
                })
            })
            .collect()
    }

    /// Draw the active buffer's task problems as end-of-line virtual text on
    /// their own layer, per the `inline_diagnostics` setting. Only touches the
    /// already-open active buffer; called once per tick.
    pub(super) fn refresh_task_problem_texts(&mut self) {
        let mode = InlineMode::parse(&self.settings.inline_diagnostics);
        let path = self.active_path();
        let cursor = self.editor.cursor_line();
        let texts: Vec<EolText> = match (mode, path) {
            (InlineMode::Off, _) | (_, None) => Vec::new(),
            (mode, Some(path)) if !self.task_problems.is_empty() => {
                let mut texts: Vec<EolText> = Vec::new();
                for (i, p) in self.task_problems.problems().iter().enumerate() {
                    let line = p.line.saturating_sub(1);
                    if (mode == InlineMode::CursorLine && line != cursor)
                        || texts.iter().any(|t| t.line == line)
                        || self.task_problems.path(i) != Some(path.as_path())
                    {
                        continue;
                    }
                    texts.push(
                        EolText::new(line, matcher_first_line(&p.message))
                            .with_color(severity_color(p.severity)),
                    );
                }
                texts
            }
            _ => Vec::new(),
        };
        if let Some(t) = self.editor.active_tab_mut()
            && (!texts.is_empty()
                || !t
                    .editor
                    .eol_texts(VirtualTextLayer::TaskProblems)
                    .is_empty())
        {
            t.editor
                .set_eol_texts(VirtualTextLayer::TaskProblems, texts);
        }
    }

    /// The file location a clicked bottom-dock line names, through the problem
    /// matchers' single-line patterns, resolved against the last run's cwd
    /// (else the workspace root).
    pub(super) fn dock_line_location(&self, line: &str) -> Option<(PathBuf, usize, Option<usize>)> {
        let (file, line_no, col) = matcher::locate(line)?;
        let cwd = if self.task_problems.cwd().as_os_str().is_empty() {
            self.root.as_path()
        } else {
            self.task_problems.cwd()
        };
        let path = matcher::resolve_path(cwd, &file);
        let auth = crate::authority::current();
        if auth.is_file(&path) {
            return Some((path, line_no, col));
        }
        let from_root = matcher::resolve_path(&self.root, &file);
        auth.is_file(&from_root)
            .then_some((from_root, line_no, col))
    }
}

/// The first line of a message.
fn matcher_first_line(message: &str) -> &str {
    message.lines().next().unwrap_or("")
}
