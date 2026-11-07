//! The workbench's SQL query editor buffer.
//!
//! A small plain-text editor (lines, cursor, scroll) specialized for SQL:
//! it can find the statement under the cursor — splitting on top-level
//! semicolons, ignoring those inside strings and comments — so Ctrl+Enter /
//! F5 executes only that statement and Alt+Shift+F formats only it
//! (`spec/index.md`: "Execute at Cursor", "Respects Semicolons").
//!
//! # Coordinates
//!
//! The cursor is `(row, col)` where `col` counts **chars**, not bytes, so
//! non-ASCII SQL edits correctly; the private `byte_col` converts when
//! slicing a line. Statement spans from [`statement_spans`] are likewise
//! **char offsets** into [`Query::text`] (lines joined with `\n`), which is
//! what [`Query::cursor_offset`] returns, so the two compare directly.
//!
//! The module also hosts [`is_write_statement`], the client-side guard behind
//! the read-only default and the write-confirmation prompt; it is a
//! conservative lexical scan, not a SQL parser. Owning spec:
//! `crates/vix-db/spec/index.md`.

/// The query editor: lines of SQL plus cursor and scroll state.
///
/// Invariant: `lines` is never empty and `row`/`col` always address a valid
/// position (each mutator re-clamps), so indexing `lines[row]` cannot panic.
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
        Query {
            lines: vec![String::new()],
            row: 0,
            col: 0,
            scroll: 0,
        }
    }
}

impl Query {
    /// A buffer holding `text` (split on `\n`; a trailing `\r` is dropped
    /// from each line), with the cursor at the start — how a `.sql` file or
    /// an editor buffer is loaded into the workbench (T786).
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::editor::Query;
    /// let q = Query::from_text("select 1;\r\nselect 2;");
    /// assert_eq!(q.lines(), ["select 1;", "select 2;"]);
    /// assert_eq!((q.row, q.col), (0, 0));
    /// ```
    #[must_use]
    pub fn from_text(text: &str) -> Query {
        Query {
            lines: text
                .split('\n')
                .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
                .collect(),
            ..Query::default()
        }
    }

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

    /// Insert `c` at the cursor and advance past it. A `\n` is not handled
    /// specially here; use [`Query::newline`] to split lines.
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
    ///
    /// `dx`/`dy` are signed step counts. Pure horizontal moves (`dy == 0`)
    /// wrap across line boundaries (left at column 0 goes to the previous
    /// line's end); vertical moves keep the column, clamped to the new line.
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
        self.col = if home {
            0
        } else {
            self.lines[self.row].chars().count()
        };
    }

    /// Move the cursor a page up or down.
    pub fn page(&mut self, up: bool, n: usize) {
        self.row = if up {
            vix_list_state::page_up(self.row, n)
        } else {
            vix_list_state::page_down(self.row, n, self.lines.len())
        };
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    /// Keep the cursor within a window of `height` visible lines.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.row, self.scroll, height, self.lines.len());
    }

    /// The char offset of the cursor within [`Query::text`] (each preceding
    /// line contributes its char count plus one for the joining newline).
    #[must_use]
    pub fn cursor_offset(&self) -> usize {
        self.lines[..self.row]
            .iter()
            .map(|l| l.chars().count() + 1)
            .sum::<usize>()
            + self.col
    }

    /// Move the cursor to char offset `offset` of [`Query::text`], clamped
    /// to the end of the buffer (the inverse of [`Query::cursor_offset`]).
    /// Used to put the cursor on a server-reported error position (T775).
    pub fn set_cursor_offset(&mut self, offset: usize) {
        let mut left = offset;
        for (row, line) in self.lines.iter().enumerate() {
            let len = line.chars().count();
            if left <= len {
                self.row = row;
                self.col = left;
                return;
            }
            left -= len + 1;
        }
        self.row = self.lines.len() - 1;
        self.col = self.lines[self.row].chars().count();
    }

    /// The char offset in [`Query::text`] where the statement under the
    /// cursor starts, after its leading whitespace: where
    /// [`Query::statement_at_cursor`]'s text begins.
    #[must_use]
    pub fn statement_offset_at_cursor(&self) -> Option<usize> {
        let text = self.text();
        let (start, end) = self.span_at_cursor()?;
        let lead = text
            .chars()
            .skip(start)
            .take(end - start)
            .take_while(|c| c.is_whitespace())
            .count();
        (lead < end - start).then_some(start + lead)
    }

    /// The statement under the cursor (or the one just before it), trimmed.
    ///
    /// `None` when the buffer holds no non-whitespace statement. Backs
    /// execute-at-cursor (F5/Ctrl+Enter), format, explain and optimize.
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::editor::Query;
    /// let mut q = Query::default();
    /// for c in "select 1; select 2".chars() {
    ///     q.insert_char(c);
    /// }
    /// assert_eq!(q.statement_at_cursor().as_deref(), Some("select 2"));
    /// ```
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
    ///
    /// Only surrounding text is preserved verbatim (including the leading
    /// whitespace of the replaced span). The cursor lands on the first line of
    /// the replacement, keeping its old column clamped to that line.
    pub fn replace_statement_at_cursor(&mut self, new: &str) {
        let Some((start, end)) = self.span_at_cursor() else {
            return;
        };
        let text = self.text();
        let head: String = text.chars().take(start).collect();
        let tail: String = text.chars().skip(end).collect();
        let row = head.split('\n').count() - 1;
        self.lines = format!("{head}{new}{tail}")
            .split('\n')
            .map(str::to_string)
            .collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = row.min(self.lines.len() - 1);
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    /// The char-offset span of the statement containing (or preceding) the
    /// cursor.
    ///
    /// Fallback order: a span containing the cursor (inclusive of its end, so
    /// a cursor right after the last char counts), else the nearest span
    /// *before* it (cursor in whitespace after a `;`), else the first span.
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
        line.char_indices()
            .nth(self.col)
            .map_or(line.len(), |(i, _)| i)
    }
}

