//! Tabbed editor: a stack of buffers, each backed by a `vix-editor`
//! widget (Tree-sitter syntax highlighting, history, selection, clipboard).
//!
//! The code editor addresses the cursor as a flat character offset; this module
//! converts to/from 1-based line/column for the status bar and go-to-line.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::Style;
use vix_editor::actions::{Delete, Duplicate, InsertText, MoveDown, MoveRight, MoveUp, SelectAll};
use vix_editor::code::Code;
pub use vix_editor::editor::Editor as CodeEditor;
use vix_editor::utils::get_lang;
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
}

/// The tab strip: a stack of open buffers and the active index.
pub struct Editor {
    /// Open buffers, left to right.
    pub tabs: Vec<Tab>,
    /// Index of the active tab.
    pub active: usize,
    /// Whether the line-number gutter is shown.
    pub line_numbers: bool,
    /// Whether visible-whitespace glyphs are shown.
    pub show_whitespace: bool,
    /// Whether long lines soft-wrap.
    pub soft_wrap: bool,
    /// String Tab inserts in every buffer (spaces or a tab).
    pub indent: String,
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
            show_whitespace,
            soft_wrap,
            indent,
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

    /// Apply the current line-number setting to every buffer.
    pub fn refresh_line_numbers(&mut self) {
        for tab in &mut self.tabs {
            tab.editor.show_line_numbers(self.line_numbers);
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

        if preview {
            if let Some(i) = self.tabs.iter().position(|t| t.preview) {
                self.tabs[i] = tab;
                self.active = i;
                return Ok(());
            }
        }
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Ok(())
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
        if let Some(t) = self.active_tab_mut() {
            if let Some(off) = t.editor.matching_bracket_offset() {
                t.editor.set_cursor(off);
                t.editor.focus(&area);
            }
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
