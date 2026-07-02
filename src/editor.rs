//! Tabbed editor: a stack of buffers, each backed by an `editor_core`
//! widget (Tree-sitter syntax highlighting, history, selection, clipboard).
//!
//! The code editor addresses the cursor as a flat character offset; this module
//! converts to/from 1-based line/column for the status bar and go-to-line.

#![warn(clippy::pedantic)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::Style;
use crate::editor_core::actions::{Delete, Duplicate, InsertText, MoveDown, MoveRight, MoveUp, SelectAll};
use crate::editor_core::code::Code;
pub use crate::editor_core::editor::Editor as CodeEditor;
use crate::editor_core::utils::get_lang;
use ratatui_image::protocol::StatefulProtocol;

use crate::theme;

/// Whether line `row` is blank — empty or only whitespace. Out-of-range rows are
/// treated as blank.
fn line_is_blank(code: &Code, row: usize) -> bool {
    if row >= code.len_lines() {
        return true;
    }
    let start = code.line_to_char(row);
    let len = code.line_len(row);
    code.char_slice(start, start + len).chars().all(|c: char| c.is_whitespace())
}

/// Whether `row` is part of a section break: a run of two or more consecutive
/// blank lines. `n` is the total line count.
fn is_section_break(code: &Code, row: usize, n: usize) -> bool {
    line_is_blank(code, row)
        && ((row > 0 && line_is_blank(code, row - 1))
            || (row + 1 < n && line_is_blank(code, row + 1)))
}

/// Char offsets where each word begins: a word char whose predecessor is not a
/// word char. Used by the Go → Word navigation.
fn word_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut prev_word = false;
    for (i, c) in text.chars().enumerate() {
        let word = is_word_char(c);
        if word && !prev_word {
            starts.push(i);
        }
        prev_word = word;
    }
    starts
}

/// Char offsets where each sentence begins: the first non-space char, then the
/// first non-space char after any `.`/`!`/`?` (plus trailing quotes/brackets)
/// followed by whitespace. Used by the Go → Sentence navigation.
fn sentence_starts(text: &str) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    if i < n {
        starts.push(i);
    }
    while i < n {
        if matches!(chars[i], '.' | '!' | '?') {
            let mut j = i + 1;
            while j < n && matches!(chars[j], '.' | '!' | '?' | '"' | '\'' | ')' | ']' | '}') {
                j += 1;
            }
            if j < n && chars[j].is_whitespace() {
                while j < n && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < n {
                    starts.push(j);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    starts.dedup();
    starts
}

/// On-save text-normalization options (from [`crate::settings::Settings`]).
#[derive(Clone, Copy)]
pub struct SaveOptions {
    /// Strip trailing spaces/tabs from every line.
    pub trim_trailing_whitespace: bool,
    /// Append a final newline if the (non-empty) file lacks one.
    pub ensure_final_newline: bool,
}

/// Strip trailing spaces and tabs from each line, preserving the line structure
/// (including any final newline and any `\r` before a `\n`).
fn trim_trailing_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.split('\n');
    if let Some(first) = lines.next() {
        out.push_str(first.trim_end_matches([' ', '\t']));
    }
    for line in lines {
        out.push('\n');
        out.push_str(line.trim_end_matches([' ', '\t']));
    }
    out
}

/// File extensions opened as images rather than text.
#[must_use] 
pub fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tiff" | "tif")
    )
}

/// Marker passed to the code editor for search hits. The monochrome theme
/// renders marks as underlines, so the specific value is not shown as a color.
pub const SEARCH_MARK: &str = "search";

/// Apply the current theme's styles to a code editor.
///
/// With a custom JSON theme active, the editor uses its per-region foreground,
/// its syntax colors, and its cursor color; otherwise everything is monochrome
/// (foreground only, no token colors) and the cursor is a reversed block.
fn apply_theme(ed: &mut CodeEditor) {
    ed.set_text_style(
        Style::default()
            .fg(theme::region_fg(theme::Region::Editor))
            .add_modifier(theme::region_modifiers(theme::Region::Editor)),
    );
    ed.set_line_number_style(theme::dim());
    ed.set_selection_style(theme::selected());
    ed.set_whitespace_style(theme::dim());
    ed.set_bracket_style(theme::selected());

    // Syntax token colors from the active custom theme (empty == monochrome).
    let syntax = theme::syntax_theme();
    let pairs: Vec<(&str, &str)> = syntax.iter().map(|(t, hex)| (*t, hex.as_str())).collect();
    ed.set_syntax_theme(&pairs);

    // Block cursor: the custom theme's cursor color (drawn as the cell bg, with
    // the editor background as fg so the glyph stays legible), else reversed.
    let cursor = match theme::editor_cursor() {
        Some(color) => Style::default().bg(color).fg(theme::region_bg(theme::Region::Editor)),
        None => theme::selected(),
    };
    ed.set_cursor_style(Some(cursor));
}

