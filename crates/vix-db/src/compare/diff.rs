//! The schema diff (T785): from a source and a target [`Snapshot`] to the
//! list of [`Change`]s that make the target look like the source.
//!
//! The diff is pure. Each change carries both definitions (for the review
//! screen) and its own migration [`Statement`]s, each tagged with the
//! [`Phase`] it runs in; [`super::plan`] puts the statements of the chosen
//! changes in an order that runs. Objects are matched by name (routines by
//! name and arguments, triggers by table and name), so a renamed object
//! shows as one removed and one added.
//!
//! A change is **destructive** when it drops a table or a column, or can
//! lose data some other way (an enum value removed, a generated column
//! rebuilt); the review screen leaves those unchecked. It is **risky** when it
//! can fail on rows already in the target: a `NOT NULL` added, a unique
//! index or constraint, a check, a foreign key, or a column type change.
//!
//! `SQLite` can only add and drop columns in place. Anything else that
//! changes on an existing `SQLite` table becomes a *manual* change: its only
//! statement is an SQL comment that says what to rebuild, and running the
//! migration skips it.

use super::snapshot::{
    Column, Constraint, EnumType, Index, Routine, Sequence, Snapshot, Table, Trigger, View,
};
use crate::catalog::quote_ident;
use crate::connect::Kind;
use std::fmt::Write as _;

use super::read::lit;

/// When a migration statement runs. The migration runs phase by phase, in
/// declaration order, and keeps the diff's order inside a phase (views
/// excepted: they are ordered by their dependencies).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// `CREATE SCHEMA`.
    CreateSchema,
    /// `CREATE TYPE … AS ENUM`.
    CreateType,
    /// `ALTER TYPE … ADD VALUE`.
    AlterType,
    /// `CREATE SEQUENCE` and `ALTER SEQUENCE`.
    CreateSequence,
    /// `DROP TRIGGER`.
    DropTrigger,
    /// `DROP VIEW`.
    DropView,
    /// `ALTER TABLE … DROP CONSTRAINT` of a foreign key.
    DropForeignKey,
    /// `ALTER TABLE … DROP CONSTRAINT` of any other constraint.
    DropConstraint,
    /// `DROP INDEX`.
    DropIndex,
    /// `DROP TABLE`.
    DropTable,
    /// `ALTER TABLE … DROP COLUMN`.
    DropColumn,
    /// `DROP FUNCTION` / `DROP PROCEDURE`.
    DropRoutine,
    /// `CREATE TABLE`.
    CreateTable,
    /// `ALTER TABLE … ADD COLUMN` / `ALTER COLUMN`.
    AlterColumn,
    /// `ALTER TABLE … ADD CONSTRAINT` (primary key, unique, check).
    AddConstraint,
    /// `CREATE INDEX`.
    CreateIndex,
    /// `ALTER TABLE … ADD CONSTRAINT … FOREIGN KEY`.
    AddForeignKey,
    /// `CREATE OR REPLACE FUNCTION` / `PROCEDURE`.
    CreateRoutine,
    /// `CREATE VIEW`.
    CreateView,
    /// `CREATE TRIGGER`.
    CreateTrigger,
    /// `COMMENT ON`.
    Comment,
    /// `DROP SEQUENCE`.
    DropSequence,
    /// `DROP TYPE`.
    DropType,
}

/// What kind of object a [`Change`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Object {
    /// The schema itself.
    Schema,
    /// A table.
    Table,
    /// A column of a table.
    Column,
    /// A primary key.
    PrimaryKey,
    /// A unique or exclusion constraint.
    Unique,
    /// A check constraint.
    Check,
    /// An index.
    Index,
    /// A foreign key.
    ForeignKey,
    /// A view or materialized view.
    View,
    /// A function or procedure.
    Routine,
    /// A trigger.
    Trigger,
    /// An enum type.
    Enum,
    /// A sequence.
    Sequence,
}

impl Object {
    /// The i18n key of the object kind's display name.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Object::Schema => "ui.db_compare_obj_schema",
            Object::Table => "ui.db_compare_obj_table",
            Object::Column => "ui.db_compare_obj_column",
            Object::PrimaryKey => "ui.db_compare_obj_primary_key",
            Object::Unique => "ui.db_compare_obj_unique",
            Object::Check => "ui.db_compare_obj_check",
            Object::Index => "ui.db_compare_obj_index",
            Object::ForeignKey => "ui.db_compare_obj_foreign_key",
            Object::View => "ui.db_compare_obj_view",
            Object::Routine => "ui.db_compare_obj_routine",
            Object::Trigger => "ui.db_compare_obj_trigger",
            Object::Enum => "ui.db_compare_obj_enum",
            Object::Sequence => "ui.db_compare_obj_sequence",
        }
    }
}

/// Whether the object was added, removed or changed in the source relative
/// to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Action {
    /// In the source only: the migration creates it.
    Added,
    /// In the target only: the migration drops it.
    Removed,
    /// In both, defined differently: the migration alters or replaces it.
    Changed,
}

impl Action {
    /// A one-character marker: `+`, `-` or `~`.
    #[must_use]
    pub fn marker(self) -> char {
        match self {
            Action::Added => '+',
            Action::Removed => '-',
            Action::Changed => '~',
        }
    }
}

/// One migration statement (no trailing `;`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Statement {
    /// When it runs.
    pub phase: Phase,
    /// The SQL; a statement that is only an SQL comment is a note for a
    /// change that must be made by hand, and is never run.
    pub sql: String,
    /// The view this statement creates or drops, for dependency ordering.
    pub(crate) view: Option<String>,
}

impl Statement {
    /// A statement in `phase`.
    #[must_use]
    pub fn new(phase: Phase, sql: impl Into<String>) -> Statement {
        Statement {
            phase,
            sql: sql.into(),
            view: None,
        }
    }

    /// A statement that creates or drops view `name`.
    pub(crate) fn on_view(phase: Phase, sql: String, name: &str) -> Statement {
        Statement {
            phase,
            sql,
            view: Some(name.to_string()),
        }
    }

    /// Whether this is a note (only SQL comments), not something to run.
    #[must_use]
    pub fn is_note(&self) -> bool {
        self.sql
            .lines()
            .all(|l| l.trim().is_empty() || l.trim_start().starts_with("--"))
    }
}

/// One difference between the source and the target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Change {
    /// The kind of object.
    pub object: Object,
    /// Added, removed or changed.
    pub action: Action,
    /// The owning table for columns, keys, constraints, indexes and
    /// triggers; empty otherwise.
    pub table: String,
    /// The object's name (`name(arguments)` for a routine).
    pub name: String,
    /// Its definition in the source (empty when removed).
    pub source: String,
    /// Its definition in the target (empty when added).
    pub target: String,
    /// The statements that apply it to the target, each with its phase.
    pub statements: Vec<Statement>,
    /// It drops a table or column, or can otherwise lose data.
    pub destructive: bool,
    /// It can fail on rows already in the target.
    pub risky: bool,
    /// The `(relation, column)` it drops or retypes (column empty for the
    /// whole relation), so views that use it are rebuilt around it.
    pub(crate) touches: Vec<(String, String)>,
}

impl Change {
    /// The qualified display name: `table.name`, or `name` alone.
    #[must_use]
    pub fn label(&self) -> String {
        if self.table.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.table, self.name)
        }
    }

    /// Whether the change can only be made by hand (every statement is a
    /// note).
    #[must_use]
    pub fn is_manual(&self) -> bool {
        !self.statements.is_empty() && self.statements.iter().all(Statement::is_note)
    }

    /// A change with no statements yet.
    fn new(object: Object, action: Action, table: &str, name: &str) -> Change {
        Change {
            object,
            action,
            table: table.to_string(),
            name: name.to_string(),
            source: String::new(),
            target: String::new(),
            statements: Vec::new(),
            destructive: false,
            risky: false,
            touches: Vec::new(),
        }
    }

    /// Set both definitions.
    fn defs(mut self, source: impl Into<String>, target: impl Into<String>) -> Change {
        self.source = source.into();
        self.target = target.into();
        self
    }

    /// Append a statement.
    fn push(&mut self, phase: Phase, sql: impl Into<String>) {
        self.statements.push(Statement::new(phase, sql));
    }
}

/// Collapse whitespace and drop a trailing `;`, so definitions that differ
/// only in layout compare equal.
fn norm(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.trim_end_matches(';').trim_end().to_string()
}

/// The SQL generator for `snapshot`'s dialect and schema.
pub(crate) fn view_sql(snapshot: &Snapshot) -> Sql<'_> {
    Sql {
        kind: snapshot.kind,
        schema: &snapshot.schema,
    }
}

/// SQL generation for one dialect and schema.
pub(crate) struct Sql<'a> {
    /// The engine.
    kind: Kind,
    /// The schema name.
    schema: &'a str,
}

