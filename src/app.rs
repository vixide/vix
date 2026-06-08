//! Application state and event handling.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui_code_editor::actions::{
    Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction, Undo as UndoAction,
};
use ratatui_code_editor::selection::Selection;
use ratatui_image::picker::Picker;
use regex::Regex;

use crate::editor::{is_image_path, Editor, Tab, SEARCH_MARK};
use crate::explorer::Explorer;
use crate::menu::{Menu, MENUS};
use crate::messages::{Level, Messages};
use crate::palette::{self, Action as PAction, Entry, Mode as PMode, Palette};
use crate::project_search::{Hit, ProjectSearch};
use crate::query::{Decision, QueryReplace};
use crate::search::{Field, SearchBar};
use crate::settings::Settings;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Explorer,
    Messages,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Open,
    SaveAs,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub title: String,
    pub input: String,
}

/// An in-progress paste, processed one source at a time so a name conflict can
/// pause for an (o)verwrite / (s)kip / (c)ancel decision.
pub struct PasteOp {
    pub target: PathBuf,
    pub cut: bool,
    pub queue: VecDeque<PathBuf>,
    pub overwrite_all: bool,
    pub skip_all: bool,
    /// The source currently awaiting a conflict decision, if any.
    pub conflict: Option<PathBuf>,
}

/// A yes/no confirmation (currently only used for delete).
pub struct Confirm {
    pub message: String,
    pub paths: Vec<PathBuf>,
}

/// A point in the position-history jump list: a file and a 1-based line/column.
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
}

/// Rectangles recorded during rendering, used for mouse hit-testing and for
/// telling the code editor which viewport to scroll within.
#[derive(Default)]
pub struct Layout {
    pub menu: Rect,
    pub tabs: Rect,
    pub editor: Rect,
    pub explorer: Rect,
    pub messages: Rect,
}

pub struct App {
    pub root: PathBuf,
    pub editor: Editor,
    pub explorer: Explorer,
    pub messages: Messages,
    pub menu: Menu,
    pub palette: Option<Palette>,
    pub search: Option<SearchBar>,
    pub query_replace: Option<QueryReplace>,
    pub project_search: Option<ProjectSearch>,
    pub prompt: Option<Prompt>,
    pub paste: Option<PasteOp>,
    pub confirm: Option<Confirm>,
    /// Explorer clipboard: paths plus whether this is a cut (move) or copy.
    pub clip: Vec<PathBuf>,
    pub clip_cut: bool,
    /// Position-history jump list (Alt+Left / Alt+Right) and the current index.
    pub nav_history: Vec<Location>,
    pub nav_idx: usize,
    /// Terminal image picker; `None` until set from a real terminal (so tests
    /// and headless use construct fine), and on terminals without graphics.
    pub picker: Option<Picker>,
    pub settings: Settings,
    pub focus: Focus,
    pub show_explorer: bool,
    pub show_messages: bool,
    pub show_calendar: bool,
    pub show_help: bool,
    pub status: String,
    pub should_quit: bool,
    pub layout: Layout,
    /// File paths under the project root, for the palette file finder.
    file_index: Vec<PathBuf>,
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        let settings = Settings::load();
        let editor = Editor::new(settings.line_numbers);
        let mut messages = Messages::default();
        messages.advice("Welcome to STRIDE. Press Ctrl+P for the command palette, F1 for help.");
        messages.info("Ctrl+B toggles the file explorer, Ctrl+E switches focus.");

