//! The database catalog: schema-tree state and the SQL that populates it.
//!
//! One query per connect lists every schema's tables, views, and functions
//! (see [`objects_sql`]); a second lists every column for autocomplete (see
//! [`columns_sql`]). The results feed a [`Tree`] the user navigates in the
//! workbench's left pane: schemas expand into Tables / Views / Functions
//! folders, which expand into the objects themselves. Table details (columns,
//! constraints, indexes, foreign keys, triggers) are fetched on demand with
//! [`detail_sql`] and shown in the results grid.
//!
//! # Design notes
//!
//! - **State vs. SQL.** [`Tree`] and friends are pure state with no I/O; every
//!   `*_sql` function is a pure `Kind -> SQL text` builder, so the whole module
//!   is unit-testable without a database. Execution lives in
//!   [`crate::session`] and the `Browser` methods that call these builders.
//! - **One query per engine.** Each builder has a `match` over [`Kind`] because
//!   `SQLite` (`sqlite_master` + `pragma_*` table-valued functions),
//!   `PostgreSQL` and `MySQL` (`information_schema`) disagree about where
//!   metadata lives. Result *shapes* are identical across engines (documented
//!   per function) so callers never branch on the engine.
//! - **Quoting.** Names interpolated into SQL are escaped with
//!   [`quote_ident`], [`quote_literal`] or the private `quote_str`; the detail
//!   builders embed names inside single-quoted literals (see the caveat on
//!   [`detail_sql`]).
//! - **SQLite has one schema** (`main`); its tree has a single schema row.
//!
//! Owning spec: `crates/vix-db/spec/index.md` ("Database Browser").

use super::connect::Kind;

/// The object folders under each schema.
///
/// The enum order (tables, views, functions) is the display order and matches
/// [`Folder::index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// Base tables.
    Tables,
    /// Views.
    Views,
    /// Stored functions / routines.
    Functions,
}

impl Folder {
    /// The i18n key of the folder's display label.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Folder::Tables => "ui.db_tree_tables",
            Folder::Views => "ui.db_tree_views",
            Folder::Functions => "ui.db_tree_functions",
        }
    }

    /// Index into a `[T; 3]` of per-folder state (e.g.
    /// [`Schema::folder_expanded`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::catalog::Folder;
    /// assert_eq!(Folder::Tables.index(), 0);
    /// assert_eq!(Folder::Functions.index(), 2);
    /// ```
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Folder::Tables => 0,
            Folder::Views => 1,
            Folder::Functions => 2,
        }
    }
}

/// A table-detail report the tree can request for the selected table.
///
/// Each variant maps to an engine-specific query built by [`detail_sql`]; the
/// result is shown as-is in the results grid, so column sets differ by engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// Column names, types, nullability, defaults.
    Columns,
    /// Indexes on the table.
    Indexes,
    /// Foreign keys leaving the table.
    ForeignKeys,
    /// Triggers on the table.
    Triggers,
    /// Table constraints (`SQLite`: the full `CREATE` statement).
    Constraints,
    /// Row-count and size statistics.
    Stats,
}

/// One schema's objects plus its expand/collapse state.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// Schema name (`main` for `SQLite`).
    pub name: String,
    /// Whether the schema row is expanded.
    pub expanded: bool,
    /// Table names, sorted.
    pub tables: Vec<String>,
    /// View names, sorted.
    pub views: Vec<String>,
    /// Function names, sorted.
    pub functions: Vec<String>,
    /// Which folders are expanded, indexed by [`Folder::index`].
    pub folder_expanded: [bool; 3],
}

/// What a visible tree row points at.
///
/// Indices are positions in [`Tree::schemas`] and in the schema's per-folder
/// vectors, *not* in the (possibly filtered) visible row list, so they stay
/// valid while a search filter narrows what is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRef {
    /// A schema row (index into [`Tree::schemas`]).
    Schema(usize),
    /// A folder row under a schema.
    Folder(usize, Folder),
    /// An object row: schema index, folder, index within the folder.
    Object(usize, Folder, usize),
}

/// One row of the flattened, currently-visible tree.
#[derive(Debug, Clone)]
pub struct RowView {
    /// Indent depth (0 = schema, 1 = folder, 2 = object).
    pub depth: usize,
    /// Display text (object/schema name; folders use [`Folder::label_key`]).
    pub text: String,
    /// Whether the row can expand (schemas and folders).
    pub expandable: bool,
    /// Whether the row is currently expanded.
    pub expanded: bool,
    /// What the row points at.
    pub reference: RowRef,
}