impl Sql<'_> {
    /// A quoted identifier.
    fn q(&self, name: &str) -> String {
        quote_ident(self.kind, name)
    }

    /// A schema-qualified name (`SQLite` has no schema prefix).
    fn qual(&self, name: &str) -> String {
        match self.kind {
            Kind::Sqlite => self.q(name),
            Kind::Postgres | Kind::Mysql => format!("{}.{}", self.q(self.schema), self.q(name)),
        }
    }

    /// `true` on `SQLite`.
    fn sqlite(&self) -> bool {
        self.kind == Kind::Sqlite
    }

    /// A column definition for `CREATE TABLE` / `ADD COLUMN`.
    fn column_def(&self, c: &Column) -> String {
        let mut out = format!("{} {}", self.q(&c.name), c.data_type);
        if c.generated {
            if !c.default.is_empty() {
                let _ = write!(out, " GENERATED ALWAYS AS ({}) STORED", c.default);
            }
        } else if !c.default.is_empty() {
            let _ = write!(out, " DEFAULT {}", c.default);
        }
        match c.identity.as_str() {
            "always" => out.push_str(" GENERATED ALWAYS AS IDENTITY"),
            "default" => out.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
            _ => {}
        }
        if !c.nullable {
            out.push_str(" NOT NULL");
        }
        out.trim_end().to_string()
    }

    /// `ALTER TABLE <t> `.
    fn alter(&self, table: &str) -> String {
        format!("ALTER TABLE {} ", self.qual(table))
    }

    /// `COMMENT ON COLUMN …` (empty comment clears it).
    fn column_comment(&self, table: &str, c: &Column) -> String {
        format!(
            "COMMENT ON COLUMN {}.{} IS {}",
            self.qual(table),
            self.q(&c.name),
            comment_value(&c.comment)
        )
    }

    /// A view's `CREATE` statement.
    pub(crate) fn create_view(&self, v: &View) -> String {
        if self.sqlite() {
            return v.definition.trim().trim_end_matches(';').to_string();
        }
        let kind = if v.materialized {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        format!(
            "CREATE {kind} {} AS\n{}",
            self.qual(&v.name),
            v.definition.trim().trim_end_matches(';').trim_end()
        )
    }

    /// A view's `DROP` statement.
    pub(crate) fn drop_view(&self, v: &View) -> String {
        let kind = if v.materialized {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        format!("DROP {kind} {}", self.qual(&v.name))
    }
}

/// The literal for `COMMENT ON … IS`: `NULL` clears it.
fn comment_value(comment: &str) -> String {
    if comment.is_empty() {
        "NULL".to_string()
    } else {
        lit(comment)
    }
}

/// The changes that make `target` look like `source`, in a stable order:
/// schema, enums, sequences, tables (each with its columns, keys,
/// constraints and indexes), views, routines, triggers.
#[must_use]
pub fn diff(source: &Snapshot, target: &Snapshot) -> Vec<Change> {
    let sql = Sql {
        kind: source.kind,
        schema: &source.schema,
    };
    let mut out = Vec::new();
    if target.missing && !source.missing && !sql.sqlite() {
        let mut c = Change::new(Object::Schema, Action::Added, "", &source.schema)
            .defs(source.schema.clone(), "");
        c.push(
            Phase::CreateSchema,
            format!("CREATE SCHEMA IF NOT EXISTS {}", sql.q(&source.schema)),
        );
        out.push(c);
    }
    diff_enums(&sql, source, target, &mut out);
    diff_sequences(&sql, source, target, &mut out);
    diff_tables(&sql, source, target, &mut out);
    diff_views(&sql, source, target, &mut out);
    diff_routines(&sql, source, target, &mut out);
    diff_triggers(&sql, source, target, &mut out);
    out
}

/// Pair two name-keyed lists: `(source, target)` for every name in either,
/// in source order then the target's extras.
fn pair<'a, T>(
    source: &'a [T],
    target: &'a [T],
    key: impl Fn(&T) -> String,
) -> Vec<(Option<&'a T>, Option<&'a T>)> {
    let mut out: Vec<(Option<&T>, Option<&T>)> = source
        .iter()
        .map(|s| (Some(s), target.iter().find(|t| key(t) == key(s))))
        .collect();
    for t in target {
        if !source.iter().any(|s| key(s) == key(t)) {
            out.push((None, Some(t)));
        }
    }
    out
}

/// Enum types.
fn diff_enums(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    let show = |e: &EnumType| {
        let values: Vec<String> = e.values.iter().map(|v| lit(v)).collect();
        format!("ENUM ({})", values.join(", "))
    };
    for (s, t) in pair(&source.enums, &target.enums, |e| e.name.clone()) {
        match (s, t) {
            (Some(s), None) => {
                let mut c = Change::new(Object::Enum, Action::Added, "", &s.name).defs(show(s), "");
                c.push(
                    Phase::CreateType,
                    format!("CREATE TYPE {} AS {}", sql.qual(&s.name), show(s)),
                );
                out.push(c);
            }
            (None, Some(t)) => {
                let mut c =
                    Change::new(Object::Enum, Action::Removed, "", &t.name).defs("", show(t));
                c.push(Phase::DropType, format!("DROP TYPE {}", sql.qual(&t.name)));
                out.push(c);
            }
            (Some(s), Some(t)) if s.values != t.values => {
                let mut c =
                    Change::new(Object::Enum, Action::Changed, "", &s.name).defs(show(s), show(t));
                enum_values(sql, s, t, &mut c);
                out.push(c);
            }
            _ => {}
        }
    }
}

/// The statements that turn enum `t`'s values into `s`'s: `ADD VALUE` when
/// `t`'s values are all still there in the same order, otherwise a manual
/// note (an enum value cannot be removed or moved in place), flagged
/// destructive.
fn enum_values(sql: &Sql, s: &EnumType, t: &EnumType, c: &mut Change) {
    let kept: Vec<&String> = s.values.iter().filter(|v| t.values.contains(v)).collect();
    let in_order = kept.len() == t.values.len() && kept.iter().zip(&t.values).all(|(a, b)| *a == b);
    if !in_order {
        c.destructive = true;
        c.push(
            Phase::AlterType,
            format!(
                "-- {} values were removed or reordered; recreate the type by hand: {}",
                sql.qual(&s.name),
                c.source
            ),
        );
        return;
    }
    let mut prev: Option<&String> = None;
    for v in &s.values {
        if !t.values.contains(v) {
            let place = match prev {
                Some(p) => format!(" AFTER {}", lit(p)),
                None => t
                    .values
                    .first()
                    .map_or_default(|first| format!(" BEFORE {}", lit(first))),
            };
            c.push(
                Phase::AlterType,
                format!(
                    "ALTER TYPE {} ADD VALUE {}{place}",
                    sql.qual(&s.name),
                    lit(v)
                ),
            );
        }
        prev = Some(v);
    }
}

/// Sequences.
fn diff_sequences(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    for (s, t) in pair(&source.sequences, &target.sequences, |q| q.name.clone()) {
        match (s, t) {
            (Some(s), None) => {
                let mut c =
                    Change::new(Object::Sequence, Action::Added, "", &s.name).defs(s.options(), "");
                c.push(
                    Phase::CreateSequence,
                    format!("CREATE SEQUENCE {} {}", sql.qual(&s.name), s.options()),
                );
                out.push(c);
            }
            (None, Some(t)) => {
                let mut c = Change::new(Object::Sequence, Action::Removed, "", &t.name)
                    .defs("", t.options());
                // A sequence a dropped serial column owned is already gone.
                c.push(
                    Phase::DropSequence,
                    format!("DROP SEQUENCE IF EXISTS {}", sql.qual(&t.name)),
                );
                out.push(c);
            }
            (Some(s), Some(t)) if sequence_differs(s, t) => {
                let mut c = Change::new(Object::Sequence, Action::Changed, "", &s.name)
                    .defs(s.options(), t.options());
                c.push(
                    Phase::CreateSequence,
                    format!("ALTER SEQUENCE {} {}", sql.qual(&s.name), s.options()),
                );
                out.push(c);
            }
            _ => {}
        }
    }
}

/// Whether two sequences' options differ.
fn sequence_differs(s: &Sequence, t: &Sequence) -> bool {
    s.options() != t.options()
}

/// Tables and everything that belongs to them.
fn diff_tables(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    for (s, t) in pair(&source.tables, &target.tables, |t| t.name.clone()) {
        match (s, t) {
            (Some(s), None) => out.push(create_table(sql, s)),
            (None, Some(t)) => {
                let mut c = Change::new(Object::Table, Action::Removed, "", &t.name)
                    .defs("", table_summary(sql, t));
                c.push(
                    Phase::DropTable,
                    format!("DROP TABLE {}", sql.qual(&t.name)),
                );
                c.destructive = true;
                c.touches.push((t.name.clone(), String::new()));
                out.push(c);
            }
            (Some(s), Some(t)) => alter_table(sql, s, t, out),
            (None, None) => {}
        }
    }
}

/// A table's columns, one per line, for display.
fn table_summary(sql: &Sql, t: &Table) -> String {
    if !t.create_sql.is_empty() {
        return t.create_sql.clone();
    }
    let cols: Vec<String> = t.columns.iter().map(|c| sql.column_def(c)).collect();
    format!("({})", cols.join(", "))
}

/// The change that creates table `s` with its keys, constraints, indexes
/// and comments, each statement in its own phase.
fn create_table(sql: &Sql, s: &Table) -> Change {
    let mut c =
        Change::new(Object::Table, Action::Added, "", &s.name).defs(table_summary(sql, s), "");
    if sql.sqlite() && !s.create_sql.is_empty() {
        // SQLite keeps the whole statement, constraints and keys inline.
        c.push(
            Phase::CreateTable,
            s.create_sql.trim().trim_end_matches(';'),
        );
    } else {
        let cols: Vec<String> = s
            .columns
            .iter()
            .map(|col| format!("    {}", sql.column_def(col)))
            .collect();
        c.push(
            Phase::CreateTable,
            format!(
                "CREATE TABLE {} (\n{}\n)",
                sql.qual(&s.name),
                cols.join(",\n")
            ),
        );
        let constraints = s.primary_key.iter().chain(&s.uniques).chain(&s.checks);
        for k in constraints {
            c.push(Phase::AddConstraint, add_constraint(sql, &s.name, k));
        }
        for k in &s.foreign_keys {
            c.push(Phase::AddForeignKey, add_constraint(sql, &s.name, k));
        }
        if !s.comment.is_empty() {
            c.push(
                Phase::Comment,
                format!(
                    "COMMENT ON TABLE {} IS {}",
                    sql.qual(&s.name),
                    lit(&s.comment)
                ),
            );
        }
        for col in s.columns.iter().filter(|col| !col.comment.is_empty()) {
            c.push(Phase::Comment, sql.column_comment(&s.name, col));
        }
    }
    for i in &s.indexes {
        c.push(
            Phase::CreateIndex,
            i.definition.trim().trim_end_matches(';'),
        );
    }
    c
}

