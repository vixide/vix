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

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Focus};
use crate::editor::is_image_path;
use crate::palette::{self, Action as PAction, Entry, Mode as PMode, Palette};

impl App {
    pub(super) fn open_palette(&mut self) {
        self.open_palette_seeded("");
    }

    /// Open the palette with `seed` already typed (e.g. `"@"` to land in
    /// go-to-symbol mode).
    pub(super) fn open_palette_seeded(&mut self, seed: &str) {
        self.build_file_index();
        self.palette_origin = self.editor.active_tab().map(|t| t.editor.get_cursor());
        let mut p = Palette::new();
        p.input = seed.to_string();
        self.palette = Some(p);
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
        let mut out = Vec::new();
        for folder in &self.workspace_folders {
            if out.len() >= 5000 {
                break;
            }
            let walker = ignore::WalkBuilder::new(folder)
                .max_depth(Some(12))
                .hidden(true) // skip dotfiles (matches the previous behavior)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .require_git(false) // apply .gitignore/.ignore even outside a git repo
                .parents(true)
                .filter_entry(|e| {
                    // Prune heavy build/vendor dirs, but never the walk root itself.
                    e.depth() == 0
                        || !matches!(e.file_name().to_str(), Some("target" | "node_modules"))
                })
                .build();
            for entry in walker.flatten() {
                if out.len() >= 5000 {
                    break;
                }
                if entry.file_type().is_some_and(|t| t.is_file()) {
                    out.push(entry.into_path());
                }
            }
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
        for path in &self.file_index {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
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

    fn recompute_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let mode = p.mode();
        let query = p.query().to_string();
        let mut entries: Vec<Entry> = Vec::new();
        match mode {
            PMode::Files => entries = self.palette_file_entries(&query),
            PMode::Commands => {
                // Score every command: when the query is empty, order by recency
                // (recently-run first) then catalog order; otherwise rank by fuzzy
                // score with recency as a tiebreak.
                let recent_rank =
                    |action: &str| self.command_recents.iter().position(|a| a == action);
                let mut scored: Vec<(i32, usize, Entry)> = Vec::new();
                for (cat_idx, (label_key, action)) in palette::COMMANDS.iter().enumerate() {
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
                    } else if let Some(score) = palette::fuzzy_score(&label, &query) {
                        let boost = recent_rank(action)
                            .map_or(0, |r| (12 - i32::try_from(r).unwrap_or(12)).max(0));
                        scored.push((score + boost, cat_idx, entry));
                    }
                }
                scored.extend(self.script_palette_entries(&query));
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                entries = scored.into_iter().map(|(_, _, e)| e).collect();
            }
            PMode::Buffers => {
                for (i, tab) in self.editor.tabs.iter().enumerate() {
                    let label = tab.display_path();
                    if query.is_empty() || palette::fuzzy_match(&label, &query) {
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
                        if query.is_empty() || palette::fuzzy_match(&sym.name, &query) {
                            entries.push(Entry {
                                label: format!("@ {}", sym.text),
                                action: PAction::GotoLine(sym.line),
                            });
                        }
                    }
                }
            }
            PMode::WorkspaceSymbols => {
                entries = self.workspace_symbol_entries(&query);
            }
        }
        if let Some(p) = self.palette.as_mut() {
            if p.selected >= entries.len() {
                p.selected = entries.len().saturating_sub(1);
            }
            p.entries = entries;
        }
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
        for path in &self.file_index {
            if entries.len() >= MAX_RESULTS {
                break;
            }
            // Skip obviously-binary or oversized files cheaply by extension/size.
            if is_image_path(path) {
                continue;
            }
            if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_FILE_BYTES) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
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
                // Cancel: revert any go-to-line preview to where the cursor was.
                self.restore_palette_origin();
                self.palette = None;
                self.palette_origin = None;
                self.palette_file_scope = None;
            }
            KeyCode::Up => {
                if let Some(p) = self.palette.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.palette.as_mut() {
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
                if let Some(p) = self.palette.as_mut() {
                    p.backspace();
                }
                self.recompute_palette();
                self.preview_goto_line();
            }
            KeyCode::Char(c) => {
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
        let _ = self.store_settings();
    }

    fn accept_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(entry) = p.selected_entry().cloned() else {
            return;
        };
        self.palette = None;
        self.palette_file_scope = None;
        // Undo any live go-to-line preview so the action below jumps from (and
        // records in the history) the cursor's real pre-palette position.
        self.restore_palette_origin();
        self.palette_origin = None;
        match entry.action {
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