/// The schema tree: per-schema objects plus selection and scroll state.
///
/// The visible rows are derived on demand by [`Tree::rows`]; `sel` and
/// `scroll` index into that derived list, so any change to expansion or the
/// filter must re-clamp them (see [`Tree::filter_key`]).
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// All schemas, sorted by name.
    pub schemas: Vec<Schema>,
    /// Selected row index into [`Tree::rows`].
    pub sel: usize,
    /// First visible row.
    pub scroll: usize,
    /// Case-insensitive substring narrowing the object rows (`/` search).
    pub filter: String,
    /// Whether the search box is capturing typed keys.
    pub filtering: bool,
}

impl Tree {
    /// Build a tree from `(schema, name, kind)` rows, where `kind` is one of
    /// `table`, `view`, `function`. The first schema and its Tables folder
    /// start expanded so the workbench opens showing something useful.
    ///
    /// Any `kind` other than `view` or `function` is treated as a table, and
    /// schemas and names are sorted for a stable display.
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::catalog::Tree;
    /// let tree = Tree::from_objects(&[
    ///     ("main".into(), "users".into(), "table".into()),
    ///     ("main".into(), "active".into(), "view".into()),
    /// ]);
    /// assert_eq!(tree.table_names(), ["active", "users"]);
    /// // schema row + Tables folder (expanded) + 1 table + collapsed Views folder
    /// assert_eq!(tree.rows().len(), 4);
    /// ```
    #[must_use]
    pub fn from_objects(objects: &[(String, String, String)]) -> Tree {
        let mut schemas: Vec<Schema> = Vec::new();
        for (schema, name, kind) in objects {
            let idx = schemas
                .iter()
                .position(|s| s.name == *schema)
                .unwrap_or_else(|| {
                    schemas.push(Schema {
                        name: schema.clone(),
                        ..Schema::default()
                    });
                    schemas.len() - 1
                });
            let entry = &mut schemas[idx];
            match kind.as_str() {
                "view" => entry.views.push(name.clone()),
                "function" => entry.functions.push(name.clone()),
                _ => entry.tables.push(name.clone()),
            }
        }
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        for s in &mut schemas {
            s.tables.sort();
            s.views.sort();
            s.functions.sort();
        }
        if let Some(first) = schemas.first_mut() {
            first.expanded = true;
            first.folder_expanded[Folder::Tables.index()] = true;
        }
        Tree {
            schemas,
            ..Tree::default()
        }
    }

    /// The flattened list of currently-visible rows. While a search filter is
    /// active, expansion state is ignored: every matching object is shown
    /// under its schema and folder headers.
    ///
    /// Empty folders are omitted, and while searching a schema with no
    /// matching objects is dropped entirely. Search is a case-insensitive
    /// substring match on the object name only.
    #[must_use]
    pub fn rows(&self) -> Vec<RowView> {
        let searching = !self.filter.is_empty();
        let needle = self.filter.to_lowercase();
        let mut out = Vec::new();
        for (si, s) in self.schemas.iter().enumerate() {
            let schema_at = out.len();
            out.push(RowView {
                depth: 0,
                text: s.name.clone(),
                expandable: true,
                expanded: s.expanded || searching,
                reference: RowRef::Schema(si),
            });
            if !s.expanded && !searching {
                continue;
            }
            // (folder, names, expanded) triples let one loop body build all
            // three folders instead of three near-identical blocks.
            for (folder, names, expanded) in [
                (
                    Folder::Tables,
                    &s.tables,
                    s.folder_expanded[Folder::Tables.index()],
                ),
                (
                    Folder::Views,
                    &s.views,
                    s.folder_expanded[Folder::Views.index()],
                ),
                (
                    Folder::Functions,
                    &s.functions,
                    s.folder_expanded[Folder::Functions.index()],
                ),
            ] {
                let matches: Vec<usize> = (0..names.len())
                    .filter(|&i| !searching || names[i].to_lowercase().contains(&needle))
                    .collect();
                if names.is_empty() || (searching && matches.is_empty()) {
                    continue;
                }
                out.push(RowView {
                    depth: 1,
                    text: String::new(),
                    expandable: true,
                    expanded: expanded || searching,
                    reference: RowRef::Folder(si, folder),
                });
                if !expanded && !searching {
                    continue;
                }
                for oi in matches {
                    out.push(RowView {
                        depth: 2,
                        text: names[oi].clone(),
                        expandable: false,
                        expanded: false,
                        reference: RowRef::Object(si, folder, oi),
                    });
                }
            }
            // A searching schema with no matching objects disappears entirely.
            if searching && out.len() == schema_at + 1 {
                out.pop();
            }
        }
        out
    }

    /// Append (`Some`) or erase (`None`) one char of the search filter,
    /// clamping the selection to the now-shorter row list.
    pub fn filter_key(&mut self, c: Option<char>) {
        match c {
            Some(ch) => self.filter.push(ch),
            None => {
                self.filter.pop();
            }
        }
        self.sel = self.sel.min(self.rows().len().saturating_sub(1));
    }