/// `ALTER TABLE … ADD CONSTRAINT name definition`.
fn add_constraint(sql: &Sql, table: &str, k: &Constraint) -> String {
    format!(
        "{}ADD CONSTRAINT {} {}",
        sql.alter(table),
        sql.q(&k.name),
        k.definition
    )
}

/// `ALTER TABLE … DROP CONSTRAINT name`.
fn drop_constraint(sql: &Sql, table: &str, k: &Constraint) -> String {
    format!("{}DROP CONSTRAINT {}", sql.alter(table), sql.q(&k.name))
}

/// The changes inside a table present on both sides.
fn alter_table(sql: &Sql, s: &Table, t: &Table, out: &mut Vec<Change>) {
    for (sc, tc) in pair(&s.columns, &t.columns, |c| c.name.clone()) {
        if let Some(c) = column_change(sql, &s.name, sc, tc) {
            out.push(c);
        }
    }
    if s.comment != t.comment && !sql.sqlite() {
        let mut c = Change::new(Object::Table, Action::Changed, "", &s.name)
            .defs(s.comment.clone(), t.comment.clone());
        c.push(
            Phase::Comment,
            format!(
                "COMMENT ON TABLE {} IS {}",
                sql.qual(&s.name),
                comment_value(&s.comment)
            ),
        );
        out.push(c);
    }
    let pk = |x: &Table| x.primary_key.iter().cloned().collect::<Vec<_>>();
    constraint_changes(sql, &s.name, Object::PrimaryKey, &pk(s), &pk(t), out);
    if !sql.sqlite() {
        constraint_changes(sql, &s.name, Object::Unique, &s.uniques, &t.uniques, out);
        constraint_changes(sql, &s.name, Object::Check, &s.checks, &t.checks, out);
    }
    index_changes(sql, &s.name, &s.indexes, &t.indexes, out);
    constraint_changes(
        sql,
        &s.name,
        Object::ForeignKey,
        &s.foreign_keys,
        &t.foreign_keys,
        out,
    );
}

/// A manual note for a `SQLite` table change that needs a table rebuild.
fn sqlite_rebuild_note(sql: &Sql, table: &str, what: &str) -> String {
    format!(
        "-- SQLite cannot alter {what} of {} in place; rebuild the table by hand",
        sql.q(table)
    )
}

/// The change for one column, if it differs.
fn column_change(sql: &Sql, table: &str, s: Option<&Column>, t: Option<&Column>) -> Option<Change> {
    let def = |c: &Column| sql.column_def(c);
    match (s, t) {
        (Some(s), None) => {
            let mut c = Change::new(Object::Column, Action::Added, table, &s.name).defs(def(s), "");
            if sql.sqlite() && s.generated {
                c.push(Phase::AlterColumn, sqlite_rebuild_note(sql, table, &s.name));
            } else {
                c.push(
                    Phase::AlterColumn,
                    format!("{}ADD COLUMN {}", sql.alter(table), def(s)),
                );
            }
            // Adding NOT NULL without a default fails once the table has rows.
            c.risky = !s.nullable && s.default.is_empty() && s.identity.is_empty();
            if !s.comment.is_empty() && !sql.sqlite() {
                c.push(Phase::Comment, sql.column_comment(table, s));
            }
            Some(c)
        }
        (None, Some(t)) => {
            let mut c =
                Change::new(Object::Column, Action::Removed, table, &t.name).defs("", def(t));
            c.push(
                Phase::DropColumn,
                format!("{}DROP COLUMN {}", sql.alter(table), sql.q(&t.name)),
            );
            c.destructive = true;
            c.touches.push((table.to_string(), t.name.clone()));
            Some(c)
        }
        (Some(s), Some(t)) if s != t => {
            let mut c =
                Change::new(Object::Column, Action::Changed, table, &s.name).defs(def(s), def(t));
            if sql.sqlite() {
                if def(s) == def(t) {
                    return None; // only comments, which SQLite has none of
                }
                c.risky = true;
                c.push(Phase::AlterColumn, sqlite_rebuild_note(sql, table, &s.name));
            } else {
                alter_column(sql, table, s, t, &mut c);
            }
            Some(c)
        }
        _ => None,
    }
}

/// The `ALTER COLUMN` statements turning column `t` into `s` (`PostgreSQL`).
fn alter_column(sql: &Sql, table: &str, s: &Column, t: &Column, c: &mut Change) {
    let at = format!("{}ALTER COLUMN {} ", sql.alter(table), sql.q(&s.name));
    if s.generated != t.generated || (s.generated && s.default != t.default) {
        // A generation expression cannot be altered in place everywhere:
        // drop the column and add it back.
        c.destructive = true;
        c.touches.push((table.to_string(), s.name.clone()));
        c.push(
            Phase::DropColumn,
            format!("{}DROP COLUMN {}", sql.alter(table), sql.q(&s.name)),
        );
        c.push(
            Phase::AlterColumn,
            format!("{}ADD COLUMN {}", sql.alter(table), sql.column_def(s)),
        );
        if s.comment != t.comment && !s.comment.is_empty() {
            c.push(Phase::Comment, sql.column_comment(table, s));
        }
        return;
    }
    let retype = s.data_type != t.data_type;
    if retype {
        c.risky = true;
        c.touches.push((table.to_string(), s.name.clone()));
        // A default of the old type may not cast: drop it first, restore it
        // after.
        if !t.default.is_empty() {
            c.push(Phase::AlterColumn, format!("{at}DROP DEFAULT"));
        }
        c.push(
            Phase::AlterColumn,
            format!(
                "{at}TYPE {ty} USING {col}::{ty}",
                ty = s.data_type,
                col = sql.q(&s.name)
            ),
        );
    }
    if s.default != t.default || (retype && !t.default.is_empty()) {
        if s.default.is_empty() {
            if !retype {
                c.push(Phase::AlterColumn, format!("{at}DROP DEFAULT"));
            }
        } else {
            c.push(Phase::AlterColumn, format!("{at}SET DEFAULT {}", s.default));
        }
    }
    if s.nullable != t.nullable {
        if s.nullable {
            c.push(Phase::AlterColumn, format!("{at}DROP NOT NULL"));
        } else {
            c.risky = true;
            c.push(Phase::AlterColumn, format!("{at}SET NOT NULL"));
        }
    }
    if s.identity != t.identity {
        let how = |i: &str| {
            if i == "always" {
                "ALWAYS"
            } else {
                "BY DEFAULT"
            }
        };
        let stmt = match (s.identity.is_empty(), t.identity.is_empty()) {
            (true, _) => format!("{at}DROP IDENTITY IF EXISTS"),
            (false, true) => format!("{at}ADD GENERATED {} AS IDENTITY", how(&s.identity)),
            (false, false) => format!("{at}SET GENERATED {}", how(&s.identity)),
        };
        c.push(Phase::AlterColumn, stmt);
    }
    if s.comment != t.comment {
        c.push(Phase::Comment, sql.column_comment(table, s));
    }
}

/// Keys and constraints of one kind (primary key, unique, check, foreign
/// key), matched by name.
fn constraint_changes(
    sql: &Sql,
    table: &str,
    object: Object,
    source: &[Constraint],
    target: &[Constraint],
    out: &mut Vec<Change>,
) {
    let fk = object == Object::ForeignKey;
    let (add_phase, drop_phase) = if fk {
        (Phase::AddForeignKey, Phase::DropForeignKey)
    } else {
        (Phase::AddConstraint, Phase::DropConstraint)
    };
    for (s, t) in pair(source, target, |k| k.name.clone()) {
        let (action, name) = match (s, t) {
            (Some(s), None) => (Action::Added, &s.name),
            (None, Some(t)) => (Action::Removed, &t.name),
            (Some(s), Some(t)) if norm(&s.definition) != norm(&t.definition) => {
                (Action::Changed, &s.name)
            }
            _ => continue,
        };
        let mut c = Change::new(object, action, table, name).defs(
            s.map_or_default(|k| k.definition.clone()),
            t.map_or_default(|k| k.definition.clone()),
        );
        // Any key or constraint added can fail on rows already there.
        c.risky = s.is_some();
        if sql.sqlite() {
            // SQLite keys live inside CREATE TABLE.
            c.push(
                add_phase,
                sqlite_rebuild_note(sql, table, &object_word(object)),
            );
        } else {
            if let Some(t) = t {
                c.push(drop_phase, drop_constraint(sql, table, t));
            }
            if let Some(s) = s {
                c.push(add_phase, add_constraint(sql, table, s));
            }
        }
        out.push(c);
    }
}

/// The SQL words for a key kind, for `SQLite` notes.
fn object_word(object: Object) -> String {
    match object {
        Object::PrimaryKey => "the primary key",
        Object::ForeignKey => "a foreign key",
        _ => "a constraint",
    }
    .to_string()
}

/// Indexes, matched by name.
fn index_changes(
    sql: &Sql,
    table: &str,
    source: &[Index],
    target: &[Index],
    out: &mut Vec<Change>,
) {
    for (s, t) in pair(source, target, |i| i.name.clone()) {
        let (action, name) = match (s, t) {
            (Some(s), None) => (Action::Added, &s.name),
            (None, Some(t)) => (Action::Removed, &t.name),
            (Some(s), Some(t)) if norm(&s.definition) != norm(&t.definition) => {
                (Action::Changed, &s.name)
            }
            _ => continue,
        };
        let mut c = Change::new(Object::Index, action, table, name).defs(
            s.map_or_default(|i| i.definition.clone()),
            t.map_or_default(|i| i.definition.clone()),
        );
        if let Some(t) = t {
            c.push(
                Phase::DropIndex,
                format!("DROP INDEX {}", sql.qual(&t.name)),
            );
        }
        if let Some(s) = s {
            c.risky = s.unique;
            c.push(
                Phase::CreateIndex,
                s.definition.trim().trim_end_matches(';'),
            );
        }
        out.push(c);
    }
}