        App {
            explorer: Explorer::new(root.clone()),
            root,
            editor,
            messages,
            menu: Menu::default(),
            palette: None,
            search: None,
            query_replace: None,
            project_search: None,
            prompt: None,
            paste: None,
            confirm: None,
            clip: Vec::new(),
            clip_cut: false,
            nav_history: Vec::new(),
            nav_idx: 0,
            picker: None,
            show_explorer: settings.show_explorer,
            show_messages: settings.show_messages,
            show_calendar: false,
            show_help: false,
            focus: Focus::Editor,
            status: "Ready".to_string(),
            should_quit: false,
            layout: Layout::default(),
            settings,
            file_index: Vec::new(),
        }
    }

    /// Open a path given on the command line.
    pub fn open_initial(&mut self, path: PathBuf) {
        self.open_path(&path, false);
    }

    // ----- top-level event entry -----------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        // Modal layers, in priority order.
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => self.show_help = false,
                _ => {}
            }
            return;
        }
        if self.query_replace.is_some() {
            self.qr_key(key);
            return;
        }
        if self.project_search.is_some() {
            self.ps_key(key);
            return;
        }
        if self.confirm.is_some() {
            self.confirm_key(key);
            return;
        }
        if self.paste.as_ref().map(|p| p.conflict.is_some()).unwrap_or(false) {
            self.paste_key(key);
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(key);
            return;
        }
        if self.palette.is_some() {
            self.palette_key(key);
            return;
        }
        if self.search.is_some() {
            self.search_key(key);
            return;
        }
        if self.menu.is_open() {
            self.menu_key(key);
            return;
        }
        if self.global_key(key) {
            return;
        }
        match self.focus {
            Focus::Editor => self.editor_key(key),
            Focus::Explorer => self.explorer_key(key),
            Focus::Messages => self.messages_key(key),
        }
    }

    fn ctrl(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }

    fn alt(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::ALT)
    }

    fn shift(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::SHIFT)
    }

    /// Global shortcuts available when no modal is active. Returns true if the
    /// key was consumed.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        if Self::ctrl(&key) {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'q' => self.run_action("file.quit"),
                    'n' => self.run_action("file.new"),
                    'o' => self.run_action("file.open"),
                    's' if Self::shift(&key) => self.run_action("file.save_as"),
                    's' => self.run_action("file.save"),
                    'w' => self.run_action("file.close"),
                    'p' => self.run_action("tools.palette"),
                    'b' => self.run_action("view.explorer"),
                    'e' => self.toggle_focus_explorer_editor(),
                    'f' if Self::shift(&key) => self.run_action("search.project"),
                    'f' => self.run_action("edit.find"),
                    'r' if Self::alt(&key) => self.run_action("edit.query_replace"),
                    'r' => self.run_action("edit.replace"),
                    _ => return false,
                }
                return true;
            }
        }
        if Self::alt(&key) {
            if let KeyCode::Char(c) = key.code {
                let idx = match c.to_ascii_lowercase() {
                    'f' => Some(0),
                    'e' => Some(1),
                    't' => Some(2),
                    'h' => Some(3),
                    _ => None,
                };
                if let Some(i) = idx {
                    self.menu.open_index(i);
                    return true;
                }
            }
        }
        match key.code {
            KeyCode::Left if Self::alt(&key) => {
                self.nav_back();
                true
            }
            KeyCode::Right if Self::alt(&key) => {
                self.nav_forward();
                true
            }
            KeyCode::F(1) => {
                self.show_help = true;
                true
            }
            KeyCode::F(10) => {
                self.menu.toggle();
                true
            }
            KeyCode::F(3) if Self::shift(&key) => {
                self.find_step(false);
                true
            }
            KeyCode::F(3) => {
                self.find_step(true);
                true
            }
            _ => false,
        }
    }

    fn toggle_focus_explorer_editor(&mut self) {
        self.focus = match self.focus {
            Focus::Explorer => Focus::Editor,
            _ => {
                if !self.show_explorer {
                    self.show_explorer = true;
                }
                Focus::Explorer
            }
        };
    }

    // ----- action dispatch (menu + palette + shortcuts) ------------------

    pub fn run_action(&mut self, action: &str) {
        match action {
            "file.new" => {
                self.editor.new_tab();
                self.focus = Focus::Editor;
                self.status = "New buffer".into();
            }
            "file.open" => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Open,
                    title: "Open file (path[:line[:col]])".into(),
                    input: String::new(),
                });
            }
            "file.save" => self.save(),
            "file.save_as" => {
                let cur = self
                    .editor
                    .active_tab()
                    .and_then(|t| t.path.clone())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.prompt = Some(Prompt {
                    kind: PromptKind::SaveAs,
                    title: "Save as (path)".into(),
                    input: cur,
                });
            }
            "file.close" => {
                self.editor.close_active();
                self.status = "Closed buffer".into();
            }
            "file.quit" => self.should_quit = true,
            "edit.undo" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(UndoAction {});
                    t.dirty = true;
                }
            }
            "edit.redo" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(RedoAction {});
                    t.dirty = true;
                }
            }
            "edit.cut" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CutAction {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.copy" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CopyAction {});
                }
            }
            "edit.paste" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(PasteAction {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.find" => self.start_search(false),
            "edit.replace" => self.start_search(true),
            "edit.query_replace" => {
                self.start_search(true);
                if let Some(s) = self.search.as_mut() {
                    s.interactive = true;
                }
            }
            "search.project" => self.open_project_search(false),
            "search.project_replace" => self.open_project_search(true),
            "tools.calendar" => self.show_calendar = !self.show_calendar,
            "tools.palette" => self.open_palette(),
            "tools.line_numbers" => {
                self.editor.line_numbers = !self.editor.line_numbers;
                self.editor.refresh_line_numbers();
                self.settings.line_numbers = self.editor.line_numbers;
                self.status = format!(
                    "Line numbers {}",
                    if self.editor.line_numbers { "on" } else { "off" }
                );
            }
            "view.explorer" => {
                self.show_explorer = !self.show_explorer;
                self.settings.show_explorer = self.show_explorer;
                if self.show_explorer {
                    if let Some(p) = self.editor.active_tab().and_then(|t| t.path.clone()) {
                        self.explorer.reveal(&p);
                    }
                }
            }
            "view.messages" => {
                self.show_messages = !self.show_messages;
                self.settings.show_messages = self.show_messages;
            }
            "tab.next" => self.editor.next_tab(),
            "tab.prev" => self.editor.prev_tab(),
            "help.shortcuts" => self.show_help = true,
            "help.website" => self.messages.info("Website: https://github.com/sixarm/stride"),
            "help.email" => self.messages.info("Email: hello@sixarm.com"),
            "help.about" => self
                .messages
                .advice("STRIDE — Simple Terminal Rust IDE. Open, edit, and save text files."),
            other => self.messages.warn(format!("Unknown action: {other}")),
        }
    }

    fn save(&mut self) {
        if self.editor.active_tab().map(Tab::is_image).unwrap_or(false) {
            self.status = "Image tabs are read-only".into();
            return;
        }
        if self.editor.active_tab().and_then(|t| t.path.as_ref()).is_none() {
            self.run_action("file.save_as");
            return;
        }
        match self.editor.save_active() {
            Ok(p) => self.status = format!("Saved {}", p.display()),
            Err(e) => self.messages.error(format!("Save failed: {e}")),
        }
    }

    // ----- focus handlers -------------------------------------------------

    /// The editor rectangle, clamped to a width the code editor can safely
    /// scroll within (see [`MIN_EDITOR_WIDTH`]).
    fn editor_view(&self) -> Rect {
        let r = self.layout.editor;
        Rect {
            width: r.width.max(MIN_EDITOR_WIDTH),
            height: r.height.max(1),
            ..r
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        // Image tabs are view-only.
        if self.editor.active_tab().map(Tab::is_image).unwrap_or(false) {
            return;
        }
        let area = self.editor_view();
        match key.code {
            KeyCode::Home => return self.editor.cursor_line_home(),
            KeyCode::End => return self.editor.cursor_line_end(),
            KeyCode::Delete => {
                self.editor.delete_forward();
                self.mark_active_dirty();
                return;
            }
            KeyCode::PageUp => {
                self.editor.page_up(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            KeyCode::PageDown => {
                self.editor.page_down(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            _ => {}
        }

        let editing = Self::is_edit_key(&key);
        if let Some(t) = self.editor.active_tab_mut() {
            let _ = t.editor.input(key, &area);
            if editing {
                t.dirty = true;
                t.preview = false;
            }
        }
    }

    fn is_edit_key(key: &KeyEvent) -> bool {
        if Self::alt(key) {
            return false;
        }
        if Self::ctrl(key) {
            return matches!(
                key.code,
                KeyCode::Char('v')
                    | KeyCode::Char('x')
                    | KeyCode::Char('z')
                    | KeyCode::Char('y')
                    | KeyCode::Char('k')
                    | KeyCode::Char('d')
            );
        }
        matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Tab | KeyCode::BackTab
        )
    }

    fn mark_active_dirty(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            t.dirty = true;
            t.preview = false;
        }
    }

    fn explorer_key(&mut self, key: KeyEvent) {
        if Self::ctrl(&key) {
            match key.code {
                KeyCode::Char('c') => return self.explorer_copy(false),
                KeyCode::Char('x') => return self.explorer_copy(true),
                KeyCode::Char('v') => return self.explorer_paste(),
                _ => {}
            }
        }
        match key.code {
            KeyCode::Up if Self::shift(&key) => self.explorer.extend(false),
            KeyCode::Down if Self::shift(&key) => self.explorer.extend(true),
            KeyCode::Up => {
                self.explorer.clear_marks();
                self.explorer.up();
                self.preview_selected();
            }
            KeyCode::Down => {
                self.explorer.clear_marks();
                self.explorer.down();
                self.preview_selected();
            }
            KeyCode::PageUp => self.explorer.page_up(10),
            KeyCode::PageDown => self.explorer.page_down(10),
            KeyCode::Home => self.explorer.first(),
            KeyCode::End => self.explorer.last(),
            KeyCode::Enter | KeyCode::Right => self.open_or_expand_selected(),
            KeyCode::Left => {
                self.explorer.toggle_selected();
            }
            KeyCode::Delete => self.explorer_delete_request(),
            KeyCode::Esc => {
                if !self.clip.is_empty() && self.clip_cut {
                    self.clip.clear();
                    self.clip_cut = false;
                    self.status = "Cut cancelled".into();
                } else if !self.explorer.marked.is_empty() {
                    self.explorer.clear_marks();
                } else {
                    self.focus = Focus::Editor;
                }
            }
            _ => {}
        }
    }

    // ----- explorer clipboard --------------------------------------------

    fn explorer_copy(&mut self, cut: bool) {
        let paths = self.explorer.selected_paths();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        self.clip = paths;
        self.clip_cut = cut;
        self.status = format!("{} {n} item(s)", if cut { "Cut" } else { "Copied" });
    }

    fn explorer_paste(&mut self) {
        if self.clip.is_empty() {
            return;
        }
        let target = match self.explorer.selected_node() {
            Some(n) if n.is_dir => n.path.clone(),
            Some(n) => n
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root.clone()),
            None => self.root.clone(),
        };
        self.paste = Some(PasteOp {
            target,
            cut: self.clip_cut,
            queue: self.clip.clone().into(),
            overwrite_all: false,
            skip_all: false,
            conflict: None,
        });
        self.process_paste();
    }

    /// Advance the paste, performing each source until the queue drains or a
    /// conflict needs a decision.
    fn process_paste(&mut self) {
        loop {
            let front = self.paste.as_ref().and_then(|op| op.queue.front().cloned());
            let Some(src) = front else {
                if let Some(op) = self.paste.take() {
                    if op.cut {
                        self.clip.clear();
                        self.clip_cut = false;
                    }
                }
                self.explorer.clear_marks();
                self.explorer.rebuild();
                self.status = "Paste complete".into();
                return;
            };
            let (target, cut, overwrite_all, skip_all) = {
                let op = self.paste.as_ref().unwrap();
                (op.target.clone(), op.cut, op.overwrite_all, op.skip_all)
            };
            let same_dir = src.parent() == Some(target.as_path());
            if cut && same_dir {
                self.paste.as_mut().unwrap().queue.pop_front();
                continue;
            }
            let mut dest = target.join(src.file_name().unwrap_or_default());
            if !cut && same_dir {
                dest = crate::fileops::unique_copy_name(&target, &src);
            } else if dest.exists() {
                if overwrite_all {
                    // fall through and overwrite
                } else if skip_all {
                    self.paste.as_mut().unwrap().queue.pop_front();
                    continue;
                } else {
                    self.paste.as_mut().unwrap().conflict = Some(src.clone());
                    return;
                }
            }
            self.perform_paste_one(&src, &dest, cut);
            self.paste.as_mut().unwrap().queue.pop_front();
        }
    }

    fn perform_paste_one(&mut self, src: &Path, dest: &Path, cut: bool) {
        // Capture the canonical source before moving (it won't exist after).
        let src_canon = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
        let res = if cut {
            crate::fileops::move_path(src, dest)
        } else {
            crate::fileops::copy_recursive(src, dest)
        };
        match res {
            Ok(()) => {
                if cut {
                    let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
                    self.relocate_buffers(&src_canon, &dest_canon);
                }
            }
            Err(e) => self.messages.error(format!("Paste failed: {e}")),
        }
    }

    fn paste_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('o') => {
                // Overwrite just this one, then continue.
                let info = self.paste.as_ref().and_then(|op| {
                    op.queue.front().map(|s| (s.clone(), op.target.clone(), op.cut))
                });
                if let Some((src, target, cut)) = info {
                    let dest = target.join(src.file_name().unwrap_or_default());
                    self.perform_paste_one(&src, &dest, cut);
                    if let Some(op) = self.paste.as_mut() {
                        op.conflict = None;
                        op.queue.pop_front();
                    }
                }
                self.process_paste();
            }
            KeyCode::Char('O') => {
                if let Some(op) = self.paste.as_mut() {
                    op.overwrite_all = true;
                    op.conflict = None;
                }
                self.process_paste();
            }
            KeyCode::Char('s') => {
                if let Some(op) = self.paste.as_mut() {
                    op.conflict = None;
                    op.queue.pop_front();
                }
                self.process_paste();
            }
            KeyCode::Char('S') => {
                if let Some(op) = self.paste.as_mut() {
                    op.skip_all = true;
                    op.conflict = None;
                    op.queue.pop_front();
                }
                self.process_paste();
            }
            KeyCode::Char('c') | KeyCode::Esc => {
                self.paste = None;
                self.status = "Paste cancelled".into();
            }
            _ => {}
        }
    }

    // ----- explorer delete (with confirm) --------------------------------

    fn explorer_delete_request(&mut self) {
        let paths = self.explorer.selected_paths();
        if paths.is_empty() {
            return;
        }
        self.confirm = Some(Confirm {
            message: format!("Delete {} item(s)? (y/n)", paths.len()),
            paths,
        });
    }

    fn confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(c) = self.confirm.take() {
                    let mut removed = 0;
                    for path in &c.paths {
                        // Canonicalize before removing so buffer paths still match.
                        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                        match crate::fileops::remove_path(path) {
                            Ok(()) => {
                                self.close_buffers_under(&canon);
                                removed += 1;
                            }
                            Err(e) => self.messages.error(format!("Delete failed: {e}")),
                        }
                    }
                    self.explorer.clear_marks();
                    self.explorer.rebuild();
                    self.status = format!("Deleted {removed} item(s)");
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
                self.status = "Delete cancelled".into();
            }
            _ => {}
        }
    }

    /// Move open buffers when their file (or containing directory) is moved.
    fn relocate_buffers(&mut self, src: &Path, dest: &Path) {
        for tab in &mut self.editor.tabs {
            let Some(p) = tab.path.clone() else { continue };
            if p == src {
                tab.path = Some(dest.to_path_buf());
            } else if let Ok(rel) = p.strip_prefix(src) {
                tab.path = Some(dest.join(rel));
            }
        }
    }

    /// Close buffers whose file (or containing directory) was deleted.
    fn close_buffers_under(&mut self, path: &Path) {
        let mut i = 0;
        while i < self.editor.tabs.len() {
            let under = self.editor.tabs[i]
                .path
                .as_ref()
                .map(|p| p == path || p.starts_with(path))
                .unwrap_or(false);
            if under {
                self.editor.tabs.remove(i);
            } else {
                i += 1;
            }
        }
        if self.editor.tabs.is_empty() {
            self.editor.new_tab();
        } else if self.editor.active >= self.editor.tabs.len() {
            self.editor.active = self.editor.tabs.len() - 1;
        }
    }

    fn open_or_expand_selected(&mut self) {
        if let Some(node) = self.explorer.selected_node() {
            if node.is_dir {
                self.explorer.toggle_selected();
            } else {
                let path = node.path.clone();
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    s.focus = Focus::Editor;
                });
            }
        }
    }

    /// Arrow-scan preview: opening the highlighted file in an ephemeral tab.
    fn preview_selected(&mut self) {
        if !self.settings.preview_tabs {
            return;
        }
        if let Some(node) = self.explorer.selected_node() {
            if !node.is_dir && !is_image_path(&node.path) {
                let path = node.path.clone();
                let _ = self.editor.open(&path, true);
            }
        }
    }

    fn messages_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.messages.up(),
            KeyCode::Down => self.messages.down(),
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Enter => self.messages.close_selected(),
            KeyCode::Esc => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn open_path(&mut self, path: &Path, preview: bool) {
        if is_image_path(path) {
            if !preview {
                self.open_image(path);
            }
            return;
        }
        match self.editor.open(path, preview) {
            Ok(()) => {
                if !preview {
                    self.editor.promote_active();
                }
                self.status = format!("Opened {}", path.display());
            }
            Err(e) => self.messages.error(format!("Open failed: {e}")),
        }
    }

    // ----- position history (Alt+Left / Alt+Right) -----------------------

    fn current_location(&self) -> Option<Location> {
        let tab = self.editor.active_tab()?;
        let path = tab.path.clone()?;
        let (line, col) = tab.cursor_1based();
        Some(Location { path, line, col })
    }

    /// Run a cursor-moving jump `f`, recording the origin and destination in the
    /// position history so Alt+Left/Right can revisit them.
    fn with_jump<F: FnOnce(&mut Self)>(&mut self, f: F) {
        let origin = self.current_location();
        f(self);
        let dest = self.current_location();
        // Drop any forward history, then append origin then destination.
        if !self.nav_history.is_empty() {
            self.nav_history.truncate(self.nav_idx + 1);
        }
        if let Some(o) = origin {
            if self.nav_history.last() != Some(&o) {
                self.nav_history.push(o);
            }
        }
        if let Some(d) = dest {
            if self.nav_history.last() != Some(&d) {
                self.nav_history.push(d);
            }
        }
        self.nav_idx = self.nav_history.len().saturating_sub(1);
    }

    fn nav_back(&mut self) {
        if self.nav_history.is_empty() || self.nav_idx == 0 {
            self.status = "No earlier position".into();
            return;
        }
        self.nav_idx -= 1;
        self.navigate_to(self.nav_history[self.nav_idx].clone());
    }

    fn nav_forward(&mut self) {
        if self.nav_idx + 1 >= self.nav_history.len() {
            self.status = "No later position".into();
            return;
        }
        self.nav_idx += 1;
        self.navigate_to(self.nav_history[self.nav_idx].clone());
    }

    /// Go to a recorded location without itself recording a new jump.
    fn navigate_to(&mut self, loc: Location) {
        self.open_path(&loc.path, false);
        self.editor.goto(loc.line, Some(loc.col), self.editor_view());
        self.focus = Focus::Editor;
        self.status = format!("{}:{}", loc.path.display(), loc.line);
    }

    fn open_image(&mut self, path: &Path) {
        let Some(picker) = self.picker.as_ref() else {
            self.messages
                .warn("Image preview needs a graphics-capable terminal");
            return;
        };
        match decode_image(path) {
            Ok(img) => {
                let proto = picker.new_resize_protocol(img);
                self.editor.open_image(path, proto);
                self.focus = Focus::Editor;
                self.status = format!("Opened image {}", path.display());
            }
            Err(e) => self.messages.error(format!("Image open failed: {e}")),
        }
    }

    // ----- mouse ----------------------------------------------------------

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        // Overlays swallow mouse input rather than acting on panes underneath.
        if self.show_help || self.palette.is_some() || self.prompt.is_some() {
            return;
        }
        let (col, row) = (mouse.column, mouse.row);

        if rect_contains(self.layout.menu, col, row) {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.menu_click(col);
            }
            return;
        }
        if self.show_explorer && rect_contains(self.layout.explorer, col, row) {
            self.explorer_mouse(mouse);
            return;
        }
        if rect_contains(self.layout.tabs, col, row) {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.tab_click(col);
            }
            return;
        }
        if rect_contains(self.layout.editor, col, row) {
            self.editor_mouse(mouse);
            return;
        }
        if self.show_messages && rect_contains(self.layout.messages, col, row) {
            self.messages_mouse(mouse);
        }
    }

    fn editor_mouse(&mut self, mouse: MouseEvent) {
        self.focus = Focus::Editor;
        let area = self.layout.editor;
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            if let Some(t) = self.editor.active_tab_mut() {
                t.preview = false;
            }
        }
        if let Some(t) = self.editor.active_tab_mut() {
            let _ = t.editor.mouse(mouse, &area);
        }
    }

    fn explorer_mouse(&mut self, mouse: MouseEvent) {
        let inner_top = self.layout.explorer.y + 1; // inside the border
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.explorer.up();
            }
            MouseEventKind::ScrollDown => {
                self.explorer.down();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::Explorer;
                if mouse.row < inner_top {
                    return;
                }
                let idx = self.explorer.top + (mouse.row - inner_top) as usize;
                if idx < self.explorer.nodes.len() {
                    let was_selected = self.explorer.selected == idx;
                    self.explorer.selected = idx;
                    let is_dir = self.explorer.nodes[idx].is_dir;
                    if was_selected && !is_dir {
                        // Second click on the same file promotes to a real tab.
                        self.open_or_expand_selected();
                    } else if is_dir {
                        self.explorer.toggle_selected();
                    } else {
                        self.preview_selected();
                    }
                }
            }
            _ => {}
        }
    }

    fn messages_mouse(&mut self, mouse: MouseEvent) {
        let area = self.layout.messages;
        let inner_top = area.y + 1;
        match mouse.kind {
            MouseEventKind::ScrollUp => self.messages.up(),
            MouseEventKind::ScrollDown => self.messages.down(),
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::Messages;
                if mouse.row >= inner_top {
                    let idx = (mouse.row - inner_top) as usize;
                    if idx < self.messages.items.len() {
                        self.messages.selected = idx;
                        // Clicking near the right edge hits the close "x".
                        if mouse.column >= area.x + area.width.saturating_sub(3) {
                            self.messages.close_selected();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn tab_click(&mut self, col: u16) {
        let mut x = self.layout.tabs.x + 1;
        for (i, tab) in self.editor.tabs.iter().enumerate() {
            let w = tab.title().chars().count() as u16;
            if col >= x && col < x + w {
                self.editor.active = i;
                self.editor.promote_active();
                self.focus = Focus::Editor;
                return;
            }
            x += w + 3; // title + " │ " divider
        }
    }

    fn menu_click(&mut self, col: u16) {
        let mut x = self.layout.menu.x + 1;
        for (i, m) in MENUS.iter().enumerate() {
            let w = m.name.chars().count() as u16 + 2;
            if col >= x && col < x + w {
                self.menu.open_index(i);
                return;
            }
            x += w;
        }
    }

    // ----- menu -----------------------------------------------------------

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.menu.left(),
            KeyCode::Right => self.menu.right(),
            KeyCode::Up => self.menu.up(),
            KeyCode::Down => self.menu.down(),
            KeyCode::Enter => {
                if let Some(action) = self.menu.selected_action() {
                    self.menu.close();
                    self.run_action(action);
                }
            }
            KeyCode::Esc | KeyCode::F(10) => self.menu.close(),
            _ => {}
        }
    }

    // ----- command palette ------------------------------------------------

    fn open_palette(&mut self) {
        self.build_file_index();
        self.palette = Some(Palette::new());
        self.recompute_palette();
    }

    fn build_file_index(&mut self) {
        let mut out = Vec::new();
        Self::walk_files(&self.root, &mut out, 0);
        self.file_index = out;
    }

    fn walk_files(dir: &std::path::Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 12 || out.len() > 5000 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                Self::walk_files(&path, out, depth + 1);
            } else {
                out.push(path);
            }
        }
    }

    fn recompute_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let mode = p.mode();
        let query = p.query().to_string();
        let mut entries: Vec<Entry> = Vec::new();
        match mode {
            PMode::Files => {
                let (qpath, target) = palette::parse_path_target(&query);
                for path in &self.file_index {
                    let rel = path
                        .strip_prefix(&self.root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    if qpath.is_empty() || palette::fuzzy_match(&rel, &qpath) {
                        entries.push(Entry {
                            label: rel,
                            action: PAction::OpenFile(path.clone(), target),
                        });
                    }
                    if entries.len() >= 200 {
                        break;
                    }
                }
            }
            PMode::Commands => {
                for (label, action) in palette::COMMANDS {
                    if query.is_empty() || palette::fuzzy_match(label, &query) {
                        entries.push(Entry {
                            label: format!("> {label}"),
                            action: PAction::RunCommand((*action).to_string()),
                        });
                    }
                }
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
        }
        if let Some(p) = self.palette.as_mut() {
            if p.selected >= entries.len() {
                p.selected = entries.len().saturating_sub(1);
            }
            p.entries = entries;
        }
    }

    fn palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.palette = None,
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
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.palette.as_mut() {
                    p.insert(c);
                }
                self.recompute_palette();
            }
            _ => {}
        }
    }

    fn accept_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(entry) = p.selected_entry().cloned() else {
            return;
        };
        self.palette = None;
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
            PAction::RunCommand(action) => self.run_action(&action),
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

    // ----- search / replace ----------------------------------------------

    fn start_search(&mut self, replacing: bool) {
        self.search = Some(SearchBar::new(replacing));
    }

    /// Whether typing in the search box should live-preview the next match.
    /// Interactive (query-replace) mode keeps the cursor put until it begins.
    fn search_should_preview(&self) -> bool {
        self.search
            .as_ref()
            .map(|s| !s.interactive && s.field == Field::Query)
            .unwrap_or(false)
    }

    /// Recompute and apply search-highlight marks for the active buffer, then
    /// move the cursor to the next/previous match.
    fn find_step(&mut self, forward: bool) {
        let Some(pat) = self.search.as_ref().and_then(|s| s.pattern()) else {
            self.clear_marks();
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = format!("bad regex: {e}");
                }
                return;
            }
        };
        let area = self.editor_view();
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let content = t.text();
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for m in re.find_iter(&content) {
            let code = t.editor.code_ref();
            matches.push((code.byte_to_char(m.start()), code.byte_to_char(m.end())));
        }
        if matches.is_empty() {
            t.editor.remove_marks();
            if let Some(s) = self.search.as_mut() {
                s.status = "no matches".into();
            }
            return;
        }
        let marks: Vec<(usize, usize, &str)> =
            matches.iter().map(|(s, e)| (*s, *e, SEARCH_MARK)).collect();
        t.editor.set_marks(marks);

        let cur = t.editor.get_cursor();
        let target = if forward {
            matches
                .iter()
                .find(|(s, _)| *s > cur)
                .copied()
                .unwrap_or(matches[0])
        } else {
            matches
                .iter()
                .rev()
                .find(|(s, _)| *s < cur)
                .copied()
                .unwrap_or(*matches.last().unwrap())
        };
        t.editor.set_cursor(target.0);
        t.editor.set_selection(Some(Selection::new(target.0, target.1)));
        t.editor.focus(&area);
        if let Some(s) = self.search.as_mut() {
            s.status = format!("{} matches", matches.len());
        }
    }

    fn clear_marks(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.remove_marks();
        }
    }

    fn end_search(&mut self) {
        self.clear_marks();
        self.search = None;
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.end_search(),
            KeyCode::F(3) if Self::shift(&key) => self.find_step(false),
            KeyCode::F(3) => self.find_step(true),
            KeyCode::Tab => {
                if let Some(s) = self.search.as_mut() {
                    s.toggle_field();
                }
            }
            KeyCode::Enter => {
                let interactive = self.search.as_ref().map(|s| s.interactive).unwrap_or(false);
                let replacing = self.search.as_ref().map(|s| s.replacing).unwrap_or(false);
                let on_replace_field =
                    self.search.as_ref().map(|s| s.field) == Some(Field::Replace);
                if interactive {
                    self.begin_query_replace();
                } else if replacing && (Self::alt(&key) || on_replace_field) {
                    self.replace_all();
                } else {
                    self.find_step(true);
                }
            }
            KeyCode::Backspace => {
                if let Some(s) = self.search.as_mut() {
                    s.active_field_mut().pop();
                }
                if self.search_should_preview() {
                    self.find_step(true);
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(s) = self.search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => s.case_sensitive = !s.case_sensitive,
                        'w' => s.whole_word = !s.whole_word,
                        'r' => s.regex = !s.regex,
                        _ => {}
                    }
                }
                // Toggles never move the cursor while in interactive mode.
                if self.search.as_ref().map(|s| !s.interactive).unwrap_or(false) {
                    self.find_step(true);
                }
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.search.as_mut() {
                    s.active_field_mut().push(c);
                }
                if self.search_should_preview() {
                    self.find_step(true);
                }
            }
            _ => {}
        }
    }

    fn replace_all(&mut self) {
        let Some(sb) = self.search.as_ref() else {
            return;
        };
        let Some(pat) = sb.pattern() else {
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = format!("bad regex: {e}");
                }
                return;
            }
        };
        let use_regex = sb.regex;
        let replacement = sb.replace.clone();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.text();
        let count = re.find_iter(&text).count();
        let new_text = if use_regex {
            let rep = unescape(&replacement);
            re.replace_all(&text, rep.as_str()).into_owned()
        } else {
            re.replace_all(&text, regex::NoExpand(&replacement)).into_owned()
        };
        tab.editor.set_content(&new_text);
        tab.editor.remove_marks();
        tab.dirty = true;
        tab.preview = false;
        if let Some(s) = self.search.as_mut() {
            s.status = format!("replaced {count}");
        }
    }

    // ----- project-wide search / replace ---------------------------------

    fn open_project_search(&mut self, replacing: bool) {
        self.build_file_index();
        self.project_search = Some(ProjectSearch::new(replacing));
        self.run_project_search();
    }

    /// The current text for a path: the open buffer if one points at it (so
    /// unsaved edits are searched), otherwise the file on disk.
    fn current_text(&self, path: &Path) -> Option<String> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(tab) = self
            .editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(canon.as_path()))
        {
            return Some(tab.text());
        }
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > 2_000_000 {
            return None; // skip very large files
        }
        std::fs::read_to_string(path).ok()
    }

    fn run_project_search(&mut self) {
        let Some(ps) = self.project_search.as_ref() else {
            return;
        };
        let Some(pat) = ps.pattern() else {
            if let Some(p) = self.project_search.as_mut() {
                p.hits.clear();
                p.selected = 0;
                p.status = "Type to search the project (2+ characters).".into();
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.project_search.as_mut() {
                    p.status = format!("bad regex: {e}");
                }
                return;
            }
        };

        let mut hits: Vec<Hit> = Vec::new();
        let mut files = 0usize;
        'outer: for path in &self.file_index {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let mut file_had_hit = false;
            for (i, line) in content.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    file_had_hit = true;
                    let clipped: String = line.trim_start().chars().take(120).collect();
                    hits.push(Hit {
                        path: path.clone(),
                        line: i + 1,
                        col: m.start() + 1,
                        display: format!("{rel}:{}: {clipped}", i + 1),
                    });
                    if hits.len() >= 5000 {
                        break 'outer;
                    }
                }
            }
            if file_had_hit {
                files += 1;
            }
        }

        if let Some(p) = self.project_search.as_mut() {
            p.status = if hits.is_empty() {
                "No matches".into()
            } else {
                format!("{} matches in {} files", hits.len(), files)
            };
            if p.selected >= hits.len() {
                p.selected = hits.len().saturating_sub(1);
            }
            p.hits = hits;
        }
    }

    fn project_replace_all(&mut self) {
        let Some(ps) = self.project_search.as_ref() else {
            return;
        };
        if !ps.replacing {
            return;
        }
        let Some(pat) = ps.pattern() else {
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.project_search.as_mut() {
                    p.status = format!("bad regex: {e}");
                }
                return;
            }
        };
        let use_regex = ps.regex;
        let replacement = ps.replace.clone();

        // Unique set of files that currently have hits.
        let mut paths: Vec<PathBuf> = ps.hits.iter().map(|h| h.path.clone()).collect();
        paths.sort();
        paths.dedup();

        let mut replaced = 0usize;
        let mut files = 0usize;
        for path in &paths {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let count = re.find_iter(&content).count();
            if count == 0 {
                continue;
            }
            let new = if use_regex {
                let rep = unescape(&replacement);
                re.replace_all(&content, rep.as_str()).into_owned()
            } else {
                re.replace_all(&content, regex::NoExpand(&replacement)).into_owned()
            };
            if new == content {
                continue;
            }
            if let Err(e) = std::fs::write(path, &new) {
                self.messages.error(format!("Write failed for {}: {e}", path.display()));
                continue;
            }
            // Keep any open buffer in sync (and clean, since we just saved).
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            for tab in &mut self.editor.tabs {
                if tab.path.as_deref() == Some(canon.as_path()) {
                    tab.editor.set_content(&new);
                    tab.dirty = false;
                }
            }
            replaced += count;
            files += 1;
        }

        self.run_project_search();
        if let Some(p) = self.project_search.as_mut() {
            p.status = format!("Replaced {replaced} in {files} files");
        }
    }

    fn ps_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.project_search = None,
            KeyCode::Up => {
                if let Some(p) = self.project_search.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.project_search.as_mut() {
                    p.down();
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self.project_search.as_mut() {
                    p.toggle_field();
                }
            }
            KeyCode::Enter => {
                let replacing = self.project_search.as_ref().map(|p| p.replacing).unwrap_or(false);
                let on_replace = self.project_search.as_ref().map(|p| p.field) == Some(Field::Replace);
                if replacing && (Self::alt(&key) || on_replace) {
                    self.project_replace_all();
                } else {
                    self.open_selected_hit();
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.project_search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => p.case_sensitive = !p.case_sensitive,
                        'r' => p.regex = !p.regex,
                        _ => {}
                    }
                }
                self.run_project_search();
            }
            KeyCode::Backspace => {
                let in_query = self.project_search.as_ref().map(|p| p.field) == Some(Field::Query);
                if let Some(p) = self.project_search.as_mut() {
                    p.active_field_mut().pop();
                }
                if in_query {
                    self.run_project_search();
                }
            }
            KeyCode::Char(c) => {
                let in_query = self.project_search.as_ref().map(|p| p.field) == Some(Field::Query);
                if let Some(p) = self.project_search.as_mut() {
                    p.active_field_mut().push(c);
                }
                if in_query {
                    self.run_project_search();
                }
            }
            _ => {}
        }
    }

    fn open_selected_hit(&mut self) {
        let target = self
            .project_search
            .as_ref()
            .and_then(|p| p.selected_hit())
            .map(|h| (h.path.clone(), h.line, h.col));
        if let Some((path, line, col)) = target {
            self.project_search = None;
            self.with_jump(|s| {
                s.open_path(&path, false);
                let area = s.editor_view();
                s.editor.goto(line, Some(col), area);
                s.focus = Focus::Editor;
            });
        }
    }

    // ----- interactive query-replace -------------------------------------

    fn begin_query_replace(&mut self) {
        let Some(sb) = self.search.as_ref() else {
            return;
        };
        let Some(pat) = sb.pattern() else {
            if let Some(s) = self.search.as_mut() {
                s.status = "type something to find".into();
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = format!("bad regex: {e}");
                }
                return;
            }
        };
        let template = if sb.regex {
            unescape(&sb.replace)
        } else {
            sb.replace.clone()
        };
        let regex = sb.regex;
        let label = sb.query.clone();
        self.search = None;

        let area = self.editor_view();
        let found = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            let from = t.editor.get_cursor();
            match next_match_from(t, &re, from) {
                Some((cs, ce)) => {
                    highlight_match(t, cs, ce, area);
                    Some((cs, ce))
                }
                None => None,
            }
        };
        match found {
            Some(current) => {
                self.query_replace = Some(QueryReplace {
                    re,
                    template,
                    regex,
                    current,
                    replaced: 0,
                    label,
                });
                self.status = "Query replace — y replace  n skip  ! rest  q quit".into();
            }
            None => self.status = "Query replace: no matches".into(),
        }
    }

    fn qr_key(&mut self, key: KeyEvent) {
        let decision = match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char(' ') => Decision::Replace,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Delete => Decision::Skip,
            KeyCode::Char('!') => Decision::ReplaceRest,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc | KeyCode::Enter => {
                Decision::Quit
            }
            _ => return,
        };
        self.qr_apply(decision);
    }

    fn qr_apply(&mut self, decision: Decision) {
        let area = self.editor_view();
        let Some(qr) = self.query_replace.as_ref() else {
            return;
        };
        let re = qr.re.clone();
        let template = qr.template.clone();
        let regex = qr.regex;
        let (cs, ce) = qr.current;
        let mut replaced = qr.replaced;

        let next = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            let result = match decision {
                Decision::Quit => {
                    t.editor.remove_marks();
                    t.editor.set_selection(None);
                    None
                }
                Decision::Skip => {
                    t.editor.remove_marks();
                    next_match_from(t, &re, ce)
                }
                Decision::Replace => {
                    let resume = do_replace(t, &re, regex, &template, (cs, ce));
                    replaced += 1;
                    t.dirty = true;
                    t.preview = false;
                    t.editor.remove_marks();
                    next_match_from(t, &re, resume)
                }
                Decision::ReplaceRest => {
                    let mut cur = (cs, ce);
                    let mut guard = 0usize;
                    loop {
                        let resume = do_replace(t, &re, regex, &template, cur);
                        replaced += 1;
                        guard += 1;
                        if guard > 1_000_000 {
                            break;
                        }
                        match next_match_from(t, &re, resume) {
                            Some(m) if m.0 >= resume => cur = m,
                            _ => break,
                        }
                    }
                    t.dirty = true;
                    t.preview = false;
                    t.editor.remove_marks();
                    t.editor.set_selection(None);
                    None
                }
            };
            if let Some((ns, ne)) = result {
                highlight_match(t, ns, ne, area);
            }
            result
        };

        match next {
            Some(current) => {
                if let Some(q) = self.query_replace.as_mut() {
                    q.current = current;
                    q.replaced = replaced;
                }
            }
            None => {
                self.query_replace = None;
                self.status = format!("Query replace: replaced {replaced}");
            }
        }
    }

    // ----- prompt (Open / Save As) ---------------------------------------

    fn prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => self.accept_prompt(),
            KeyCode::Backspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.input.push(c);
                }
            }
            _ => {}
        }
    }

    fn accept_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt.kind {
            PromptKind::Open => {
                let (path, target) = palette::parse_path_target(prompt.input.trim());
                if path.is_empty() {
                    return;
                }
                let path = self.resolve(&path);
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    if let Some((line, col)) = target {
                        let area = s.editor_view();
                        s.editor.goto(line, Some(col), area);
                    }
                    s.focus = Focus::Editor;
                });
            }
            PromptKind::SaveAs => {
                let raw = prompt.input.trim();
                if raw.is_empty() {
                    return;
                }
                let path = self.resolve(raw);
                match self.editor.save_active_as(path) {
                    Ok(p) => {
                        self.status = format!("Saved {}", p.display());
                        self.explorer.rebuild();
                    }
                    Err(e) => self.messages.error(format!("Save failed: {e}")),
                }
            }
        }
    }

    fn resolve(&self, input: &str) -> PathBuf {
        let p = PathBuf::from(input);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }

    /// Persist settings on exit; failures become a status message only.
    pub fn on_exit(&mut self) {
        if let Err(e) = self.settings.save() {
            self.messages
                .push(Level::Warn, format!("Could not save settings: {e}"));
        }
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Decode an image file into a `DynamicImage` the picker can turn into a
/// terminal protocol.
fn decode_image(path: &Path) -> Result<image::DynamicImage, String> {
    image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}

/// First match whose (char) start is at or after `from_char`, as char offsets.
fn next_match_from(t: &Tab, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    let content = t.text();
    let code = t.editor.code_ref();
    let bfrom = code.char_to_byte(from_char);
    for m in re.find_iter(&content) {
        if m.start() >= bfrom {
            return Some((code.byte_to_char(m.start()), code.byte_to_char(m.end())));
        }
    }
    None
}

/// Replace the single match at char offset `current.0` in `t`, returning the
/// char offset just past the inserted text (where searching should resume).
fn do_replace(t: &mut Tab, re: &Regex, regex: bool, template: &str, current: (usize, usize)) -> usize {
    let content = t.text();
    let bstart = t.editor.code_ref().char_to_byte(current.0);
    let mut expansion: Option<(usize, usize, String)> = None;
    for caps in re.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        if m.start() == bstart {
            let mut out = String::new();
            if regex {
                caps.expand(template, &mut out);
            } else {
                out.push_str(template);
            }
            expansion = Some((m.start(), m.end(), out));
            break;
        }
        if m.start() > bstart {
            break;
        }
    }
    match expansion {
        Some((bs, be, exp)) => {
            let mut new = String::with_capacity(content.len() + exp.len());
            new.push_str(&content[..bs]);
            new.push_str(&exp);
            new.push_str(&content[be..]);
            let resume = current.0 + exp.chars().count();
            t.editor.set_content(&new);
            resume
        }
        None => current.1,
    }
}

