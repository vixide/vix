//! The SQL edit surface: a statement-oriented view of a `.sql` buffer.
//!
//! `vix-edit-sql` mode parses the active buffer into its individual SQL
//! statements (splitting on top-level semicolons, ignoring those inside string
//! literals and comments) and presents them as a navigable list. From there the
//! user can reorder, delete, and format statements (uppercasing keywords), then
//! save the result back to the buffer. It mirrors the other `edit_*` surfaces:
//! pure logic here, rendering and file I/O in the host.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the host should do after [`Editor::handle_key`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The key was handled; nothing further.
    Consumed,
    /// Close the editor without saving.
    Close,
    /// Serialize back to the buffer and save.
    Save,
}

/// Common SQL keywords, uppercased on format.
const KEYWORDS: &[&str] = &[
    "select",
    "from",
    "where",
    "insert",
    "into",
    "values",
    "update",
    "set",
    "delete",
    "create",
    "table",
    "view",
    "index",
    "alter",
    "drop",
    "add",
    "column",
    "primary",
    "key",
    "foreign",
    "references",
    "join",
    "inner",
    "left",
    "right",
    "outer",
    "full",
    "on",
    "group",
    "by",
    "order",
    "having",
    "limit",
    "offset",
    "distinct",
    "as",
    "and",
    "or",
    "not",
    "null",
    "is",
    "in",
    "like",
    "between",
    "exists",
    "union",
    "all",
    "with",
    "case",
    "when",
    "then",
    "else",
    "end",
    "grant",
    "usage",
    "to",
    "default",
    "constraint",
    "unique",
    "check",
    "begin",
    "commit",
    "rollback",
    "returning",
    "using",
    "cascade",
    "if",
    "function",
    "trigger",
    "extension",
    "role",
    "user",
    "asc",
    "desc",
    "count",
    "sum",
    "avg",
    "min",
    "max",
];

/// Maximum retained undo steps.
const HISTORY_CAP: usize = 200;

/// A point-in-time snapshot for undo/redo.
#[derive(Clone)]
struct Snapshot {
    statements: Vec<String>,
    sel: usize,
}