/// Views, compared by their normalized definitions.
fn diff_views(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    for (s, t) in pair(&source.views, &target.views, |v| v.name.clone()) {
        match (s, t) {
            (Some(s), None) => {
                let mut c = Change::new(Object::View, Action::Added, "", &s.name)
                    .defs(s.definition.trim(), "");
                c.statements.push(Statement::on_view(
                    Phase::CreateView,
                    sql.create_view(s),
                    &s.name,
                ));
                if !s.comment.is_empty() {
                    c.push(Phase::Comment, view_comment(sql, s));
                }
                out.push(c);
            }
            (None, Some(t)) => {
                let mut c = Change::new(Object::View, Action::Removed, "", &t.name)
                    .defs("", t.definition.trim());
                c.statements.push(Statement::on_view(
                    Phase::DropView,
                    sql.drop_view(t),
                    &t.name,
                ));
                c.touches.push((t.name.clone(), String::new()));
                out.push(c);
            }
            (Some(s), Some(t)) => {
                let body_differs =
                    norm(&s.definition) != norm(&t.definition) || s.materialized != t.materialized;
                if !body_differs && s.comment == t.comment {
                    continue;
                }
                let mut c = Change::new(Object::View, Action::Changed, "", &s.name)
                    .defs(s.definition.trim(), t.definition.trim());
                if body_differs {
                    c.statements.push(Statement::on_view(
                        Phase::DropView,
                        sql.drop_view(t),
                        &t.name,
                    ));
                    c.statements.push(Statement::on_view(
                        Phase::CreateView,
                        sql.create_view(s),
                        &s.name,
                    ));
                    c.touches.push((t.name.clone(), String::new()));
                }
                if (body_differs && !s.comment.is_empty()) || s.comment != t.comment {
                    c.push(Phase::Comment, view_comment(sql, s));
                }
                out.push(c);
            }
            (None, None) => {}
        }
    }
}

/// `COMMENT ON VIEW …` (`PostgreSQL` only; `SQLite` has no comments).
fn view_comment(sql: &Sql, v: &View) -> String {
    let kind = if v.materialized {
        "MATERIALIZED VIEW"
    } else {
        "VIEW"
    };
    format!(
        "COMMENT ON {kind} {} IS {}",
        sql.qual(&v.name),
        comment_value(&v.comment)
    )
}

/// Functions and procedures, matched by name and arguments.
fn diff_routines(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    for (s, t) in pair(&source.routines, &target.routines, Routine::signature) {
        match (s, t) {
            (Some(s), None) => {
                let mut c = Change::new(Object::Routine, Action::Added, "", &s.signature())
                    .defs(s.definition.trim(), "");
                c.push(
                    Phase::CreateRoutine,
                    s.definition.trim().trim_end_matches(';'),
                );
                out.push(c);
            }
            (None, Some(t)) => {
                let mut c = Change::new(Object::Routine, Action::Removed, "", &t.signature())
                    .defs("", t.definition.trim());
                let kind = if t.procedure { "PROCEDURE" } else { "FUNCTION" };
                c.push(
                    Phase::DropRoutine,
                    format!("DROP {kind} {}({})", sql.qual(&t.name), t.arguments),
                );
                out.push(c);
            }
            (Some(s), Some(t)) if norm(&s.definition) != norm(&t.definition) => {
                let mut c = Change::new(Object::Routine, Action::Changed, "", &s.signature())
                    .defs(s.definition.trim(), t.definition.trim());
                // pg_get_functiondef already says CREATE OR REPLACE.
                c.push(
                    Phase::CreateRoutine,
                    s.definition.trim().trim_end_matches(';'),
                );
                out.push(c);
            }
            _ => {}
        }
    }
}

