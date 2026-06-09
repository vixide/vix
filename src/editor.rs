//! Tabbed editor: a stack of buffers, each backed by a `vix-code-editor-panel`
//! widget (Tree-sitter syntax highlighting, history, selection, clipboard).
//!
//! The code editor addresses the cursor as a flat character offset; this module
//! converts to/from 1-based line/column for the status bar and go-to-line.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::Style;
use vix_code_editor_panel::actions::{Delete, MoveDown, MoveRight, MoveUp};
pub use vix_code_editor_panel::editor::Editor as CodeEditor;
use vix_code_editor_panel::utils::get_lang;
use ratatui_image::protocol::StatefulProtocol;

use crate::theme;

/// File extensions opened as images rather than text.
pub fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
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

fn make_editor(path: Option<&Path>, text: &str, line_numbers: bool) -> CodeEditor {
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
    apply_theme(&mut ed);
    ed
}

/// Build a small, theme-styled, single-line text field holding `content`. Used by
/// the Vix menu's Website/Email dialogs so the text is selectable and copyable
/// from inside the TUI (select with the mouse or keyboard, then `Ctrl+C`).
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
        self.text().lines().map(|s| s.to_string()).collect()
    }

    /// Title shown on the tab and in the buffer switcher.
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".to_string());
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
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "untitled".to_string())
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
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new(true)
    }
}

impl Editor {
    /// Create an editor with one empty buffer and the given gutter setting.
    pub fn new(line_numbers: bool) -> Self {
        let mut e = Editor {
            tabs: Vec::new(),
            active: 0,
            line_numbers,
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
        let editor = make_editor(None, "", self.line_numbers);
        self.tabs.push(Tab {
            editor,
            path: None,
            dirty: false,
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
        let editor = make_editor(Some(&canon), "", self.line_numbers);
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

    /// Re-apply the current theme's styles to every buffer (after a theme switch).
    pub fn refresh_theme(&mut self) {
        for tab in &mut self.tabs {
            apply_theme(&mut tab.editor);
        }
    }

    /// Open a file: focus it if already open, otherwise load it. `preview`
    /// requests an ephemeral tab that the next preview reuses.
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
        let editor = make_editor(Some(&canon), &content, self.line_numbers);
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
    pub fn save_active(&mut self) -> io::Result<PathBuf> {
        let idx = self.active;
        let path = self.tabs[idx]
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "buffer has no path"))?;
        self.write_to(idx, &path)?;
        Ok(path)
    }

    /// Save the active buffer to `path` and adopt it as the buffer's path.
    ///
    /// # Errors
    ///
    /// Returns an error if the write fails.
    pub fn save_active_as(&mut self, path: PathBuf) -> io::Result<PathBuf> {
        let idx = self.active;
        self.write_to(idx, &path)?;
        self.tabs[idx].path = Some(path.clone());
        Ok(path)
    }

    fn write_to(&mut self, idx: usize, path: &Path) -> io::Result<()> {
        let mut data = self.tabs[idx].text();
        if !data.ends_with('\n') {
            data.push('\n');
        }
        fs::write(path, data)?;
        self.tabs[idx].dirty = false;
        self.tabs[idx].preview = false;
        Ok(())
    }

    /// Close the active tab; keeps at least one empty buffer open.
    pub fn close_active(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.tabs.remove(self.active);
        if self.tabs.is_empty() {
            self.new_tab();
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
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
    pub fn cursor_1based(&self) -> (usize, usize) {
        self.active_tab().map(|t| t.cursor_1based()).unwrap_or((1, 1))
    }

    /// Total line count of the active buffer (for the scrollbar).
    pub fn active_line_count(&self) -> usize {
        self.active_tab()
            .map(|t| t.editor.code_ref().len_lines())
            .unwrap_or(1)
    }

    /// Jump the active buffer to a 1-based line and optional 1-based column,
    /// keeping the target visible within `area`.
    pub fn goto(&mut self, line: usize, col: Option<usize>, area: Rect) {
        if let Some(t) = self.active_tab_mut() {
            let off = t
                .editor
                .code_ref()
                .offset(line.saturating_sub(1), col.unwrap_or(1).saturating_sub(1));
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    /// Move the active cursor to the start of its current line.
    pub fn cursor_line_home(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            let cur = t.editor.get_cursor();
            let row = t.editor.code_ref().char_to_line(cur);
            let off = t.editor.code_ref().line_to_char(row);
            t.editor.set_cursor(off);
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

    /// Forward-delete (the `Delete` key): step right, then delete back.
    pub fn delete_forward(&mut self) {
        if let Some(t) = self.active_tab_mut() {
            t.editor.apply(MoveRight { shift: false });
            t.editor.apply(Delete {});
        }
    }

    /// Move the cursor up by `lines` (PageUp).
    pub fn page_up(&mut self, lines: usize) {
        if let Some(t) = self.active_tab_mut() {
            for _ in 0..lines.max(1) {
                t.editor.apply(MoveUp { shift: false });
            }
        }
    }

    /// Move the cursor down by `lines` (PageDown).
    pub fn page_down(&mut self, lines: usize) {
        if let Some(t) = self.active_tab_mut() {
            for _ in 0..lines.max(1) {
                t.editor.apply(MoveDown { shift: false });
            }
        }
    }
}