fn make_editor(
    path: Option<&Path>,
    text: &str,
    line_numbers: bool,
    show_whitespace: bool,
    soft_wrap: bool,
    indent: &str,
) -> CodeEditor {
    let name = path
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lang = if name.is_empty() {
        "text".to_string()
    } else {
        get_lang(&name)
    };
    // An empty syntax theme keeps the editor monochrome (no token colors), as the
    // theme spec requires. `CodeEditor::new` falls back to plain "text" on an
    // unknown grammar.
    let mut ed = CodeEditor::new(&lang, text, Vec::new())
        .or_else(|_| CodeEditor::new("text", text, Vec::new()))
        .expect("code editor init for plain text never fails");
    ed.show_line_numbers(line_numbers);
    ed.show_whitespace(show_whitespace);
    ed.set_soft_wrap(soft_wrap);
    ed.set_indent(Some(indent.to_string()));
    apply_theme(&mut ed);
    ed
}

/// Build a small, theme-styled, single-line text field holding `content`. Used by
/// the Vix menu's Website/Email dialogs so the text is selectable and copyable
/// from inside the TUI (select with the mouse or keyboard, then `Ctrl+C`).
///
/// # Panics
///
/// Never in practice: the underlying `"text"` grammar always initializes.
#[must_use]
pub fn text_field(content: &str) -> CodeEditor {
    let mut ed = CodeEditor::new("text", content, Vec::new())
        .expect("text field init for plain text never fails");
    ed.show_line_numbers(false);
    apply_theme(&mut ed);
    ed
}

/// One open buffer shown as one tab.
pub struct Tab {
    /// The underlying code-editor widget/state.
    pub editor: CodeEditor,
    /// File path backing this buffer, or `None` for an untitled buffer.
    pub path: Option<PathBuf>,
    /// Set when the buffer has unsaved edits.
    pub dirty: bool,
    /// Ephemeral preview tab (single-click / arrow-scan from the explorer).
    pub preview: bool,
    /// When set, this tab shows an image instead of the text editor.
    pub image: Option<StatefulProtocol>,
}

impl Tab {
    /// Whether this tab displays an image rather than editable text.
    pub fn is_image(&self) -> bool {
        self.image.is_some()
    }

    /// Whole buffer text.
    pub fn text(&self) -> String {
        self.editor.get_content()
    }

    /// Buffer split into lines (no trailing empty element).
    pub fn lines(&self) -> Vec<String> {
        self.text().lines().map(std::string::ToString::to_string).collect()
    }

    /// Title shown on the tab and in the buffer switcher.
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name()).map_or_else(|| "untitled".to_string(), |s| s.to_string_lossy().into_owned());
        let icon = theme::file_icon(&name);
        let flag = if self.dirty {
            format!(" {}", theme::icon::FILE_DIRTY)
        } else {
            String::new()
        };
        format!("{icon} {name}{flag}")
    }

    /// Full path for display (status bar / buffer switcher), or `"untitled"`.
    pub fn display_path(&self) -> String {
        self.path
            .as_ref().map_or_else(|| "untitled".to_string(), |p| p.display().to_string())
    }

    /// 1-based (line, column) of the cursor.
    pub fn cursor_1based(&self) -> (usize, usize) {
        let cur = self.editor.get_cursor();
        let code = self.editor.code_ref();
        let row = code.char_to_line(cur);
        let col = cur - code.line_to_char(row);
        (row + 1, col + 1)
    }

    /// Total line count of this buffer (for the scrollbar).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.editor.code_ref().len_lines()
    }
}

/// Orientation of an editor split.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitDir {
    /// Two panes side by side.
    Vertical,
    /// Two panes stacked top/bottom.
    Horizontal,
}

/// The tab strip: a stack of open buffers and the active index.
// Several independent display toggles; a flags struct would just relocate the lint.
#[allow(clippy::struct_excessive_bools)]
pub struct Editor {
    /// Open buffers, left to right.
    pub tabs: Vec<Tab>,
    /// Index of the active tab.
    pub active: usize,
    /// Whether the line-number gutter is shown.
    pub line_numbers: bool,
    /// Whether line numbers are shown relative to the cursor line.
    pub relative_line_numbers: bool,
    /// Whether visible-whitespace glyphs are shown.
    pub show_whitespace: bool,
    /// Whether long lines soft-wrap.
    pub soft_wrap: bool,
    /// String Tab inserts in every buffer (spaces or a tab).
    pub indent: String,
    /// Split layout (a binary tree of panes), when the editor area is divided.
    /// `None` is the normal single-pane case.
    pub split_root: Option<crate::pane_tree::Pane>,
    /// In-order index of the focused leaf within `split_root` (0 when unsplit).
    pub focused_leaf: usize,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new(true, false, false, "    ".to_string())
    }
}

impl Editor {
    /// Create an editor with one empty buffer and the given gutter / whitespace /
    /// soft-wrap / indentation settings.
    #[must_use]
    pub fn new(line_numbers: bool, show_whitespace: bool, soft_wrap: bool, indent: String) -> Self {
        let mut e = Editor {
            tabs: Vec::new(),
            active: 0,
            line_numbers,
            relative_line_numbers: false,
            show_whitespace,
            soft_wrap,
            indent,
            split_root: None,
            focused_leaf: 0,
        };
        e.new_tab();
        e
    }

    /// The active tab, if any.
    #[must_use]
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// Mutable access to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// Whether the editor area is split into more than one pane.
    #[must_use]
    pub fn is_split(&self) -> bool {
        self.split_root.is_some()
    }

