//! Tabbed editor: a stack of buffers, each backed by a `ratatui-code-editor`
//! widget (Tree-sitter syntax highlighting, history, selection, clipboard).
//!
//! The code editor addresses the cursor as a flat character offset; this module
//! converts to/from 1-based line/column for the status bar and go-to-line.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui_code_editor::actions::{Delete, MoveDown, MoveRight, MoveUp};
use ratatui_code_editor::editor::Editor as CodeEditor;
use ratatui_code_editor::theme::vesper;
use ratatui_code_editor::utils::get_lang;
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

/// Highlight color used to mark search hits.
pub const SEARCH_MARK: &str = "#ffd866";

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
    // `CodeEditor::new` already falls back to plain "text" on an unknown grammar.
    let mut ed = CodeEditor::new(&lang, text, vesper())
        .or_else(|_| CodeEditor::new("text", text, vesper()))
        .expect("code editor init for plain text never fails");
    ed.show_line_numbers(line_numbers);
    ed
}

/// One open buffer shown as one tab.
pub struct Tab {
    pub editor: CodeEditor,
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

pub struct Editor {
    pub tabs: Vec<Tab>,
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

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

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

    pub fn save_active(&mut self) -> io::Result<PathBuf> {
        let idx = self.active;
        let path = self.tabs[idx]
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "buffer has no path"))?;
        self.write_to(idx, &path)?;
        Ok(path)
    }

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

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

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