/// Move the cursor to a match, select it, and add a search highlight mark.
fn highlight_match(t: &mut Tab, cs: usize, ce: usize, area: Rect) {
    t.editor.set_cursor(cs);
    t.editor.set_selection(Some(Selection::new(cs, ce)));
    t.editor.set_marks(vec![(cs, ce, SEARCH_MARK)]);
    t.editor.focus(&area);
}

/// Minimum editor width handed to the code editor. Its `focus()` computes
/// `visible_width - 10` and underflows (panics) when the area is narrower than
/// the line-number gutter plus that step, so we never pass it a smaller width.
const MIN_EDITOR_WIDTH: u16 = 20;

/// Interpret `\n`, `\t`, `\r`, `\\` escapes in a regex replacement template,
/// leaving `$group` references intact for the regex engine to expand.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_project_search_panel_with_hits() {
        let dir = std::env::temp_dir().join(format!("stride-ps-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), "the needle is here\n").unwrap();

        let mut app = App::new(dir.clone());
        app.run_action("search.project");
        for c in "needle".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.project_search.as_ref().unwrap().hits.len(), 1);

        let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
        terminal.draw(|f| crate::ui::draw(&mut app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Search in Project"), "panel title rendered");
        assert!(text.contains("needle"), "the matching line is shown");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renders_image_tab_without_panic() {
        let mut app = App::new(std::env::temp_dir());
        // Halfblocks renders into a plain cell buffer — no real terminal needed.
        let picker = Picker::halfblocks();
        let img = image::DynamicImage::new_rgb8(8, 8);
        let proto = picker.new_resize_protocol(img);
        app.editor.open_image(Path::new("/tmp/stride-test.png"), proto);
        assert!(app.editor.active_tab().unwrap().is_image());

        // A full draw of the image tab must not panic.
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::ui::draw(&mut app, f)).unwrap();

        // Editing keys are ignored on an image tab.
        app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!app.editor.active_tab().unwrap().dirty);
    }
}
