//! The command palette (`Ctrl+P`): opening it (plain or seeded), the
//! ranked file index and its five modes (files/commands/buffers/goto-line/
//! symbols/workspace-symbols), live recompute as the query changes, key
//! handling, and accepting the highlighted entry.
//!
//! Moved out of `app.rs` verbatim (T141, slice 4); named `command_palette`
//! rather than `palette` because `app.rs` already imports the top-level
//! `vix-palette` crate under the local name `palette` -- a `mod palette;`
//! here would collide with that `use`. Mostly contiguous, unlike the
//! git/scripts slices -- closer in shape to keymap dispatch.

use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent};

use super::{App, Focus};
use crate::editor::is_image_path;
use crate::palette::{
    self, Action as PAction, Entry, Mode as PMode, Palette, PaletteMemory, Preview,
};
use crate::workspace_search::{Flags as WorkspaceFlags, WorkspaceSearch};

/// Which picker closed last, so `palette.resume` reopens that one (T630).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerKind {
    /// The command palette (any mode, or Search Everywhere).
    Palette,
    /// The workspace-search panel.
    WorkspaceSearch,
}

/// How the workspace-search panel was left (T630).
#[derive(Clone, Debug)]
pub(crate) struct WorkspaceMemory {
    /// The search text.
    pub(crate) query: String,
    /// The replacement text.
    pub(crate) replace: String,
    /// The include-path filter.
    pub(crate) include_path: String,
    /// The exclude-path filter.
    pub(crate) exclude_path: String,
    /// Replace mode, case and regex toggles.
    pub(crate) flags: WorkspaceFlags,
    /// The highlighted hit.
    pub(crate) selected: usize,
}

/// The last state of each picker plus which one closed last (T630).
#[derive(Clone, Debug, Default)]
pub(crate) struct PickerMemory {
    /// The palette's last `(mode, query, selection)`.
    pub(crate) palette: Option<PaletteMemory>,
    /// The workspace-search panel's last state.
    pub(crate) workspace: Option<WorkspaceMemory>,
    /// Which picker closed most recently.
    pub(crate) last: Option<PickerKind>,
}

/// Where a preview's text comes from: a file path, or the active buffer
/// (the `@` symbol mode, whose buffer may be untitled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewSource {
    /// A file, previewed from its open buffer if there is one, else disk.
    File(PathBuf),
    /// The active buffer.
    ActiveBuffer,
}

/// The cached last preview: its source, line, and result.
pub(crate) type PreviewCache = (PreviewSource, usize, Option<Preview>);

impl App {
    pub(super) fn open_palette(&mut self) {
        self.open_palette_seeded("");
    }

    /// Open the palette with `seed` already typed (e.g. `"@"` to land in
    /// go-to-symbol mode).
    pub(super) fn open_palette_seeded(&mut self, seed: &str) {
        self.palette_everywhere = false;
        self.picker_preview_cache = None;
        self.palette_history.reset();
        self.build_file_index();
        self.palette_origin = self.editor.active_tab().map(|t| t.editor.get_cursor());
        let mut p = Palette::new();
        p.input = seed.to_string();
        self.palette = Some(p);
        self.palette_files_override = None;
        self.recompute_palette();
    }

    /// Open the palette's Files mode over `files` instead of the project
    /// index (a Cowork guest browsing the host's shared files, T696).
    #[cfg(feature = "cowork")]
    pub(super) fn open_palette_over(&mut self, files: Vec<std::path::PathBuf>) {
        self.open_palette_seeded("");
        self.palette_files_override = Some(files);
        self.recompute_palette();
    }

    /// Live `:` go-to-line preview: move the cursor to the number being typed.
    /// Always reverts to the captured origin first, so editing the number (or
    /// leaving go-to-line mode) tracks the latest input.
    fn preview_goto_line(&mut self) {
        self.restore_palette_origin();
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        if p.mode() != PMode::GotoLine {
            return;
        }
        let Ok(n) = p.query().trim().parse::<usize>() else {
            return;
        };
        if n >= 1 {
            let area = self.editor_view();
            self.editor.goto(n, None, area);
        }
    }