/// The SQL statement list with a cursor, scroll offset, and edit history.
pub struct Editor {
    statements: Vec<String>,
    sel: usize,
    scroll: usize,
    dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    /// Parse `text` into statements.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Editor {
            statements: split_statements(text),
            sel: 0,
            scroll: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Number of statements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.statements.len()
    }

    /// Whether there are no statements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }

    /// The highlighted statement index.
    #[must_use]
    pub fn sel(&self) -> usize {
        self.sel
    }

    /// The first visible row.
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether there are unsaved edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The statement text at `i`.
    #[must_use]
    pub fn statement(&self, i: usize) -> &str {
        self.statements.get(i).map_or("", String::as_str)
    }

    /// A short kind label (`SELECT`, `INSERT`, …) for the statement at `i`.
    #[must_use]
    pub fn kind(&self, i: usize) -> &'static str {
        kind_of(self.statement(i))
    }

    /// A one-line preview of the statement at `i` (whitespace collapsed).
    #[must_use]
    pub fn preview(&self, i: usize) -> String {
        self.statement(i)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Join the statements back into buffer text (one per block, `;`-terminated).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for s in &self.statements {
            out.push_str(s.trim());
            out.push_str(";\n\n");
        }
        // Trim the trailing blank line, keeping a single newline.
        while out.ends_with('\n') {
            out.pop();
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// Mark the current content as saved (clears the dirty flag).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Keep the selection within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + height {
            self.scroll = self.sel + 1 - height;
        }
        let max_scroll = self.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Handle a key, returning what the host should do.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('s') if ctrl => return Outcome::Save,
            KeyCode::Up if alt => self.move_stmt(true),
            KeyCode::Down if alt => self.move_stmt(false),
            KeyCode::Char('K') => self.move_stmt(true),
            KeyCode::Char('J') => self.move_stmt(false),
            KeyCode::Up | KeyCode::Char('k') => self.step(true, 1),
            KeyCode::Down | KeyCode::Char('j') => self.step(false, 1),
            KeyCode::PageUp => self.step(true, page.max(1)),
            KeyCode::PageDown => self.step(false, page.max(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = self.len().saturating_sub(1),
            KeyCode::Char('d') | KeyCode::Delete => self.delete(),
            KeyCode::Char('f') => self.format_selected(),
            KeyCode::Char('F') => self.format_all(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Move the selection `n` rows up or down, clamped.
    fn step(&mut self, up: bool, n: usize) {
        if up {
            self.sel = self.sel.saturating_sub(n);
        } else {
            self.sel = (self.sel + n).min(self.len().saturating_sub(1));
        }
    }

    /// Reorder the selected statement up or down by one.
    fn move_stmt(&mut self, up: bool) {
        let n = self.len();
        if n < 2 {
            return;
        }
        let target = if up {
            if self.sel == 0 {
                return;
            }
            self.sel - 1
        } else {
            if self.sel + 1 >= n {
                return;
            }
            self.sel + 1
        };
        self.push_undo();
        self.statements.swap(self.sel, target);
        self.sel = target;
        self.dirty = true;
    }

    /// Delete the selected statement.
    fn delete(&mut self) {
        if self.statements.is_empty() {
            return;
        }
        self.push_undo();
        self.statements.remove(self.sel);
        if self.sel >= self.statements.len() {
            self.sel = self.statements.len().saturating_sub(1);
        }
        self.dirty = true;
    }

    /// Uppercase keywords in the selected statement.
    fn format_selected(&mut self) {
        if self.statements.is_empty() {
            return;
        }
        self.push_undo();
        let i = self.sel;
        self.statements[i] = format_sql(&self.statements[i]);
        self.dirty = true;
    }

    /// Uppercase keywords in every statement.
    fn format_all(&mut self) {
        if self.statements.is_empty() {
            return;
        }
        self.push_undo();
        for s in &mut self.statements {
            *s = format_sql(s);
        }
        self.dirty = true;
    }

    /// Push the current state onto the undo stack (clearing redo).
    fn push_undo(&mut self) {
        self.undo.push(Snapshot {
            statements: self.statements.clone(),
            sel: self.sel,
        });
        if self.undo.len() > HISTORY_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Undo the last edit.
    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(Snapshot {
                statements: self.statements.clone(),
                sel: self.sel,
            });
            self.statements = prev.statements;
            self.sel = prev.sel.min(self.statements.len().saturating_sub(1));
            self.dirty = true;
        }
    }

    /// Redo the last undone edit.
    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(Snapshot {
                statements: self.statements.clone(),
                sel: self.sel,
            });
            self.statements = next.statements;
            self.sel = next.sel.min(self.statements.len().saturating_sub(1));
            self.dirty = true;
        }
    }
}

/// The kind label for a statement (its leading keyword, uppercased), or `SQL`.
fn kind_of(stmt: &str) -> &'static str {
    let first = stmt
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    match first.as_str() {
        "SELECT" => "SELECT",
        "INSERT" => "INSERT",
        "UPDATE" => "UPDATE",
        "DELETE" => "DELETE",
        "CREATE" => "CREATE",
        "ALTER" => "ALTER",
        "DROP" => "DROP",
        "WITH" => "WITH",
        "GRANT" => "GRANT",
        "TRUNCATE" => "TRUNCATE",
        "BEGIN" => "BEGIN",
        "COMMIT" => "COMMIT",
        "" => "EMPTY",
        _ => "SQL",
    }
}