    /// Toggle (or set, when `expand` is given) the selected row's folder state.
    /// A no-op on object rows or when nothing is selected. Operates on the
    /// *stored* expansion flags, so it has no visible effect during a search
    /// (which forces everything open).
    pub fn toggle(&mut self, expand: Option<bool>) {
        let rows = self.rows();
        let Some(row) = rows.get(self.sel) else {
            return;
        };
        match row.reference {
            RowRef::Schema(si) => {
                let cur = self.schemas[si].expanded;
                self.schemas[si].expanded = expand.unwrap_or(!cur);
            }
            RowRef::Folder(si, folder) => {
                let flag = &mut self.schemas[si].folder_expanded[folder.index()];
                *flag = expand.unwrap_or(!*flag);
            }
            RowRef::Object(..) => {}
        }
    }

    /// Move the selection `n` rows up or down, clamped to the visible rows
    /// (delegates the arithmetic to `vix_list_state`).
    pub fn step(&mut self, up: bool, n: usize) {
        self.sel = if up {
            vix_list_state::page_up(self.sel, n)
        } else {
            vix_list_state::page_down(self.sel, n, self.rows().len())
        };
    }

    /// Keep the selection within a window of `height` visible rows by
    /// adjusting [`Tree::scroll`]; the host calls this after each layout.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.sel, self.scroll, height, self.rows().len());
    }

    /// The selected object as `(schema, name, folder)`, if an object row is
    /// selected.
    #[must_use]
    pub fn selected_object(&self) -> Option<(String, String, Folder)> {
        let rows = self.rows();
        let row = rows.get(self.sel)?;
        if let RowRef::Object(si, folder, oi) = row.reference {
            let s = &self.schemas[si];
            let names = match folder {
                Folder::Tables => &s.tables,
                Folder::Views => &s.views,
                Folder::Functions => &s.functions,
            };
            return Some((s.name.clone(), names.get(oi)?.clone(), folder));
        }
        None
    }

    /// Every table and view name across all schemas (for autocomplete and
    /// details), sorted and de-duplicated. Functions are excluded.
    #[must_use]
    pub fn table_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.schemas {
            out.extend(s.tables.iter().cloned());
            out.extend(s.views.iter().cloned());
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Escape a name for embedding in a single-quoted SQL string literal
/// (doubles each `'`). Does not add the surrounding quotes.
fn quote_str(name: &str) -> String {
    name.replace('\'', "''")
}

/// Whether `s` is a plain integer or decimal number (optionally signed) — the
/// only shape safe to emit as a bare SQL literal. Deliberately excludes the
/// forms `f64::parse` also accepts (`inf`, `nan`, `1e3`, `0x…`), which some
/// engines treat as bare identifiers or re-render unexpectedly.
fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for c in body.chars() {
        match c {
            '0'..='9' => seen_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => return false,
        }
    }
    seen_digit
}

/// A `value` as a SQL literal: bare when it is a plain integer/decimal number,
/// otherwise a single-quoted, escaped string. Used by foreign-key follow to
/// build a `WHERE col = <value>` clause safely. Any SQL metacharacter forces the
/// quoted branch, so this is not an injection surface; the number check is kept
/// strict to avoid `inf`/`nan`/exponent value confusion.
///
/// # Examples
///
/// ```
/// use vix_db::catalog::quote_literal;
/// assert_eq!(quote_literal("42"), "42");
/// assert_eq!(quote_literal("O'Brien"), "'O''Brien'");
/// assert_eq!(quote_literal("1e3"), "'1e3'");
/// ```
#[must_use]
pub fn quote_literal(value: &str) -> String {
    let trimmed = value.trim();
    if is_plain_number(trimmed) {
        trimmed.to_string()
    } else {
        format!("'{}'", quote_str(value))
    }
}