    /// Return the cursor to where it was when the palette opened (used to revert a
    /// go-to-line preview).
    fn restore_palette_origin(&mut self) {
        let Some(off) = self.palette_origin else {
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Rebuild the project file index used by the fuzzy file finder.
    ///
    /// Walks the project with the `ignore` crate so the index honors `.gitignore`,
    /// `.ignore`, git's global excludes, and hidden-file rules — the same engine
    /// ripgrep uses — instead of a hand-rolled walk. `target`/`node_modules` are
    /// always pruned (some projects don't ignore them), depth is bounded, and the
    /// index is capped to keep startup and matching responsive.
    pub(super) fn build_file_index(&mut self) {
        const CAP: usize = 5000;
        let auth = crate::authority::current();
        let mut out = Vec::new();
        for folder in &self.workspace_folders {
            if out.len() >= CAP {
                break;
            }
            // Through the authority slot (T710); the local one walks with
            // `ignore` exactly as before.
            let opts = crate::authority::WalkOptions::file_index(CAP - out.len());
            out.extend(auth.walk_files(folder, &opts));
        }
        self.file_index = out;
    }

    /// The palette's Files-mode entries for `query`, honoring
    /// `palette_file_scope` when set (`project.subproject.find_file`).
    /// Grouped out of [`App::recompute_palette`] to keep it within the line
    /// limit.
    ///
    /// Ranked with [`palette::fuzzy_score`], tie-broken on the path — the
    /// same shape `recompute_palette`'s `PMode::Commands` arm uses (T153).
    /// Before this, entries kept `self.file_index`'s raw `ignore::
    /// WalkBuilder` traversal order: not portable across filesystems
    /// (ext4 vs APFS order differently), not ranked by relevance, and
    /// capped to the first 200 hits *in that arbitrary order* rather than
    /// the 200 best — a large workspace could easily bury a strong match
    /// behind 200 weaker ones the walk happened to visit first. An empty
    /// query scores every candidate `0` (see `fuzzy_score`'s docs), so the
    /// path tie-break alone puts the unfiltered list in alphabetical order,
    /// which is also strictly better than raw walk order.
    fn palette_file_entries(&self, query: &str) -> Vec<Entry> {
        let (qpath, target) = palette::parse_path_target(query);
        let mut scored: Vec<(i32, String, Entry)> = Vec::new();
        let source = self
            .palette_files_override
            .as_ref()
            .unwrap_or(&self.file_index);
        for path in source {
            let shown = path.to_string_lossy();
            // A Cowork guest's list holds `cowork:`-prefixed host paths (T696).
            let rel = match shown.strip_prefix(crate::editor::REMOTE_PATH_PREFIX) {
                Some(remote) => remote.to_string(),
                None => path
                    .strip_prefix(&self.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
            };
            // `project.subproject.find_file` scopes this list to one
            // subproject's files (see `App::palette_file_scope`).
            if let Some(scope) = self.palette_file_scope.as_deref()
                && rel != scope
                && !rel.starts_with(&format!("{scope}/"))
            {
                continue;
            }
            let Some(score) = palette::fuzzy_score(&rel, &qpath) else {
                continue;
            };
            scored.push((
                score,
                rel.clone(),
                Entry {
                    label: rel,
                    action: PAction::OpenFile(path.clone(), target),
                },
            ));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().take(200).map(|(_, _, e)| e).collect()
    }

    /// Score every `vix_action_catalog::CATALOG` entry against `query`,
    /// continuing the numbering from `start_idx` so catalog, static, and
    /// script commands never collide as a stable-sort tiebreak (mirrors
    /// [`App::script_palette_entries`]'s own pattern). These are the
    /// actions titled nowhere else (T147) — `tests/action_catalog.rs`'s
    /// `no_catalog_entry_duplicates_a_palette_commands_entry` keeps the
    /// catalog itself free of any id `palette::COMMANDS` already has; the
    /// `any` check below is a cheap second guard against the same action
    /// ever appearing twice under two different labels, in case that
    /// invariant ever drifts before a test run catches it.
    fn catalog_palette_entries(&self, query: &str, start_idx: usize) -> Vec<(i32, usize, Entry)> {
        let recent_rank = |action: &str| self.command_recents.iter().position(|a| a == action);
        let mut scored = Vec::new();
        for (i, a) in vix_action_catalog::CATALOG
            .iter()
            .filter(|a| !palette::COMMANDS.iter().any(|(_, action)| *action == a.id))
            .filter(|a| self.action_listed(a.id))
            .enumerate()
        {
            let idx = start_idx + i;
            let label = t!(a.title).to_string();
            let entry = Entry {
                label: format!("> {label}"),
                action: PAction::RunCommand(a.id.to_string()),
            };
            if query.is_empty() {
                let key = recent_rank(a.id).map_or(1000 + i32::try_from(idx).unwrap_or(0), |r| {
                    i32::try_from(r).unwrap_or(0)
                });
                scored.push((-key, idx, entry));
            } else if let Some(score) = palette::fuzzy_score(&label, query) {
                let boost =
                    recent_rank(a.id).map_or(0, |r| (12 - i32::try_from(r).unwrap_or(12)).max(0));
                scored.push((score + boost, idx, entry));
            }
        }
        scored
    }

    pub(super) fn recompute_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let mode = p.mode();
        let query = p.query().to_string();
        let entries = if self.palette_everywhere && mode == PMode::Files {
            self.everywhere_entries(&query)
        } else {
            self.palette_entries(mode, &query)
        };
        if let Some(p) = self.palette.as_mut() {
            if p.selected >= entries.len() {
                p.selected = entries.len().saturating_sub(1);
            }
            p.entries = entries;
        }
    }

    /// The entries of palette `mode` for `query` (the prefix already removed).
    pub(super) fn palette_entries(&self, mode: PMode, query: &str) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        match mode {
            PMode::Files => entries = self.palette_file_entries(query),
            PMode::Commands => {
                // Score every command: when the query is empty, order by recency
                // (recently-run first) then catalog order; otherwise rank by fuzzy
                // score with recency as a tiebreak.
                let recent_rank =
                    |action: &str| self.command_recents.iter().position(|a| a == action);
                let mut scored: Vec<(i32, usize, Entry)> = Vec::new();
                for (cat_idx, (label_key, action)) in palette::COMMANDS.iter().enumerate() {
                    if !self.action_listed(action) {
                        continue;
                    }
                    let label = t!(*label_key).to_string();
                    let entry = Entry {
                        label: format!("> {label}"),
                        action: PAction::RunCommand((*action).to_string()),
                    };
                    if query.is_empty() {
                        // Recents (rank 0..) sort above everything; non-recents keep
                        // catalog order after them.
                        let key = recent_rank(action)
                            .map_or(1000 + i32::try_from(cat_idx).unwrap_or(0), |r| {
                                i32::try_from(r).unwrap_or(0)
                            });
                        scored.push((-key, cat_idx, entry));
                    } else if let Some(score) = palette::fuzzy_score(&label, query) {
                        let boost = recent_rank(action)
                            .map_or(0, |r| (12 - i32::try_from(r).unwrap_or(12)).max(0));
                        scored.push((score + boost, cat_idx, entry));
                    }
                }
                scored.extend(self.script_palette_entries(query));
                let catalog_start = palette::COMMANDS.len() + self.script_command_count();
                scored.extend(self.catalog_palette_entries(query, catalog_start));
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                entries = scored.into_iter().map(|(_, _, e)| e).collect();
            }
            PMode::Buffers => {
                for (i, tab) in self.editor.tabs.iter().enumerate() {
                    let label = tab.display_path();
                    if query.is_empty() || palette::fuzzy_match(&label, query) {
                        entries.push(Entry {
                            label: format!("# {label}"),
                            action: PAction::SwitchBuffer(i),
                        });
                    }
                }
            }
            PMode::GotoLine => {
                if let Ok(n) = query.trim().parse::<usize>() {
                    entries.push(Entry {
                        label: format!(": go to line {n}"),
                        action: PAction::GotoLine(n),
                    });
                }
            }
            PMode::Symbols => {
                if let Some(tab) = self.editor.active_tab()
                    && !tab.is_image()
                {
                    let text = tab.text();
                    for sym in palette::symbols(&text) {
                        if query.is_empty() || palette::fuzzy_match(&sym.name, query) {
                            entries.push(Entry {
                                label: format!("@ {}", sym.text),
                                action: PAction::GotoLine(sym.line),
                            });
                        }
                    }
                }
            }
            PMode::WorkspaceSymbols => {
                entries = self.workspace_symbol_entries(query);
            }
        }
        entries
    }

    /// Scan the workspace's indexed files for declaration symbols matching
    /// `query`, returning palette entries that open the file at the symbol's
    /// line. An empty query returns nothing (the workspace is too large to list
    /// every symbol); results and files scanned are capped to stay responsive.
    fn workspace_symbol_entries(&self, query: &str) -> Vec<Entry> {
        const MAX_RESULTS: usize = 200;
        const MAX_FILE_BYTES: u64 = 512 * 1024;
        let mut entries = Vec::new();
        if query.trim().is_empty() {
            return entries;
        }
        let auth = crate::authority::current();
        for path in &self.file_index {
            if entries.len() >= MAX_RESULTS {
                break;
            }
            // Skip obviously-binary or oversized files cheaply by extension/size.
            if is_image_path(path) {
                continue;
            }
            if auth.metadata(path).is_ok_and(|m| m.len > MAX_FILE_BYTES) {
                continue;
            }
            let Ok(text) = auth.read_to_string(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            for sym in palette::symbols(&text) {
                if !palette::fuzzy_match(&sym.name, query) {
                    continue;
                }
                entries.push(Entry {
                    label: format!("@ {}  ·  {}:{}", sym.name, rel, sym.line),
                    action: PAction::OpenFile(path.clone(), Some((sym.line, 1))),
                });
                if entries.len() >= MAX_RESULTS {
                    break;
                }
            }
        }
        entries
    }

    pub(super) fn palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.remember_palette();
                // Cancel: revert any go-to-line preview to where the cursor was.
                self.restore_palette_origin();
                self.palette = None;
                self.palette_origin = None;
                self.palette_file_scope = None;
                self.palette_files_override = None;
            }
            KeyCode::Up => {
                if !self.palette_history_step(true)
                    && let Some(p) = self.palette.as_mut()
                {
                    p.up();
                }
            }
            KeyCode::Down => {
                if !self.palette_history_step(false)
                    && let Some(p) = self.palette.as_mut()
                {
                    p.down();
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self.palette.as_mut() {
                    p.selected = 0;
                }
                self.accept_palette();
            }
            KeyCode::Enter => self.accept_palette(),
            KeyCode::Backspace => {
                self.palette_history.reset();
                if let Some(p) = self.palette.as_mut() {
                    p.backspace();
                }
                self.recompute_palette();
                self.preview_goto_line();
            }
            KeyCode::Char(c) => {
                self.palette_history.reset();
                if let Some(p) = self.palette.as_mut() {
                    p.insert(c);
                }
                self.recompute_palette();
                self.preview_goto_line();
            }
            _ => {}
        }
    }

    /// Record `action` as the most-recently-run palette command (deduped,
    /// most-recent first, capped).
    fn record_command_recent(&mut self, action: &str) {
        const MAX_RECENTS: usize = 12;
        self.command_recents.retain(|a| a != action);
        self.command_recents.insert(0, action.to_string());
        self.command_recents.truncate(MAX_RECENTS);
        // Persist so the order survives across sessions.
        self.settings
            .command_recents
            .clone_from(&self.command_recents);
        self.store_settings_or_warn();
    }

    fn accept_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(entry) = p.selected_entry().cloned() else {
            return;
        };
        let input = p.input.clone();
        self.remember_palette();
        self.palette_history.push(&input);
        self.palette = None;
        self.palette_file_scope = None;
        self.palette_files_override = None;
        // Undo any live go-to-line preview so the action below jumps from (and
        // records in the history) the cursor's real pre-palette position.
        self.restore_palette_origin();
        self.palette_origin = None;
        match entry.action {
            PAction::OpenFile(path, _) if crate::editor::is_remote_path(&path) => {
                self.cowork_request_open(&path);
            }
            PAction::OpenFile(path, target) => {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    if let Some((line, col)) = target {
                        let area = s.editor_view();
                        s.editor.goto(line, Some(col), area);
                    }
                    s.focus = Focus::Editor;
                });
            }
            PAction::RunCommand(action) => {
                self.record_command_recent(&action);
                self.run_action(&action);
            }
            PAction::SwitchBuffer(i) => {
                self.with_jump(|s| {
                    if i < s.editor.tabs.len() {
                        s.editor.active = i;
                    }
                    s.focus = Focus::Editor;
                });
            }
            PAction::GotoLine(n) => {
                self.with_jump(|s| {
                    let area = s.editor_view();
                    s.editor.goto(n, None, area);
                    s.focus = Focus::Editor;
                });
            }
        }
    }
}

