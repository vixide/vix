//! Reading a [`Snapshot`] through catalog queries (T785).
//!
//! The readers take a `run` callback (SQL in, text rows out) instead of a
//! session, so the parsing is tested on hand-written rows and the SQL for
//! shape only; [`read_session`] wires them to a live [`Session`].
//!
//! sqlx's `Any` driver decodes only text, numeric, boolean and byte values,
//! so every `PostgreSQL` catalog query casts what it returns to `text`
//! (`format_type(…)`, `pg_get_viewdef(oid)::text`, …) and coalesces `NULL`s,
//! which the session would otherwise show as empty strings anyway. Objects
//! that belong to an extension are left out: the extension owns them.
//!
//! `MySQL` is not supported; [`read`] returns [`ReadError::Unsupported`].

use super::snapshot::{
    Column, Constraint, EnumType, Index, Routine, Sequence, Snapshot, Table, Trigger, View,
};
use crate::connect::Kind;
use crate::session::{Session, SessionError};
use std::fmt::Write as _;

/// Why a snapshot could not be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadError {
    /// The engine has no schema reader (`MySQL`).
    Unsupported(Kind),
    /// A catalog query failed.
    Query(SessionError),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Unsupported(kind) => {
                f.write_str(&t!("msg.db_compare_unsupported", kind = kind.label()))
            }
            ReadError::Query(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Query(e) => Some(e),
            ReadError::Unsupported(_) => None,
        }
    }
}

impl From<SessionError> for ReadError {
    fn from(e: SessionError) -> Self {
        ReadError::Query(e)
    }
}

/// Rows of a text result set.
type Rows = Vec<Vec<String>>;

/// Whether `kind` has a schema reader.
#[must_use]
pub fn supported(kind: Kind) -> bool {
    matches!(kind, Kind::Postgres | Kind::Sqlite)
}

/// The schema compared when nothing else is chosen: `public` on
/// `PostgreSQL`, `main` on `SQLite`.
#[must_use]
pub fn default_schema(kind: Kind) -> &'static str {
    match kind {
        Kind::Sqlite => "main",
        Kind::Postgres | Kind::Mysql => "public",
    }
}

/// Read `schema` on a live session.
///
/// # Errors
///
/// [`ReadError::Unsupported`] for `MySQL`, or the first failing catalog
/// query.
pub fn read_session(
    session: &mut Session,
    kind: Kind,
    schema: &str,
) -> Result<Snapshot, ReadError> {
    read(kind, schema, &mut |sql| {
        session.run(sql).map(|(_, rows)| rows)
    })
}

/// Read `schema` on a `kind` engine, running each catalog query through
/// `run`.
///
/// # Errors
///
/// [`ReadError::Unsupported`] for `MySQL`, or the first failing catalog
/// query.
pub fn read(
    kind: Kind,
    schema: &str,
    run: &mut dyn FnMut(&str) -> Result<Rows, SessionError>,
) -> Result<Snapshot, ReadError> {
    let mut snapshot = match kind {
        Kind::Postgres => read_postgres(schema, run)?,
        Kind::Sqlite => read_sqlite(run)?,
        Kind::Mysql => return Err(ReadError::Unsupported(kind)),
    };
    snapshot.sort();
    Ok(snapshot)
}

/// A string literal, always quoted (unlike `catalog::quote_literal`, which
/// leaves numbers bare).
pub(crate) fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Cell `i` of `row`, or empty when the row is short.
fn cell(row: &[String], i: usize) -> String {
    row.get(i).cloned().unwrap_or_default()
}

/// Excludes objects that belong to an extension; `{oid}` is the object's oid
/// column.
fn not_extension(oid: &str) -> String {
    format!("NOT EXISTS (SELECT 1 FROM pg_depend e WHERE e.objid = {oid} AND e.deptype = 'e')")
}