/// Quote `name` as an identifier for `kind`: backticks for `MySQL`, standard
/// double quotes for `PostgreSQL` and `SQLite` (internal quotes doubled).
///
/// # Examples
///
/// ```
/// use vix_db::{catalog::quote_ident, connect::Kind};
/// assert_eq!(quote_ident(Kind::Postgres, "my\"t"), "\"my\"\"t\"");
/// assert_eq!(quote_ident(Kind::Mysql, "t"), "`t`");
/// ```
#[must_use]
pub fn quote_ident(kind: Kind, name: &str) -> String {
    match kind {
        Kind::Mysql => format!("`{}`", name.replace('`', "``")),
        Kind::Postgres | Kind::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// Rows shown by the tree's table-data preview (the `LIMIT` in
/// [`preview_sql`]).
pub const PREVIEW_LIMIT: usize = 200;

/// A `SELECT * … LIMIT` preview of `schema`.`table`.
///
/// `SQLite` has a single schema, so it omits the schema qualifier.
///
/// # Examples
///
/// ```
/// use vix_db::{catalog::preview_sql, connect::Kind};
/// assert_eq!(
///     preview_sql(Kind::Postgres, "public", "users"),
///     "SELECT * FROM \"public\".\"users\" LIMIT 200;"
/// );
/// ```
#[must_use]
pub fn preview_sql(kind: Kind, schema: &str, table: &str) -> String {
    let target = match kind {
        Kind::Sqlite => quote_ident(kind, table),
        Kind::Postgres | Kind::Mysql => {
            format!("{}.{}", quote_ident(kind, schema), quote_ident(kind, table))
        }
    };
    format!("SELECT * FROM {target} LIMIT {PREVIEW_LIMIT};")
}

/// Wrap `stmt` in the engine's EXPLAIN. `SQLite` has no ANALYZE variant of
/// its readable plan, so both flavors use `EXPLAIN QUERY PLAN` there.
///
/// `EXPLAIN ANALYZE` actually *executes* the statement on `PostgreSQL`/`MySQL`,
/// so callers must only pass `analyze = true` for statements the user has
/// already been allowed to run (the AI validation path always passes `false`).
///
/// # Examples
///
/// ```
/// use vix_db::{catalog::explain_sql, connect::Kind};
/// assert_eq!(explain_sql(Kind::Postgres, "SELECT 1", true), "EXPLAIN ANALYZE SELECT 1;");
/// assert_eq!(explain_sql(Kind::Sqlite, "SELECT 1", true), "EXPLAIN QUERY PLAN SELECT 1;");
/// ```
#[must_use]
pub fn explain_sql(kind: Kind, stmt: &str, analyze: bool) -> String {
    let prefix = match (kind, analyze) {
        (Kind::Sqlite, _) => "EXPLAIN QUERY PLAN",
        (_, true) => "EXPLAIN ANALYZE",
        (_, false) => "EXPLAIN",
    };
    format!("{prefix} {stmt};")
}

/// Whether an EXPLAIN result reports a full table scan — the plan-doctor
/// heuristic behind the workbench's "consider an index" insight.
///
/// It is a text heuristic over every cell of the plan (engine-specific
/// markers), so it can false-positive on, say, a data value `ALL` on `MySQL`.
///
/// # Examples
///
/// ```
/// use vix_db::{catalog::scan_insight, connect::Kind};
/// let scan = vec![vec!["SCAN users".to_string()]];
/// let search = vec![vec!["SEARCH users USING INDEX i (id=?)".to_string()]];
/// assert!(scan_insight(Kind::Sqlite, &scan));
/// assert!(!scan_insight(Kind::Sqlite, &search));
/// ```
#[must_use]
pub fn scan_insight(kind: Kind, rows: &[Vec<String>]) -> bool {
    let cells = rows.iter().flatten();
    match kind {
        Kind::Postgres => cells.into_iter().any(|c| c.contains("Seq Scan")),
        // `SCAN t` without an index; `SEARCH t USING INDEX …` is the good case.
        Kind::Sqlite => cells
            .into_iter()
            .any(|c| c.contains("SCAN") && !c.contains("USING INDEX")),
        // EXPLAIN's access `type` column; `ALL` is a full scan.
        Kind::Mysql => cells.into_iter().any(|c| c == "ALL"),
    }
}

/// The query listing every `(schema, name, kind)` object, where `kind` is
/// `table`, `view`, or `function`.
///
/// `SQLite` reports `main` as the schema and has no functions; the server
/// engines exclude system schemas (`MySQL` is scoped to the current database).
/// This is the input to [`Tree::from_objects`] (via the host's row triples).
#[must_use]
pub fn objects_sql(kind: Kind) -> &'static str {
    match kind {
        Kind::Sqlite => {
            "SELECT 'main' AS schema_name, name, type FROM sqlite_master \
             WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY type, name;"
        }
        Kind::Postgres => {
            "SELECT table_schema, table_name, CASE table_type WHEN 'VIEW' THEN 'view' ELSE 'table' END \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             UNION ALL \
             SELECT routine_schema, routine_name, 'function' FROM information_schema.routines \
             WHERE routine_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY 1, 3, 2;"
        }
        Kind::Mysql => {
            "SELECT table_schema, table_name, IF(table_type='VIEW','view','table') \
             FROM information_schema.tables WHERE table_schema = DATABASE() \
             UNION ALL \
             SELECT routine_schema, routine_name, 'function' FROM information_schema.routines \
             WHERE routine_schema = DATABASE() \
             ORDER BY 1, 3, 2;"
        }
    }
}

/// The query listing every `(table, column)` pair, for autocomplete, in
/// declaration order per table.
#[must_use]
pub fn columns_sql(kind: Kind) -> &'static str {
    match kind {
        Kind::Sqlite => {
            "SELECT m.name AS table_name, p.name AS column_name \
             FROM sqlite_master m JOIN pragma_table_info(m.name) p \
             WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY m.name, p.cid;"
        }
        Kind::Postgres => {
            "SELECT table_name, column_name FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY table_name, ordinal_position;"
        }
        Kind::Mysql => {
            "SELECT table_name, column_name FROM information_schema.columns \
             WHERE table_schema = DATABASE() ORDER BY table_name, ordinal_position;"
        }
    }
}