// ----- T630: query history, resume, and the preview pane ------------------

impl App {
    /// `Up`/`Down` through the palette's query history (`older` for `Up`).
    /// Returns `false` when the key should move the selection instead: the
    /// input is neither empty nor an unchanged recall, or (for `Down`) no
    /// recall is under way.
    fn palette_history_step(&mut self, older: bool) -> bool {
        let Some(p) = self.palette.as_ref() else {
            return false;
        };
        let starts_recall = older && p.input.is_empty();
        if !starts_recall && !self.palette_history.is_browsing(&p.input) {
            return false;
        }
        let recalled = if older {
            self.palette_history.older()
        } else {
            self.palette_history.newer()
        };
        let Some(text) = recalled.map(str::to_string) else {
            return false;
        };
        if let Some(p) = self.palette.as_mut() {
            p.input = text;
            p.selected = 0;
        }
        self.recompute_palette();
        self.preview_goto_line();
        true
    }

    /// Store the open palette's `(mode, query, selection)` for
    /// `palette.resume`.
    fn remember_palette(&mut self) {
        if let Some(p) = self.palette.as_ref() {
            self.picker_memory.palette = Some(PaletteMemory::of(p, self.palette_everywhere));
            self.picker_memory.last = Some(PickerKind::Palette);
        }
    }

