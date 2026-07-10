//! The workbench's SQL query editor buffer.
//!
//! A small plain-text editor (lines, cursor, scroll) specialized for SQL:
//! it can find the statement under the cursor — splitting on top-level
//! semicolons, ignoring those inside strings and comments — so Ctrl+Enter /
//! F5 executes only that statement and Alt+Shift+F formats only it
//! (`spec/db`: "Execute at Cursor", "Respects Semicolons").

/// The query editor: lines of SQL plus cursor and scroll state.
#[derive(Debug, Clone)]
pub struct Query {
    /// Buffer lines; always at least one (possibly empty) line.
    lines: Vec<String>,
    /// Cursor line index.
    pub row: usize,
    /// Cursor char column within the line.
    pub col: usize,
    /// First visible line.
    pub scroll: usize,
}

impl Default for Query {
    fn default() -> Self {
        Query { lines: vec![String::new()], row: 0, col: 0, scroll: 0 }
    }
}

impl Query {
    /// The buffer lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The whole buffer as one string.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Insert `c` at the cursor.
    pub fn insert_char(&mut self, c: char) {
        let col = self.byte_col();
        self.lines[self.row].insert(col, c);
        self.col += 1;
    }

    /// Replace the chars from `start` to the cursor on the current line with
    /// `text` (autocomplete acceptance), leaving the cursor after it.
    pub fn replace_prefix(&mut self, start: usize, text: &str) {
        let line = &self.lines[self.row];
        let head: String = line.chars().take(start).collect();
        let tail: String = line.chars().skip(self.col).collect();
        self.lines[self.row] = format!("{head}{text}{tail}");
        self.col = start + text.chars().count();
    }

    /// Split the current line at the cursor.
    pub fn newline(&mut self) {
        let col = self.byte_col();
        let rest = self.lines[self.row].split_off(col);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
    }

    /// Delete the char before the cursor (joining lines at column 0).
    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
            let col = self.byte_col();
            self.lines[self.row].remove(col);
        } else if self.row > 0 {
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.lines[self.row].push_str(&line);
        }
    }

    /// Delete the char under the cursor (joining lines at end of line).
    pub fn delete(&mut self) {
        let len = self.lines[self.row].chars().count();
        if self.col < len {
            let col = self.byte_col();
            self.lines[self.row].remove(col);
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        }
    }

    /// Move the cursor one step in the given direction, clamped.
    pub fn arrow(&mut self, dx: isize, dy: isize) {
        if dy < 0 {
            self.row = self.row.saturating_sub(dy.unsigned_abs());
        } else {
            self.row = (self.row + dy.unsigned_abs()).min(self.lines.len() - 1);
        }
        if dx < 0 {
            if self.col == 0 && self.row > 0 && dy == 0 {
                self.row -= 1;
                self.col = self.lines[self.row].chars().count();
            } else {
                self.col = self.col.saturating_sub(dx.unsigned_abs());
            }
        } else if dx > 0 {
            let len = self.lines[self.row].chars().count();
            if self.col >= len && self.row + 1 < self.lines.len() && dy == 0 {
                self.row += 1;
                self.col = 0;
            } else {
                self.col = (self.col + dx.unsigned_abs()).min(len);
            }
        }
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    /// Move the cursor to the start (`home`) or end of the current line.
    pub fn home_end(&mut self, home: bool) {
        self.col = if home { 0 } else { self.lines[self.row].chars().count() };
    }

    /// Move the cursor a page up or down.
    pub fn page(&mut self, up: bool, n: usize) {
        if up {
            self.row = self.row.saturating_sub(n.max(1));
        } else {
            self.row = (self.row + n.max(1)).min(self.lines.len() - 1);
        }
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    /// Keep the cursor within a window of `height` visible lines.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if self.row >= self.scroll + height {
            self.scroll = self.row + 1 - height;
        }
    }

    /// The char offset of the cursor within [`Query::text`].
    #[must_use]
    pub fn cursor_offset(&self) -> usize {
        self.lines[..self.row].iter().map(|l| l.chars().count() + 1).sum::<usize>() + self.col
    }

    /// The statement under the cursor (or the one just before it), trimmed.
    #[must_use]
    pub fn statement_at_cursor(&self) -> Option<String> {
        let text = self.text();
        let (start, end) = self.span_at_cursor()?;
        let stmt: String = text.chars().skip(start).take(end - start).collect();
        let stmt = stmt.trim().to_string();
        (!stmt.is_empty()).then_some(stmt)
    }

    /// Replace the statement under the cursor with `new`, moving the cursor to
    /// the start of the replacement.
    pub fn replace_statement_at_cursor(&mut self, new: &str) {
        let Some((start, end)) = self.span_at_cursor() else {
            return;
        };
        let text = self.text();
        let head: String = text.chars().take(start).collect();
        let tail: String = text.chars().skip(end).collect();
        let row = head.split('\n').count() - 1;
        self.lines = format!("{head}{new}{tail}").split('\n').map(str::to_string).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = row.min(self.lines.len() - 1);
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    /// The char-offset span of the statement containing (or preceding) the
    /// cursor.
    fn span_at_cursor(&self) -> Option<(usize, usize)> {
        let text = self.text();
        let spans = statement_spans(&text);
        let at = self.cursor_offset();
        spans
            .iter()
            .find(|(s, e)| at >= *s && at <= *e)
            .or_else(|| spans.iter().rev().find(|(s, _)| *s <= at))
            .or_else(|| spans.first())
            .copied()
    }

    /// The cursor's byte offset within the current line.
    fn byte_col(&self) -> usize {
        let line = &self.lines[self.row];
        line.char_indices().nth(self.col).map_or(line.len(), |(i, _)| i)
    }
}