/// The query listing every `(table, column, type)` triple for base tables,
/// feeding the ERD entity blocks.
#[must_use]
pub fn columns_typed_sql(kind: Kind) -> &'static str {
    match kind {
        Kind::Sqlite => {
            "SELECT m.name AS table_name, p.name AS column_name, p.type AS data_type \
             FROM sqlite_master m JOIN pragma_table_info(m.name) p \
             WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY m.name, p.cid;"
        }
        Kind::Postgres => {
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY table_name, ordinal_position;"
        }
        Kind::Mysql => {
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema = DATABASE() ORDER BY table_name, ordinal_position;"
        }
    }
}

/// The query listing every foreign-key edge as
/// `(child_table, child_column, parent_table, parent_column)`, feeding the ERD
/// relationships.
#[must_use]
pub fn relationships_sql(kind: Kind) -> &'static str {
    match kind {
        Kind::Sqlite => {
            "SELECT m.name AS child, f.\"from\" AS child_col, f.\"table\" AS parent, f.\"to\" AS parent_col \
             FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f \
             WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY child, parent;"
        }
        Kind::Postgres => {
            "SELECT tc.table_name AS child, kcu.column_name AS child_col, \
             ccu.table_name AS parent, ccu.column_name AS parent_col \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
             ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu \
             ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema \
             WHERE tc.constraint_type = 'FOREIGN KEY' \
             AND tc.table_schema NOT IN ('pg_catalog','information_schema') \
             ORDER BY child, parent;"
        }
        Kind::Mysql => {
            "SELECT table_name AS child, column_name AS child_col, \
             referenced_table_name AS parent, referenced_column_name AS parent_col \
             FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() AND referenced_table_name IS NOT NULL \
             ORDER BY child, parent;"
        }
    }
}

/// A query returning the primary-key column names of `schema`.`table`, in key
/// order — the columns that identify a row for staged cell edits (empty result
/// ⇒ the table is not editable).
#[must_use]
pub fn primary_key_sql(kind: Kind, schema: &str, table: &str) -> String {
    let (s, t) = (quote_str(schema), quote_str(table));
    match kind {
        Kind::Sqlite => {
            format!("SELECT name FROM pragma_table_info('{t}') WHERE pk > 0 ORDER BY pk;")
        }
        Kind::Postgres => format!(
            "SELECT a.attname FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = '\"{s}\".\"{t}\"'::regclass AND i.indisprimary \
             ORDER BY array_position(i.indkey, a.attnum);"
        ),
        Kind::Mysql => format!(
            "SELECT column_name FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() AND table_name = '{t}' \
             AND constraint_name = 'PRIMARY' ORDER BY ordinal_position;"
        ),
    }
}

/// A query whose first row's **last** column is the `CREATE` statement for
/// `schema`.`table`. `SQLite` reads it from `sqlite_master`, `MySQL` uses
/// `SHOW CREATE TABLE`, and `PostgreSQL` reconstructs a basic definition from
/// `information_schema` (columns and types; constraints are not reproduced).
#[must_use]
pub fn ddl_sql(kind: Kind, schema: &str, table: &str) -> String {
    let (s, t) = (quote_str(schema), quote_str(table));
    match kind {
        Kind::Sqlite => {
            format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{t}';")
        }
        Kind::Mysql => format!("SHOW CREATE TABLE {};", quote_ident(kind, table)),
        Kind::Postgres => format!(
            "SELECT 'CREATE TABLE ' || quote_ident('{t}') || ' (' || \
             string_agg(quote_ident(column_name) || ' ' || data_type, ', ' ORDER BY ordinal_position) \
             || ');' FROM information_schema.columns \
             WHERE table_schema='{s}' AND table_name='{t}';"
        ),
    }
}

/// The query for one table-detail report on `schema`.`table`.
///
/// Names are escaped for a single-quoted literal (`'` doubled) only. Table
/// names containing a double quote can still produce a malformed query in the
/// builders that wrap the name in `"…"` (`SQLite` stats, `PostgreSQL` size
/// lookups); the tree only offers names the catalog itself returned.
#[must_use]
pub fn detail_sql(kind: Kind, detail: Detail, schema: &str, table: &str) -> String {
    let (s, t) = (quote_str(schema), quote_str(table));
    match kind {
        Kind::Sqlite => sqlite_detail_sql(detail, &t),
        Kind::Postgres => postgres_detail_sql(detail, &s, &t),
        Kind::Mysql => mysql_detail_sql(detail, &t),
    }
}

