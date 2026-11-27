//! The SQL edit surface: a statement-oriented view of a `.sql` buffer.
//!
//! `vix-edit-sql` mode parses the active buffer into its individual SQL
//! statements (splitting on top-level semicolons, ignoring those inside string
//! literals and comments) and presents them as a navigable list. From there the
//! user can reorder, delete, and format statements (uppercasing keywords), then
//! save the result back to the buffer. It mirrors the other `edit_*` surfaces:
//! pure logic here, rendering and file I/O in the host.
//!
//! Known scope limits of the hand-rolled scanner (shared by
//! [`split_statements`] and [`format_sql`]): it understands `'...'` / `"..."`
//! strings, `-- ...` and `/* ... */` comments only. A doubled quote (`''`) works
//! by closing and immediately reopening the string; PostgreSQL dollar-quoted
//! bodies (`$$ ... $$`) are *not* recognised, so a `;` inside one splits the
//! statement. Block comments do not nest.
//!
//! Owning spec: `crates/vix-edit-sql/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

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

/// Common SQL keywords, uppercased on format. Lowercase, matched
/// case-insensitively. Includes a few very common aggregate names (`count`,
/// `sum`, ...) and words that are also frequent identifiers (`user`, `key`),
/// so formatting can capitalise an identically-named column; it is a
/// readability aid, not a parser.
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

/// Maximum retained undo steps (oldest dropped first).
const HISTORY_CAP: usize = 200;

/// A point-in-time snapshot for undo/redo.
#[derive(Clone)]
struct Snapshot {
    /// The full statement list at that moment (cheap: statements are small).
    statements: Vec<String>,
    /// The selection to restore with it.
    sel: usize,
}

/// The SQL statement list with a cursor, scroll offset, and edit history.
pub struct Editor {
    /// One trimmed statement per entry, without its terminating `;`.
    statements: Vec<String>,
    /// Highlighted statement index (kept `< len` except when empty).
    sel: usize,
    /// First visible row; maintained by [`Editor::ensure_visible`].
    scroll: usize,
    /// Set by any edit (including undo/redo), cleared by `mark_saved`.
    dirty: bool,
    /// Snapshots to restore on `u`, newest last, capped at `HISTORY_CAP`.
    undo: Vec<Snapshot>,
    /// Snapshots undone and re-appliable with Ctrl-R; cleared by a new edit.
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
    ///
    /// # Examples
    ///
    /// ```
    /// let e = vix_edit_sql::Editor::from_text("select 1; select 2");
    /// assert_eq!(e.to_text(), "select 1;\n\nselect 2;\n");
    /// ```
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for s in &self.statements {
            let s = s.trim();
            out.push_str(s);
            // A statement ending in a `--` comment gets its `;` on the
            // next line, or the comment would swallow it (T831).
            if scan(s).1 {
                out.push('\n');
            }
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
        self.scroll = vix_list_state::ensure_visible(self.sel, self.scroll, height, self.len());
    }

    /// Handle a key, returning what the host should do.
    ///
    /// Vim-style `j`/`k`, arrows and paging move the selection; `J`/`K` or
    /// Alt-Up/Down reorder; `d`/Delete removes; `f`/`F` format the selected/all
    /// statements; `u`/Ctrl-R undo/redo; Ctrl-S saves; Esc/`q` closes.
    /// `page` is the host's visible row count, used for PageUp/PageDown.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('s') if ctrl => return Outcome::Save,
            // The Alt arms must precede the plain Up/Down arms below, since
            // match arms are tried in order.
            KeyCode::Up if alt => self.move_stmt(true),
            KeyCode::Down if alt => self.move_stmt(false),
            KeyCode::Char('K') => self.move_stmt(true),
            KeyCode::Char('J') => self.move_stmt(false),
            KeyCode::Up | KeyCode::Char('k') => self.step(true, 1),
            KeyCode::Down | KeyCode::Char('j') => self.step(false, 1),
            KeyCode::PageUp => self.step(true, page.max(1)),
            KeyCode::PageDown => self.step(false, page.max(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = sub_or(self.len(), 1, 0),
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
            self.sel = sub_or(self.sel, n, 0);
        } else {
            self.sel = add_or(self.sel, n, usize::MAX).min(sub_or(self.len(), 1, 0));
        }
    }

