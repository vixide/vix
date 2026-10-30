//! Code lenses (T726, `spec/ide-parity`): a language server's lenses
//! ("Run | Debug", "3 references") and local "Run test" lenses drawn as
//! dimmed virtual rows above their lines (`vix-editor-core`'s `lens.rs`
//! does the drawing), kept fresh, and run from the cursor's line.
//!
//! Refresh: a buffer's lenses are recomputed when it is first seen, after
//! edits (debounced by [`LENS_DEBOUNCE`] from the last change), on save and
//! when the server sends `workspace/codeLens/refresh`. Lenses the server sent
//! without a command are resolved (`codeLens/resolve`) only once their line
//! scrolls into view. Local "Run test" lenses come from `vix-tasks`'
//! test-at-point heuristics and are shown only where the server put no lens
//! on the same line, so they also work with no language server at all.
//!
//! Running: a lens command a language server advertised goes back to it as
//! `workspace/executeCommand`; well-known client-side commands map to Vix's
//! own actions (rust-analyzer's `runSingle` runs its cargo command through
//! the command pipeline, `showReferences` opens the references list); any
//! other command is refused with a translated message.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::App;
use super::ide_parity::{ChoiceList, ChoicePurpose};

/// How long after the last edit a buffer's lenses are re-requested.
pub(crate) const LENS_DEBOUNCE: Duration = Duration::from_millis(500);

/// How many times an unsendable lens request (server still starting) is
/// retried, [`LENS_DEBOUNCE`] apart.
const LENS_RETRIES: u8 = 20;

/// A debounced "refresh is due" deadline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Debounce {
    /// When the refresh becomes due; `None` when none is pending.
    due: Option<Instant>,
}

impl Debounce {
    /// Make a refresh due at `now` (keeping an earlier deadline).
    pub(crate) fn now(&mut self, now: Instant) {
        self.due = Some(self.due.map_or(now, |d| d.min(now)));
    }

    /// Push the deadline to `delay` after `now`: each call (each edit)
    /// restarts the wait, so a burst of typing makes one request.
    pub(crate) fn after(&mut self, now: Instant, delay: Duration) {
        self.due = Some(now + delay);
    }

    /// Whether a refresh is due at `now`; clears it if so.
    pub(crate) fn take(&mut self, now: Instant) -> bool {
        match self.due {
            Some(d) if d <= now => {
                self.due = None;
                true
            }
            _ => false,
        }
    }
}

/// What running a lens does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LensAction {
    /// A language server's lens command and its arguments.
    Lsp {
        /// The command id, e.g. `rust-analyzer.runSingle`.
        command: String,
        /// The command's arguments, verbatim.
        arguments: Value,
    },
    /// A local test-at-point lens: the shell command that runs the test.
    Test {
        /// The command line.
        command: String,
    },
}

/// One lens as shown: its line, title and what it does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lens {
    /// 0-based line it annotates.
    pub line: usize,
    /// Display title (server text with `$(icon)` markup removed).
    pub title: String,
    /// What running it does.
    pub action: LensAction,
}

/// One buffer's lens bookkeeping.
#[derive(Debug, Default)]
pub(crate) struct LensState {
    /// The language server's lenses that carry a command.
    lsp: Vec<Lens>,
    /// Lenses still to resolve: `(line, raw lens, resolve already asked)`.
    unresolved: Vec<(usize, Value, bool)>,
    /// Local "Run test" lenses.
    local: Vec<Lens>,
    /// When to recompute.
    refresh: Debounce,
    /// The buffer revision last seen, to notice edits.
    seen_rev: Option<u64>,
    /// Refreshes retried because no request could be sent yet.
    retries: u8,
}

impl LensState {
    /// The lenses to show, by line: the server's, plus a local one only on a
    /// line where the server put none.
    pub(crate) fn shown(&self) -> Vec<Lens> {
        let mut out = self.lsp.clone();
        out.extend(
            self.local
                .iter()
                .filter(|l| !self.lsp.iter().any(|s| s.line == l.line))
                .cloned(),
        );
        out.sort_by_key(|l| l.line);
        out
    }

