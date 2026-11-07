//! Ordering the migration (T785): the statements of the chosen
//! [`Change`]s, put in an order that runs.
//!
//! Statements run phase by phase in [`Phase`] order — types and sequences
//! first; then drops (triggers, views, foreign keys, constraints, indexes,
//! tables, columns, routines); then creates (tables, columns, constraints,
//! indexes, foreign keys, routines, views, triggers); then comments; and
//! dropped sequences and types last. Inside a phase the diff's order is kept,
//! except that views are ordered by their dependencies: dependents are
//! dropped before what they use, and created after it.
//!
//! A view in the target that uses something the migration drops or retypes
//! (a column, a table, another view) would block that change, so it is
//! dropped before and recreated after, from the source's definition (the
//! target's when the source has no such view), even when the view itself
//! did not change. Statements are de-duplicated.

use std::collections::{HashMap, HashSet};

use super::diff::{Change, Phase, Statement};
use super::snapshot::Snapshot;

/// The ordered migration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Plan {
    /// Every statement in run order, notes (manual changes) included.
    pub statements: Vec<Statement>,
}

impl Plan {
    /// The statements that run, in order (notes left out).
    #[must_use]
    pub fn runnable(&self) -> Vec<&str> {
        self.statements
            .iter()
            .filter(|s| !s.is_note())
            .map(|s| s.sql.as_str())
            .collect()
    }