/// Char-offset spans of the statements in `text`, split on top-level
/// semicolons (ignored inside strings and `--` / `/* … */` comments). Spans
/// exclude the semicolon; whitespace-only pieces are dropped.
///
/// This is a small lexer, not a parser: it does not understand `PostgreSQL`
/// dollar-quoted bodies (`$$ … $$`), so a `;` inside one splits the statement.
/// Unterminated strings/comments simply run to the end of the text.
///
/// # Examples
///
/// ```
/// use vix_db::editor::statement_spans;
/// // The `;` inside the string literal does not split.
/// assert_eq!(statement_spans("select ';'; select 2"), vec![(0, 10), (11, 20)]);
/// ```
#[must_use]
pub fn statement_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    // Lexer state: inside a '…' string, a "…" identifier, a `--` line comment
    // or a `/* … */` block comment. At most one is set at a time; the checks
    // below test them in that exclusive order. Doubled quotes (`''`) need no
    // special case: closing then immediately re-opening nets out the same.
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

/// Record the char-offset span `[start, end)` if it holds any non-whitespace
/// (so blank segments between consecutive `;` never become statements).
fn push_span(out: &mut Vec<(usize, usize)>, chars: &[char], start: usize, end: usize) {
    if chars[start..end].iter().any(|c| !c.is_whitespace()) {
        out.push((start, end));
    }
}