/// The `PostgreSQL` catalog queries, in the order [`read`] runs them:
/// schema presence, tables, columns, constraints, indexes, views, view
/// dependencies, routines, triggers, enums, sequences. Each casts its values
/// to `text`. Exposed for shape tests.
#[must_use]
pub fn postgres_sql(schema: &str) -> Vec<String> {
    let s = lit(schema);
    vec![
        format!("SELECT count(*)::text FROM pg_namespace WHERE nspname = {s}"),
        format!(
            "SELECT c.relname::text, COALESCE(obj_description(c.oid, 'pg_class'), '')::text \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = {s} AND c.relkind IN ('r', 'p') AND NOT c.relispartition \
             AND {} ORDER BY 1",
            not_extension("c.oid")
        ),
        format!(
            "SELECT c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod)::text, \
             CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END, \
             COALESCE(pg_get_expr(d.adbin, d.adrelid), '')::text, \
             CASE WHEN a.attgenerated = 's' THEN 'YES' ELSE 'NO' END, \
             CASE a.attidentity WHEN 'a' THEN 'always' WHEN 'd' THEN 'default' ELSE '' END, \
             COALESCE(col_description(c.oid, a.attnum), '')::text \
             FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE n.nspname = {s} AND c.relkind IN ('r', 'p') AND a.attnum > 0 \
             AND NOT a.attisdropped ORDER BY c.relname, a.attnum"
        ),
        format!(
            "SELECT c.relname::text, con.conname::text, con.contype::text, \
             pg_get_constraintdef(con.oid)::text, COALESCE(f.relname::text, '') \
             FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_class f ON f.oid = con.confrelid \
             WHERE n.nspname = {s} AND con.contype IN ('p', 'u', 'c', 'f', 'x') \
             ORDER BY 1, 2"
        ),
        format!(
            "SELECT t.relname::text, i.relname::text, pg_get_indexdef(i.oid)::text, \
             CASE WHEN x.indisunique THEN 'YES' ELSE 'NO' END \
             FROM pg_index x JOIN pg_class i ON i.oid = x.indexrelid \
             JOIN pg_class t ON t.oid = x.indrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             WHERE n.nspname = {s} AND t.relkind IN ('r', 'p', 'm') AND NOT EXISTS \
             (SELECT 1 FROM pg_constraint con WHERE con.conindid = x.indexrelid \
             AND con.conrelid = x.indrelid AND con.contype IN ('p', 'u', 'x')) \
             AND {} ORDER BY 1, 2",
            not_extension("i.oid")
        ),
        format!(
            "SELECT c.relname::text, CASE WHEN c.relkind = 'm' THEN 'YES' ELSE 'NO' END, \
             pg_get_viewdef(c.oid)::text, COALESCE(obj_description(c.oid, 'pg_class'), '')::text \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = {s} AND c.relkind IN ('v', 'm') AND {} ORDER BY 1",
            not_extension("c.oid")
        ),
        format!(
            "SELECT DISTINCT v.relname::text, t.relname::text, COALESCE(a.attname::text, '') \
             FROM pg_depend d JOIN pg_rewrite r ON r.oid = d.objid \
             JOIN pg_class v ON v.oid = r.ev_class \
             JOIN pg_namespace n ON n.oid = v.relnamespace \
             JOIN pg_class t ON t.oid = d.refobjid AND t.relnamespace = n.oid \
             LEFT JOIN pg_attribute a ON a.attrelid = d.refobjid AND a.attnum = d.refobjsubid \
             AND d.refobjsubid > 0 \
             WHERE d.classid = 'pg_rewrite'::regclass AND d.refclassid = 'pg_class'::regclass \
             AND n.nspname = {s} AND v.relkind IN ('v', 'm') AND v.oid <> t.oid \
             ORDER BY 1, 2, 3"
        ),
        format!(
            "SELECT p.proname::text, pg_get_function_identity_arguments(p.oid)::text, \
             CASE WHEN p.prokind = 'p' THEN 'YES' ELSE 'NO' END, \
             pg_get_functiondef(p.oid)::text \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = {s} AND p.prokind IN ('f', 'p') AND {} ORDER BY 1, 2",
            not_extension("p.oid")
        ),
        format!(
            "SELECT c.relname::text, t.tgname::text, pg_get_triggerdef(t.oid)::text \
             FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = {s} AND NOT t.tgisinternal ORDER BY 1, 2"
        ),
        format!(
            "SELECT t.typname::text, e.enumlabel::text \
             FROM pg_type t JOIN pg_enum e ON e.enumtypid = t.oid \
             JOIN pg_namespace n ON n.oid = t.typnamespace \
             WHERE n.nspname = {s} AND {} ORDER BY t.typname, e.enumsortorder",
            not_extension("t.oid")
        ),
        format!(
            "SELECT s.sequencename::text, s.data_type::text, s.start_value::text, \
             s.increment_by::text, s.min_value::text, s.max_value::text, s.cache_size::text, \
             CASE WHEN s.cycle THEN 'YES' ELSE 'NO' END \
             FROM pg_sequences s WHERE s.schemaname = {s} AND NOT EXISTS \
             (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_depend d ON d.objid = c.oid AND d.deptype IN ('i', 'e') \
             WHERE n.nspname = s.schemaname AND c.relname = s.sequencename) ORDER BY 1"
        ),
    ]
}