    /// The migration as one script: `header` lines as SQL comments, then the
    /// statements inside `BEGIN;` … `COMMIT;`, notes kept as comments.
    #[must_use]
    pub fn script(&self, header: &[String]) -> String {
        let mut out = String::new();
        for line in header {
            out.push_str("-- ");
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("BEGIN;\n");
        for s in &self.statements {
            out.push('\n');
            out.push_str(s.sql.trim_end());
            if !s.is_note() {
                out.push(';');
            }
            out.push('\n');
        }
        out.push_str("\nCOMMIT;\n");
        out
    }
}

/// Whether a dependency on `(rel, col)` is hit by a change touching
/// `(trel, tcol)`: same relation, and either side means the whole of it or
/// the columns match.
fn hits(dep: &(String, String), touch: &(String, String)) -> bool {
    dep.0 == touch.0 && (touch.1.is_empty() || dep.1.is_empty() || dep.1 == touch.1)
}

/// Order the statements of `changes` (the chosen ones) so the migration
/// runs on `target`, rebuilding the target views that block a change.
#[must_use]
pub fn plan(changes: &[&Change], source: &Snapshot, target: &Snapshot) -> Plan {
    let mut steps: Vec<Statement> = changes
        .iter()
        .flat_map(|c| c.statements.iter().cloned())
        .collect();
    rebuild_dependent_views(changes, source, target, &mut steps);
    // A stable sort keeps the diff's order inside each phase.
    steps.sort_by_key(|s| s.phase);
    let steps = order_views(steps, source, target);
    let mut seen = HashSet::new();
    let statements = steps
        .into_iter()
        .filter(|s| seen.insert(s.sql.clone()))
        .collect();
    Plan { statements }
}

/// Add a drop and a recreate for every target view that uses something the
/// chosen changes drop or retype, directly or through another view.
fn rebuild_dependent_views(
    changes: &[&Change],
    source: &Snapshot,
    target: &Snapshot,
    steps: &mut Vec<Statement>,
) {
    let mut touched: Vec<(String, String)> = changes
        .iter()
        .flat_map(|c| c.touches.iter().cloned())
        .collect();
    let in_phase = |phase: Phase| -> HashSet<String> {
        steps
            .iter()
            .filter(|s| s.phase == phase)
            .filter_map(|s| s.view.clone())
            .collect()
    };
    let dropped = in_phase(Phase::DropView);
    let created = in_phase(Phase::CreateView);
    let mut rebuild: Vec<&str> = Vec::new();
    loop {
        let before = rebuild.len();
        for v in &target.views {
            if rebuild.contains(&v.name.as_str()) {
                continue;
            }
            if v.depends_on
                .iter()
                .any(|d| touched.iter().any(|t| hits(d, t)))
            {
                rebuild.push(&v.name);
                touched.push((v.name.clone(), String::new()));
            }
        }
        if rebuild.len() == before {
            break;
        }
    }
    let sql = super::diff::view_sql;
    for name in rebuild {
        if !dropped.contains(name)
            && let Some(v) = target.view(name)
        {
            steps.push(Statement::on_view(
                Phase::DropView,
                sql(target).drop_view(v),
                name,
            ));
        }
        if created.contains(name) {
            continue;
        }
        let def = source
            .view(name)
            .map(|v| (v, source))
            .or_else(|| target.view(name).map(|v| (v, target)));
        if let Some((v, snap)) = def {
            steps.push(Statement::on_view(
                Phase::CreateView,
                sql(snap).create_view(v),
                name,
            ));
        }
    }
}

/// Reorder the view statements of the drop-view and create-view phases by
/// dependency, leaving every other statement where the sort put it.
fn order_views(steps: Vec<Statement>, source: &Snapshot, target: &Snapshot) -> Vec<Statement> {
    // name -> the views it uses, from both sides.
    let mut uses: HashMap<String, HashSet<String>> = HashMap::new();
    for snap in [source, target] {
        for v in &snap.views {
            let deps = uses.entry(v.name.clone()).or_default();
            for (rel, _) in &v.depends_on {
                if snap.view(rel).is_some() && *rel != v.name {
                    deps.insert(rel.clone());
                }
            }
        }
    }
    let mut out = Vec::with_capacity(steps.len());
    let mut pending: Vec<Statement> = Vec::new();
    let mut current: Option<Phase> = None;
    for s in steps {
        let view_phase = matches!(s.phase, Phase::DropView | Phase::CreateView);
        if current != Some(s.phase) {
            flush(&mut pending, current, &uses, &mut out);
            current = Some(s.phase);
        }
        if view_phase {
            pending.push(s);
        } else {
            out.push(s);
        }
    }
    flush(&mut pending, current, &uses, &mut out);
    out
}

/// Emit the pending view statements of one phase in dependency order: a
/// stable topological sort (first ready statement wins), creates after
/// what they use, drops before it. A cycle is broken in diff order.
fn flush(
    pending: &mut Vec<Statement>,
    phase: Option<Phase>,
    uses: &HashMap<String, HashSet<String>>,
    out: &mut Vec<Statement>,
) {
    let create = phase == Some(Phase::CreateView);
    let name = |s: &Statement| s.view.clone().unwrap_or_default();
    while !pending.is_empty() {
        let names: Vec<String> = pending.iter().map(name).collect();
        let ready = pending.iter().position(|s| {
            let me = name(s);
            if create {
                // Everything it uses that is still waiting must come first.
                uses.get(&me)
                    .is_none_or(|deps| !deps.iter().any(|d| *d != me && names.contains(d)))
            } else {
                // Nothing still waiting may use it.
                !names.iter().any(|other| {
                    *other != me && uses.get(other).is_some_and(|deps| deps.contains(&me))
                })
            }
        });
        out.push(pending.remove(ready.unwrap_or(0)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::diff::tests::{col, pg, table};
    use super::super::diff::{Object, diff};
    use super::super::snapshot::{Constraint, EnumType, Trigger, View};
    use super::*;

    fn all(changes: &[Change]) -> Vec<&Change> {
        changes.iter().collect()
    }

    fn view(name: &str, def: &str, deps: &[(&str, &str)]) -> View {
        View {
            name: name.into(),
            definition: def.into(),
            depends_on: deps
                .iter()
                .map(|(r, c)| ((*r).to_string(), (*c).to_string()))
                .collect(),
            ..View::default()
        }
    }

    fn pos(plan: &Plan, needle: &str) -> usize {
        plan.statements
            .iter()
            .position(|s| s.sql.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not in {:#?}", plan.statements))
    }

    #[test]
    fn a_dropped_column_used_by_a_view_rebuilds_the_view_around_it() {
        let mut source = pg();
        source
            .tables
            .push(table("users", vec![col("id", "integer", false)]));
        source
            .views
            .push(view("ids", " SELECT id FROM users;", &[("users", "id")]));
        // A view on that view must be rebuilt too, in dependency order.
        source
            .views
            .push(view("ids2", " SELECT id FROM ids;", &[("ids", "id")]));
        let mut target = pg();
        target.tables.push(table(
            "users",
            vec![col("id", "integer", false), col("email", "text", true)],
        ));
        target.views.push(view(
            "ids",
            " SELECT id FROM users;",
            &[("users", "id"), ("users", "email")],
        ));
        target
            .views
            .push(view("ids2", " SELECT id FROM ids;", &[("ids", "id")]));
        let changes = diff(&source, &target);
        // The views did not change by themselves...
        assert!(changes.iter().all(|c| c.object != Object::View));
        let p = plan(&all(&changes), &source, &target);
        let drop2 = pos(&p, "DROP VIEW \"public\".\"ids2\"");
        let drop1 = pos(&p, "DROP VIEW \"public\".\"ids\"");
        let column = pos(&p, "DROP COLUMN \"email\"");
        let create1 = pos(&p, "CREATE VIEW \"public\".\"ids\"");
        let create2 = pos(&p, "CREATE VIEW \"public\".\"ids2\"");
        assert!(drop2 < drop1 && drop1 < column && column < create1 && create1 < create2);
        // ...and nothing is rebuilt when the column change is not chosen.
        let none = plan(&[], &source, &target);
        assert_eq!(none.statements.len(), 0);
    }

    #[test]
    fn phases_run_in_order_and_keep_the_diff_order_inside_a_phase() {
        let mut source = pg();
        source.enums.push(EnumType {
            name: "mood".into(),
            values: vec!["ok".into()],
        });
        source
            .tables
            .push(table("a", vec![col("id", "integer", false)]));
        let mut b = table(
            "b",
            vec![col("id", "integer", false), col("a_id", "integer", true)],
        );
        b.foreign_keys.push(Constraint {
            name: "b_a_fk".into(),
            definition: "FOREIGN KEY (a_id) REFERENCES a(id)".into(),
            references: "a".into(),
        });
        source.tables.push(b);
        source.triggers.push(Trigger {
            table: "a".into(),
            name: "t".into(),
            definition: "CREATE TRIGGER t AFTER INSERT ON public.a EXECUTE FUNCTION f()".into(),
        });
        let mut target = pg();
        target
            .tables
            .push(table("old", vec![col("id", "integer", false)]));
        let changes = diff(&source, &target);
        let p = plan(&all(&changes), &source, &target);
        let phases: Vec<Phase> = p.statements.iter().map(|s| s.phase).collect();
        let mut sorted = phases.clone();
        sorted.sort();
        assert_eq!(phases, sorted);
        assert!(pos(&p, "CREATE TYPE") < pos(&p, "DROP TABLE"));
        assert!(pos(&p, "DROP TABLE") < pos(&p, "CREATE TABLE \"public\".\"a\""));
        // Both tables exist before the foreign key, and the diff order of the
        // two tables is kept.
        assert!(
            pos(&p, "CREATE TABLE \"public\".\"a\"") < pos(&p, "CREATE TABLE \"public\".\"b\"")
        );
        assert!(pos(&p, "CREATE TABLE \"public\".\"b\"") < pos(&p, "FOREIGN KEY"));
        assert!(pos(&p, "FOREIGN KEY") < pos(&p, "CREATE TRIGGER"));
    }

    #[test]
    fn a_retyped_column_rebuilds_its_view_but_a_default_change_does_not() {
        let mut s = col("n", "bigint", true);
        let mut source = pg();
        source.tables.push(table("t", vec![s.clone()]));
        source
            .views
            .push(view("v", " SELECT n FROM t;", &[("t", "n")]));
        let mut target = source.clone();
        target.tables[0].columns[0].data_type = "integer".into();
        let changes = diff(&source, &target);
        let p = plan(&all(&changes), &source, &target);
        assert!(pos(&p, "DROP VIEW") < pos(&p, "TYPE bigint"));
        assert!(pos(&p, "TYPE bigint") < pos(&p, "CREATE VIEW"));
        s.default = "1".into();
        let mut source2 = source.clone();
        source2.tables[0].columns[0] = s;
        let changes = diff(&source2, &source);
        let p = plan(&all(&changes), &source2, &source);
        assert_eq!(p.runnable().len(), 1, "{:#?}", p.statements);
    }

    #[test]
    fn the_script_wraps_one_transaction_and_keeps_notes_as_comments() {
        let p = Plan {
            statements: vec![
                Statement::new(Phase::CreateTable, "CREATE TABLE t (a int)"),
                Statement::new(Phase::AlterColumn, "-- rebuild by hand"),
            ],
        };
        assert_eq!(p.runnable(), vec!["CREATE TABLE t (a int)"]);
        assert_eq!(
            p.script(&["Vix".to_string()]),
            "-- Vix\nBEGIN;\n\nCREATE TABLE t (a int);\n\n-- rebuild by hand\n\nCOMMIT;\n"
        );
    }

    #[test]
    fn duplicate_statements_are_dropped() {
        let mut source = pg();
        source
            .tables
            .push(table("t", vec![col("id", "integer", false)]));
        source.views.push(view("v", " SELECT 1;", &[]));
        let mut target = pg();
        target.tables.push(table(
            "t",
            vec![col("id", "integer", false), col("x", "text", true)],
        ));
        // The changed view is dropped by its own change and touched by the
        // dropped column: it is dropped once.
        target
            .views
            .push(view("v", " SELECT x FROM t;", &[("t", "x")]));
        let changes = diff(&source, &target);
        let p = plan(&all(&changes), &source, &target);
        let drops = p
            .runnable()
            .iter()
            .filter(|s| s.starts_with("DROP VIEW"))
            .count();
        assert_eq!(drops, 1);
        assert!(pos(&p, "DROP VIEW") < pos(&p, "DROP COLUMN"));
    }
}