/// Split SQL into statements on top-level semicolons, ignoring semicolons inside
/// single/double-quoted strings, line comments (`-- …`), and block comments
/// (`/* … */`). Blank pieces are dropped; each is trimmed.
#[must_use]
pub fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = sql.chars().peekable();
    let (mut single, mut double, mut line_c, mut block_c) = (false, false, false, false);
    while let Some(c) = chars.next() {
        if line_c {
            cur.push(c);
            if c == '\n' {
                line_c = false;
            }
            continue;
        }
        if block_c {
            cur.push(c);
            if c == '*' && chars.peek() == Some(&'/') {
                if let Some(slash) = chars.next() {
                    cur.push(slash);
                }
                block_c = false;
            }
            continue;
        }
        if single {
            cur.push(c);
            if c == '\'' {
                single = false;
            }
            continue;
        }
        if double {
            cur.push(c);
            if c == '"' {
                double = false;
            }
            continue;
        }
        match c {
            '\'' => {
                single = true;
                cur.push(c);
            }
            '"' => {
                double = true;
                cur.push(c);
            }
            '-' if chars.peek() == Some(&'-') => {
                line_c = true;
                cur.push(c);
            }
            '/' if chars.peek() == Some(&'*') => {
                block_c = true;
                cur.push(c);
            }
            ';' => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Uppercase SQL keywords in `stmt`, leaving string/comment spans untouched.
#[must_use]
pub fn format_sql(stmt: &str) -> String {
    let mut out = String::with_capacity(stmt.len());
    let chars: Vec<char> = stmt.chars().collect();
    let mut i = 0;
    let (mut single, mut double, mut line_c, mut block_c) = (false, false, false, false);
    while i < chars.len() {
        let c = chars[i];
        if line_c {
            out.push(c);
            if c == '\n' {
                line_c = false;
            }
            i += 1;
            continue;
        }
        if block_c {
            out.push(c);
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                out.push('/');
                i += 2;
                block_c = false;
                continue;
            }
            i += 1;
            continue;
        }
        if single {
            out.push(c);
            if c == '\'' {
                single = false;
            }
            i += 1;
            continue;
        }
        if double {
            out.push(c);
            if c == '"' {
                double = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                single = true;
                out.push(c);
                i += 1;
            }
            '"' => {
                double = true;
                out.push(c);
                i += 1;
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                line_c = true;
                out.push(c);
                i += 1;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                block_c = true;
                out.push(c);
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
                    out.push_str(&word.to_ascii_uppercase());
                } else {
                    out.push_str(&word);
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_top_level_semicolons_only() {
        let sql = "select 1; insert into t values (';'); -- a; b\nupdate t set x=1;";
        let s = split_statements(sql);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], "select 1");
        assert!(
            s[1].contains("(';')"),
            "semicolon inside the string is kept"
        );
        assert!(
            s[2].starts_with("-- a; b"),
            "comment semicolon is not a split"
        );
    }

    #[test]
    fn round_trips_through_text() {
        let e = Editor::from_text("select 1;\nselect 2;");
        assert_eq!(e.to_text(), "select 1;\n\nselect 2;\n");
    }

    #[test]
    fn formats_keywords_outside_strings() {
        assert_eq!(
            format_sql("select * from t where a='from'"),
            "SELECT * FROM t WHERE a='from'"
        );
    }

    #[test]
    fn reports_statement_kinds() {
        let e = Editor::from_text("select 1; insert into t values (1); create table x();");
        assert_eq!(e.kind(0), "SELECT");
        assert_eq!(e.kind(1), "INSERT");
        assert_eq!(e.kind(2), "CREATE");
    }

    #[test]
    fn delete_and_move_and_undo() {
        let mut e = Editor::from_text("a; b; c");
        let page = 4;
        e.handle_key(KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE), page); // move 'a' down
        assert_eq!(e.statement(1), "a");
        assert_eq!(e.sel(), 1);
        e.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE), page); // delete 'a'
        assert_eq!(e.len(), 2);
        e.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE), page); // undo delete
        assert_eq!(e.len(), 3);
    }

    #[test]
    fn format_all_uppercases_every_statement() {
        let mut e = Editor::from_text("select 1; update t set a=1");
        e.handle_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE), 4);
        assert_eq!(e.statement(0), "SELECT 1");
        assert!(e.statement(1).starts_with("UPDATE t SET"));
        assert!(e.is_dirty());
    }
}