    /// Split the focused pane in `dir`, showing another tab (the next one, else
    /// the active tab) in the new pane. Creates the split on the first call; later
    /// calls split the currently focused pane. Capped at `MAX_LEAVES` panes.
    pub fn set_split(&mut self, dir: SplitDir) {
        let other = (0..self.tabs.len()).find(|&i| i != self.active).unwrap_or(self.active);
        match self.split_root.as_mut() {
            None => {
                self.split_root = Some(crate::pane_tree::Pane::Split {
                    dir,
                    ratio: 50,
                    first: Box::new(crate::pane_tree::Pane::Leaf(self.active)),
                    second: Box::new(crate::pane_tree::Pane::Leaf(other)),
                });
                self.focused_leaf = 0;
            }
            Some(root) => {
                root.set_leaf_tab(self.focused_leaf, self.active);
                root.split_leaf(self.focused_leaf, dir, other);
            }
        }
    }

    /// Remove the focused pane, collapsing the split; a single remaining pane
    /// returns to the unsplit state.
    pub fn unsplit(&mut self) {
        let Some(root) = self.split_root.take() else { return };
        match root.remove_leaf(self.focused_leaf) {
            Some(tree) if tree.leaf_count() > 1 => {
                self.focused_leaf = self.focused_leaf.min(tree.leaf_count() - 1);
                if let Some(tab) = tree.leaf_tab(self.focused_leaf) {
                    self.active = tab.min(self.tabs.len().saturating_sub(1));
                }
                self.split_root = Some(tree);
            }
            Some(tree) => {
                if let Some(tab) = tree.leaf_tab(0) {
                    self.active = tab.min(self.tabs.len().saturating_sub(1));
                }
                self.split_root = None;
                self.focused_leaf = 0;
            }
            None => {
                self.split_root = None;
                self.focused_leaf = 0;
            }
        }
    }

    /// Sync the focused leaf to the active tab and clamp every leaf to the open
    /// tabs. Call before reading the layout so the focused pane follows `active`.
    fn sync_split(&mut self) {
        let count = self.tabs.len();
        if let Some(root) = self.split_root.as_mut() {
            root.set_leaf_tab(self.focused_leaf, self.active);
            root.clamp_tabs(count);
            self.focused_leaf = self.focused_leaf.min(root.leaf_count().saturating_sub(1));
        }
    }

    /// Focus the leaf at in-order index `i`, making its tab active.
    pub fn focus_leaf(&mut self, i: usize) {
        self.sync_split();
        let Some(root) = self.split_root.as_ref() else { return };
        if let Some(tab) = root.leaf_tab(i) {
            self.focused_leaf = i;
            self.active = tab.min(self.tabs.len().saturating_sub(1));
        }
    }

    /// Move focus to the next pane (wrapping), making its tab active.
    pub fn focus_other_pane(&mut self) {
        self.sync_split();
        let Some(root) = self.split_root.as_ref() else { return };
        let n = root.leaf_count();
        if n > 0 {
            self.focus_leaf((self.focused_leaf + 1) % n);
        }
    }

    /// The in-order index of the focused pane.
    #[must_use]
    pub fn focused_leaf(&self) -> usize {
        self.focused_leaf
    }

    /// Lay out the split panes into `area` (focused leaf synced to `active`).
    /// Empty when unsplit.
    pub fn split_layout(&mut self, area: Rect) -> Vec<crate::pane_tree::LeafBox> {
        self.sync_split();
        self.split_root.as_ref().map(|r| r.layout(area)).unwrap_or_default()
    }

    /// The split divider rectangles for drawing, given the editor `area`.
    #[must_use]
    pub fn split_dividers(
        &self,
        area: Rect,
    ) -> Vec<(SplitDir, Rect)> {
        self.split_root.as_ref().map(|r| r.dividers(area)).unwrap_or_default()
    }

    /// Focus the pane under `(col, row)` within `area`; returns whether a pane was
    /// hit.
    pub fn focus_pane_at(&mut self, area: Rect, col: u16, row: u16) -> bool {
        self.sync_split();
        let Some(leaf) = self.split_root.as_ref().and_then(|r| r.leaf_at(area, col, row)) else {
            return false;
        };
        self.focus_leaf(leaf);
        true
    }

    /// Drag a split divider under `(col, row)` within `area`; returns whether one
    /// was resized.
    pub fn resize_split_at(&mut self, area: Rect, col: u16, row: u16) -> bool {
        self.split_root.as_mut().is_some_and(|r| r.resize_at(area, col, row))
    }

    /// Create an empty untitled buffer and focus it.
    pub fn new_tab(&mut self) {
        let editor = make_editor(None, "", self.line_numbers, self.show_whitespace, self.soft_wrap, &self.indent);
        self.tabs.push(Tab {
            editor,
            path: None,
            dirty: false,
            preview: false,
            image: None,
        });
        self.active = self.tabs.len() - 1;
    }

    /// Open a new untitled tab pre-filled with `content` (e.g. AI output), marked
    /// dirty so the user is reminded to save it. Becomes the active tab.
    pub fn new_tab_with_content(&mut self, content: &str) {
        let editor =
            make_editor(None, content, self.line_numbers, self.show_whitespace, self.soft_wrap, &self.indent);
        self.tabs.push(Tab {
            editor,
            path: None,
            dirty: true,
            preview: false,
            image: None,
        });
        self.active = self.tabs.len() - 1;
    }