/// Read a `PostgreSQL` schema.
fn read_postgres(
    schema: &str,
    run: &mut dyn FnMut(&str) -> Result<Rows, SessionError>,
) -> Result<Snapshot, ReadError> {
    let sql = postgres_sql(schema);
    let mut results = Vec::with_capacity(sql.len());
    for q in &sql {
        results.push(run(q)?);
    }
    Ok(postgres_from_rows(schema, &results))
}

/// Build a `PostgreSQL` snapshot from the results of [`postgres_sql`], in
/// the same order. Missing results read as empty.
#[must_use]
pub fn postgres_from_rows(schema: &str, results: &[Rows]) -> Snapshot {
    let part = |i: usize| results.get(i).map_or(&[][..], Vec::as_slice);
    let mut s = Snapshot::new(Kind::Postgres, schema);
    s.missing = part(0).first().is_some_and(|r| cell(r, 0).trim() == "0");
    for r in part(1) {
        s.tables.push(Table {
            name: cell(r, 0),
            comment: cell(r, 1),
            ..Table::default()
        });
    }
    for r in part(2) {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == cell(r, 0)) else {
            continue;
        };
        t.columns.push(Column {
            name: cell(r, 1),
            data_type: cell(r, 2),
            nullable: cell(r, 3) == "YES",
            default: cell(r, 4),
            generated: cell(r, 5) == "YES",
            identity: cell(r, 6),
            comment: cell(r, 7),
        });
    }
    for r in part(3) {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == cell(r, 0)) else {
            continue;
        };
        let c = Constraint {
            name: cell(r, 1),
            definition: cell(r, 3),
            references: cell(r, 4),
        };
        match cell(r, 2).as_str() {
            "p" => t.primary_key = Some(c),
            "u" | "x" => t.uniques.push(c),
            "c" => t.checks.push(c),
            "f" => t.foreign_keys.push(c),
            _ => {}
        }
    }
    for r in part(4) {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == cell(r, 0)) else {
            continue;
        };
        t.indexes.push(Index {
            name: cell(r, 1),
            definition: cell(r, 2),
            unique: cell(r, 3) == "YES",
        });
    }
    postgres_objects(&mut s, results);
    s
}