    /// Store the open workspace-search panel's state for `palette.resume`.
    /// A static result list (go-to-definition candidates) is not kept: it
    /// can't be re-run from a query.
    pub(super) fn remember_workspace_search(&mut self) {
        let Some(ps) = self.workspace_search.as_ref() else {
            return;
        };
        if ps.flags.contains(WorkspaceFlags::STATIC_RESULTS) {
            return;
        }
        self.picker_memory.workspace = Some(WorkspaceMemory {
            query: ps.query.clone(),
            replace: ps.replace.clone(),
            include_path: ps.include_path.clone(),
            exclude_path: ps.exclude_path.clone(),
            flags: ps.flags,
            selected: ps.selected,
        });
        self.picker_memory.last = Some(PickerKind::WorkspaceSearch);
    }

    /// `palette.resume`: reopen the picker that closed last with its mode,
    /// query, and selection.
    pub(super) fn resume_picker(&mut self) {
        match self.picker_memory.last {
            Some(PickerKind::Palette) => {
                let Some(m) = self.picker_memory.palette.clone() else {
                    return;
                };
                self.open_palette_seeded(&m.input());
                if m.everywhere {
                    self.palette_everywhere = true;
                    self.recompute_palette();
                }
                if let Some(p) = self.palette.as_mut() {
                    p.selected = m.selected.min(p.entries.len().saturating_sub(1));
                }
                self.preview_goto_line();
            }
            Some(PickerKind::WorkspaceSearch) => {
                let Some(m) = self.picker_memory.workspace.clone() else {
                    return;
                };
                self.build_file_index();
                self.picker_preview_cache = None;
                let mut ps = WorkspaceSearch::new(m.flags.contains(WorkspaceFlags::REPLACING));
                ps.query = m.query;
                ps.replace = m.replace;
                ps.include_path = m.include_path;
                ps.exclude_path = m.exclude_path;
                ps.flags = m.flags;
                self.workspace_search = Some(ps);
                self.run_workspace_search();
                if let Some(ps) = self.workspace_search.as_mut() {
                    ps.selected = m.selected.min(ps.hits.len().saturating_sub(1));
                }
            }
            None => self.status = t!("status.picker_resume_none").to_string(),
        }
    }