    /// Open an image file in a tab, rendered with the given protocol.
    pub fn open_image(&mut self, path: &Path, proto: StatefulProtocol) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(canon.as_path()))
        {
            self.tabs[i].image = Some(proto);
            self.active = i;
            return;
        }
        let editor = make_editor(Some(&canon), "", self.line_numbers, self.show_whitespace, self.soft_wrap, &self.indent);
        self.tabs.push(Tab {
            editor,
            path: Some(canon),
            dirty: false,
            preview: false,
            image: Some(proto),
        });
        self.active = self.tabs.len() - 1;
    }

    /// Apply the current line-number settings (visibility + relative) to every
    /// buffer.
    pub fn refresh_line_numbers(&mut self) {
        for tab in &mut self.tabs {
            tab.editor.show_line_numbers(self.line_numbers);
            tab.editor.set_relative_line_numbers(self.relative_line_numbers);
        }
    }

    /// Apply the current visible-whitespace setting to every buffer.
    pub fn refresh_whitespace(&mut self) {
        for tab in &mut self.tabs {
            tab.editor.show_whitespace(self.show_whitespace);
        }
    }

    /// Apply the current soft-wrap setting to every buffer.
    pub fn refresh_soft_wrap(&mut self) {
        for tab in &mut self.tabs {
            tab.editor.set_soft_wrap(self.soft_wrap);
        }
    }

    /// Re-apply the current theme's styles to every buffer (after a theme switch).
    pub fn refresh_theme(&mut self) {
        for tab in &mut self.tabs {
            apply_theme(&mut tab.editor);
        }
    }

    /// Open a file: focus it if already open, otherwise load it. `preview`
    /// requests an ephemeral tab that the next preview reuses.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn open(&mut self, path: &Path, preview: bool) -> io::Result<()> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(canon.as_path()))
        {
            self.active = i;
            if !preview {
                self.tabs[i].preview = false;
            }
            return Ok(());
        }

        let content = fs::read_to_string(&canon)?;
        let editor = make_editor(Some(&canon), &content, self.line_numbers, self.show_whitespace, self.soft_wrap, &self.indent);
        let tab = Tab {
            editor,
            path: Some(canon),
            dirty: false,
            preview,
            image: None,
        };

        if preview
            && let Some(i) = self.tabs.iter().position(|t| t.preview) {
                self.tabs[i] = tab;
                self.active = i;
                return Ok(());
            }
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Ok(())
    }

    /// Reload every clean (non-dirty) file-backed buffer from disk, preserving the
    /// cursor where it still fits. Used after an external change to the working
    /// tree (e.g. a git branch switch). Dirty buffers are left untouched so unsaved
    /// edits are never discarded. Returns the number of buffers reloaded.
    pub fn reload_clean_from_disk(&mut self) -> usize {
        let mut reloaded = 0;
        for tab in &mut self.tabs {
            if tab.dirty || tab.image.is_some() {
                continue;
            }
            let Some(path) = tab.path.clone() else { continue };
            let Ok(content) = fs::read_to_string(&path) else { continue };
            if content == tab.editor.get_content() {
                continue;
            }
            let cursor = tab.editor.get_cursor();
            tab.editor.set_content(&content);
            tab.editor.set_cursor(cursor.min(content.chars().count()));
            tab.dirty = false;
            reloaded += 1;
        }
        reloaded
    }

    /// Promote the active preview tab to a permanent tab.
    pub fn promote_active(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            t.preview = false;
        }
    }

    /// Save the active buffer to its existing path.
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer has no path, or the write fails.
    pub fn save_active(&mut self, opts: SaveOptions) -> io::Result<PathBuf> {
        let idx = self.active;
        let path = self.tabs[idx]
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "buffer has no path"))?;
        self.write_to(idx, &path, opts)?;
        Ok(path)
    }

    /// Save the active buffer to `path` and adopt it as the buffer's path.
    ///
    /// # Errors
    ///
    /// Returns an error if the write fails.
    pub fn save_active_as(&mut self, path: PathBuf, opts: SaveOptions) -> io::Result<PathBuf> {
        let idx = self.active;
        self.write_to(idx, &path, opts)?;
        self.tabs[idx].path = Some(path.clone());
        Ok(path)
    }

    fn write_to(&mut self, idx: usize, path: &Path, opts: SaveOptions) -> io::Result<()> {
        let mut data = self.tabs[idx].text();
        if opts.trim_trailing_whitespace {
            data = trim_trailing_whitespace(&data);
        }
        if opts.ensure_final_newline && !data.is_empty() && !data.ends_with('\n') {
            data.push('\n');
        }
        fs::write(path, data)?;
        self.tabs[idx].dirty = false;
        self.tabs[idx].preview = false;
        Ok(())
    }

    /// Close the active tab; keeps at least one empty buffer open. Returns the
    /// closed tab's file path, if it had one (for "reopen closed tab").
    pub fn close_active(&mut self) -> Option<PathBuf> {
        if self.tabs.is_empty() {
            return None;
        }
        let closed = self.tabs.remove(self.active).path;
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        closed
    }

    /// Close every tab, leaving a single empty untitled buffer. Returns the file
    /// paths of the closed tabs, oldest first (for "reopen closed tab").
    pub fn close_all(&mut self) -> Vec<PathBuf> {
        let closed: Vec<PathBuf> = self.tabs.drain(..).filter_map(|t| t.path).collect();
        self.active = 0;
        self.new_tab();
        closed
    }

    /// Activate the next tab, wrapping around.
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Activate the previous tab, wrapping around.
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// 1-based (line, column) of the active cursor, for the status bar.
    #[must_use] 
    pub fn cursor_1based(&self) -> (usize, usize) {
        self.active_tab().map_or((1, 1), Tab::cursor_1based)
    }

    /// Total line count of the active buffer (for the scrollbar).
    #[must_use] 
    pub fn active_line_count(&self) -> usize {
        self.active_tab()
            .map_or(1, |t| t.editor.code_ref().len_lines())
    }

    /// Jump the active buffer to a 1-based line and optional 1-based column,
    /// keeping the target visible within `area`. The line is clamped to the
    /// buffer's range (an out-of-range line would otherwise panic).
    pub fn goto(&mut self, line: usize, col: Option<usize>, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let last = t.editor.code_ref().len_lines().max(1);
            let line = line.clamp(1, last);
            let off = t
                .editor
                .code_ref()
                .offset(line - 1, col.unwrap_or(1).saturating_sub(1));
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the start of its current line.
    /// Smart Home: jump to the first non-blank character of the line; if already
    /// there (or the line is blank), jump to column 0. Pressing Home repeatedly
    /// toggles between the two.
    pub fn cursor_line_home(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let code = t.editor.code_ref();
            let row = code.char_to_line(cur);
            let line_start = code.line_to_char(row);
            let line_len = code.line_len(row);
            let cur_col = cur - line_start;
            let indent = code
                .char_slice(line_start, line_start + line_len)
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .count();
            // A blank/all-whitespace line has no first non-blank: treat it as 0.
            let indent = if indent >= line_len { 0 } else { indent };
            let target = if cur_col == indent { 0 } else { indent };
            t.editor.set_cursor(line_start + target);
        }
    }

    /// Move the active cursor to the very start of the buffer and scroll it into
    /// `area`.
    pub fn cursor_document_start(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.set_cursor(0);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the very end of the buffer (past the last
    /// character) and scroll it into `area`.
    pub fn cursor_document_end(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let code = t.editor.code_ref();
            let last = code.len_lines().saturating_sub(1);
            let off = code.line_to_char(last) + code.line_len(last);
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the end of its current line.
    pub fn cursor_line_end(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let code = t.editor.code_ref();
            let row = code.char_to_line(cur);
            let off = code.line_to_char(row) + code.line_len(row);
            t.editor.set_cursor(off);
        }
    }

    /// Move the active cursor to column 0 of its current line (the literal line
    /// start, unlike the smart-Home [`cursor_line_home`](Self::cursor_line_home)).
    pub fn cursor_line_start(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let code = t.editor.code_ref();
            let row = code.char_to_line(cur);
            let off = code.line_to_char(row);
            t.editor.set_cursor(off);
        }
    }

    /// Move the active cursor to the first line of the current paragraph and
    /// scroll it into `area`. Paragraphs are delimited by blank (empty or
    /// whitespace-only) lines; from a blank line, climbs to the paragraph above.
    pub fn cursor_paragraph_start(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let off = {
                let code = t.editor.code_ref();
                let is_blank = |r: usize| line_is_blank(code, r);
                let mut r = code.char_to_line(cur);
                while r > 0 && is_blank(r) {
                    r -= 1;
                }
                while r > 0 && !is_blank(r - 1) {
                    r -= 1;
                }
                code.line_to_char(r)
            };
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the end of the last line of the current
    /// paragraph and scroll it into `area`. See [`cursor_paragraph_start`].
    pub fn cursor_paragraph_end(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let off = {
                let code = t.editor.code_ref();
                let n = code.len_lines().max(1);
                let is_blank = |r: usize| line_is_blank(code, r);
                let mut r = code.char_to_line(cur);
                while r + 1 < n && is_blank(r) {
                    r += 1;
                }
                while r + 1 < n && !is_blank(r + 1) {
                    r += 1;
                }
                code.line_to_char(r) + code.line_len(r)
            };
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the first line of the current section and scroll
    /// it into `area`. Sections are delimited by a run of two or more consecutive
    /// blank lines (a larger break than a paragraph).
    pub fn cursor_section_start(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let off = {
                let code = t.editor.code_ref();
                let n = code.len_lines().max(1);
                let sep = |r: usize| is_section_break(code, r, n);
                let mut r = code.char_to_line(cur);
                while r > 0 && !sep(r - 1) {
                    r -= 1;
                }
                code.line_to_char(r)
            };
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the end of the last line of the current section
    /// and scroll it into `area`. See [`cursor_section_start`].
    pub fn cursor_section_end(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let off = {
                let code = t.editor.code_ref();
                let n = code.len_lines().max(1);
                let sep = |r: usize| is_section_break(code, r, n);
                let mut r = code.char_to_line(cur);
                while r + 1 < n && !sep(r + 1) {
                    r += 1;
                }
                code.line_to_char(r) + code.line_len(r)
            };
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor down one line (Go → Line → Next).
    pub fn cursor_line_down(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.cursor_down();
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor up one line (Go → Line → Previous).
    pub fn cursor_line_up(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.cursor_up();
            t.editor.focus(&area);
        }
    }

    /// 0-based row of the active cursor (0 when there is no buffer).
    fn cursor_row(&self) -> usize {
        self.active_tab().map_or(0, |t| {
            let code = t.editor.code_ref();
            code.char_to_line(t.editor.get_cursor())
        })
    }

    /// 0-based line of the active cursor (0 when there is no buffer).
    #[must_use]
    pub fn cursor_line(&self) -> usize {
        self.cursor_row()
    }

    /// Install any completed background reparse for the active buffer (large
    /// files). Returns `true` if the syntax tree changed (request a redraw).
    pub fn poll_parse(&mut self) -> bool {
        self.active_tab_mut().is_some_and(|t| t.editor.poll_parse())
    }

    /// Whether a background reparse is in flight for the active buffer.
    #[must_use]
    pub fn parse_pending(&self) -> bool {
        self.active_tab().is_some_and(|t| t.editor.parse_pending())
    }

    /// Cycle which undo-tree branch the next redo follows in the active buffer.
    /// Returns `true` if the current state has more than one branch.
    pub fn switch_undo_branch(&mut self) -> bool {
        self.active_tab_mut().is_some_and(|t| t.editor.switch_undo_branch())
    }

    /// The 0-based index of the active buffer's first visible line (vertical
    /// scroll offset), or 0 when there is no buffer.
    #[must_use]
    pub fn top_visible_line(&self) -> usize {
        self.active_tab().map_or(0, |t| t.editor.top_line())
    }

    /// Set the passive word-occurrence marks on the active buffer.
    pub fn set_word_marks(&mut self, marks: Vec<(usize, usize)>) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.set_word_marks(marks);
        }
    }

    /// The 0-based start rows of every paragraph (`section == false`) or section
    /// (`section == true`) in the active buffer. A paragraph is a run of non-blank
    /// lines; a section is a run separated by a section break (2+ blank lines).
    fn block_starts(&self, section: bool) -> Vec<usize> {
        let Some(t) = self.active_tab() else { return Vec::new() };
        let code = t.editor.code_ref();
        let n = code.len_lines().max(1);
        (0..n)
            .filter(|&r| {
                if section {
                    !is_section_break(code, r, n) && (r == 0 || is_section_break(code, r - 1, n))
                } else {
                    !line_is_blank(code, r) && (r == 0 || line_is_blank(code, r - 1))
                }
            })
            .collect()
    }

    /// Move the active cursor to the start of line `row` and scroll it into view.
    fn goto_row(&mut self, row: usize, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let off = t.editor.code_ref().line_to_char(row);
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move to the start of the next paragraph (Go → Paragraph → Next).
    pub fn cursor_paragraph_next(&mut self, area: Rect) {
        let row = self.cursor_row();
        if let Some(&next) = self.block_starts(false).iter().find(|&&r| r > row) {
            self.goto_row(next, area);
        }
    }

    /// Move to the start of the previous paragraph (Go → Paragraph → Previous).
    pub fn cursor_paragraph_prev(&mut self, area: Rect) {
        let row = self.cursor_row();
        if let Some(&prev) = self.block_starts(false).iter().rev().find(|&&r| r < row) {
            self.goto_row(prev, area);
        }
    }

    /// Jump to the 1-based `n`th paragraph (Go → Paragraph → Number).
    pub fn goto_paragraph(&mut self, n: usize, area: Rect) {
        if let Some(&row) = self.block_starts(false).get(n.saturating_sub(1)) {
            self.goto_row(row, area);
        }
    }

    /// Move to the start of the next section (Go → Section → Next).
    pub fn cursor_section_next(&mut self, area: Rect) {
        let row = self.cursor_row();
        if let Some(&next) = self.block_starts(true).iter().find(|&&r| r > row) {
            self.goto_row(next, area);
        }
    }

    /// Move to the start of the previous section (Go → Section → Previous).
    pub fn cursor_section_prev(&mut self, area: Rect) {
        let row = self.cursor_row();
        if let Some(&prev) = self.block_starts(true).iter().rev().find(|&&r| r < row) {
            self.goto_row(prev, area);
        }
    }

    /// Jump to the 1-based `n`th section (Go → Section → Number).
    pub fn goto_section(&mut self, n: usize, area: Rect) {
        if let Some(&row) = self.block_starts(true).get(n.saturating_sub(1)) {
            self.goto_row(row, area);
        }
    }

    /// Move the active cursor to char offset `off` and scroll it into view.
    fn goto_offset(&mut self, off: usize, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move to the start of the current sentence (Go → Sentence → Start).
    pub fn cursor_sentence_start(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        let off = sentence_starts(&t.editor.get_content()).into_iter().rev().find(|&s| s <= cur).unwrap_or(0);
        self.goto_offset(off, area);
    }

    /// Move to the end of the current sentence — its last non-blank char (Go →
    /// Sentence → End).
    pub fn cursor_sentence_end(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        let text = t.editor.get_content();
        let chars: Vec<char> = text.chars().collect();
        let off = match sentence_starts(&text).into_iter().find(|&s| s > cur) {
            Some(mut e) => {
                while e > 0 && chars.get(e - 1).is_some_and(|c| c.is_whitespace()) {
                    e -= 1;
                }
                e
            }
            None => chars.len(),
        };
        self.goto_offset(off, area);
    }

    /// Move to the start of the next sentence (Go → Sentence → Next).
    pub fn cursor_sentence_next(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        if let Some(next) = sentence_starts(&t.editor.get_content()).into_iter().find(|&s| s > cur) {
            self.goto_offset(next, area);
        }
    }

    /// Move to the start of the previous sentence (Go → Sentence → Previous).
    pub fn cursor_sentence_prev(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        if let Some(prev) = sentence_starts(&t.editor.get_content()).into_iter().rev().find(|&s| s < cur) {
            self.goto_offset(prev, area);
        }
    }

    /// Jump to the 1-based `n`th sentence (Go → Sentence → Number).
    pub fn goto_sentence(&mut self, n: usize, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let starts = sentence_starts(&t.editor.get_content());
        if let Some(&off) = starts.get(n.saturating_sub(1)) {
            self.goto_offset(off, area);
        }
    }

    /// Move to the start of the current word — or the previous word's start if the
    /// cursor is not on a word (Go → Word → Start).
    pub fn cursor_word_start(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        let off = word_starts(&t.editor.get_content()).into_iter().rev().find(|&s| s <= cur).unwrap_or(0);
        self.goto_offset(off, area);
    }

    /// Move to the end of the current word (Go → Word → End).
    pub fn cursor_word_end(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        let chars: Vec<char> = t.editor.get_content().chars().collect();
        // Skip any non-word chars under the cursor, then run to the word's end.
        let mut e = cur;
        while e < chars.len() && !is_word_char(chars[e]) {
            e += 1;
        }
        while e < chars.len() && is_word_char(chars[e]) {
            e += 1;
        }
        self.goto_offset(e, area);
    }

    /// Move to the start of the next word (Go → Word → Next).
    pub fn cursor_word_next(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        if let Some(next) = word_starts(&t.editor.get_content()).into_iter().find(|&s| s > cur) {
            self.goto_offset(next, area);
        }
    }

    /// Move to the start of the previous word (Go → Word → Previous).
    pub fn cursor_word_prev(&mut self, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let cur = t.editor.get_cursor();
        if let Some(prev) = word_starts(&t.editor.get_content()).into_iter().rev().find(|&s| s < cur) {
            self.goto_offset(prev, area);
        }
    }

    /// Jump to the 1-based `n`th word (Go → Word → Number).
    pub fn goto_word(&mut self, n: usize, area: Rect) {
        let Some(t) = self.active_tab() else { return };
        let starts = word_starts(&t.editor.get_content());
        if let Some(&off) = starts.get(n.saturating_sub(1)) {
            self.goto_offset(off, area);
        }
    }

    /// Select the current line, including its trailing newline (so it can be cut
    /// as a whole line), and scroll it into `area`.
    pub fn select_line(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let (start, end) = {
                let code = t.editor.code_ref();
                let row = code.char_to_line(t.editor.get_cursor());
                let start = code.line_to_char(row);
                let end = if row + 1 < code.len_lines() {
                    code.line_to_char(row + 1)
                } else {
                    start + code.line_len(row)
                };
                (start, end)
            };
            t.editor.set_selection_range(start, end);
            t.editor.focus(&area);
        }
    }

    /// Select the whole paragraph under the cursor (blank-line delimited) and
    /// scroll it into `area`.
    pub fn select_paragraph(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let (start, end) = {
                let code = t.editor.code_ref();
                let n = code.len_lines().max(1);
                let is_blank = |r: usize| line_is_blank(code, r);
                let row = code.char_to_line(t.editor.get_cursor());
                let mut s = row;
                while s > 0 && is_blank(s) {
                    s -= 1;
                }
                while s > 0 && !is_blank(s - 1) {
                    s -= 1;
                }
                let mut e = row;
                while e + 1 < n && is_blank(e) {
                    e += 1;
                }
                while e + 1 < n && !is_blank(e + 1) {
                    e += 1;
                }
                (code.line_to_char(s), code.line_to_char(e) + code.line_len(e))
            };
            t.editor.set_selection_range(start, end);
            t.editor.focus(&area);
        }
    }

    /// Select the whole section under the cursor (delimited by runs of two or
    /// more blank lines) and scroll it into `area`.
    pub fn select_section(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let (start, end) = {
                let code = t.editor.code_ref();
                let n = code.len_lines().max(1);
                let sep = |r: usize| is_section_break(code, r, n);
                let row = code.char_to_line(t.editor.get_cursor());
                let mut s = row;
                while s > 0 && !sep(s - 1) {
                    s -= 1;
                }
                let mut e = row;
                while e + 1 < n && !sep(e + 1) {
                    e += 1;
                }
                (code.line_to_char(s), code.line_to_char(e) + code.line_len(e))
            };
            t.editor.set_selection_range(start, end);
            t.editor.focus(&area);
        }
    }

    /// Insert `text` at the active buffer's cursor and scroll it into `area`.
    /// Returns whether the text was inserted (false when there is no editable
    /// buffer, e.g. an image tab). Marks the buffer dirty and promotes a preview.
    pub fn insert_str(&mut self, text: &str, area: Rect) -> bool {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return false;
            }
            t.editor.apply(InsertText { text: text.to_string() });
            t.editor.focus(&area);
            t.dirty = true;
            t.preview = false;
            return true;
        }
        false
    }

    /// Select the entire active buffer.
    pub fn select_all(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.apply(SelectAll {});
        }
    }

    /// Duplicate the cursor line (or the selection) in the active buffer.
    pub fn duplicate_line(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            t.editor.apply(Duplicate {});
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Join the active buffer's current line with the next (or all selected
    /// lines into one).
    pub fn join_lines(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            t.editor.join_lines();
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Sort the active buffer's selected lines ascending, or the whole buffer
    /// when nothing is selected.
    pub fn sort_lines(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            t.editor.sort_lines();
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Strip trailing whitespace from the selected lines (or the whole buffer).
    pub fn trim_trailing_whitespace(&mut self) {
        self.edit_active_lines(CodeEditor::trim_trailing_whitespace);
    }

    /// Remove duplicate lines in the selection (or the whole buffer).
    pub fn remove_duplicate_lines(&mut self) {
        self.edit_active_lines(CodeEditor::remove_duplicate_lines);
    }

    /// Reverse the order of the selected lines (or the whole buffer).
    pub fn reverse_lines(&mut self) {
        self.edit_active_lines(CodeEditor::reverse_lines);
    }

    /// Sort the selected lines ascending and drop duplicates (or the whole buffer).
    pub fn sort_unique(&mut self) {
        self.edit_active_lines(CodeEditor::sort_unique);
    }

    /// Randomly reorder the selected lines (or the whole buffer).
    pub fn shuffle_lines(&mut self) {
        self.edit_active_lines(CodeEditor::shuffle_lines);
    }

    /// Run a line-editing op on the active (non-image) buffer, marking it dirty.
    fn edit_active_lines(&mut self, f: impl FnOnce(&mut CodeEditor)) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            f(&mut t.editor);
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Move the active buffer's cursor line up or down by one row, scrolling it
    /// into `area`.
    pub fn move_line(&mut self, down: bool, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            if down {
                t.editor.move_line_down();
            } else {
                t.editor.move_line_up();
            }
            t.editor.focus(&area);
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Extend the selection by one word: `forward` grows the active end to the
    /// next word boundary on the right ("Select More"), otherwise to the previous
    /// word boundary on the left ("Select Less").
    pub fn select_word(&mut self, forward: bool, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            if t.is_image() {
                return;
            }
            let text: Vec<char> = t.text().chars().collect();
            let cur = t.editor.get_cursor();
            let target = if forward { next_word(&text, cur) } else { prev_word(&text, cur) };
            if target != cur {
                t.editor.extend_selection(target);
                t.editor.set_cursor(target);
                t.editor.focus(&area);
            }
        }
    }

    /// Jump the cursor to the partner of the bracket at (or just before) it,
    /// scrolling it into `area`. No-op when the cursor is not on a bracket.
    pub fn jump_matching_bracket(&mut self, area: Rect) {
        if let Some(t) = self.active_tab_mut()
            && let Some(off) = t.editor.matching_bracket_offset() {
                t.editor.set_cursor(off);
                t.editor.focus(&area);
            }
    }

    /// Forward-delete (the `Delete` key): step right, then delete back.
    pub fn delete_forward(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.apply(MoveRight { shift: false });
            t.editor.apply(Delete {});
        }
    }

    /// Move the cursor up by `lines` (`PageUp`).
    pub fn page_up(&mut self, lines: usize) {
        if let Some(t) = self.active_tab_mut() {
            for _ in 0..lines.max(1) {
                t.editor.apply(MoveUp { shift: false });
            }
        }
    }

    /// Move the cursor down by `lines` (`PageDown`).
    pub fn page_down(&mut self, lines: usize) {
        if let Some(t) = self.active_tab_mut() {
            for _ in 0..lines.max(1) {
                t.editor.apply(MoveDown { shift: false });
            }
        }
    }
}

/// Whether `c` is part of a word (alphanumeric or underscore).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The next word-boundary char index at or after `pos`: skip any non-word
/// characters, then the following word run.
fn next_word(text: &[char], pos: usize) -> usize {
    let n = text.len();
    let mut i = pos.min(n);
    while i < n && !is_word_char(text[i]) {
        i += 1;
    }
    while i < n && is_word_char(text[i]) {
        i += 1;
    }
    i
}

/// The previous word-boundary char index at or before `pos`: skip any non-word
/// characters to the left, then the preceding word run.
fn prev_word(text: &[char], pos: usize) -> usize {
    let mut i = pos.min(text.len());
    while i > 0 && !is_word_char(text[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(text[i - 1]) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod nav_tests {
    use super::{sentence_starts, word_starts};

    #[test]
    fn word_starts_finds_each_word() {
        // "foo  bar_baz qux" — words at 0, 5, 13 (underscore stays in one word).
        assert_eq!(word_starts("foo  bar_baz qux"), vec![0, 5, 13]);
        assert_eq!(word_starts("   leading"), vec![3]);
        assert!(word_starts("...").is_empty());
    }

    #[test]
    fn sentence_starts_splits_on_terminators() {
        let text = "One. Two! Three?  Four.";
        // Starts at "One"(0), "Two"(5), "Three"(10), "Four"(18).
        assert_eq!(sentence_starts(text), vec![0, 5, 10, 18]);
        // A period not followed by whitespace (e.g. a decimal) is not a boundary.
        assert_eq!(sentence_starts("pi is 3.14 today"), vec![0]);
    }
}