/// The schema-level objects of [`postgres_from_rows`]: views and their
/// dependencies, routines, triggers, enums and sequences.
fn postgres_objects(s: &mut Snapshot, results: &[Rows]) {
    let part = |i: usize| results.get(i).map_or(&[][..], Vec::as_slice);
    for r in part(5) {
        s.views.push(View {
            name: cell(r, 0),
            materialized: cell(r, 1) == "YES",
            definition: cell(r, 2),
            depends_on: Vec::new(),
            comment: cell(r, 3),
        });
    }
    for r in part(6) {
        if let Some(v) = s.views.iter_mut().find(|v| v.name == cell(r, 0)) {
            v.depends_on.push((cell(r, 1), cell(r, 2)));
        }
    }
    for r in part(7) {
        s.routines.push(Routine {
            name: cell(r, 0),
            arguments: cell(r, 1),
            procedure: cell(r, 2) == "YES",
            definition: cell(r, 3),
        });
    }
    for r in part(8) {
        s.triggers.push(Trigger {
            table: cell(r, 0),
            name: cell(r, 1),
            definition: cell(r, 2),
        });
    }
    for r in part(9) {
        let name = cell(r, 0);
        match s.enums.iter_mut().find(|e| e.name == name) {
            Some(e) => e.values.push(cell(r, 1)),
            None => s.enums.push(EnumType {
                name,
                values: vec![cell(r, 1)],
            }),
        }
    }
    for r in part(10) {
        s.sequences.push(Sequence {
            name: cell(r, 0),
            data_type: cell(r, 1),
            start: cell(r, 2),
            increment: cell(r, 3),
            min: cell(r, 4),
            max: cell(r, 5),
            cache: cell(r, 6),
            cycle: cell(r, 7) == "YES",
        });
    }
}

/// Excludes `SQLite`'s own `sqlite_…` objects.
const SQLITE_USER: &str = "name NOT LIKE 'sqlite\\_%' ESCAPE '\\'";

/// The `SQLite` catalog queries, in the order [`read`] runs them: tables,
/// columns, foreign keys, indexes, views, triggers. Exposed for shape tests.
#[must_use]
pub fn sqlite_sql() -> Vec<String> {
    vec![
        format!(
            "SELECT name, COALESCE(sql, '') FROM sqlite_master \
             WHERE type = 'table' AND {SQLITE_USER} ORDER BY name"
        ),
        format!(
            "SELECT m.name, p.name, p.type, p.\"notnull\", COALESCE(p.dflt_value, ''), \
             p.hidden, p.pk FROM sqlite_master m JOIN pragma_table_xinfo(m.name) p \
             WHERE m.type = 'table' AND m.{SQLITE_USER} ORDER BY m.name, p.cid"
        ),
        format!(
            "SELECT m.name, f.id, f.\"table\", f.\"from\", COALESCE(f.\"to\", ''), \
             f.on_update, f.on_delete FROM sqlite_master m \
             JOIN pragma_foreign_key_list(m.name) f \
             WHERE m.type = 'table' AND m.{SQLITE_USER} ORDER BY m.name, f.id, f.seq"
        ),
        format!(
            "SELECT tbl_name, name, sql FROM sqlite_master \
             WHERE type = 'index' AND sql IS NOT NULL AND {SQLITE_USER} ORDER BY 1, 2"
        ),
        "SELECT name, sql FROM sqlite_master WHERE type = 'view' ORDER BY name".to_string(),
        "SELECT tbl_name, name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY 1, 2"
            .to_string(),
    ]
}

/// Read a `SQLite` database's `main` schema.
fn read_sqlite(
    run: &mut dyn FnMut(&str) -> Result<Rows, SessionError>,
) -> Result<Snapshot, ReadError> {
    let sql = sqlite_sql();
    let mut results = Vec::with_capacity(sql.len());
    for q in &sql {
        results.push(run(q)?);
    }
    Ok(sqlite_from_rows(&results))
}