/// `SQLite` detail queries (PRAGMA table-valued output is TSV-friendly).
fn sqlite_detail_sql(detail: Detail, t: &str) -> String {
    match detail {
        Detail::Columns => format!("PRAGMA table_info('{t}');"),
        Detail::Indexes => format!("PRAGMA index_list('{t}');"),
        Detail::ForeignKeys => format!("PRAGMA foreign_key_list('{t}');"),
        Detail::Triggers => format!(
            "SELECT name, sql FROM sqlite_master WHERE type='trigger' AND tbl_name='{t}' ORDER BY name;"
        ),
        Detail::Constraints => {
            format!("SELECT name, sql FROM sqlite_master WHERE name='{t}';")
        }
        Detail::Stats => format!(
            "SELECT (SELECT count(*) FROM \"{t}\") AS row_count, \
             (SELECT count(*) FROM pragma_table_info('{t}')) AS columns, \
             (SELECT count(*) FROM pragma_index_list('{t}')) AS indexes;"
        ),
    }
}

/// `PostgreSQL` detail queries over `information_schema` / `pg_indexes`.
fn postgres_detail_sql(detail: Detail, s: &str, t: &str) -> String {
    match detail {
        Detail::Columns => format!(
            "SELECT column_name, data_type, is_nullable, column_default \
             FROM information_schema.columns \
             WHERE table_schema='{s}' AND table_name='{t}' ORDER BY ordinal_position;"
        ),
        Detail::Indexes => format!(
            "SELECT indexname, indexdef FROM pg_indexes \
             WHERE schemaname='{s}' AND tablename='{t}' ORDER BY indexname;"
        ),
        Detail::ForeignKeys => format!(
            "SELECT tc.constraint_name, kcu.column_name, \
             ccu.table_name AS references_table, ccu.column_name AS references_column \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
             ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu \
             ON tc.constraint_name = ccu.constraint_name AND tc.table_schema = ccu.table_schema \
             WHERE tc.constraint_type='FOREIGN KEY' AND tc.table_schema='{s}' AND tc.table_name='{t}' \
             ORDER BY tc.constraint_name;"
        ),
        Detail::Triggers => format!(
            "SELECT trigger_name, event_manipulation, action_timing \
             FROM information_schema.triggers \
             WHERE event_object_schema='{s}' AND event_object_table='{t}' ORDER BY trigger_name;"
        ),
        Detail::Constraints => format!(
            "SELECT constraint_name, constraint_type FROM information_schema.table_constraints \
             WHERE table_schema='{s}' AND table_name='{t}' ORDER BY constraint_name;"
        ),
        Detail::Stats => format!(
            "SELECT reltuples::bigint AS est_rows, \
             pg_size_pretty(pg_total_relation_size('\"{s}\".\"{t}\"')) AS total_size, \
             pg_size_pretty(pg_relation_size('\"{s}\".\"{t}\"')) AS table_size \
             FROM pg_class WHERE oid = '\"{s}\".\"{t}\"'::regclass;"
        ),
    }
}

/// `MySQL` detail queries over `information_schema`.
fn mysql_detail_sql(detail: Detail, t: &str) -> String {
    match detail {
        Detail::Columns => format!(
            "SELECT column_name, column_type, is_nullable, column_default \
             FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name='{t}' ORDER BY ordinal_position;"
        ),
        Detail::Indexes => format!(
            "SELECT index_name, column_name, non_unique FROM information_schema.statistics \
             WHERE table_schema = DATABASE() AND table_name='{t}' ORDER BY index_name, seq_in_index;"
        ),
        Detail::ForeignKeys => format!(
            "SELECT constraint_name, column_name, referenced_table_name, referenced_column_name \
             FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() AND table_name='{t}' \
             AND referenced_table_name IS NOT NULL ORDER BY constraint_name;"
        ),
        Detail::Triggers => format!(
            "SELECT trigger_name, event_manipulation, action_timing \
             FROM information_schema.triggers \
             WHERE event_object_schema = DATABASE() AND event_object_table='{t}' ORDER BY trigger_name;"
        ),
        Detail::Constraints => format!(
            "SELECT constraint_name, constraint_type FROM information_schema.table_constraints \
             WHERE table_schema = DATABASE() AND table_name='{t}' ORDER BY constraint_name;"
        ),
        Detail::Stats => format!(
            "SELECT table_rows AS est_rows, data_length, index_length \
             FROM information_schema.tables \
             WHERE table_schema = DATABASE() AND table_name='{t}';"
        ),
    }
}