/// Char-offset spans of the statements in `text`, split on top-level
/// semicolons (ignored inside strings and `--` / `/* … */` comments). Spans
/// exclude the semicolon; whitespace-only pieces are dropped.
#[must_use]
pub fn statement_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let (mut single, mut double, mut line_c, mut block_c) = (false, false, false, false);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if line_c {
            line_c = c != '\n';
        } else if block_c {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                block_c = false;
                i += 1;
            }
        } else if single {
            single = c != '\'';
        } else if double {
            double = c != '"';
        } else {
            match c {
                '\'' => single = true,
                '"' => double = true,
                '-' if chars.get(i + 1) == Some(&'-') => line_c = true,
                '/' if chars.get(i + 1) == Some(&'*') => block_c = true,
                ';' => {
                    push_span(&mut out, &chars, start, i);
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    push_span(&mut out, &chars, start, chars.len());
    out
}

/// Record `[start, end)` if it holds any non-whitespace.
fn push_span(out: &mut Vec<(usize, usize)>, chars: &[char], start: usize, end: usize) {
    if chars[start..end].iter().any(|c| !c.is_whitespace()) {
        out.push((start, end));
    }
}

/// Keywords that make a statement a write / DDL (confirmation required).
const WRITE_KEYWORDS: &[&str] = &[
    "insert", "update", "delete", "drop", "alter", "truncate", "create", "grant", "revoke",
    "merge", "replace", "vacuum", "reindex", "attach", "detach",
];

/// Whether `sql` writes or changes schema, so execution should be confirmed.
/// Scans every keyword outside strings and comments — which also catches a
/// CTE like `WITH … AS (…) DELETE FROM …` whose first word is a harmless
/// `WITH` (pgsavvy calls this CTE detection).
#[must_use]
pub fn is_write_statement(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let (mut single, mut double, mut line_c, mut block_c) = (false, false, false, false);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if line_c {
            line_c = c != '\n';
        } else if block_c {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                block_c = false;
                i += 1;
            }
        } else if single {
            single = c != '\'';
        } else if double {
            double = c != '"';
        } else {
            match c {
                '\'' => single = true,
                '"' => double = true,
                '-' if chars.get(i + 1) == Some(&'-') => line_c = true,
                '/' if chars.get(i + 1) == Some(&'*') => block_c = true,
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    if WRITE_KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
                        return true;
                    }
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> Query {
        Query { lines: text.split('\n').map(str::to_string).collect(), ..Query::default() }
    }

    #[test]
    fn typing_newlines_and_backspace_edit_the_buffer() {
        let mut q = Query::default();
        for c in "select 1".chars() {
            q.insert_char(c);
        }
        q.newline();
        q.insert_char('x');
        assert_eq!(q.text(), "select 1\nx");
        q.backspace();
        q.backspace(); // joins the lines
        assert_eq!(q.text(), "select 1");
        assert_eq!((q.row, q.col), (0, 8));
    }

    #[test]
    fn statement_at_cursor_picks_the_one_under_the_cursor() {
        let mut q = query("select 1;\nselect 2;\nselect 3;");
        q.row = 1;
        q.col = 3;
        assert_eq!(q.statement_at_cursor().unwrap(), "select 2");
        // A cursor at the very end belongs to the last statement.
        q.row = 2;
        q.col = 9;
        assert_eq!(q.statement_at_cursor().unwrap(), "select 3");
    }

    #[test]
    fn semicolons_in_strings_and_comments_do_not_split() {
        let q = query("select ';' -- tail; here");
        let spans = statement_spans(&q.text());
        assert_eq!(spans.len(), 1);
        assert_eq!(q.statement_at_cursor().unwrap(), "select ';' -- tail; here");
    }

    #[test]
    fn replace_statement_swaps_only_the_cursor_statement() {
        let mut q = query("select 1; select 2");
        q.row = 0;
        q.col = 14; // inside "select 2"
        q.replace_statement_at_cursor("SELECT\n    2");
        assert_eq!(q.text(), "select 1;SELECT\n    2");
    }

    #[test]
    fn replace_prefix_accepts_a_completion() {
        let mut q = query("select us");
        q.col = 9;
        q.replace_prefix(7, "users");
        assert_eq!(q.text(), "select users");
        assert_eq!(q.col, 12);
    }

    #[test]
    fn write_detection_sees_through_ctes_but_not_strings() {
        assert!(is_write_statement("DELETE FROM t"));
        assert!(is_write_statement("with old as (select 1) delete from t"), "CTE write");
        assert!(is_write_statement("CREATE TABLE t (a int)"));
        assert!(!is_write_statement("SELECT * FROM users"));
        assert!(!is_write_statement("select 'drop table t'"), "keyword inside a string");
        assert!(!is_write_statement("select 1 -- delete later"), "keyword inside a comment");
        assert!(!is_write_statement("select deleted, updated from audit"), "substrings");
    }

    #[test]
    fn arrows_clamp_and_wrap_line_ends() {
        let mut q = query("ab\ncd");
        q.arrow(1, 0);
        q.arrow(1, 0);
        q.arrow(1, 0); // wraps to line 2 start
        assert_eq!((q.row, q.col), (1, 0));
        q.arrow(-1, 0); // wraps back to line 1 end
        assert_eq!((q.row, q.col), (0, 2));
    }
}