/// Build a `SQLite` snapshot from the results of [`sqlite_sql`], in the same
/// order. Constraints other than the primary key and foreign keys live inside
/// [`Table::create_sql`]; view dependencies are found by matching the names
/// of tables and columns among the words of the view's SQL, since `SQLite`
/// keeps no dependency catalog.
#[must_use]
pub fn sqlite_from_rows(results: &[Rows]) -> Snapshot {
    let part = |i: usize| results.get(i).map_or(&[][..], Vec::as_slice);
    let mut s = Snapshot::new(Kind::Sqlite, "main");
    for r in part(0) {
        s.tables.push(Table {
            name: cell(r, 0),
            create_sql: cell(r, 1),
            ..Table::default()
        });
    }
    let mut pk: Vec<(String, usize, String)> = Vec::new();
    for r in part(1) {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == cell(r, 0)) else {
            continue;
        };
        let hidden = cell(r, 5);
        if hidden == "1" {
            continue; // a virtual table's hidden column
        }
        t.columns.push(Column {
            name: cell(r, 1),
            data_type: cell(r, 2),
            nullable: cell(r, 3) != "1",
            default: cell(r, 4),
            generated: hidden == "2" || hidden == "3",
            identity: String::new(),
            comment: String::new(),
        });
        if let Ok(n) = cell(r, 6).parse::<usize>()
            && n > 0
        {
            pk.push((cell(r, 0), n, cell(r, 1)));
        }
    }
    pk.sort();
    for t in &mut s.tables {
        let cols: Vec<String> = pk
            .iter()
            .filter(|(table, _, _)| *table == t.name)
            .map(|(_, _, c)| crate::catalog::quote_ident(Kind::Sqlite, c))
            .collect();
        if !cols.is_empty() {
            t.primary_key = Some(Constraint {
                name: "PRIMARY KEY".to_string(),
                definition: format!("PRIMARY KEY ({})", cols.join(", ")),
                references: String::new(),
            });
        }
    }
    sqlite_foreign_keys(&mut s, part(2));
    for r in part(3) {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == cell(r, 0)) else {
            continue;
        };
        let definition = cell(r, 2);
        t.indexes.push(Index {
            name: cell(r, 1),
            unique: definition.to_ascii_uppercase().starts_with("CREATE UNIQUE"),
            definition,
        });
    }
    for r in part(4) {
        s.views.push(View {
            name: cell(r, 0),
            definition: cell(r, 1),
            ..View::default()
        });
    }
    for r in part(5) {
        s.triggers.push(Trigger {
            table: cell(r, 0),
            name: cell(r, 1),
            definition: cell(r, 2),
        });
    }
    sqlite_view_dependencies(&mut s);
    s
}

/// A `SQLite` foreign key's identity: its table and its id there.
type ForeignKeyId = (String, String);

/// Group `pragma_foreign_key_list` rows (one per column) into one
/// constraint per foreign key. `SQLite` foreign keys have no names, so each
/// is named by its definition, which is what makes two databases' keys match.
fn sqlite_foreign_keys(s: &mut Snapshot, rows: &[Vec<String>]) {
    let q = |c: &str| crate::catalog::quote_ident(Kind::Sqlite, c);
    let mut groups: Vec<(ForeignKeyId, Vec<&Vec<String>>)> = Vec::new();
    for r in rows {
        let key = (cell(r, 0), cell(r, 1));
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(r),
            None => groups.push((key, vec![r])),
        }
    }
    for ((table, _), g) in groups {
        let Some(t) = s.tables.iter_mut().find(|t| t.name == table) else {
            continue;
        };
        let Some(first) = g.first() else { continue };
        let parent = cell(first, 2);
        let from: Vec<String> = g.iter().map(|r| q(&cell(r, 3))).collect();
        let to: Vec<String> = g
            .iter()
            .map(|r| cell(r, 4))
            .filter(|c| !c.is_empty())
            .map(|c| q(&c))
            .collect();
        let mut def = format!(
            "FOREIGN KEY ({}) REFERENCES {}",
            from.join(", "),
            q(&parent)
        );
        if !to.is_empty() {
            let _ = write!(def, " ({})", to.join(", "));
        }
        for (clause, action) in [("ON UPDATE", cell(first, 5)), ("ON DELETE", cell(first, 6))] {
            if !action.is_empty() && action != "NO ACTION" {
                let _ = write!(def, " {clause} {action}");
            }
        }
        t.foreign_keys.push(Constraint {
            name: def.clone(),
            definition: def,
            references: parent,
        });
    }
}