/// Triggers, matched by table and name. A removed table's triggers go with
/// it, so they are not listed separately.
fn diff_triggers(sql: &Sql, source: &Snapshot, target: &Snapshot, out: &mut Vec<Change>) {
    let key = |t: &Trigger| format!("{}\u{0}{}", t.table, t.name);
    let drop = |t: &Trigger| {
        if sql.sqlite() {
            format!("DROP TRIGGER {}", sql.q(&t.name))
        } else {
            format!("DROP TRIGGER {} ON {}", sql.q(&t.name), sql.qual(&t.table))
        }
    };
    for (s, t) in pair(&source.triggers, &target.triggers, key) {
        let (action, trig) = match (s, t) {
            (Some(s), None) => (Action::Added, s),
            (None, Some(t)) if source.table(&t.table).is_some() => (Action::Removed, t),
            (Some(s), Some(t)) if norm(&s.definition) != norm(&t.definition) => {
                (Action::Changed, s)
            }
            _ => continue,
        };
        let mut c = Change::new(Object::Trigger, action, &trig.table, &trig.name).defs(
            s.map_or_default(|x| x.definition.trim().to_string()),
            t.map_or_default(|x| x.definition.trim().to_string()),
        );
        if let Some(t) = t {
            c.push(Phase::DropTrigger, drop(t));
        }
        if let Some(s) = s {
            c.push(
                Phase::CreateTrigger,
                s.definition.trim().trim_end_matches(';'),
            );
        }
        out.push(c);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A column.
    pub(crate) fn col(name: &str, ty: &str, nullable: bool) -> Column {
        Column {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            ..Column::default()
        }
    }

    /// A table with columns and a primary key on `id` (when it has one).
    pub(crate) fn table(name: &str, columns: Vec<Column>) -> Table {
        let primary_key = columns.iter().any(|c| c.name == "id").then(|| Constraint {
            name: format!("{name}_pkey"),
            definition: "PRIMARY KEY (id)".into(),
            references: String::new(),
        });
        Table {
            name: name.into(),
            columns,
            primary_key,
            ..Table::default()
        }
    }

    /// An empty `PostgreSQL` snapshot of `public`.
    pub(crate) fn pg() -> Snapshot {
        Snapshot::new(Kind::Postgres, "public")
    }

    fn sqls(c: &Change) -> Vec<&str> {
        c.statements.iter().map(|s| s.sql.as_str()).collect()
    }

    fn find<'a>(changes: &'a [Change], object: Object, name: &str) -> &'a Change {
        changes
            .iter()
            .find(|c| c.object == object && c.name == name)
            .unwrap_or_else(|| panic!("no {object:?} {name} in {changes:#?}"))
    }

    #[test]
    fn identical_snapshots_have_no_changes() {
        let mut s = pg();
        s.tables
            .push(table("users", vec![col("id", "integer", false)]));
        assert_eq!(diff(&s, &s.clone()), Vec::new());
    }

    #[test]
    fn a_new_table_with_a_foreign_key_to_another_new_table() {
        let mut source = pg();
        source
            .tables
            .push(table("users", vec![col("id", "integer", false)]));
        let mut orders = table(
            "orders",
            vec![col("id", "integer", false), col("user_id", "integer", true)],
        );
        orders.foreign_keys.push(Constraint {
            name: "orders_user_fk".into(),
            definition: "FOREIGN KEY (user_id) REFERENCES users(id)".into(),
            references: "users".into(),
        });
        source.tables.push(orders);
        let changes = diff(&source, &pg());
        let orders = find(&changes, Object::Table, "orders");
        assert_eq!(orders.action, Action::Added);
        assert!(!orders.destructive && !orders.risky);
        let phases: Vec<Phase> = orders.statements.iter().map(|s| s.phase).collect();
        assert_eq!(
            phases,
            vec![
                Phase::CreateTable,
                Phase::AddConstraint,
                Phase::AddForeignKey
            ]
        );
        assert_eq!(
            orders.statements[0].sql,
            "CREATE TABLE \"public\".\"orders\" (\n    \"id\" integer NOT NULL,\n    \"user_id\" integer\n)"
        );
        assert!(orders.statements[2].sql.ends_with(
            "ADD CONSTRAINT \"orders_user_fk\" FOREIGN KEY (user_id) REFERENCES users(id)"
        ));
    }

    #[test]
    fn a_dropped_column_is_destructive_and_touches_the_column() {
        let mut source = pg();
        source
            .tables
            .push(table("users", vec![col("id", "integer", false)]));
        let mut target = pg();
        target.tables.push(table(
            "users",
            vec![col("id", "integer", false), col("email", "text", true)],
        ));
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Column, "email");
        assert_eq!(c.action, Action::Removed);
        assert!(c.destructive);
        assert_eq!(
            sqls(c),
            vec!["ALTER TABLE \"public\".\"users\" DROP COLUMN \"email\""]
        );
        assert_eq!(c.touches, vec![("users".to_string(), "email".to_string())]);
        assert_eq!(c.label(), "users.email");
    }

    #[test]
    fn an_enum_value_added_uses_add_value_in_place() {
        let mut source = pg();
        source.enums.push(EnumType {
            name: "mood".into(),
            values: vec!["sad".into(), "ok".into(), "happy".into(), "glad".into()],
        });
        let mut target = pg();
        target.enums.push(EnumType {
            name: "mood".into(),
            values: vec!["sad".into(), "happy".into()],
        });
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Enum, "mood");
        assert!(!c.destructive);
        assert_eq!(
            sqls(c),
            vec![
                "ALTER TYPE \"public\".\"mood\" ADD VALUE 'ok' AFTER 'sad'",
                "ALTER TYPE \"public\".\"mood\" ADD VALUE 'glad' AFTER 'happy'",
            ]
        );
        // A value removed cannot be done in place: a manual, destructive note.
        let changes = diff(&target, &source);
        let c = find(&changes, Object::Enum, "mood");
        assert!(c.destructive && c.is_manual());
    }

    #[test]
    fn not_null_added_is_risky() {
        let mut source = pg();
        source.tables.push(table(
            "users",
            vec![col("id", "integer", false), col("email", "text", false)],
        ));
        let mut target = pg();
        target.tables.push(table(
            "users",
            vec![col("id", "integer", false), col("email", "text", true)],
        ));
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Column, "email");
        assert!(c.risky && !c.destructive);
        assert_eq!(
            sqls(c),
            vec!["ALTER TABLE \"public\".\"users\" ALTER COLUMN \"email\" SET NOT NULL"]
        );
        // A new NOT NULL column without a default is risky too.
        let mut fresh = source.clone();
        fresh.tables[0].columns.push(col("age", "integer", false));
        let c = diff(&fresh, &source).into_iter().next().unwrap();
        assert!(c.risky);
    }

    #[test]
    fn a_type_change_drops_and_restores_the_default() {
        let mut s = col("n", "bigint", true);
        s.default = "0".into();
        let mut t = col("n", "integer", true);
        t.default = "0".into();
        let mut source = pg();
        source.tables.push(table("t", vec![s]));
        let mut target = pg();
        target.tables.push(table("t", vec![t]));
        let c = diff(&source, &target).into_iter().next().unwrap();
        assert!(c.risky);
        assert_eq!(
            sqls(&c),
            vec![
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"n\" DROP DEFAULT",
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"n\" TYPE bigint USING \"n\"::bigint",
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"n\" SET DEFAULT 0",
            ]
        );
        assert_eq!(c.touches, vec![("t".to_string(), "n".to_string())]);
    }

    #[test]
    fn a_changed_index_is_dropped_and_recreated() {
        let idx = |def: &str, unique: bool| Index {
            name: "users_email_idx".into(),
            definition: def.into(),
            unique,
        };
        let mut source = pg();
        let mut u = table("users", vec![col("id", "integer", false)]);
        u.indexes.push(idx(
            "CREATE UNIQUE INDEX users_email_idx ON public.users USING btree (lower(email))",
            true,
        ));
        source.tables.push(u);
        let mut target = pg();
        let mut u = table("users", vec![col("id", "integer", false)]);
        u.indexes.push(idx(
            "CREATE INDEX users_email_idx ON public.users USING btree (email)",
            false,
        ));
        target.tables.push(u);
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Index, "users_email_idx");
        assert_eq!(c.action, Action::Changed);
        assert!(c.risky, "a unique index can fail on duplicates");
        assert_eq!(c.statements[0].phase, Phase::DropIndex);
        assert_eq!(
            c.statements[0].sql,
            "DROP INDEX \"public\".\"users_email_idx\""
        );
        assert_eq!(c.statements[1].phase, Phase::CreateIndex);
        // Layout-only differences are not changes.
        let mut same = target.clone();
        same.tables[0].indexes[0].definition =
            "CREATE INDEX users_email_idx  ON public.users USING btree (email);".into();
        assert_eq!(diff(&same, &target), Vec::new());
    }

    #[test]
    fn a_changed_function_is_replaced_and_a_removed_one_dropped() {
        let f = |body: &str| Routine {
            name: "total".into(),
            arguments: "o integer".into(),
            procedure: false,
            definition: format!(
                "CREATE OR REPLACE FUNCTION public.total(o integer)\n RETURNS integer\n AS $$ {body} $$"
            ),
        };
        let mut source = pg();
        source.routines.push(f("SELECT 2"));
        let mut target = pg();
        target.routines.push(f("SELECT 1"));
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Routine, "total(o integer)");
        assert_eq!(c.action, Action::Changed);
        assert_eq!(c.statements[0].phase, Phase::CreateRoutine);
        assert!(c.statements[0].sql.contains("SELECT 2"));
        let changes = diff(&pg(), &target);
        assert_eq!(
            sqls(&changes[0]),
            vec!["DROP FUNCTION \"public\".\"total\"(o integer)"]
        );
    }

    #[test]
    fn a_trigger_added_on_an_existing_table() {
        let mut source = pg();
        source
            .tables
            .push(table("users", vec![col("id", "integer", false)]));
        let mut target = source.clone();
        source.triggers.push(Trigger {
            table: "users".into(),
            name: "audit".into(),
            definition: "CREATE TRIGGER audit AFTER INSERT ON public.users FOR EACH ROW EXECUTE FUNCTION log()".into(),
        });
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Trigger, "audit");
        assert_eq!(c.action, Action::Added);
        assert_eq!(c.statements[0].phase, Phase::CreateTrigger);
        // A removed table's triggers are not listed on their own.
        target.triggers = source.triggers.clone();
        let changes = diff(&pg(), &target);
        assert!(changes.iter().all(|c| c.object != Object::Trigger));
    }

    #[test]
    fn a_missing_schema_is_created() {
        let mut target = pg();
        target.missing = true;
        let changes = diff(&pg(), &target);
        assert_eq!(
            sqls(&changes[0]),
            vec!["CREATE SCHEMA IF NOT EXISTS \"public\""]
        );
    }

    #[test]
    fn sqlite_alter_column_is_a_manual_note() {
        let mut source = Snapshot::new(Kind::Sqlite, "main");
        source
            .tables
            .push(table("t", vec![col("a", "TEXT", false)]));
        let mut target = Snapshot::new(Kind::Sqlite, "main");
        target.tables.push(table("t", vec![col("a", "TEXT", true)]));
        let c = diff(&source, &target).into_iter().next().unwrap();
        assert!(c.is_manual() && c.risky);
        // Adding and dropping columns work in place.
        target.tables[0].columns.push(col("b", "INTEGER", true));
        let changes = diff(&source, &target);
        let c = find(&changes, Object::Column, "b");
        assert_eq!(sqls(c), vec!["ALTER TABLE \"t\" DROP COLUMN \"b\""]);
    }

    #[test]
    fn a_view_body_change_drops_and_recreates_it() {
        let v = |def: &str| View {
            name: "v".into(),
            definition: def.into(),
            ..View::default()
        };
        let mut source = pg();
        source.views.push(v(" SELECT 2;"));
        let mut target = pg();
        target.views.push(v(" SELECT 1;"));
        let c = diff(&source, &target).into_iter().next().unwrap();
        assert_eq!(
            sqls(&c),
            vec![
                "DROP VIEW \"public\".\"v\"",
                "CREATE VIEW \"public\".\"v\" AS\nSELECT 2"
            ]
        );
    }

    #[test]
    fn sequences_are_created_altered_and_dropped_last() {
        let seq = |max: &str| Sequence {
            name: "n".into(),
            max: max.into(),
            ..Sequence::default()
        };
        let mut source = pg();
        source.sequences.push(seq("10"));
        let mut target = pg();
        target.sequences.push(seq("20"));
        let c = diff(&source, &target).into_iter().next().unwrap();
        assert_eq!(
            c.statements[0].sql,
            "ALTER SEQUENCE \"public\".\"n\" MAXVALUE 10 NO CYCLE"
        );
        let c = diff(&pg(), &target).into_iter().next().unwrap();
        assert_eq!(c.statements[0].phase, Phase::DropSequence);
    }

    /// `source` and `target` holding one `users` table whose `name` column
    /// is `s` and `t`.
    fn column_pair(s: Column, t: Column) -> (Snapshot, Snapshot) {
        let mut source = pg();
        source
            .tables
            .push(table("users", vec![col("id", "integer", false), s]));
        let mut target = pg();
        target
            .tables
            .push(table("users", vec![col("id", "integer", false), t]));
        (source, target)
    }

    #[test]
    fn a_column_default_is_set_changed_and_dropped() {
        let plain = col("name", "text", true);
        let with = |d: &str| Column {
            default: d.into(),
            ..col("name", "text", true)
        };
        let (s, t) = column_pair(with("'x'"), plain.clone());
        let c = find(&diff(&s, &t), Object::Column, "name").clone();
        assert!(
            sqls(&c).iter().any(|q| q.ends_with("SET DEFAULT 'x'")),
            "{c:#?}"
        );
        let (s, t) = column_pair(plain, with("'x'"));
        let c = find(&diff(&s, &t), Object::Column, "name").clone();
        assert!(
            sqls(&c).iter().any(|q| q.ends_with("DROP DEFAULT")),
            "{c:#?}"
        );
        assert!(!c.destructive);
    }

    #[test]
    fn a_column_allowing_null_again_drops_not_null() {
        let (s, t) = column_pair(col("name", "text", true), col("name", "text", false));
        let c = find(&diff(&s, &t), Object::Column, "name").clone();
        assert!(
            sqls(&c).iter().any(|q| q.ends_with("DROP NOT NULL")),
            "{c:#?}"
        );
        assert!(!c.risky, "relaxing a constraint is safe");
    }

    #[test]
    fn a_changed_generated_column_is_dropped_and_added_back() {
        let gen_col = |expr: &str| Column {
            generated: true,
            default: expr.into(),
            comment: "computed".into(),
            ..col("name", "text", true)
        };
        let (s, t) = column_pair(gen_col("upper(x)"), gen_col("lower(x)"));
        let c = find(&diff(&s, &t), Object::Column, "name").clone();
        assert!(c.destructive, "the stored values are recomputed");
        let q = sqls(&c);
        assert!(q.iter().any(|q| q.contains("DROP COLUMN")), "{q:#?}");
        assert!(q.iter().any(|q| q.contains("ADD COLUMN")), "{q:#?}");
        assert!(
            c.touches
                .contains(&("users".to_string(), "name".to_string()))
        );
    }

    #[test]
    fn a_column_comment_change_is_a_comment_statement() {
        let commented = |text: &str| Column {
            comment: text.into(),
            ..col("name", "text", true)
        };
        let (s, t) = column_pair(commented("new note"), commented("old note"));
        let c = find(&diff(&s, &t), Object::Column, "name").clone();
        assert!(
            c.statements.iter().any(|st| st.phase == Phase::Comment),
            "{c:#?}"
        );
    }

    #[test]
    fn unique_and_check_constraints_are_added_changed_and_dropped() {
        let kc = |name: &str, def: &str| Constraint {
            name: name.into(),
            definition: def.into(),
            references: String::new(),
        };
        let mut source = pg();
        let mut st = table(
            "users",
            vec![col("id", "integer", false), col("email", "text", true)],
        );
        st.uniques.push(kc("users_email_key", "UNIQUE (email)"));
        st.checks.push(kc("users_id_check", "CHECK (id > 0)"));
        source.tables.push(st);
        let mut target = pg();
        let mut tt = table(
            "users",
            vec![col("id", "integer", false), col("email", "text", true)],
        );
        tt.checks.push(kc("users_id_check", "CHECK (id >= 0)"));
        tt.checks.push(kc("users_old_check", "CHECK (true)"));
        target.tables.push(tt);
        let changes = diff(&source, &target);
        assert_eq!(
            find(&changes, Object::Unique, "users_email_key").action,
            Action::Added
        );
        let check = find(&changes, Object::Check, "users_id_check");
        assert_eq!(check.action, Action::Changed);
        let phases: Vec<Phase> = check.statements.iter().map(|s| s.phase).collect();
        assert_eq!(
            phases,
            [Phase::DropConstraint, Phase::AddConstraint],
            "{check:#?}"
        );
        assert_eq!(
            find(&changes, Object::Check, "users_old_check").action,
            Action::Removed
        );
    }

    #[test]
    fn every_object_and_action_has_its_own_label_and_marker() {
        let objects = [
            Object::Schema,
            Object::Table,
            Object::Column,
            Object::PrimaryKey,
            Object::Unique,
            Object::Check,
            Object::Index,
            Object::ForeignKey,
            Object::View,
            Object::Routine,
            Object::Trigger,
            Object::Enum,
            Object::Sequence,
        ];
        let keys: std::collections::HashSet<&str> = objects.iter().map(|o| o.label_key()).collect();
        assert_eq!(keys.len(), objects.len(), "no two kinds share a label");
        assert!(keys.iter().all(|k| k.starts_with("ui.db_compare_obj_")));
        let markers: Vec<char> = [Action::Added, Action::Removed, Action::Changed]
            .iter()
            .map(|a| a.marker())
            .collect();
        assert_eq!(markers, ['+', '-', '~']);
    }

    fn lite() -> Snapshot {
        Snapshot::new(Kind::Sqlite, "main")
    }

    fn one_table(mut snap: Snapshot, t: Table) -> Snapshot {
        snap.tables.push(t);
        snap
    }

    fn enum_of(name: &str, values: &[&str]) -> EnumType {
        EnumType {
            name: name.into(),
            values: values.iter().map(|v| (*v).to_string()).collect(),
        }
    }

    #[test]
    fn labels_markers_and_notes() {
        let mut c = Change::new(Object::Column, Action::Added, "users", "email");
        assert_eq!(c.label(), "users.email");
        c.table.clear();
        assert_eq!(c.label(), "email");
        assert!(!c.is_manual(), "no statements is not manual");
        c.push(Phase::AlterColumn, "-- by hand");
        assert!(c.is_manual());
        c.push(Phase::AlterColumn, "ALTER TABLE x");
        assert!(!c.is_manual());
        assert_eq!(Action::Added.marker(), '+');
        assert_eq!(Action::Removed.marker(), '-');
        assert_eq!(Action::Changed.marker(), '~');
        for (object, suffix) in [
            (Object::Schema, "schema"),
            (Object::Table, "table"),
            (Object::Column, "column"),
            (Object::PrimaryKey, "primary_key"),
            (Object::Unique, "unique"),
            (Object::Check, "check"),
            (Object::Index, "index"),
            (Object::ForeignKey, "foreign_key"),
            (Object::View, "view"),
            (Object::Routine, "routine"),
            (Object::Trigger, "trigger"),
            (Object::Enum, "enum"),
            (Object::Sequence, "sequence"),
        ] {
            assert_eq!(object.label_key(), format!("ui.db_compare_obj_{suffix}"));
        }
    }

    #[test]
    fn enum_types_are_created_dropped_and_extended_in_place() {
        let mut source = pg();
        let mut target = pg();
        source.enums = vec![
            enum_of("mood", &["sad", "ok", "happy"]),
            enum_of("fresh", &["a"]),
        ];
        target.enums = vec![enum_of("mood", &["ok", "happy"]), enum_of("gone", &["x"])];
        let changes = diff(&source, &target);
        let fresh = find(&changes, Object::Enum, "fresh");
        assert_eq!(fresh.action, Action::Added);
        assert_eq!(
            sqls(fresh),
            ["CREATE TYPE \"public\".\"fresh\" AS ENUM ('a')"]
        );
        let gone = find(&changes, Object::Enum, "gone");
        assert_eq!(sqls(gone), ["DROP TYPE \"public\".\"gone\""]);
        let mood = find(&changes, Object::Enum, "mood");
        assert_eq!(mood.action, Action::Changed);
        assert_eq!(
            sqls(mood),
            ["ALTER TYPE \"public\".\"mood\" ADD VALUE 'sad' BEFORE 'ok'"]
        );
        // A value in the middle goes AFTER its predecessor.
        source.enums = vec![enum_of("mood", &["ok", "meh", "happy"])];
        target.enums = vec![enum_of("mood", &["ok", "happy"])];
        let changes = diff(&source, &target);
        assert_eq!(
            sqls(find(&changes, Object::Enum, "mood")),
            ["ALTER TYPE \"public\".\"mood\" ADD VALUE 'meh' AFTER 'ok'"]
        );
        // Removing or reordering values cannot be done in place.
        source.enums = vec![enum_of("mood", &["happy", "ok"])];
        let changes = diff(&source, &target);
        let mood = find(&changes, Object::Enum, "mood");
        assert!(mood.destructive && mood.is_manual());
        assert!(mood.statements[0].sql.contains("recreate the type by hand"));
    }

    fn seq(name: &str, start: &str, cycle: bool) -> Sequence {
        Sequence {
            name: name.into(),
            data_type: "bigint".into(),
            start: start.into(),
            increment: "1".into(),
            cycle,
            ..Sequence::default()
        }
    }

    #[test]
    fn sequences_are_created_dropped_and_altered() {
        let mut source = pg();
        let mut target = pg();
        source.sequences = vec![seq("new_seq", "1", false), seq("both", "10", true)];
        target.sequences = vec![seq("old_seq", "1", false), seq("both", "1", true)];
        let changes = diff(&source, &target);
        assert_eq!(
            sqls(find(&changes, Object::Sequence, "new_seq")),
            [
                "CREATE SEQUENCE \"public\".\"new_seq\" AS bigint INCREMENT BY 1 START WITH 1 NO CYCLE"
            ]
        );
        assert_eq!(
            sqls(find(&changes, Object::Sequence, "old_seq")),
            ["DROP SEQUENCE IF EXISTS \"public\".\"old_seq\""]
        );
        let both = find(&changes, Object::Sequence, "both");
        assert_eq!(both.action, Action::Changed);
        assert!(sqls(both)[0].starts_with("ALTER SEQUENCE \"public\".\"both\""));
        assert!(sqls(both)[0].contains("START WITH 10 CYCLE"));
    }

    #[test]
    fn removed_tables_are_destructive_and_summarized() {
        let target = one_table(pg(), table("old", vec![col("id", "integer", false)]));
        let changes = diff(&pg(), &target);
        let c = find(&changes, Object::Table, "old");
        assert!(c.destructive);
        assert_eq!(c.touches, [("old".to_string(), String::new())]);
        assert_eq!(c.target, "(\"id\" integer NOT NULL)");
        assert_eq!(sqls(c), ["DROP TABLE \"public\".\"old\""]);
        // SQLite keeps its own CREATE text for the summary.
        let mut t = table("old", vec![col("id", "integer", false)]);
        t.create_sql = "CREATE TABLE old (id INTEGER)".into();
        let changes = diff(&lite(), &one_table(lite(), t));
        assert_eq!(
            find(&changes, Object::Table, "old").target,
            "CREATE TABLE old (id INTEGER)"
        );
    }

    #[test]
    fn a_new_table_carries_constraints_comments_and_indexes() {
        let mut t = table(
            "items",
            vec![
                Column {
                    name: "id".into(),
                    data_type: "integer".into(),
                    identity: "always".into(),
                    ..Column::default()
                },
                Column {
                    name: "slug".into(),
                    data_type: "text".into(),
                    default: "'x'".into(),
                    comment: "the slug".into(),
                    ..Column::default()
                },
                Column {
                    name: "total".into(),
                    data_type: "integer".into(),
                    generated: true,
                    default: "1 + 1".into(),
                    nullable: true,
                    identity: "default".into(),
                    ..Column::default()
                },
            ],
        );
        t.uniques.push(Constraint {
            name: "items_slug_key".into(),
            definition: "UNIQUE (slug)".into(),
            references: String::new(),
        });
        t.checks.push(Constraint {
            name: "items_total_check".into(),
            definition: "CHECK (total > 0)".into(),
            references: String::new(),
        });
        t.comment = "all the items".into();
        t.indexes.push(Index {
            name: "items_slug_idx".into(),
            definition: "CREATE INDEX items_slug_idx ON public.items (slug);".into(),
            unique: false,
        });
        let changes = diff(&one_table(pg(), t), &pg());
        let c = find(&changes, Object::Table, "items");
        let statements = sqls(c);
        assert!(statements[0].contains("\"id\" integer GENERATED ALWAYS AS IDENTITY NOT NULL"));
        assert!(statements[0].contains("\"slug\" text DEFAULT 'x' NOT NULL"));
        assert!(
            statements[0]
                .contains("GENERATED ALWAYS AS (1 + 1) STORED GENERATED BY DEFAULT AS IDENTITY")
        );
        assert!(
            statements
                .iter()
                .any(|s| s.contains("ADD CONSTRAINT \"items_slug_key\" UNIQUE (slug)"))
        );
        assert!(
            statements
                .iter()
                .any(|s| s.contains("ADD CONSTRAINT \"items_total_check\" CHECK"))
        );
        assert!(statements.contains(&"COMMENT ON TABLE \"public\".\"items\" IS 'all the items'"));
        assert!(
            statements.contains(&"COMMENT ON COLUMN \"public\".\"items\".\"slug\" IS 'the slug'")
        );
        assert!(statements.contains(&"CREATE INDEX items_slug_idx ON public.items (slug)"));
    }

    #[test]
    fn a_new_sqlite_table_uses_its_own_create_statement() {
        let mut t = table("t", vec![col("id", "integer", false)]);
        t.create_sql = "CREATE TABLE t (id INTEGER PRIMARY KEY);".into();
        t.indexes.push(Index {
            name: "t_idx".into(),
            definition: "CREATE INDEX t_idx ON t (id)".into(),
            unique: false,
        });
        let changes = diff(&one_table(lite(), t), &lite());
        let c = find(&changes, Object::Table, "t");
        assert_eq!(
            sqls(c),
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY)",
                "CREATE INDEX t_idx ON t (id)"
            ]
        );
    }

    #[test]
    fn a_column_added_not_null_without_a_default_is_risky_and_commented() {
        let mut source = pg();
        source.tables.push(table(
            "users",
            vec![
                col("id", "integer", false),
                Column {
                    comment: "who".into(),
                    ..col("name", "text", false)
                },
                Column {
                    default: "'n/a'".into(),
                    ..col("nick", "text", false)
                },
            ],
        ));
        let target = one_table(pg(), table("users", vec![col("id", "integer", false)]));
        let changes = diff(&source, &target);
        let name = find(&changes, Object::Column, "name");
        assert!(name.risky);
        assert_eq!(
            sqls(name),
            [
                "ALTER TABLE \"public\".\"users\" ADD COLUMN \"name\" text NOT NULL",
                "COMMENT ON COLUMN \"public\".\"users\".\"name\" IS 'who'"
            ]
        );
        assert!(
            !find(&changes, Object::Column, "nick").risky,
            "it has a default"
        );
    }

    fn altered(source: Column, target: Column) -> Change {
        let src = one_table(pg(), table("t", vec![source]));
        let tgt = one_table(pg(), table("t", vec![target]));
        diff(&src, &tgt).into_iter().next().expect("one change")
    }

    #[test]
    fn retyping_a_column_drops_and_restores_its_default() {
        let c = altered(
            Column {
                default: "'1'".into(),
                ..col("v", "bigint", true)
            },
            Column {
                default: "0".into(),
                ..col("v", "integer", true)
            },
        );
        assert!(c.risky);
        assert_eq!(
            sqls(&c),
            [
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" DROP DEFAULT",
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" TYPE bigint USING \"v\"::bigint",
                "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" SET DEFAULT '1'",
            ]
        );
        assert_eq!(c.touches, [("t".to_string(), "v".to_string())]);
        // Retyping to a column with no default leaves it dropped.
        let c = altered(
            col("v", "bigint", true),
            Column {
                default: "0".into(),
                ..col("v", "integer", true)
            },
        );
        assert_eq!(sqls(&c).len(), 2);
        // Changing only the default, in both directions.
        let c = altered(
            col("v", "integer", true),
            Column {
                default: "0".into(),
                ..col("v", "integer", true)
            },
        );
        assert_eq!(
            sqls(&c),
            ["ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" DROP DEFAULT"]
        );
        let c = altered(
            Column {
                default: "5".into(),
                ..col("v", "integer", true)
            },
            col("v", "integer", true),
        );
        assert_eq!(
            sqls(&c),
            ["ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" SET DEFAULT 5"]
        );
    }

    #[test]
    fn nullability_identity_and_comment_changes() {
        let c = altered(col("v", "integer", true), col("v", "integer", false));
        assert_eq!(
            sqls(&c),
            ["ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" DROP NOT NULL"]
        );
        assert!(!c.risky);
        let c = altered(col("v", "integer", false), col("v", "integer", true));
        assert_eq!(
            sqls(&c),
            ["ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" SET NOT NULL"]
        );
        assert!(c.risky);
        let with = |identity: &str| Column {
            identity: identity.into(),
            ..col("v", "integer", true)
        };
        assert_eq!(
            sqls(&altered(with(""), with("always")))[0],
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" DROP IDENTITY IF EXISTS"
        );
        assert_eq!(
            sqls(&altered(with("always"), with("")))[0],
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" ADD GENERATED ALWAYS AS IDENTITY"
        );
        assert_eq!(
            sqls(&altered(with("default"), with("always")))[0],
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"v\" SET GENERATED BY DEFAULT"
        );
        let c = altered(
            Column {
                comment: "now".into(),
                ..col("v", "integer", true)
            },
            col("v", "integer", true),
        );
        assert_eq!(
            sqls(&c),
            ["COMMENT ON COLUMN \"public\".\"t\".\"v\" IS 'now'"]
        );
    }

    #[test]
    fn changing_a_generation_expression_drops_and_re_adds_the_column() {
        let generated = |expr: &str, comment: &str| Column {
            generated: true,
            default: expr.into(),
            comment: comment.into(),
            ..col("g", "integer", true)
        };
        let c = altered(generated("a * 3", "why"), generated("a * 2", ""));
        assert!(c.destructive);
        assert_eq!(c.touches, [("t".to_string(), "g".to_string())]);
        let statements = sqls(&c);
        assert!(statements[0].ends_with("DROP COLUMN \"g\""));
        assert!(
            statements[1].contains("ADD COLUMN \"g\" integer GENERATED ALWAYS AS (a * 3) STORED")
        );
        assert!(statements[2].contains("IS 'why'"));
    }

    #[test]
    fn table_comments_and_constraint_changes_on_postgres() {
        let mut s = table("t", vec![col("id", "integer", false)]);
        let mut t = s.clone();
        s.comment = "new".into();
        t.comment = "old".into();
        let key = |name: &str, def: &str| Constraint {
            name: name.into(),
            definition: def.into(),
            references: String::new(),
        };
        s.uniques = vec![key("u_new", "UNIQUE (a)"), key("u_both", "UNIQUE (b, c)")];
        t.uniques = vec![key("u_old", "UNIQUE (z)"), key("u_both", "UNIQUE (b)")];
        s.checks = vec![key("c1", "CHECK (a > 0)")];
        s.primary_key = Some(key("t_pkey", "PRIMARY KEY (id, x)"));
        let changes = diff(&one_table(pg(), s), &one_table(pg(), t));
        let comment = changes
            .iter()
            .find(|c| c.object == Object::Table)
            .expect("comment change");
        assert_eq!(
            sqls(comment),
            ["COMMENT ON TABLE \"public\".\"t\" IS 'new'"]
        );
        let added = find(&changes, Object::Unique, "u_new");
        assert!(added.risky);
        assert_eq!(added.statements[0].phase, Phase::AddConstraint);
        let removed = find(&changes, Object::Unique, "u_old");
        assert!(!removed.risky);
        assert_eq!(removed.statements[0].phase, Phase::DropConstraint);
        let altered_key = find(&changes, Object::Unique, "u_both");
        assert_eq!(altered_key.action, Action::Changed);
        assert!(
            sqls(altered_key)[0].contains("DROP CONSTRAINT")
                && sqls(altered_key)[1].contains("ADD CONSTRAINT")
        );
        assert!(
            changes
                .iter()
                .any(|c| c.object == Object::Check && c.name == "c1")
        );
        let pk = find(&changes, Object::PrimaryKey, "t_pkey");
        assert_eq!(pk.action, Action::Changed);
        assert!(pk.risky);
        // Clearing a comment writes NULL.
        let mut s = table("t", vec![col("id", "integer", false)]);
        let mut t = s.clone();
        t.comment = "old".into();
        s.comment.clear();
        let changes = diff(&one_table(pg(), s), &one_table(pg(), t));
        assert_eq!(
            sqls(&changes[0]),
            ["COMMENT ON TABLE \"public\".\"t\" IS NULL"]
        );
    }

    #[test]
    fn foreign_keys_and_indexes_change_by_name() {
        let fk = |name: &str, def: &str| Constraint {
            name: name.into(),
            definition: def.into(),
            references: "p".into(),
        };
        let ix = |name: &str, def: &str, unique: bool| Index {
            name: name.into(),
            definition: def.into(),
            unique,
        };
        let mut s = table("t", vec![col("id", "integer", false)]);
        let mut t = s.clone();
        s.foreign_keys = vec![fk("fk_new", "FOREIGN KEY (a) REFERENCES p(id)")];
        t.foreign_keys = vec![fk("fk_old", "FOREIGN KEY (b) REFERENCES p(id)")];
        s.indexes = vec![
            ix("ix_new", "CREATE UNIQUE INDEX ix_new ON t (a)", true),
            ix("ix_both", "CREATE INDEX ix_both ON t (a, b)", false),
        ];
        t.indexes = vec![
            ix("ix_old", "CREATE INDEX ix_old ON t (z)", false),
            ix("ix_both", "CREATE INDEX ix_both ON t (a)", false),
        ];
        let changes = diff(&one_table(pg(), s), &one_table(pg(), t));
        let fk_new = find(&changes, Object::ForeignKey, "fk_new");
        assert_eq!(fk_new.statements[0].phase, Phase::AddForeignKey);
        let fk_old = find(&changes, Object::ForeignKey, "fk_old");
        assert_eq!(fk_old.statements[0].phase, Phase::DropForeignKey);
        let ix_new = find(&changes, Object::Index, "ix_new");
        assert!(ix_new.risky, "a unique index can fail on existing rows");
        let ix_old = find(&changes, Object::Index, "ix_old");
        assert_eq!(sqls(ix_old), ["DROP INDEX \"public\".\"ix_old\""]);
        let ix_both = find(&changes, Object::Index, "ix_both");
        assert_eq!(ix_both.statements.len(), 2);
        assert!(!ix_both.risky);
    }

    #[test]
    fn sqlite_changes_that_need_a_table_rebuild_become_notes() {
        let mut s = table(
            "t",
            vec![
                col("id", "integer", false),
                Column {
                    generated: true,
                    default: "id * 2".into(),
                    ..col("g", "integer", true)
                },
                col("changed", "text", true),
                Column {
                    comment: "only a comment".into(),
                    ..col("note", "text", true)
                },
            ],
        );
        let mut t = table(
            "t",
            vec![
                col("id", "integer", false),
                col("changed", "integer", true),
                col("note", "text", true),
            ],
        );
        s.foreign_keys.push(Constraint {
            name: "fk".into(),
            definition: "FOREIGN KEY (id) REFERENCES p(id)".into(),
            references: "p".into(),
        });
        t.primary_key = None;
        s.comment = "ignored on sqlite".into();
        let changes = diff(&one_table(lite(), s), &one_table(lite(), t));
        let generated = find(&changes, Object::Column, "g");
        assert!(generated.is_manual());
        assert!(
            generated.statements[0]
                .sql
                .contains("rebuild the table by hand")
        );
        let retyped = find(&changes, Object::Column, "changed");
        assert!(retyped.risky && retyped.is_manual());
        assert!(
            !changes.iter().any(|c| c.name == "note"),
            "a comment-only change is invisible on SQLite"
        );
        assert!(!changes.iter().any(|c| c.object == Object::Table));
        let fk = find(&changes, Object::ForeignKey, "fk");
        assert!(fk.is_manual());
        assert!(fk.statements[0].sql.contains("a foreign key"));
        let pk = find(&changes, Object::PrimaryKey, "t_pkey");
        assert!(pk.statements[0].sql.contains("the primary key"));
    }

    fn view(name: &str, body: &str) -> View {
        View {
            name: name.into(),
            definition: body.into(),
            ..View::default()
        }
    }

    #[test]
    fn views_are_created_dropped_and_rebuilt() {
        let mut source = pg();
        let mut target = pg();
        source.views = vec![
            View {
                comment: "c".into(),
                ..view("v_new", "SELECT 1;")
            },
            View {
                materialized: true,
                ..view("v_mat", "SELECT 2")
            },
            view("v_body", "SELECT 3"),
            View {
                comment: "fresh".into(),
                ..view("v_comment", "SELECT 4")
            },
            view("v_same", "SELECT  5 ;"),
        ];
        target.views = vec![
            view("v_old", "SELECT 0"),
            view("v_mat", "SELECT 2"),
            View {
                comment: "x".into(),
                ..view("v_body", "SELECT 33")
            },
            view("v_comment", "SELECT 4"),
            view("v_same", "SELECT 5"),
        ];
        let changes = diff(&source, &target);
        let v_new = find(&changes, Object::View, "v_new");
        assert_eq!(
            sqls(v_new),
            [
                "CREATE VIEW \"public\".\"v_new\" AS\nSELECT 1",
                "COMMENT ON VIEW \"public\".\"v_new\" IS 'c'"
            ]
        );
        assert_eq!(
            sqls(find(&changes, Object::View, "v_old")),
            ["DROP VIEW \"public\".\"v_old\""]
        );
        let mat = find(&changes, Object::View, "v_mat");
        assert_eq!(
            sqls(mat),
            [
                "DROP VIEW \"public\".\"v_mat\"",
                "CREATE MATERIALIZED VIEW \"public\".\"v_mat\" AS\nSELECT 2"
            ]
        );
        let body = find(&changes, Object::View, "v_body");
        assert_eq!(
            body.statements.len(),
            3,
            "drop, create, and clear the old comment"
        );
        assert_eq!(
            body.statements[2].sql,
            "COMMENT ON VIEW \"public\".\"v_body\" IS NULL"
        );
        let only_comment = find(&changes, Object::View, "v_comment");
        assert_eq!(
            sqls(only_comment),
            ["COMMENT ON VIEW \"public\".\"v_comment\" IS 'fresh'"]
        );
        assert!(!changes.iter().any(|c| c.name == "v_same"));
        // A rebuilt materialized view and a dropped one name their kind.
        target.views = vec![View {
            materialized: true,
            ..view("m", "SELECT 1")
        }];
        source.views.clear();
        let dropped = diff(&source, &target);
        assert_eq!(
            sqls(find(&dropped, Object::View, "m")),
            ["DROP MATERIALIZED VIEW \"public\".\"m\""]
        );
        assert_eq!(
            view_comment(
                &view_sql(&pg()),
                &View {
                    materialized: true,
                    ..view("m", "x")
                }
            ),
            "COMMENT ON MATERIALIZED VIEW \"public\".\"m\" IS NULL"
        );
    }

    #[test]
    fn sqlite_views_use_their_own_statement() {
        let source = {
            let mut s = lite();
            s.views = vec![view("v", "CREATE VIEW v AS SELECT 1;")];
            s
        };
        let changes = diff(&source, &lite());
        assert_eq!(
            sqls(find(&changes, Object::View, "v")),
            ["CREATE VIEW v AS SELECT 1"]
        );
    }

    fn routine(name: &str, args: &str, procedure: bool, body: &str) -> Routine {
        Routine {
            name: name.into(),
            arguments: args.into(),
            procedure,
            definition: body.into(),
        }
    }

    #[test]
    fn routines_are_matched_by_signature() {
        let mut source = pg();
        let mut target = pg();
        source.routines = vec![
            routine(
                "f",
                "integer",
                false,
                "CREATE OR REPLACE FUNCTION f(integer) AS $$ 1 $$;",
            ),
            routine("g", "", false, "CREATE OR REPLACE FUNCTION g() AS $$ 2 $$"),
            routine("same", "", false, "CREATE FUNCTION same()"),
        ];
        target.routines = vec![
            routine("g", "", false, "CREATE OR REPLACE FUNCTION g() AS $$ 1 $$"),
            routine("p", "text", true, "CREATE PROCEDURE p(text)"),
            routine("same", "", false, "CREATE  FUNCTION same();"),
        ];
        let changes = diff(&source, &target);
        let f = find(&changes, Object::Routine, "f(integer)");
        assert_eq!(
            sqls(f),
            ["CREATE OR REPLACE FUNCTION f(integer) AS $$ 1 $$"]
        );
        let g = find(&changes, Object::Routine, "g()");
        assert_eq!(g.action, Action::Changed);
        let p = find(&changes, Object::Routine, "p(text)");
        assert_eq!(sqls(p), ["DROP PROCEDURE \"public\".\"p\"(text)"]);
        assert!(!changes.iter().any(|c| c.name == "same()"));
        target.routines = vec![routine("fn", "", false, "x")];
        source.routines.clear();
        let changes = diff(&source, &target);
        assert_eq!(
            sqls(find(&changes, Object::Routine, "fn()")),
            ["DROP FUNCTION \"public\".\"fn\"()"]
        );
    }

    fn trigger(table: &str, name: &str, body: &str) -> Trigger {
        Trigger {
            table: table.into(),
            name: name.into(),
            definition: body.into(),
        }
    }

    #[test]
    fn triggers_follow_their_tables() {
        let mut source = pg();
        let mut target = pg();
        let users = table("users", vec![col("id", "integer", false)]);
        source.tables.push(users.clone());
        target.tables.push(users);
        target
            .tables
            .push(table("gone", vec![col("id", "integer", false)]));
        source.triggers = vec![
            trigger(
                "users",
                "t_new",
                "CREATE TRIGGER t_new AFTER INSERT ON users;",
            ),
            trigger(
                "users",
                "t_chg",
                "CREATE TRIGGER t_chg AFTER UPDATE ON users",
            ),
        ];
        target.triggers = vec![
            trigger(
                "users",
                "t_old",
                "CREATE TRIGGER t_old AFTER DELETE ON users",
            ),
            trigger(
                "users",
                "t_chg",
                "CREATE TRIGGER t_chg AFTER INSERT ON users",
            ),
            trigger(
                "gone",
                "t_gone",
                "CREATE TRIGGER t_gone AFTER INSERT ON gone",
            ),
        ];
        let changes = diff(&source, &target);
        let new = find(&changes, Object::Trigger, "t_new");
        assert_eq!(sqls(new), ["CREATE TRIGGER t_new AFTER INSERT ON users"]);
        assert_eq!(new.label(), "users.t_new");
        let old = find(&changes, Object::Trigger, "t_old");
        assert_eq!(
            sqls(old),
            ["DROP TRIGGER \"t_old\" ON \"public\".\"users\""]
        );
        let chg = find(&changes, Object::Trigger, "t_chg");
        assert_eq!(chg.statements.len(), 2);
        assert!(
            !changes.iter().any(|c| c.name == "t_gone"),
            "a dropped table takes its triggers with it"
        );
        // SQLite names the trigger alone.
        let mut lsrc = lite();
        let mut ltgt = lite();
        lsrc.tables
            .push(table("users", vec![col("id", "integer", false)]));
        ltgt.tables = lsrc.tables.clone();
        ltgt.triggers = vec![trigger("users", "t", "CREATE TRIGGER t ...")];
        let changes = diff(&lsrc, &ltgt);
        assert_eq!(
            sqls(find(&changes, Object::Trigger, "t")),
            ["DROP TRIGGER \"t\""]
        );
    }

    #[test]
    fn a_missing_postgres_schema_is_created_first() {
        let mut source = pg();
        source
            .tables
            .push(table("a", vec![col("id", "integer", false)]));
        let mut target = pg();
        target.missing = true;
        let changes = diff(&source, &target);
        assert_eq!(changes[0].object, Object::Schema);
        assert!(sqls(&changes[0])[0].starts_with("CREATE SCHEMA"));
    }
}