    /// Note the buffer is at `rev`: the first sighting makes a refresh due
    /// now, a change restarts the debounce.
    pub(crate) fn observe(&mut self, rev: u64, now: Instant) {
        match self.seen_rev {
            None => self.refresh.now(now),
            Some(r) if r != rev => self.refresh.after(now, LENS_DEBOUNCE),
            Some(_) => {}
        }
        self.seen_rev = Some(rev);
    }
}

/// A server lens title for display: VS Code `$(icon)` markup removed and
/// whitespace tidied.
pub(crate) fn clean_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(i) = rest.find("$(") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find(')') else {
            rest = &rest[i..];
            break;
        };
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Quote `arg` for a POSIX shell when it needs it.
fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@+,%".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// The shell command (and its directory) for a rust-analyzer `Runnable`
/// (the argument of `rust-analyzer.runSingle`): `cargo <cargoArgs>
/// <cargoExtraArgs> -- <executableArgs>` for a `cargo` runnable,
/// `<program> <args>` for a `shell` one.
pub(crate) fn runnable_command(runnable: &Value) -> Option<(String, Option<PathBuf>)> {
    let args = runnable.get("args")?;
    let strings = |key: &str| -> Vec<String> {
        args.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(shell_quote)
                    .collect()
            })
            .unwrap_or_default()
    };
    let dir = args
        .get("cwd")
        .or_else(|| args.get("workspaceRoot"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let kind = runnable
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("cargo");
    let cmd = if kind == "shell" {
        let program = args.get("program").and_then(Value::as_str)?;
        let mut parts = vec![shell_quote(program)];
        parts.extend(strings("args"));
        parts.join(" ")
    } else {
        let program = args
            .get("overrideCargo")
            .and_then(Value::as_str)
            .unwrap_or("cargo");
        let mut parts = vec![shell_quote(program)];
        parts.extend(strings("cargoArgs"));
        parts.extend(strings("cargoExtraArgs"));
        let exe = strings("executableArgs");
        if !exe.is_empty() {
            parts.push("--".to_string());
            parts.extend(exe);
        }
        parts.join(" ")
    };
    Some((cmd, dir))
}

/// The locations of a `showReferences`-style lens command
/// (`[uri, position, locations]`), as `(path, line, character)`.
pub(crate) fn reference_locations(arguments: &Value) -> Vec<(PathBuf, u32, u32)> {
    arguments
        .get(2)
        .and_then(Value::as_array)
        .map(|locs| {
            locs.iter()
                .filter_map(|l| {
                    let uri = l.get("uri")?.as_str()?;
                    let start = l.get("range")?.get("start")?;
                    let line = u32::try_from(start.get("line")?.as_u64()?).ok()?;
                    let ch = u32::try_from(start.get("character")?.as_u64()?).ok()?;
                    Some((crate::lsp::uri_to_path(uri), line, ch))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl App {
    /// Dispatch the code-lens actions. Returns `true` if handled.
    pub(super) fn run_code_lens_action(&mut self, action: &str) -> bool {
        match action {
            "lsp.code_lens" => self.choose_code_lens(false),
            "lsp.code_lens_run" => self.choose_code_lens(true),
            "view.code_lens" => self.toggle_code_lens(),
            _ => return false,
        }
        true
    }

    /// Turn lens rows on or off (the `code_lens` setting).
    fn toggle_code_lens(&mut self) {
        let on = !self.settings.language_features.code_lens;
        self.settings.language_features.code_lens = on;
        if on {
            let now = Instant::now();
            for st in self.code_lenses.values_mut() {
                st.refresh.now(now);
            }
        } else {
            for tab in &mut self.editor.tabs {
                tab.editor.clear_code_lenses();
            }
        }
        self.status = t!("status.code_lens", on = on).to_string();
    }

    /// Per-tick lens upkeep for the active buffer: notice edits, refresh
    /// when due, and resolve visible unresolved lenses. Called from
    /// [`App::tick`] after the language server is synced.
    pub(crate) fn poll_code_lens(&mut self) {
        if !self.settings.language_features.code_lens {
            return;
        }
        let Some(path) = self.active_path() else {
            return;
        };
        let Some(rev) = self.editor.active_tab().map(|t| t.editor.revision()) else {
            return;
        };
        let now = Instant::now();
        let due = {
            let st = self.code_lenses.entry(path.clone()).or_default();
            st.observe(rev, now);
            st.refresh.take(now)
        };
        if due {
            let local = self.local_test_lenses();
            if let Some(st) = self.code_lenses.get_mut(&path) {
                st.local = local;
            }
            // Not sent (the server has not opened the document yet, or offers
            // no lenses): try again after the debounce, a bounded number of
            // times so a server without lenses is not asked forever.
            if self.lsp.handles(&path)
                && !self.lsp.refresh_code_lens(&path)
                && let Some(st) = self.code_lenses.get_mut(&path)
                && st.retries < LENS_RETRIES
            {
                st.retries += 1;
                st.refresh.after(now, LENS_DEBOUNCE);
            }
            self.apply_code_lenses(&path);
        }
        self.resolve_visible_lenses(&path);
    }

    /// Make the lenses of `path` due for a refresh now (after a save).
    pub(crate) fn code_lens_saved(&mut self, path: &Path) {
        if let Some(st) = self.code_lenses.get_mut(path) {
            st.refresh.now(Instant::now());
        }
    }

    /// Handle the lens events of the language server.
    pub(crate) fn on_code_lens_event(&mut self, event: crate::lsp::LspEvent) {
        match event {
            crate::lsp::LspEvent::CodeLensUpdate {
                path,
                lenses,
                unresolved,
            } => {
                let st = self.code_lenses.entry(path.clone()).or_default();
                st.lsp = lenses.into_iter().map(lsp_lens).collect();
                st.unresolved = unresolved
                    .into_iter()
                    .map(|(line, raw)| (line as usize, raw, false))
                    .collect();
                self.apply_code_lenses(&path);
            }
            crate::lsp::LspEvent::CodeLensResolved { path, lens, range } => {
                let Some(st) = self.code_lenses.get_mut(&path) else {
                    return;
                };
                st.unresolved
                    .retain(|(_, raw, _)| raw.get("range") != Some(&range));
                st.lsp.push(lsp_lens(lens));
                self.apply_code_lenses(&path);
            }
            crate::lsp::LspEvent::CodeLensRefresh => {
                let now = Instant::now();
                for st in self.code_lenses.values_mut() {
                    st.refresh.now(now);
                }
            }
            _ => {}
        }
    }

    /// Ask the server to resolve the unresolved lenses of `path` whose lines
    /// are on screen, once each.
    fn resolve_visible_lenses(&mut self, path: &Path) {
        let Some(st) = self.code_lenses.get(path) else {
            return;
        };
        if st.unresolved.iter().all(|u| u.2) || !self.lsp.resolves_code_lens(path) {
            return;
        }
        let top = self.editor.top_visible_line();
        let bottom = top + self.layout.editor.height as usize;
        let wanted: Vec<Value> = st
            .unresolved
            .iter()
            .filter(|(line, _, asked)| !asked && (top..bottom).contains(line))
            .map(|(_, raw, _)| raw.clone())
            .collect();
        for raw in &wanted {
            self.lsp.resolve_code_lens(path, raw);
        }
        if let Some(st) = self.code_lenses.get_mut(path) {
            for u in &mut st.unresolved {
                if wanted.contains(&u.1) {
                    u.2 = true;
                }
            }
        }
    }

    /// Push the shown lenses of `path` to its tab's editor.
    fn apply_code_lenses(&mut self, path: &Path) {
        let rows: Vec<(usize, String)> = self
            .code_lenses
            .get(path)
            .map(|st| st.shown().into_iter().map(|l| (l.line, l.title)).collect())
            .unwrap_or_default();
        let on = self.settings.language_features.code_lens;
        if let Some(tab) = self
            .editor
            .tabs
            .iter_mut()
            .find(|t| t.path.as_deref() == Some(path))
        {
            if on {
                tab.editor.set_code_lenses(rows);
            } else {
                tab.editor.clear_code_lenses();
            }
        }
    }

    /// Local "Run test" lenses for the active buffer, from test-at-point.
    fn local_test_lenses(&self) -> Vec<Lens> {
        let Some(tab) = self.editor.active_tab() else {
            return Vec::new();
        };
        let Some(path) = tab.path.as_ref() else {
            return Vec::new();
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = tab.text();
        let lines: Vec<&str> = text.lines().collect();
        let title = t!("ui.code_lens_run_test").to_string();
        crate::tasks::test_at_point::tests_in_file(
            ext,
            &rel,
            &lines,
            crate::tasks::test_at_point::TestFrameworkHint::Unknown,
        )
        .into_iter()
        .map(|(line, m)| Lens {
            line,
            title: title.clone(),
            action: LensAction::Test { command: m.command },
        })
        .collect()
    }

    /// The active buffer's shown lenses.
    fn active_lenses(&self) -> Vec<Lens> {
        self.active_path()
            .and_then(|p| self.code_lenses.get(&p))
            .map(LensState::shown)
            .unwrap_or_default()
    }

    /// `lsp.code_lens` (`on_line` false): pick from every lens in the file,
    /// starting at the cursor's. `lsp.code_lens_run` (`on_line` true): run the
    /// cursor line's lens, or pick when it has several. With nothing shown
    /// yet and a language server, the file's lenses are requested and the
    /// reply opens the list.
    fn choose_code_lens(&mut self, on_line: bool) {
        let cursor = self.editor.cursor_line();
        let mut lenses = self.active_lenses();
        if on_line {
            lenses.retain(|l| l.line == cursor);
            match lenses.len() {
                0 => self.status = t!("status.code_lens_none_here").to_string(),
                1 => self.run_lens(&lenses[0]),
                _ => self.open_lens_choice(lenses, cursor),
            }
            return;
        }
        if !lenses.is_empty() {
            self.open_lens_choice(lenses, cursor);
        } else if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            self.lsp.request_code_lens(&path);
        } else {
            self.status = t!("status.code_lens_none").to_string();
        }
    }

    /// The reply to an explicit `lsp.code_lens` request: list the lenses.
    pub(super) fn on_code_lenses(&mut self, lenses: Vec<crate::lsp_core::message::CodeLens>) {
        if lenses.is_empty() {
            self.status = t!("status.code_lens_none").to_string();
            return;
        }
        let mut lenses: Vec<Lens> = lenses.into_iter().map(lsp_lens).collect();
        lenses.sort_by_key(|l| l.line);
        let cursor = self.editor.cursor_line();
        self.open_lens_choice(lenses, cursor);
    }

    /// Open the pick-one list over `lenses` (sorted by line), each row led by
    /// its 1-based line, starting on the first lens at or after `cursor`.
    fn open_lens_choice(&mut self, lenses: Vec<Lens>, cursor: usize) {
        let rows = lenses
            .iter()
            .map(|l| format!("{:>5}  {}", l.line + 1, l.title))
            .collect();
        let selected = lenses
            .iter()
            .position(|l| l.line >= cursor)
            .unwrap_or(lenses.len().saturating_sub(1));
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("menu.item.lsp.code_lens").to_string(),
                t!("ui.run_config_hint").to_string(),
                rows,
            )
            .with_selected(selected),
            purpose: ChoicePurpose::CodeLens { lenses },
        });
    }

    /// Run one lens.
    pub(super) fn run_lens(&mut self, lens: &Lens) {
        match &lens.action {
            LensAction::Test { command } => {
                self.run_command(command);
                self.status = t!("status.lens_run", title = lens.title).to_string();
            }
            LensAction::Lsp { command, arguments } => {
                self.run_lsp_lens_command(&lens.title, command, arguments);
            }
        }
    }

    /// Run a language server's lens command: Vix's own action for a
    /// well-known client-side command, `workspace/executeCommand` for one the
    /// server advertised, otherwise a refusal.
    fn run_lsp_lens_command(&mut self, title: &str, command: &str, arguments: &Value) {
        match command {
            "rust-analyzer.runSingle" | "rust-analyzer.run" => {
                if let Some((cmd, dir)) = arguments.get(0).and_then(runnable_command) {
                    let dir = dir
                        .filter(|d| crate::authority::current().is_dir(d))
                        .unwrap_or_else(|| self.root.clone());
                    self.run_command_in(&dir, &cmd);
                    self.status = t!("status.lens_run", title = title).to_string();
                    return;
                }
            }
            "rust-analyzer.showReferences" | "editor.action.showReferences" => {
                let locs = reference_locations(arguments);
                if !locs.is_empty() {
                    self.show_references(&locs);
                    return;
                }
            }
            _ => {}
        }
        if let Some(path) = self.active_path()
            && self.lsp.executes_command(&path, command)
        {
            self.lsp.execute_command(&path, command, arguments);
            self.status = t!("status.lens_run", title = title).to_string();
            return;
        }
        self.status = t!("status.code_lens_unsupported", command = command).to_string();
    }
}

/// A language server lens as shown.
fn lsp_lens((line, title, command, arguments): crate::lsp_core::message::CodeLens) -> Lens {
    Lens {
        line: line as usize,
        title: clean_title(&title),
        action: LensAction::Lsp { command, arguments },
    }
}

/// The per-buffer lens bookkeeping, keyed by file path.
pub(crate) type LensMap = HashMap<PathBuf, LensState>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn debounce_waits_for_quiet_and_restarts_on_each_edit() {
        let t0 = Instant::now();
        let mut st = LensState::default();
        st.observe(1, t0);
        assert!(st.refresh.take(t0), "first sighting refreshes at once");
        assert!(!st.refresh.take(t0), "only once");
        st.observe(1, t0);
        assert!(!st.refresh.take(t0 + LENS_DEBOUNCE), "no edit, no refresh");
        // An edit, then another 300ms later: due 500ms after the last one.
        st.observe(2, t0);
        st.observe(3, t0 + Duration::from_millis(300));
        assert!(!st.refresh.take(t0 + LENS_DEBOUNCE));
        assert!(st.refresh.take(t0 + Duration::from_millis(800)));
    }

    #[test]
    fn a_save_or_server_refresh_is_due_immediately() {
        let t0 = Instant::now();
        let mut d = Debounce::default();
        d.after(t0, LENS_DEBOUNCE);
        d.now(t0);
        assert!(d.take(t0), "now() overrides a pending debounce");
    }

    #[test]
    fn local_lenses_yield_to_server_lenses_on_the_same_line() {
        let lsp = |line, title: &str| Lens {
            line,
            title: title.into(),
            action: LensAction::Lsp {
                command: "c".into(),
                arguments: json!([]),
            },
        };
        let local = |line| Lens {
            line,
            title: "Run test".into(),
            action: LensAction::Test {
                command: "cargo test".into(),
            },
        };
        let st = LensState {
            lsp: vec![lsp(4, "Run"), lsp(1, "2 references")],
            local: vec![local(4), local(9)],
            ..LensState::default()
        };
        let shown: Vec<(usize, String)> =
            st.shown().into_iter().map(|l| (l.line, l.title)).collect();
        assert_eq!(
            shown,
            vec![
                (1, "2 references".to_string()),
                (4, "Run".to_string()),
                (9, "Run test".to_string())
            ]
        );
    }

    #[test]
    fn titles_lose_icon_markup() {
        assert_eq!(clean_title("$(play) Run Test"), "Run Test");
        assert_eq!(clean_title("▶\u{fe0e} Run "), "▶\u{fe0e} Run");
        assert_eq!(clean_title("odd $(unclosed"), "odd $(unclosed");
    }

    #[test]
    fn rust_analyzer_runnables_become_cargo_commands() {
        let r = json!({
            "label": "test tests::it_works",
            "kind": "cargo",
            "args": {
                "workspaceRoot": "/w",
                "cargoArgs": ["test", "--package", "demo", "--lib"],
                "executableArgs": ["tests::it_works", "--exact", "--nocapture"]
            }
        });
        let (cmd, dir) = runnable_command(&r).unwrap();
        assert_eq!(
            cmd,
            "cargo test --package demo --lib -- tests::it_works --exact --nocapture"
        );
        assert_eq!(dir, Some(PathBuf::from("/w")));
        let shell = json!({"kind": "shell", "args": {"program": "npm", "args": ["run", "a b"]}});
        assert_eq!(runnable_command(&shell).unwrap().0, "npm run 'a b'");
        assert_eq!(runnable_command(&json!({})), None);
    }

    #[test]
    fn show_references_arguments_parse() {
        let args = json!(["file:///a.rs", {"line": 0, "character": 0}, [
            {"uri": "file:///b.rs", "range": {"start": {"line": 4, "character": 2},
                                              "end": {"line": 4, "character": 5}}}
        ]]);
        assert_eq!(
            reference_locations(&args),
            vec![(PathBuf::from("/b.rs"), 4, 2)]
        );
    }
}