    /// The highlighted palette entry's preview target, for the modes that
    /// have a preview pane: files, `@` symbols, `@@` workspace symbols.
    fn palette_preview_target(&self) -> Option<(PreviewSource, usize)> {
        let p = self.palette.as_ref()?;
        let entry = p.selected_entry()?;
        match (&entry.action, p.mode()) {
            (PAction::OpenFile(path, target), PMode::Files | PMode::WorkspaceSymbols) => {
                Some((PreviewSource::File(path.clone()), target.map_or(1, |t| t.0)))
            }
            (PAction::GotoLine(n), PMode::Symbols) => Some((PreviewSource::ActiveBuffer, *n)),
            _ => None,
        }
    }

    /// Whether the open palette's mode has a preview pane (Search
    /// Everywhere's mixed list does not).
    #[must_use]
    pub(crate) fn palette_has_preview(&self) -> bool {
        self.palette.as_ref().is_some_and(|p| {
            !self.palette_everywhere
                && matches!(
                    p.mode(),
                    PMode::Files | PMode::Symbols | PMode::WorkspaceSymbols
                )
        })
    }

    /// The preview of the highlighted palette entry (T630), or `None` when
    /// there is no entry or it can't be previewed.
    pub(crate) fn palette_preview(&mut self) -> Option<(String, Preview)> {
        let (source, line) = self.palette_preview_target()?;
        self.picker_preview(source, line)
    }