/// The `SQLite` query listing every table and view column as
/// `(object, kind, column, type)` rows, `kind` being `table` or `view`, in
/// declaration order (T731, the MCP server's `db_schema` tool).
pub const SQLITE_SCHEMA_SQL: &str = "SELECT m.name, m.type, p.name, p.type \
     FROM sqlite_master m JOIN pragma_table_info(m.name) p \
     WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' \
     ORDER BY m.type, m.name, p.cid;";

/// The sqlx URL that opens the `SQLite` file at `path` **read-only**
/// (`mode=ro`: the file is never created or written). `%`, `?` and `#` in
/// the path are percent-encoded so they cannot end the path early.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// let url = vix_db::catalog::sqlite_read_only_url(Path::new("/tmp/a#b.db"));
/// assert_eq!(url, "sqlite:///tmp/a%23b.db?mode=ro");
/// ```
#[must_use]
pub fn sqlite_read_only_url(path: &std::path::Path) -> String {
    let path = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23");
    format!("sqlite://{path}?mode=ro")
}

/// Read the schema of the `SQLite` database at `path` without writing to it:
/// opened through [`sqlite_read_only_url`] with `PRAGMA query_only = ON`,
/// then [`SQLITE_SCHEMA_SQL`]. Returns `(object, kind, column, type)` rows.
/// No credentials are involved: a `SQLite` file needs none.
///
/// # Errors
///
/// Returns the driver's error when the file cannot be opened or queried.
pub fn read_sqlite_schema(
    path: &std::path::Path,
) -> Result<Vec<(String, String, String, String)>, crate::session::SessionError> {
    // Belt and braces: `mode=ro` already blocks writes at the file level, and
    // `query_only` makes the intent explicit to the engine as well.
    let setup: Vec<String> = crate::connect::read_only_sql(Kind::Sqlite, true)
        .into_iter()
        .collect();
    let mut session = crate::session::Session::connect(&sqlite_read_only_url(path), &setup)?;
    let (_, rows) = session.run(SQLITE_SCHEMA_SQL)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let mut cells = row.into_iter();
            Some((cells.next()?, cells.next()?, cells.next()?, cells.next()?))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_read_only_url_escapes_query_characters() {
        assert_eq!(
            sqlite_read_only_url(std::path::Path::new("/w/a?b#c%d.db")),
            "sqlite:///w/a%3Fb%23c%25d.db?mode=ro"
        );
    }

    #[test]
    fn read_sqlite_schema_lists_columns_and_leaves_the_file_alone() {
        let dir = std::env::temp_dir().join(format!("vix-db-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.db");
        let url = format!("sqlite:{}?mode=rwc", file.display());
        let mut seed = crate::session::Session::connect(&url, &[]).unwrap();
        seed.run("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);")
            .unwrap();
        seed.run("CREATE VIEW names AS SELECT name FROM users;")
            .unwrap();
        drop(seed);
        let before = std::fs::read(&file).unwrap();
        let rows = read_sqlite_schema(&file).unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    "users".into(),
                    "table".into(),
                    "id".into(),
                    "INTEGER".into()
                ),
                ("users".into(), "table".into(), "name".into(), "TEXT".into()),
                ("names".into(), "view".into(), "name".into(), "TEXT".into()),
            ]
        );
        assert_eq!(std::fs::read(&file).unwrap(), before);
        assert!(read_sqlite_schema(&dir.join("missing.db")).is_err());
    }

    #[test]
    fn quote_literal_only_emits_plain_numbers_bare() {
        // Plain integers/decimals are bare.
        assert_eq!(quote_literal("42"), "42");
        assert_eq!(quote_literal("-3.14"), "-3.14");
        assert_eq!(quote_literal("+7"), "+7");
        // Value-confusion forms that `f64` would accept are quoted.
        assert_eq!(quote_literal("inf"), "'inf'");
        assert_eq!(quote_literal("nan"), "'nan'");
        assert_eq!(quote_literal("1e3"), "'1e3'");
        assert_eq!(quote_literal("0x10"), "'0x10'");
        // Injection attempts are always quoted and escaped.
        assert_eq!(quote_literal("a' OR '1'='1"), "'a'' OR ''1''=''1'");
        assert_eq!(quote_literal(""), "''");
    }

    fn sample() -> Tree {
        Tree::from_objects(&[
            ("main".into(), "users".into(), "table".into()),
            ("main".into(), "orders".into(), "table".into()),
            ("main".into(), "v_totals".into(), "view".into()),
        ])
    }

    #[test]
    fn builds_sorted_tree_with_first_schema_open() {
        let tree = sample();
        assert_eq!(tree.schemas.len(), 1);
        assert_eq!(tree.schemas[0].tables, vec!["orders", "users"]);
        assert!(tree.schemas[0].expanded);
        assert!(tree.schemas[0].folder_expanded[Folder::Tables.index()]);
        // Visible: schema, Tables folder, 2 tables, Views folder (collapsed).
        assert_eq!(tree.rows().len(), 5);
    }

    #[test]
    fn toggle_collapses_and_expands() {
        let mut tree = sample();
        tree.toggle(None); // collapse the schema (row 0 selected)
        assert_eq!(tree.rows().len(), 1);
        tree.toggle(Some(true));
        assert_eq!(tree.rows().len(), 5);
    }

    #[test]
    fn selected_object_names_the_table() {
        let mut tree = sample();
        tree.sel = 2; // schema, Tables, [orders]
        let (schema, name, folder) = tree.selected_object().expect("object row");
        assert_eq!((schema.as_str(), name.as_str()), ("main", "orders"));
        assert_eq!(folder, Folder::Tables);
        tree.sel = 0;
        assert!(
            tree.selected_object().is_none(),
            "schema row is not an object"
        );
    }

    #[test]
    fn table_names_cover_tables_and_views() {
        assert_eq!(sample().table_names(), vec!["orders", "users", "v_totals"]);
    }

    #[test]
    fn detail_sql_quotes_hostile_names() {
        let sql = detail_sql(
            Kind::Sqlite,
            Detail::Columns,
            "main",
            "users'; DROP TABLE x;--",
        );
        assert!(
            sql.contains("users''; DROP TABLE x;--"),
            "single quotes are doubled"
        );
    }

    #[test]
    fn objects_sql_exists_for_every_engine() {
        for kind in [Kind::Sqlite, Kind::Postgres, Kind::Mysql] {
            assert!(objects_sql(kind).to_lowercase().contains("select"));
            assert!(columns_sql(kind).to_lowercase().contains("column"));
        }
    }

    #[test]
    fn search_filter_shows_matches_regardless_of_expansion() {
        let mut tree = sample();
        tree.toggle(None); // collapse the schema entirely
        assert_eq!(tree.rows().len(), 1);
        for c in "tot".chars() {
            tree.filter_key(Some(c));
        }
        // Searching ignores collapse: schema + Views folder + v_totals.
        let rows = tree.rows();
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(rows[2].text, "v_totals");
        tree.filter_key(None); // "to" still matches only v_totals
        assert_eq!(tree.rows().len(), 3);
        tree.filter.clear();
        assert_eq!(tree.rows().len(), 1, "collapse state resumes after search");
    }

    #[test]
    fn quote_ident_per_engine() {
        assert_eq!(quote_ident(Kind::Mysql, "od`d"), "`od``d`");
        assert_eq!(quote_ident(Kind::Postgres, "od\"d"), "\"od\"\"d\"");
        assert_eq!(quote_ident(Kind::Sqlite, "users"), "\"users\"");
    }

    #[test]
    fn preview_sql_qualifies_servers_but_not_sqlite() {
        assert_eq!(
            preview_sql(Kind::Sqlite, "main", "users"),
            "SELECT * FROM \"users\" LIMIT 200;"
        );
        assert_eq!(
            preview_sql(Kind::Postgres, "public", "users"),
            "SELECT * FROM \"public\".\"users\" LIMIT 200;"
        );
        assert!(preview_sql(Kind::Mysql, "shop", "orders").contains("`shop`.`orders`"));
    }

    #[test]
    fn explain_sql_uses_engine_dialect() {
        assert_eq!(
            explain_sql(Kind::Postgres, "select 1", true),
            "EXPLAIN ANALYZE select 1;"
        );
        assert_eq!(
            explain_sql(Kind::Postgres, "select 1", false),
            "EXPLAIN select 1;"
        );
        assert_eq!(
            explain_sql(Kind::Sqlite, "select 1", true),
            "EXPLAIN QUERY PLAN select 1;",
            "sqlite has no readable ANALYZE variant"
        );
    }

    #[test]
    fn scan_insight_flags_full_scans_only() {
        let rows =
            |cells: &[&str]| vec![cells.iter().map(|c| (*c).to_string()).collect::<Vec<_>>()];
        assert!(scan_insight(
            Kind::Postgres,
            &rows(&["Seq Scan on users  (cost=0.00..1.10)"])
        ));
        assert!(!scan_insight(
            Kind::Postgres,
            &rows(&["Index Scan using users_pkey"])
        ));
        assert!(scan_insight(
            Kind::Sqlite,
            &rows(&["2", "0", "0", "SCAN users"])
        ));
        assert!(!scan_insight(
            Kind::Sqlite,
            &rows(&["3", "0", "0", "SEARCH users USING INDEX idx (a=?)"])
        ));
        assert!(scan_insight(Kind::Mysql, &rows(&["1", "ALL", "NULL"])));
        assert!(!scan_insight(Kind::Mysql, &rows(&["1", "ref", "idx"])));
    }
}
