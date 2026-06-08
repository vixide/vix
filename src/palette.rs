//! Command palette (Ctrl+P) with prefix-driven modes.
//!
//! | Prefix | Mode        |
//! |--------|-------------|
//! | (none) | File finder |
//! | `>`    | Commands    |
//! | `#`    | Buffers     |
//! | `:`    | Go to line  |

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Files,
    Commands,
    Buffers,
    GotoLine,
}

impl Mode {
    pub fn from_input(input: &str) -> Mode {
        match input.chars().next() {
            Some('>') => Mode::Commands,
            Some('#') => Mode::Buffers,
            Some(':') => Mode::GotoLine,
            _ => Mode::Files,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Files => "Files",
            Mode::Commands => "Commands",
            Mode::Buffers => "Buffers",
            Mode::GotoLine => "Go to line",
        }
    }
}

/// What happens when an entry is accepted.
#[derive(Clone)]
pub enum Action {
    OpenFile(PathBuf, Option<(usize, usize)>),
    RunCommand(String),
    SwitchBuffer(usize),
    GotoLine(usize),
}

#[derive(Clone)]
pub struct Entry {
    pub label: String,
    pub action: Action,
}

pub struct Palette {
    pub input: String,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::new()
    }
}

impl Palette {
    pub fn new() -> Self {
        Palette {
            input: String::new(),
            entries: Vec::new(),
            selected: 0,
        }
    }

    pub fn mode(&self) -> Mode {
        Mode::from_input(&self.input)
    }

    /// The query with any mode-prefix stripped.
    pub fn query(&self) -> &str {
        let s = self.input.as_str();
        match self.mode() {
            Mode::Files => s,
            _ => s.get(1..).unwrap_or(""),
        }
    }

    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn insert(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.selected = 0;
    }
}

/// Commands offered in `>` mode. `(label, action)` where the action is the same
/// identifier the menu bar dispatches.
pub const COMMANDS: &[(&str, &str)] = &[
    ("New File", "file.new"),
    ("Open File\u{2026}", "file.open"),
    ("Save", "file.save"),
    ("Save As\u{2026}", "file.save_as"),
    ("Close Tab", "file.close"),
    ("Quit", "file.quit"),
    ("Undo", "edit.undo"),
    ("Redo", "edit.redo"),
    ("Find", "edit.find"),
    ("Find & Replace", "edit.replace"),
    ("Query Replace", "edit.query_replace"),
    ("Search in Project", "search.project"),
    ("Search and Replace in Project", "search.project_replace"),
    ("Toggle Line Numbers", "tools.line_numbers"),
    ("Toggle Explorer", "view.explorer"),
    ("Toggle Messages", "view.messages"),
    ("Toggle Calendar", "tools.calendar"),
    ("Next Tab", "tab.next"),
    ("Previous Tab", "tab.prev"),
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