    /// Reorder the selected statement up or down by one.
    fn move_stmt(&mut self, up: bool) {
        let n = self.len();
        if n < 2 {
            return;
        }
        let target = if up {
            let Some(earlier) = self.sel.checked_sub(1) else {
                return;
            };
            earlier
        } else {
            let later = add_or(self.sel, 1, usize::MAX);
            if later >= n {
                return;
            }
            later
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
            self.sel = sub_or(self.statements.len(), 1, 0);
        }
        self.dirty = true;
    }

    /// Uppercase keywords in the selected statement.
    fn format_selected(&mut self) {
        if self.statements.is_empty() {
            return;
        }
        self.push_undo();
        if let Some(stmt) = self.statements.get_mut(self.sel) {
            *stmt = format_sql(stmt);
        }
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
    /// Call *before* mutating, so the snapshot is the pre-edit state.
    fn push_undo(&mut self) {
        // Delegates to `vix-capped-stack` (Run H, T525) -- this crate and
        // four siblings used to each hand-roll this identical clamp.
        vix_capped_stack::push_capped(
            &mut self.undo,
            Snapshot {
                statements: self.statements.clone(),
                sel: self.sel,
            },
            HISTORY_CAP,
        );
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
            self.sel = prev.sel.min(sub_or(self.statements.len(), 1, 0));
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
            self.sel = next.sel.min(sub_or(self.statements.len(), 1, 0));
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
///
/// The `;` terminators are consumed, not kept; see [`Editor::to_text`] for the
/// inverse. See the crate docs for unsupported constructs (dollar quoting).
///
/// # Examples
///
/// ```
/// use vix_edit_sql::split_statements;
/// assert_eq!(
///     split_statements("select ';'; -- c;\nselect 2;"),
///     vec!["select ';'", "-- c;\nselect 2"]
/// );
/// ```
#[must_use]
pub fn split_statements(sql: &str) -> Vec<String> {
    scan(sql).0
}

/// [`split_statements`], plus whether `sql` ends inside a `--` line
/// comment (so a `;` appended to it would be commented out).
fn scan(sql: &str) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = sql.chars().peekable();
    // Lexer state: at most one of these is true at a time. Inside any, `;`
    // is ordinary text. Comments are kept in the statement text, not stripped.
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
    (out, line_c)
}

/// Uppercase SQL keywords in `stmt`, leaving string/comment spans untouched.
///
/// Only whole words from the built-in keyword list are changed; identifiers
/// that merely contain a keyword (`selected`) are left alone.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     vix_edit_sql::format_sql("select a from t where b = 'from'"),
///     "SELECT a FROM t WHERE b = 'from'"
/// );
/// ```
#[must_use]
pub fn format_sql(stmt: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = stmt.chars().collect();
    let next = |i: usize| add_or(i, 1, usize::MAX);
    let mut i = 0;
    let (mut single, mut double, mut line_c, mut block_c) = (false, false, false, false);
    while let Some(&c) = chars.get(i) {
        if line_c {
            out.push(c);
            if c == '\n' {
                line_c = false;
            }
            i = next(i);
            continue;
        }
        if block_c {
            out.push(c);
            if c == '*' && chars.get(next(i)) == Some(&'/') {
                out.push('/');
                i = add_or(i, 2, usize::MAX);
                block_c = false;
                continue;
            }
            i = next(i);
            continue;
        }
        if single {
            out.push(c);
            if c == '\'' {
                single = false;
            }
            i = next(i);
            continue;
        }
        if double {
            out.push(c);
            if c == '"' {
                double = false;
            }
            i = next(i);
            continue;
        }
        match c {
            '\'' => {
                single = true;
                out.push(c);
                i = next(i);
            }
            '"' => {
                double = true;
                out.push(c);
                i = next(i);
            }
            '-' if chars.get(next(i)) == Some(&'-') => {
                line_c = true;
                out.push(c);
                i = next(i);
            }
            '/' if chars.get(next(i)) == Some(&'*') => {
                block_c = true;
                out.push(c);
                i = next(i);
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                {
                    i = next(i);
                }
                let word: String = chars.get(start..i).unwrap_or(&[]).iter().collect();
                if KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
                    out.push_str(&word.to_ascii_uppercase());
                } else {
                    out.push_str(&word);
                }
            }
            _ => {
                out.push(c);
                i = next(i);
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
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn press(e: &mut Editor, code: KeyCode) -> Outcome {
        e.handle_key(key(code), 3)
    }

    fn ctrl(e: &mut Editor, c: char) -> Outcome {
        e.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL), 3)
    }

    fn texts(e: &Editor) -> Vec<&str> {
        (0..e.len()).map(|i| e.statement(i)).collect()
    }

    #[test]
    fn accessors_on_fresh_editor() {
        let e = Editor::from_text("select  a,\n   b  from t; delete from t");
        assert_eq!(e.len(), 2);
        assert!(!e.is_empty());
        assert_eq!(e.sel(), 0);
        assert_eq!(e.scroll(), 0);
        assert!(!e.is_dirty());
        assert_eq!(e.preview(0), "select a, b from t");
        assert_eq!(e.kind(1), "DELETE");
        // Out-of-range index degrades to empty.
        assert_eq!(e.statement(9), "");
        assert_eq!(e.preview(9), "");
        assert_eq!(e.kind(9), "EMPTY");
        let empty = Editor::from_text("  ;; ");
        assert!(empty.is_empty());
        assert_eq!(empty.to_text(), "");
    }

    #[test]
    fn kind_labels_cover_every_keyword() {
        let e = Editor::from_text(
            "update t set a=1; alter table t; drop table t; with x as (select 1) select 1; \
             grant all on t to u; truncate t; begin; commit; vacuum",
        );
        let kinds: Vec<&str> = (0..e.len()).map(|i| e.kind(i)).collect();
        assert_eq!(
            kinds,
            [
                "UPDATE", "ALTER", "DROP", "WITH", "GRANT", "TRUNCATE", "BEGIN", "COMMIT", "SQL"
            ]
        );
    }

    #[test]
    fn mark_saved_clears_dirty() {
        let mut e = Editor::from_text("a; b");
        press(&mut e, KeyCode::Char('d'));
        assert!(e.is_dirty());
        e.mark_saved();
        assert!(!e.is_dirty());
    }

    #[test]
    fn ensure_visible_scrolls_to_selection() {
        let mut e = Editor::from_text("a;b;c;d;e;f");
        press(&mut e, KeyCode::End);
        e.ensure_visible(2);
        assert_eq!(e.sel(), 5);
        assert_eq!(e.scroll(), 4);
        press(&mut e, KeyCode::Home);
        e.ensure_visible(2);
        assert_eq!(e.scroll(), 0);
    }

    #[test]
    fn navigation_keys_clamp() {
        let mut e = Editor::from_text("a;b;c;d;e");
        press(&mut e, KeyCode::Up);
        assert_eq!(e.sel(), 0);
        press(&mut e, KeyCode::Char('j'));
        press(&mut e, KeyCode::Down);
        assert_eq!(e.sel(), 2);
        press(&mut e, KeyCode::Char('k'));
        assert_eq!(e.sel(), 1);
        // page size 3
        press(&mut e, KeyCode::PageDown);
        assert_eq!(e.sel(), 4);
        press(&mut e, KeyCode::PageDown);
        assert_eq!(e.sel(), 4);
        press(&mut e, KeyCode::PageUp);
        assert_eq!(e.sel(), 1);
        press(&mut e, KeyCode::PageUp);
        assert_eq!(e.sel(), 0);
        // A zero page still moves one row.
        e.handle_key(key(KeyCode::PageDown), 0);
        assert_eq!(e.sel(), 1);
        press(&mut e, KeyCode::End);
        assert_eq!(e.sel(), 4);
        press(&mut e, KeyCode::Home);
        assert_eq!(e.sel(), 0);
        assert!(!e.is_dirty(), "navigation never dirties");
    }

    #[test]
    fn navigation_on_empty_list_is_safe() {
        let mut e = Editor::from_text("");
        for code in [
            KeyCode::Down,
            KeyCode::PageDown,
            KeyCode::End,
            KeyCode::Up,
            KeyCode::Home,
        ] {
            assert_eq!(press(&mut e, code), Outcome::Consumed);
            assert_eq!(e.sel(), 0);
        }
        for code in [
            KeyCode::Char('d'),
            KeyCode::Char('f'),
            KeyCode::Char('F'),
            KeyCode::Char('J'),
            KeyCode::Char('K'),
            KeyCode::Char('u'),
        ] {
            press(&mut e, code);
        }
        ctrl(&mut e, 'r');
        assert!(e.is_empty());
        assert!(!e.is_dirty());
    }

    #[test]
    fn outcomes_for_save_close_and_unknown() {
        let mut e = Editor::from_text("a");
        assert_eq!(ctrl(&mut e, 's'), Outcome::Save);
        assert_eq!(press(&mut e, KeyCode::Esc), Outcome::Close);
        assert_eq!(press(&mut e, KeyCode::Char('q')), Outcome::Close);
        assert_eq!(press(&mut e, KeyCode::Char('z')), Outcome::Consumed);
        // Plain `s` (no Ctrl) and plain `r` do nothing.
        assert_eq!(press(&mut e, KeyCode::Char('s')), Outcome::Consumed);
        assert_eq!(press(&mut e, KeyCode::Char('r')), Outcome::Consumed);
        assert!(!e.is_dirty());
    }

    #[test]
    fn reorder_with_alt_arrows_and_kk() {
        let mut e = Editor::from_text("a;b;c");
        let alt = |c| KeyEvent::new(c, KeyModifiers::ALT);
        e.handle_key(alt(KeyCode::Down), 3);
        assert_eq!(texts(&e), ["b", "a", "c"]);
        assert_eq!(e.sel(), 1);
        e.handle_key(alt(KeyCode::Down), 3);
        assert_eq!(texts(&e), ["b", "c", "a"]);
        // At the bottom: no-op, not even an undo entry.
        e.handle_key(alt(KeyCode::Down), 3);
        assert_eq!(texts(&e), ["b", "c", "a"]);
        press(&mut e, KeyCode::Char('K'));
        assert_eq!(texts(&e), ["b", "a", "c"]);
        e.handle_key(alt(KeyCode::Up), 3);
        assert_eq!(texts(&e), ["a", "b", "c"]);
        assert_eq!(e.sel(), 0);
        // At the top: no-op.
        press(&mut e, KeyCode::Char('K'));
        assert_eq!(texts(&e), ["a", "b", "c"]);
        assert_eq!(e.sel(), 0);
    }

    #[test]
    fn reorder_single_statement_is_noop() {
        let mut e = Editor::from_text("only");
        press(&mut e, KeyCode::Char('J'));
        press(&mut e, KeyCode::Char('K'));
        assert_eq!(texts(&e), ["only"]);
        assert!(!e.is_dirty());
        press(&mut e, KeyCode::Char('u'));
        assert!(!e.is_dirty(), "no-op reorder pushed no undo entry");
    }

    #[test]
    fn delete_last_moves_selection_up_and_delete_key_works() {
        let mut e = Editor::from_text("a;b;c");
        press(&mut e, KeyCode::End);
        press(&mut e, KeyCode::Delete);
        assert_eq!(texts(&e), ["a", "b"]);
        assert_eq!(e.sel(), 1);
        press(&mut e, KeyCode::Char('d'));
        press(&mut e, KeyCode::Char('d'));
        assert!(e.is_empty());
        assert_eq!(e.sel(), 0);
        assert!(e.is_dirty());
    }

    #[test]
    fn format_selected_only_touches_selection() {
        let mut e = Editor::from_text("select 1; select 2");
        press(&mut e, KeyCode::Down);
        press(&mut e, KeyCode::Char('f'));
        assert_eq!(texts(&e), ["select 1", "SELECT 2"]);
        assert!(e.is_dirty());
        press(&mut e, KeyCode::Char('u'));
        assert_eq!(texts(&e), ["select 1", "select 2"]);
    }

    #[test]
    fn undo_redo_cycle_restores_selection() {
        let mut e = Editor::from_text("a;b;c");
        press(&mut e, KeyCode::End);
        press(&mut e, KeyCode::Char('d'));
        assert_eq!((e.len(), e.sel()), (2, 1));
        press(&mut e, KeyCode::Char('u'));
        assert_eq!((e.len(), e.sel()), (3, 2));
        ctrl(&mut e, 'r');
        assert_eq!((e.len(), e.sel()), (2, 1));
        // Nothing more to redo / undo past the start.
        ctrl(&mut e, 'r');
        assert_eq!(e.len(), 2);
        press(&mut e, KeyCode::Char('u'));
        press(&mut e, KeyCode::Char('u'));
        assert_eq!(e.len(), 3);
        assert!(e.is_dirty());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut e = Editor::from_text("a;b");
        press(&mut e, KeyCode::Char('d'));
        press(&mut e, KeyCode::Char('u'));
        press(&mut e, KeyCode::Char('d'));
        ctrl(&mut e, 'r');
        assert_eq!(texts(&e), ["b"], "redo was cleared by the new edit");
    }

    #[test]
    fn undo_history_is_capped() {
        let mut e = Editor::from_text("a;b");
        for _ in 0..(HISTORY_CAP + 10) {
            press(&mut e, KeyCode::Char('J'));
            press(&mut e, KeyCode::Char('K'));
        }
        assert_eq!(e.undo.len(), HISTORY_CAP);
    }

    #[test]
    fn to_text_trims_each_statement() {
        let e = Editor::from_text("  select 1  ;\n\n  select 2\n");
        assert_eq!(e.to_text(), "select 1;\n\nselect 2;\n");
    }

    #[test]
    fn split_handles_double_quotes_block_comments_and_unterminated() {
        assert_eq!(
            split_statements("select \"a;b\" from t; /* x; y */ select 2; select 'open;"),
            [
                "select \"a;b\" from t",
                "/* x; y */ select 2",
                "select 'open;"
            ]
        );
        // `-` and `/` alone are plain text.
        assert_eq!(
            split_statements("select 4-1; select 4/2"),
            ["select 4-1", "select 4/2"]
        );
        // Doubled quote closes and reopens the string.
        assert_eq!(
            split_statements("select 'it''s;ok'; x"),
            ["select 'it''s;ok'", "x"]
        );
        // A `*` inside a block comment not followed by `/`.
        assert_eq!(split_statements("/* a * b; */ x;"), ["/* a * b; */ x"]);
    }

    #[test]
    fn format_leaves_comments_and_quoted_text_untouched() {
        assert_eq!(
            format_sql("select 1 -- from here\nfrom t"),
            "SELECT 1 -- from here\nFROM t"
        );
        assert_eq!(
            format_sql("select /* where * from */ \"select\" from t"),
            "SELECT /* where * from */ \"select\" FROM t"
        );
        assert_eq!(
            format_sql("select 4-1, 4/2 from t"),
            "SELECT 4-1, 4/2 FROM t"
        );
    }

    #[test]
    fn format_only_changes_whole_keyword_words() {
        assert_eq!(
            format_sql("Select selected, from_x, _in from T"),
            "SELECT selected, from_x, _in FROM T"
        );
        assert_eq!(format_sql(""), "");
        assert_eq!(format_sql("a1 In (1)"), "a1 IN (1)");
    }

    /// T831: a statement ending in a `--` comment keeps its terminator out
    /// of the comment, so saving does not merge it with the next statement.
    #[test]
    fn to_text_keeps_the_terminator_out_of_a_trailing_line_comment() {
        let e = Editor::from_text("select 1 -- one\n; select 2;");
        assert_eq!(e.to_text(), "select 1 -- one\n;\n\nselect 2;\n");
        assert_eq!(
            split_statements(&e.to_text()),
            ["select 1 -- one", "select 2"]
        );
    }
}