    /// The preview of the highlighted workspace-search hit (T630).
    pub(crate) fn workspace_search_preview(&mut self) -> Option<(String, Preview)> {
        let hit = self.workspace_search.as_ref()?.selected_hit()?;
        let (path, line) = (hit.path.clone(), hit.line);
        self.picker_preview(PreviewSource::File(path), line)
    }

    /// The preview of `source` around 1-based `line` with its pane title,
    /// served from the one-entry cache when the highlight hasn't moved.
    pub(crate) fn picker_preview(
        &mut self,
        source: PreviewSource,
        line: usize,
    ) -> Option<(String, Preview)> {
        let title = match &source {
            PreviewSource::File(path) => path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned(),
            PreviewSource::ActiveBuffer => self
                .editor
                .active_tab()
                .map(crate::editor::Tab::display_path)
                .unwrap_or_default(),
        };
        if let Some((s, l, cached)) = self.picker_preview_cache.as_ref()
            && *s == source
            && *l == line
        {
            return cached.clone().map(|p| (title, p));
        }
        let built = self.build_preview(&source, line);
        self.picker_preview_cache = Some((source, line, built.clone()));
        built.map(|p| (title, p))
    }

    /// Build a preview without ever reading more than `PREVIEW_BYTES` of a
    /// file: an open buffer's window comes from its rope line by line;
    /// anything else is one bounded read through the local authority. A
    /// remote (non-local) authority, a Cowork path, or an image gets none.
    fn build_preview(&self, source: &PreviewSource, line: usize) -> Option<Preview> {
        let tab = match source {
            PreviewSource::ActiveBuffer => self.editor.active_tab(),
            PreviewSource::File(path) => self.open_tab_for(path),
        };
        if let Some(tab) = tab {
            if tab.is_image() {
                return None;
            }
            let code = tab.editor.code_ref();
            let total = code.len_lines();
            return Some(palette::preview_from_lines(total, line, |i| {
                code.line(i).to_string()
            }));
        }
        let PreviewSource::File(path) = source else {
            return None;
        };
        if crate::editor::is_remote_path(path) || is_image_path(path) {
            return None;
        }
        let auth = crate::authority::current();
        if !auth.is_local() {
            return None;
        }
        let bytes = auth.read_limited(path, palette::PREVIEW_BYTES).ok()?;
        let truncated = bytes.len() as u64 >= palette::PREVIEW_BYTES;
        palette::preview_from_bytes(&bytes, line, truncated)
    }

    /// The open tab showing `path`, compared as given and then canonicalized.
    fn open_tab_for(&self, path: &Path) -> Option<&crate::editor::Tab> {
        let find = |p: &Path| {
            self.editor
                .tabs
                .iter()
                .find(|t| t.path.as_deref() == Some(p))
        };
        find(path).or_else(|| {
            let canon = crate::authority::current().canonicalize(path).ok()?;
            find(&canon)
        })
    }
}