/// The lowercase identifier words of `sql` (quotes stripped).
fn words(sql: &str) -> std::collections::HashSet<String> {
    sql.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Estimate each `SQLite` view's dependencies from the words of its SQL: a
/// relation whose name appears, and those of its columns that appear too.
/// Errs on the side of too many, which only costs a harmless view rebuild.
fn sqlite_view_dependencies(s: &mut Snapshot) {
    let relations: Vec<(String, Vec<String>)> = s
        .tables
        .iter()
        .map(|t| {
            (
                t.name.clone(),
                t.columns.iter().map(|c| c.name.clone()).collect(),
            )
        })
        .chain(s.views.iter().map(|v| (v.name.clone(), Vec::new())))
        .collect();
    for v in &mut s.views {
        let w = words(&v.definition);
        for (rel, cols) in &relations {
            if *rel == v.name || !w.contains(&rel.to_lowercase()) {
                continue;
            }
            if cols.is_empty() {
                v.depends_on.push((rel.clone(), String::new()));
            }
            for c in cols {
                if w.contains(&c.to_lowercase()) {
                    v.depends_on.push((rel.clone(), c.clone()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|c| (*c).to_string()).collect()
    }

    #[test]
    fn postgres_sql_casts_to_text_and_filters_the_schema() {
        let sql = postgres_sql("it's");
        assert_eq!(sql.len(), 11);
        for q in &sql {
            assert!(q.starts_with("SELECT "), "{q}");
            // The schema name is always a quoted, escaped literal.
            assert!(q.contains("'it''s'"), "{q}");
        }
        // Catalog values the Any driver cannot decode are cast to text.
        for needle in [
            "pg_get_viewdef(c.oid)::text",
            "pg_get_constraintdef(con.oid)::text",
            "pg_get_indexdef(i.oid)::text",
            "pg_get_functiondef(p.oid)::text",
            "pg_get_triggerdef(t.oid)::text",
            "format_type(a.atttypid, a.atttypmod)::text",
        ] {
            assert!(sql.iter().any(|q| q.contains(needle)), "{needle}");
        }
        // Extension-owned objects are left out.
        assert!(sql.iter().filter(|q| q.contains("deptype = 'e'")).count() >= 4);
    }

    #[test]
    fn postgres_rows_build_a_snapshot() {
        let results = vec![
            vec![row(&["1"])],
            vec![row(&["users", "people"]), row(&["orders", ""])],
            vec![
                row(&["users", "id", "integer", "NO", "", "NO", "always", ""]),
                row(&[
                    "users", "email", "text", "YES", "''::text", "NO", "", "login",
                ]),
                row(&["orders", "user_id", "integer", "NO", "", "NO", "", ""]),
            ],
            vec![
                row(&["users", "users_pkey", "p", "PRIMARY KEY (id)", ""]),
                row(&["users", "users_email_key", "u", "UNIQUE (email)", ""]),
                row(&[
                    "orders",
                    "orders_user_fk",
                    "f",
                    "FOREIGN KEY (user_id) REFERENCES users(id)",
                    "users",
                ]),
                row(&["orders", "positive", "c", "CHECK ((user_id > 0))", ""]),
            ],
            vec![row(&[
                "orders",
                "orders_user_idx",
                "CREATE INDEX orders_user_idx ON public.orders USING btree (user_id)",
                "NO",
            ])],
            vec![row(&["v", "NO", " SELECT id FROM users;", ""])],
            vec![row(&["v", "users", "id"])],
            vec![row(&[
                "f",
                "a integer",
                "NO",
                "CREATE OR REPLACE FUNCTION …",
            ])],
            vec![row(&["users", "t", "CREATE TRIGGER t …"])],
            vec![row(&["mood", "sad"]), row(&["mood", "happy"])],
            vec![row(&["s", "bigint", "1", "1", "1", "9", "1", "NO"])],
        ];
        let s = postgres_from_rows("public", &results);
        assert!(!s.missing);
        let users = s.table("users").unwrap();
        assert_eq!(users.comment, "people");
        assert_eq!(users.columns.len(), 2);
        assert_eq!(users.columns[0].identity, "always");
        assert!(!users.columns[0].nullable);
        assert!(users.columns[1].nullable);
        assert_eq!(users.columns[1].comment, "login");
        assert_eq!(users.primary_key.as_ref().unwrap().name, "users_pkey");
        assert_eq!(users.uniques.len(), 1);
        let orders = s.table("orders").unwrap();
        assert_eq!(orders.foreign_keys[0].references, "users");
        assert_eq!(orders.checks.len(), 1);
        assert_eq!(orders.indexes[0].name, "orders_user_idx");
        assert_eq!(s.views[0].depends_on, vec![("users".into(), "id".into())]);
        assert_eq!(s.routines[0].signature(), "f(a integer)");
        assert_eq!(s.enums[0].values, vec!["sad", "happy"]);
        assert_eq!(s.sequences[0].max, "9");
        let missing = postgres_from_rows("x", &[vec![row(&["0"])]]);
        assert!(missing.missing);
    }

    #[test]
    fn sqlite_rows_build_a_snapshot_with_inline_constraints() {
        let results = vec![
            vec![
                row(&[
                    "users",
                    "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                ]),
                row(&[
                    "orders",
                    "CREATE TABLE orders (id INTEGER, user_id INTEGER)",
                ]),
            ],
            vec![
                row(&["orders", "id", "INTEGER", "0", "", "0", "0"]),
                row(&["orders", "user_id", "INTEGER", "1", "", "0", "0"]),
                row(&["users", "id", "INTEGER", "0", "", "0", "1"]),
                row(&["users", "name", "TEXT", "0", "'x'", "0", "0"]),
            ],
            vec![row(&[
                "orders",
                "0",
                "users",
                "user_id",
                "id",
                "NO ACTION",
                "CASCADE",
            ])],
            vec![row(&[
                "orders",
                "o_idx",
                "CREATE UNIQUE INDEX o_idx ON orders(user_id)",
            ])],
            vec![row(&["v", "CREATE VIEW v AS SELECT name FROM users"])],
            vec![row(&["users", "trg", "CREATE TRIGGER trg …"])],
        ];
        let s = sqlite_from_rows(&results);
        let users = s.table("users").unwrap();
        assert_eq!(
            users.primary_key.as_ref().unwrap().definition,
            "PRIMARY KEY (\"id\")"
        );
        assert_eq!(users.columns[1].default, "'x'");
        let orders = s.table("orders").unwrap();
        assert!(!orders.columns[1].nullable);
        assert_eq!(
            orders.foreign_keys[0].definition,
            "FOREIGN KEY (\"user_id\") REFERENCES \"users\" (\"id\") ON DELETE CASCADE"
        );
        assert!(orders.indexes[0].unique);
        assert_eq!(s.views[0].depends_on, vec![("users".into(), "name".into())]);
        assert_eq!(s.triggers[0].name, "trg");
        assert!(sqlite_sql().iter().all(|q| q.starts_with("SELECT ")));
    }

    #[test]
    fn mysql_is_unsupported() {
        let err = read(Kind::Mysql, "x", &mut |_| Ok(Vec::new())).unwrap_err();
        assert!(matches!(err, ReadError::Unsupported(Kind::Mysql)));
        assert!(!supported(Kind::Mysql));
    }

    #[test]
    fn read_runs_every_query_and_sorts() {
        let mut n = 0;
        let s = read(Kind::Sqlite, "main", &mut |_| {
            n += 1;
            Ok(Vec::new())
        })
        .unwrap();
        assert_eq!(n, sqlite_sql().len());
        assert_eq!(s.tables.len(), 0);
    }

    #[test]
    fn a_real_sqlite_file_reads_end_to_end() {
        let mut session = Session::connect("sqlite::memory:", &[]).unwrap();
        session
            .run(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE); \
                 CREATE TABLE orders (id INTEGER PRIMARY KEY, \
                 user_id INTEGER REFERENCES users(id), total REAL DEFAULT 0); \
                 CREATE INDEX orders_user ON orders(user_id); \
                 CREATE VIEW big AS SELECT id, total FROM orders WHERE total > 10;",
            )
            .unwrap();
        let s = read_session(&mut session, Kind::Sqlite, "main").unwrap();
        assert_eq!(s.tables.len(), 2);
        let orders = s.table("orders").unwrap();
        assert_eq!(orders.columns[2].default, "0");
        assert_eq!(orders.foreign_keys.len(), 1);
        assert_eq!(orders.indexes[0].name, "orders_user");
        assert!(
            s.views[0]
                .depends_on
                .contains(&("orders".into(), "total".into()))
        );
    }
}