/// The psql meta-commands in `text`, as `(line, command)` pairs with
/// one-based line numbers: every line whose first non-blank character is a
/// backslash, outside strings, quoted identifiers, comments and
/// dollar-quoted bodies (`\copy`, `\connect`, `\c`, `\i`, `\set`, …).
///
/// They are commands of the psql client, not SQL, so a server would reject
/// them; the workbench refuses to run a script holding any (T786). `command`
/// is the backslash word, e.g. `\copy`.
///
/// # Examples
///
/// ```
/// use vix_db::editor::meta_commands;
/// let script = "\\set ON_ERROR_STOP on\nselect '\n\\not a command';\n  \\i other.sql\n";
/// assert_eq!(
///     meta_commands(script),
///     vec![(1, "\\set".to_string()), (4, "\\i".to_string())]
/// );
/// ```
#[must_use]
pub fn meta_commands(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut single, mut double, mut line_c) = (false, false, false);
    // Block comments nest in PostgreSQL; a dollar quote ends at its own tag.
    let mut block_depth = 0usize;
    let mut dollar: Option<String> = None;
    let mut line = 1usize;
    let mut at_line_start = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            at_line_start = true;
            line_c = false;
            i += 1;
            continue;
        }
        let in_code = !single && !double && !line_c && block_depth == 0 && dollar.is_none();
        if at_line_start && !c.is_whitespace() {
            at_line_start = false;
            if in_code && c == '\\' {
                let end = (i + 1..chars.len())
                    .find(|&j| chars[j].is_whitespace())
                    .unwrap_or(chars.len());
                out.push((line, chars[i..end].iter().collect()));
                // The rest of the line belongs to the command.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
        }
        if line_c {
            // Skips to the newline handled above.
        } else if block_depth > 0 {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                block_depth -= 1;
                i += 1;
            } else if c == '/' && chars.get(i + 1) == Some(&'*') {
                block_depth += 1;
                i += 1;
            }
        } else if let Some(tag) = &dollar {
            if c == '$'
                && chars[i..]
                    .iter()
                    .take(tag.chars().count())
                    .copied()
                    .eq(tag.chars())
            {
                i += tag.chars().count();
                dollar = None;
                continue;
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
                '/' if chars.get(i + 1) == Some(&'*') => {
                    block_depth = 1;
                    i += 1;
                }
                '$' => {
                    if let Some(tag) = dollar_tag(&chars[i..]) {
                        i += tag.chars().count();
                        dollar = Some(tag);
                        continue;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    out
}

/// The dollar-quote opener at the start of `chars` (`$$` or `$tag$`), if any.
/// A tag is letters, digits and `_`, not starting with a digit — so a
/// positional parameter like `$1` is not a quote.
fn dollar_tag(chars: &[char]) -> Option<String> {
    let close = chars.iter().skip(1).position(|&c| c == '$')? + 1;
    let tag = &chars[1..close];
    let valid = tag.first().is_none_or(|c| !c.is_ascii_digit())
        && tag.iter().all(|c| c.is_alphanumeric() || *c == '_');
    valid.then(|| chars[..=close].iter().collect())
}

/// Keywords that make a statement a write / DDL (confirmation required).
/// Lowercase; compared case-insensitively against whole words.
pub(crate) const WRITE_KEYWORDS: &[&str] = &[
    "insert", "update", "delete", "drop", "alter", "truncate", "create", "grant", "revoke",
    "merge", "replace", "vacuum", "reindex", "attach", "detach",
];

/// Whether `sql` writes or changes schema, so execution should be confirmed.
/// Scans every keyword outside strings and comments — which also catches a
/// CTE like `WITH … AS (…) DELETE FROM …` whose first word is a harmless
/// `WITH` (pgsavvy calls this CTE detection).
///
/// The scan is deliberately conservative: it matches whole words anywhere
/// outside strings and comments, so a harmless call such as `replace(x, …)`
/// or a column named `update` is reported as a write (an extra confirmation
/// at worst, never a missed one). Only a keyword split across tokens or hidden
/// in a dollar-quoted body could evade it, and the database-level read-only
/// pragma (see [`crate::connect::read_only_sql`]) backs it up.
///
/// # Examples
///
/// ```
/// use vix_db::editor::is_write_statement;
/// assert!(is_write_statement("WITH x AS (SELECT 1) DELETE FROM t"));
/// assert!(!is_write_statement("select 'drop table' -- delete"));
/// ```
#[must_use]
pub fn is_write_statement(sql: &str) -> bool {
    has_keyword(sql, WRITE_KEYWORDS)
}

/// Keywords that destroy data or schema wholesale; these always ask before
/// running, on any connection and inside a transaction too (T781).
const DESTRUCTIVE_KEYWORDS: &[&str] = &["drop", "truncate"];

/// Whether `sql` contains `DROP` or `TRUNCATE` outside strings and comments
/// (the same whole-word scan as [`is_write_statement`]). Such a statement
/// always asks for confirmation, whatever the connection's environment and
/// even inside an explicit transaction, since `TRUNCATE` and most `DROP`s are
/// not undone by `ROLLBACK` on every engine (T781).
///
/// # Examples
///
/// ```
/// use vix_db::editor::is_destructive_statement;
/// assert!(is_destructive_statement("DROP TABLE users"));
/// assert!(is_destructive_statement("truncate orders"));
/// assert!(!is_destructive_statement("DELETE FROM t WHERE id = 1"));
/// assert!(!is_destructive_statement("select 'drop' -- truncate"));
/// ```
#[must_use]
pub fn is_destructive_statement(sql: &str) -> bool {
    has_keyword(sql, DESTRUCTIVE_KEYWORDS)
}

/// Whether running `sql` must first show it and ask (T781), given the
/// connection's `environment` and whether an explicit transaction is open.
///
/// A read never asks. `DROP` and `TRUNCATE` always ask. On
/// [`crate::connect::Environment::Production`] every write asks, even inside
/// a transaction. Elsewhere a write asks only outside a transaction, where
/// `ROLLBACK` cannot undo it.
///
/// # Examples
///
/// ```
/// use vix_db::connect::Environment;
/// use vix_db::editor::needs_confirmation;
/// let update = "UPDATE t SET x = 1";
/// assert!(needs_confirmation(update, Environment::Local, false));
/// assert!(!needs_confirmation(update, Environment::Local, true));
/// assert!(needs_confirmation(update, Environment::Production, true));
/// assert!(needs_confirmation("DROP TABLE t", Environment::Local, true));
/// assert!(!needs_confirmation("SELECT 1", Environment::Production, false));
/// ```
#[must_use]
pub fn needs_confirmation(
    sql: &str,
    environment: crate::connect::Environment,
    in_transaction: bool,
) -> bool {
    if !is_write_statement(sql) {
        return false;
    }
    is_destructive_statement(sql) || environment.is_production() || !in_transaction
}

/// Whether any of `keywords` (lowercase) appears in `sql` as a whole word
/// outside strings and comments.
pub(crate) fn has_keyword(sql: &str, keywords: &[&str]) -> bool {
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
                    // Consume a whole identifier so `updated_at` is one word
                    // (not the keyword `update`), then skip the trailing `i += 1`.
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    if keywords.contains(&word.to_ascii_lowercase().as_str()) {
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
    use crate::connect::Environment;

    #[test]
    fn destructive_statements_are_drop_and_truncate_only() {
        assert!(is_destructive_statement("drop table t"));
        assert!(is_destructive_statement("WITH x AS (SELECT 1) TRUNCATE t"));
        assert!(is_destructive_statement("/* c */ DROP INDEX i"));
        assert!(!is_destructive_statement("delete from t"));
        assert!(!is_destructive_statement("update t set dropped = 1"));
        assert!(!is_destructive_statement(
            "select 'drop' from t -- truncate"
        ));
        assert!(!is_destructive_statement("select 1 /* drop */"));
    }

    #[test]
    fn confirmation_policy_per_environment_and_transaction() {
        for env in Environment::ALL {
            // Reads never ask.
            assert!(!needs_confirmation("select * from t", env, false));
            assert!(!needs_confirmation("select * from t", env, true));
            // DROP and TRUNCATE always ask.
            assert!(needs_confirmation("drop table t", env, true));
            assert!(needs_confirmation("truncate t", env, false));
            // Any write outside a transaction asks.
            assert!(needs_confirmation("insert into t values (1)", env, false));
        }
        // Inside a transaction only production asks for a plain write.
        assert!(!needs_confirmation(
            "delete from t",
            Environment::Local,
            true
        ));
        assert!(!needs_confirmation(
            "delete from t",
            Environment::Staging,
            true
        ));
        assert!(needs_confirmation(
            "delete from t",
            Environment::Production,
            true
        ));
    }

    fn query(text: &str) -> Query {
        Query {
            lines: text.split('\n').map(str::to_string).collect(),
            ..Query::default()
        }
    }

    #[test]
    fn set_cursor_offset_is_the_inverse_of_cursor_offset() {
        let mut q = query("select 1;\nselect  é;\n");
        for offset in 0..=q.text().chars().count() {
            q.set_cursor_offset(offset);
            assert_eq!(q.cursor_offset(), offset, "round-trips at {offset}");
        }
        q.set_cursor_offset(10_000);
        assert_eq!((q.row, q.col), (2, 0), "clamped to the end of the buffer");
    }

    #[test]
    fn statement_offset_skips_leading_whitespace() {
        let mut q = query("select 1;\n   select 2");
        q.row = 1;
        q.col = 5;
        assert_eq!(q.statement_offset_at_cursor(), Some(13));
        let text = q.text();
        let stmt: String = text.chars().skip(13).collect();
        assert_eq!(stmt, q.statement_at_cursor().unwrap());
        assert_eq!(query("   ").statement_offset_at_cursor(), None);
    }

    #[test]
    fn meta_commands_are_found_outside_strings_comments_and_dollar_bodies() {
        let script = "\\connect app\n\
                      select 1; -- \\not this\n\
                      /* outer /* nested */\n\
                      \\still a comment */\n\
                      create function f() returns int as $body$\n\
                      \\inside the body\n\
                      $body$ language sql;\n\
                      select $1, $$\n\
                      \\in dollar\n\
                      $$;\n\
                      \t\\copy t to 'x.csv'\n\
                      select \"a\n\
                      \\b\";\n\
                      \\c other";
        assert_eq!(
            meta_commands(script),
            vec![
                (1, "\\connect".to_string()),
                (11, "\\copy".to_string()),
                (14, "\\c".to_string()),
            ]
        );
        assert!(
            meta_commands("select 1 \\gset").is_empty(),
            "only a line's first character counts"
        );
        assert_eq!(meta_commands(""), Vec::new());
    }

    #[test]
    fn from_text_loads_lines_and_keeps_the_invariant() {
        let q = Query::from_text("");
        assert_eq!(q.lines(), [""]);
        let q = Query::from_text("a\nb\n");
        assert_eq!(q.lines(), ["a", "b", ""]);
        assert_eq!(q.text(), "a\nb\n");
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
        assert!(
            is_write_statement("with old as (select 1) delete from t"),
            "CTE write"
        );
        assert!(is_write_statement("CREATE TABLE t (a int)"));
        assert!(!is_write_statement("SELECT * FROM users"));
        assert!(
            !is_write_statement("select 'drop table t'"),
            "keyword inside a string"
        );
        assert!(
            !is_write_statement("select 1 -- delete later"),
            "keyword inside a comment"
        );
        assert!(
            !is_write_statement("select deleted, updated from audit"),
            "substrings"
        );
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
