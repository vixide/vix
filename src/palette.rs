//! Command palette (Ctrl+P) with prefix-driven modes.
//!
//! | Prefix | Mode        |
//! |--------|-------------|
//! | (none) | File finder |
//! | `>`    | Commands    |
//! | `#`    | Buffers     |
//! | `:`    | Go to line  |

use std::path::PathBuf;

/// Which palette sub-mode is active, chosen by the input's leading prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fuzzy file finder (no prefix).
    Files,
    /// Command list (`>` prefix).
    Commands,
    /// Open-buffer switcher (`#` prefix).
    Buffers,
    /// Jump to a line number (`:` prefix).
    GotoLine,
}

impl Mode {
    /// Infer the mode from the palette input's first character.
    #[must_use]
    pub fn from_input(input: &str) -> Mode {
        match input.chars().next() {
            Some('>') => Mode::Commands,
            Some('#') => Mode::Buffers,
            Some(':') => Mode::GotoLine,
            _ => Mode::Files,
        }
    }

    /// Mode name translated into the active locale (shown in the palette title).
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Mode::Files => t!("palette.mode.files"),
            Mode::Commands => t!("palette.mode.commands"),
            Mode::Buffers => t!("palette.mode.buffers"),
            Mode::GotoLine => t!("palette.mode.goto_line"),
        }
        .to_string()
    }
}

/// What happens when an entry is accepted.
#[derive(Clone)]
pub enum Action {
    /// Open a file, optionally jumping to a 1-based (line, column).
    OpenFile(PathBuf, Option<(usize, usize)>),
    /// Dispatch an `App::run_action` command.
    RunCommand(String),
    /// Switch to the open buffer at this index.
    SwitchBuffer(usize),
    /// Jump to a 1-based line in the active buffer.
    GotoLine(usize),
}

/// One row shown in the palette list.
#[derive(Clone)]
pub struct Entry {
    /// Already-translated, display-ready label.
    pub label: String,
    /// Action performed when the entry is accepted.
    pub action: Action,
}

/// Command palette state.
pub struct Palette {
    /// Raw input text, including any mode prefix.
    pub input: String,
    /// Currently matching entries.
    pub entries: Vec<Entry>,
    /// Index of the highlighted entry.
    pub selected: usize,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::new()
    }
}

impl Palette {
    /// An empty palette.
    #[must_use]
    pub fn new() -> Self {
        Palette {
            input: String::new(),
            entries: Vec::new(),
            selected: 0,
        }
    }

    /// The active mode, derived from the input prefix.
    #[must_use]
    pub fn mode(&self) -> Mode {
        Mode::from_input(&self.input)
    }

    /// The query with any mode-prefix stripped.
    #[must_use]
    pub fn query(&self) -> &str {
        let s = self.input.as_str();
        match self.mode() {
            Mode::Files => s,
            _ => s.get(1..).unwrap_or(""),
        }
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// The highlighted entry, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    /// Append a typed character and reset the selection.
    pub fn insert(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
    }

    /// Delete the last character and reset the selection.
    pub fn backspace(&mut self) {
        self.input.pop();
        self.selected = 0;
    }
}

/// Commands offered in `>` mode: `(label_key, action)`, where `label_key` is an
/// i18n key (translated at render time) and `action` is the same identifier the
/// menu bar dispatches.
pub const COMMANDS: &[(&str, &str)] = &[
    ("cmd.new_file", "file.new"),
    ("cmd.open_file", "file.open"),
    ("cmd.save", "file.save"),
    ("cmd.save_as", "file.save_as"),
    ("cmd.close_tab", "file.close"),
    ("cmd.quit", "file.quit"),
    ("cmd.undo", "edit.undo"),
    ("cmd.redo", "edit.redo"),
    ("cmd.find", "edit.find"),
    ("cmd.replace", "edit.replace"),
    ("cmd.query_replace", "edit.query_replace"),
    ("cmd.search_project", "search.project"),
    ("cmd.search_replace_project", "search.project_replace"),
    ("cmd.goto_definition", "nav.goto_definition"),
    ("cmd.themes", "view.themes"),
    ("cmd.toggle_left_dock", "view.left_dock"),
    ("cmd.toggle_right_dock", "view.right_dock"),
    ("cmd.toggle_line_numbers", "view.line_numbers"),
    ("cmd.toggle_calendar", "tools.calendar"),
    ("cmd.next_tab", "tab.next"),
    ("cmd.prev_tab", "tab.prev"),
];

/// Case-insensitive, space-separated subsequence match. Every whitespace term
/// must appear (in order, as a subsequence) in `haystack`.
pub fn fuzzy_match(haystack: &str, query: &str) -> bool {
    let hay = haystack.to_lowercase();
    query
        .split_whitespace()
        .all(|term| subsequence(&hay, &term.to_lowercase()))
}

fn subsequence(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut chars = hay.chars();
    for nc in needle.chars() {
        loop {
            match chars.next() {
                Some(hc) if hc == nc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Parse a `path:line[:col]` suffix. Returns the path part and an optional
/// (line, col) target.
pub fn parse_path_target(input: &str) -> (String, Option<(usize, usize)>) {
    let mut parts = input.splitn(3, ':');
    let path = parts.next().unwrap_or("").to_string();
    let line = parts.next().and_then(|s| s.parse::<usize>().ok());
    let col = parts.next().and_then(|s| s.parse::<usize>().ok());
    match line {
        Some(l) => (path, Some((l, col.unwrap_or(1)))),
        None => (path, None),
    }
}
