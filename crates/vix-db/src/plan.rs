//! `PostgreSQL` query plans as a tree (T776).
//!
//! On `PostgreSQL`, `F6`/`F7` ask for a structured plan instead of the text
//! one, and this module turns it into a flat, pre-order list of [`Node`]s the
//! results grid shows as an indented tree. With `ANALYZE` each node gets its
//! **exclusive** time (its own work, without its children's), that time's
//! share of the whole plan, a "slowest" mark for the few nodes that dominate,
//! and a flag when the planner's row estimate is off from the real count by
//! [`MISESTIMATE_FACTOR`] or more.
//!
//! # Two input formats, one model
//!
//! `PostgreSQL` can emit the plan as JSON (`FORMAT JSON`) or YAML
//! (`FORMAT YAML`) with exactly the same structure. The workbench requests
//! **YAML**, because the `json` column type `FORMAT JSON` returns cannot be
//! decoded by sqlx's `Any` driver (it only maps text, numeric, boolean and
//! byte types), while YAML comes back as plain `text`. [`parse_json`] and
//! [`parse_yaml`] both build the same [`serde_json::Value`] and hand it to
//! [`from_value`], so one model serves both. The YAML reader is deliberately
//! a *subset* reader: it understands the indentation-based shape
//! `PostgreSQL`'s `explain.c` writes (block mappings, `- ` sequences, string
//! values JSON-escaped in double quotes, bare numbers and booleans), not
//! YAML in general.
//!
//! # Time accounting
//!
//! `Actual Total Time` is per loop and includes the node's children, so a
//! node's inclusive time is `Actual Total Time × Actual Loops`, and its
//! exclusive time is that minus the sum of its direct children's inclusive
//! times, clamped at zero (parallel workers can make children add up to more
//! than the parent's wall-clock time). The share is the exclusive time over
//! the sum of every node's exclusive time, so the shares add up to 100%.
//!
//! Everything here is pure and unit-tested on hand-recorded plans; the owning
//! spec is `crates/vix-db/spec/index.md` ("Plan tree").

use serde_json::{Map, Number, Value};

/// A row estimate off from the actual count by at least this factor (either
/// way) is flagged.
pub const MISESTIMATE_FACTOR: f64 = 10.0;

/// At most this many nodes are marked as the slowest.
pub const SLOWEST_COUNT: usize = 3;

/// A node needs at least this share of the total time to be marked slowest,
/// so a plan whose time is spread evenly highlights nothing trivial.
pub const SLOWEST_MIN_SHARE: f64 = 0.10;

/// The plan keys shown as a node's details, in display order. They are
/// `PostgreSQL`'s own field names, shown as the server writes them (like
/// the text plan does).
const DETAIL_KEYS: &[&str] = &[
    "Index Cond",
    "Recheck Cond",
    "Hash Cond",
    "Merge Cond",
    "Join Filter",
    "Filter",
    "One-Time Filter",
    "Sort Key",
    "Group Key",
    "Rows Removed by Filter",
    "Rows Removed by Join Filter",
];

/// One plan node, flattened in pre-order (a parent comes before its
/// children).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Node {
    /// Nesting depth; the root is 0.
    pub depth: usize,
    /// The index of the parent node in [`Plan::nodes`], `None` for the root.
    pub parent: Option<usize>,
    /// `Node Type`, e.g. `Seq Scan`, `Hash Join`, `ModifyTable`.
    pub node_type: String,
    /// `Operation` of a `ModifyTable` (`Insert`, `Update`, `Delete`, `Merge`).
    pub operation: Option<String>,
    /// The scanned or modified relation (`Relation Name`, else `CTE Name` or
    /// `Function Name`).
    pub relation: Option<String>,
    /// The relation's alias, when it differs from the relation name.
    pub alias: Option<String>,
    /// `Index Name` for index scans.
    pub index: Option<String>,
    /// `Join Type` for joins (`Inner`, `Left`, `Semi`, …).
    pub join_type: Option<String>,
    /// `Subplan Name` (`InitPlan 1`, `SubPlan 2`, `CTE x`).
    pub subplan: Option<String>,
    /// Key details as `(PostgreSQL key, value)`: filter, index condition,
    /// sort key, join filter, and the like (see `DETAIL_KEYS`).
    pub details: Vec<(String, String)>,
    /// `Total Cost`, the planner's estimate.
    pub cost: Option<f64>,
    /// `Plan Rows`, the estimated rows per loop.
    pub plan_rows: Option<f64>,
    /// `Actual Rows`, the real rows per loop (`ANALYZE` only).
    pub actual_rows: Option<f64>,
    /// `Actual Loops` (`ANALYZE` only).
    pub loops: Option<f64>,
    /// `Actual Total Time × Actual Loops` in milliseconds, children included
    /// (`ANALYZE` only).
    pub inclusive_ms: Option<f64>,
    /// The node's own time in milliseconds, children excluded (`ANALYZE`
    /// only).
    pub exclusive_ms: Option<f64>,
    /// [`Node::exclusive_ms`] as a fraction (0–1) of the plan's total
    /// exclusive time (`ANALYZE` only, and only when that total is above
    /// zero).
    pub share: Option<f64>,
    /// Whether this is one of the slowest nodes (see [`SLOWEST_COUNT`] and
    /// [`SLOWEST_MIN_SHARE`]).
    pub slowest: bool,
}

impl Node {
    /// A node with only its type set; the parser fills in the rest.
    fn new(node_type: String, depth: usize, parent: Option<usize>) -> Self {
        Self {
            depth,
            parent,
            node_type,
            operation: None,
            relation: None,
            alias: None,
            index: None,
            join_type: None,
            subplan: None,
            details: Vec::new(),
            cost: None,
            plan_rows: None,
            actual_rows: None,
            loops: None,
            inclusive_ms: None,
            exclusive_ms: None,
            share: None,
            slowest: false,
        }
    }

    /// The node's one-line title in `PostgreSQL`'s own words, e.g.
    /// `Index Scan using users_pkey on users u`, `Hash Join (Left)`, or
    /// `Update on users` for a `ModifyTable`.
    ///
    /// # Examples
    ///
    /// ```
    /// let plan = vix_db::plan::parse_json(
    ///     r#"[{"Plan": {"Node Type": "Seq Scan", "Relation Name": "users", "Alias": "u"}}]"#,
    /// )
    /// .unwrap();
    /// assert_eq!(plan.nodes[0].title(), "Seq Scan on users u");
    /// ```
    #[must_use]
    pub fn title(&self) -> String {
        use std::fmt::Write as _;
        let mut out = match (&self.operation, self.node_type.as_str()) {
            (Some(op), "ModifyTable") => op.clone(),
            _ => self.node_type.clone(),
        };
        if let Some(join) = self.join_type.as_deref().filter(|j| *j != "Inner") {
            let _ = write!(out, " ({join})");
        }
        if let Some(index) = &self.index {
            let _ = write!(out, " using {index}");
        }
        if let Some(rel) = &self.relation {
            let _ = write!(out, " on {rel}");
            if let Some(alias) = &self.alias {
                let _ = write!(out, " {alias}");
            }
        }
        if let Some(sub) = &self.subplan {
            out = format!("{sub}: {out}");
        }
        out
    }

    /// Whether the node never ran (`ANALYZE` reported zero loops).
    #[must_use]
    pub fn never_executed(&self) -> bool {
        self.loops.is_some_and(|l| l <= 0.0)
    }

    /// How far off the row estimate was, as `(factor, actual_was_higher)`,
    /// when the factor is at least [`MISESTIMATE_FACTOR`]. Compares rows per
    /// loop (how `PostgreSQL` reports both), with counts below one treated
    /// as one so an estimate of 1 against an actual 0 is not "infinitely"
    /// off. `None` without `ANALYZE`, for a node that never ran, or when the
    /// estimate was close enough.
    ///
    /// # Examples
    ///
    /// ```
    /// let plan = vix_db::plan::parse_json(
    ///     r#"[{"Plan": {"Node Type": "Seq Scan", "Plan Rows": 5,
    ///        "Actual Rows": 500, "Actual Loops": 1, "Actual Total Time": 1.0}}]"#,
    /// )
    /// .unwrap();
    /// let (factor, under) = plan.nodes[0].misestimate().unwrap();
    /// assert!((factor - 100.0).abs() < 1e-9 && under);
    /// ```
    #[must_use]
    pub fn misestimate(&self) -> Option<(f64, bool)> {
        if self.never_executed() {
            return None;
        }
        let estimate = self.plan_rows?.max(1.0);
        let actual = self.actual_rows?.max(1.0);
        let (factor, under) = if actual >= estimate {
            (actual / estimate, true)
        } else {
            (estimate / actual, false)
        };
        (factor >= MISESTIMATE_FACTOR).then_some((factor, under))
    }
}

/// A parsed plan: its nodes in pre-order plus the statement-level timings.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Plan {
    /// Every node, parents before children.
    pub nodes: Vec<Node>,
    /// Whether the plan carries `ANALYZE` measurements.
    pub analyzed: bool,
    /// `Planning Time` in milliseconds, when reported.
    pub planning_ms: Option<f64>,
    /// `Execution Time` in milliseconds (`ANALYZE` only).
    pub execution_ms: Option<f64>,
}

impl Plan {
    /// The indices of the direct children of node `i`.
    #[must_use]
    pub fn children(&self, i: usize) -> Vec<usize> {
        (i + 1..self.nodes.len())
            .filter(|&c| self.nodes[c].parent == Some(i))
            .collect()
    }
}

/// Parse a `FORMAT JSON` plan (the text of the single `QUERY PLAN` cell).
/// `None` when the text is not a plan.
///
/// # Examples
///
/// ```
/// let plan = vix_db::plan::parse_json(
///     r#"[{"Plan": {"Node Type": "Result", "Total Cost": 0.01, "Plan Rows": 1}}]"#,
/// )
/// .unwrap();
/// assert_eq!(plan.nodes.len(), 1);
/// assert!(!plan.analyzed);
/// ```
#[must_use]
pub fn parse_json(text: &str) -> Option<Plan> {
    from_value(&serde_json::from_str(text).ok()?)
}

/// Parse a `FORMAT YAML` plan (the text of the single `QUERY PLAN` cell) —
/// what the workbench actually requests (see the module docs). `None` when
/// the text is not a plan.
///
/// # Examples
///
/// ```
/// let text = "- Plan: \n    Node Type: \"Seq Scan\"\n    Relation Name: \"users\"\n    Plan Rows: 3\n  Planning Time: 0.05";
/// let plan = vix_db::plan::parse_yaml(text).unwrap();
/// assert_eq!(plan.nodes[0].title(), "Seq Scan on users");
/// ```
#[must_use]
pub fn parse_yaml(text: &str) -> Option<Plan> {
    from_value(&yaml_value(text)?)
}

/// Build a [`Plan`] from the decoded plan document: the top-level array of
/// `{"Plan": …, "Planning Time": …}` objects `PostgreSQL` returns (only the
/// first statement's plan is used), or one such object on its own.
#[must_use]
pub fn from_value(value: &Value) -> Option<Plan> {
    let top = match value {
        Value::Array(items) => items.first()?,
        other => other,
    };
    let root = top.get("Plan")?;
    let mut nodes = Vec::new();
    walk(root, 0, None, &mut nodes)?;
    let analyzed = nodes.iter().any(|n| n.loops.is_some());
    let mut plan = Plan {
        nodes,
        analyzed,
        planning_ms: number(top.get("Planning Time")),
        execution_ms: number(top.get("Execution Time")),
    };
    if analyzed {
        account_time(&mut plan);
    }
    Some(plan)
}

/// Push `value`'s node and then, recursively, its `Plans` children.
fn walk(value: &Value, depth: usize, parent: Option<usize>, out: &mut Vec<Node>) -> Option<()> {
    let obj = value.as_object()?;
    let mut node = Node::new(text(obj.get("Node Type"))?, depth, parent);
    node.operation = text(obj.get("Operation"));
    node.relation = text(obj.get("Relation Name"))
        .or_else(|| text(obj.get("CTE Name")))
        .or_else(|| text(obj.get("Function Name")));
    node.alias = text(obj.get("Alias")).filter(|a| node.relation.as_ref() != Some(a));
    node.index = text(obj.get("Index Name"));
    node.join_type = text(obj.get("Join Type"));
    node.subplan = text(obj.get("Subplan Name"));
    node.details = DETAIL_KEYS
        .iter()
        .filter_map(|&k| text(obj.get(k)).map(|v| (k.to_string(), v)))
        .collect();
    node.cost = number(obj.get("Total Cost"));
    node.plan_rows = number(obj.get("Plan Rows"));
    node.actual_rows = number(obj.get("Actual Rows"));
    node.loops = number(obj.get("Actual Loops"));
    node.inclusive_ms = number(obj.get("Actual Total Time"))
        .zip(node.loops)
        .map(|(t, l)| t * l)
        .or_else(|| node.loops.map(|_| 0.0));
    let me = out.len();
    out.push(node);
    if let Some(Value::Array(children)) = obj.get("Plans") {
        for child in children {
            walk(child, depth + 1, Some(me), out)?;
        }
    }
    Some(())
}

/// Fill in exclusive times, shares, and the slowest marks.
fn account_time(plan: &mut Plan) {
    for i in 0..plan.nodes.len() {
        let children: f64 = plan
            .children(i)
            .iter()
            .filter_map(|&c| plan.nodes[c].inclusive_ms)
            .sum();
        let node = &mut plan.nodes[i];
        node.exclusive_ms = node.inclusive_ms.map(|t| (t - children).max(0.0));
    }
    let total: f64 = plan.nodes.iter().filter_map(|n| n.exclusive_ms).sum();
    if total <= 0.0 {
        return;
    }
    for node in &mut plan.nodes {
        node.share = node.exclusive_ms.map(|t| t / total);
    }
    let mut order: Vec<usize> = (0..plan.nodes.len()).collect();
    order.sort_by(|&a, &b| {
        let share = |i: usize| plan.nodes[i].share.unwrap_or(0.0);
        share(b).total_cmp(&share(a))
    });
    for i in order.into_iter().take(SLOWEST_COUNT) {
        if plan.nodes[i].share.unwrap_or(0.0) >= SLOWEST_MIN_SHARE {
            plan.nodes[i].slowest = true;
        }
    }
}

/// A string-ish field: strings as-is, numbers and booleans formatted, and a
/// list of strings (`Sort Key`, `Group Key`) joined with `, `.
fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(items) if !items.is_empty() => Some(
            items
                .iter()
                .filter_map(|v| text(Some(v)))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => None,
    }
}

/// A numeric field (also accepting a number written as a string).
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Format a row count: whole numbers without decimals, fractional averages
/// (`PostgreSQL` 18 reports `Actual Rows` per loop with two decimals) with
/// two.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_db::plan::format_rows(1200.0), "1200");
/// assert_eq!(vix_db::plan::format_rows(0.5), "0.50");
/// ```
#[must_use]
pub fn format_rows(rows: f64) -> String {
    if rows.fract() == 0.0 {
        format!("{rows:.0}")
    } else {
        format!("{rows:.2}")
    }
}

/// The plan as results-grid content: translated headers, one row per node
/// (the first cell is the node's title indented by depth), and each row's
/// emphasis mark for the renderer.
///
/// With `ANALYZE` the columns are node, time share, self time (ms),
/// estimated rows, actual rows, loops, a note (slowest, row estimate off,
/// never executed) and details; without it, node, estimated rows, cost and
/// details.
#[must_use]
pub fn grid(plan: &Plan) -> (Vec<String>, Vec<Vec<String>>, Vec<crate::results::Mark>) {
    let headers: Vec<String> = if plan.analyzed {
        vec![
            t!("ui.db_plan_node").to_string(),
            t!("ui.db_plan_time").to_string(),
            t!("ui.db_plan_self_ms").to_string(),
            t!("ui.db_plan_est_rows").to_string(),
            t!("ui.db_plan_rows").to_string(),
            t!("ui.db_plan_loops").to_string(),
            t!("ui.db_plan_note").to_string(),
            t!("ui.db_plan_details").to_string(),
        ]
    } else {
        vec![
            t!("ui.db_plan_node").to_string(),
            t!("ui.db_plan_est_rows").to_string(),
            t!("ui.db_plan_cost").to_string(),
            t!("ui.db_plan_details").to_string(),
        ]
    };
    let rows = plan
        .nodes
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let arrow = if n.depth > 0 { "\u{2192} " } else { "" };
            let title = format!("{indent}{arrow}{}", n.title());
            let details = n
                .details
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(" \u{b7} ");
            let est = n.plan_rows.map(format_rows).unwrap_or_default();
            if plan.analyzed {
                vec![
                    title,
                    n.share
                        .map(|s| format!("{:.1}%", s * 100.0))
                        .unwrap_or_default(),
                    n.exclusive_ms
                        .map(|t| format!("{t:.3}"))
                        .unwrap_or_default(),
                    est,
                    n.actual_rows.map(format_rows).unwrap_or_default(),
                    n.loops.map(format_rows).unwrap_or_default(),
                    note(n),
                    details,
                ]
            } else {
                vec![
                    title,
                    est,
                    n.cost.map(|c| format!("{c:.2}")).unwrap_or_default(),
                    details,
                ]
            }
        })
        .collect();
    let marks = plan
        .nodes
        .iter()
        .map(|n| crate::results::Mark {
            hot: n.slowest,
            warn: n.misestimate().is_some(),
        })
        .collect();
    (headers, rows, marks)
}

/// The `ANALYZE` note cell: slowest, how far off the row estimate is, or
/// never executed.
fn note(node: &Node) -> String {
    let mut parts = Vec::new();
    if node.slowest {
        parts.push(t!("ui.db_plan_slowest").to_string());
    }
    if let Some((factor, under)) = node.misestimate() {
        let arrow = if under { "\u{2191}" } else { "\u{2193}" };
        let factor = format!("{factor:.0}");
        parts.push(format!(
            "{arrow} {}",
            t!("ui.db_plan_misestimate", factor = factor)
        ));
    }
    if node.never_executed() {
        parts.push(t!("ui.db_plan_never").to_string());
    }
    parts.join(", ")
}

/// One significant source line of a YAML document: its indentation and its
/// trimmed text.
struct YamlLine {
    /// Leading spaces.
    indent: usize,
    /// The rest of the line, without trailing whitespace.
    text: String,
}

/// Read `PostgreSQL`'s `FORMAT YAML` plan into the same [`Value`] its
/// `FORMAT JSON` would decode to. `None` when the text does not fit the
/// subset (see the module docs) or has trailing lines it cannot place.
fn yaml_value(text: &str) -> Option<Value> {
    let mut lines: Vec<YamlLine> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let body = l.trim_end();
            let indent = body.len() - body.trim_start_matches(' ').len();
            YamlLine {
                indent,
                text: body.trim_start_matches(' ').to_string(),
            }
        })
        .collect();
    let first = lines.first()?.indent;
    let mut i = 0;
    let value = yaml_block(&mut lines, &mut i, first);
    (i == lines.len()).then_some(value)
}

/// Whether a line starts a sequence item.
fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// Split a `key: value` line; `None` for a scalar (a quoted string, or text
/// without a key separator).
fn split_key(text: &str) -> Option<(String, String)> {
    if text.starts_with('"') {
        return None;
    }
    if let Some(key) = text.strip_suffix(':') {
        return Some((key.to_string(), String::new()));
    }
    let (key, value) = text.split_once(": ")?;
    Some((key.to_string(), value.trim().to_string()))
}

/// Read the block (a sequence or a mapping) whose lines sit at `indent`.
fn yaml_block(lines: &mut [YamlLine], i: &mut usize, indent: usize) -> Value {
    if lines.get(*i).is_some_and(|l| is_item(&l.text)) {
        yaml_seq(lines, i, indent)
    } else {
        yaml_map(lines, i, indent)
    }
}

/// Read a `- ` sequence at `indent`. An item that opens with `key: value` is
/// a mapping whose first entry shares the dash's line, so that line is
/// rewritten in place as an ordinary entry at the item's content column.
fn yaml_seq(lines: &mut [YamlLine], i: &mut usize, indent: usize) -> Value {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*i) {
        if line.indent != indent || !is_item(&line.text) {
            break;
        }
        let rest = line.text[1..].trim_start().to_string();
        let column = indent + line.text.len() - rest.len();
        if rest.is_empty() {
            *i += 1;
            match lines.get(*i).map(|l| l.indent) {
                Some(child) if child > indent => items.push(yaml_block(lines, i, child)),
                _ => items.push(Value::Null),
            }
        } else if split_key(&rest).is_some() {
            lines[*i] = YamlLine {
                indent: column,
                text: rest,
            };
            items.push(yaml_map(lines, i, column));
        } else {
            items.push(yaml_scalar(&rest));
            *i += 1;
        }
    }
    Value::Array(items)
}

/// Read a block mapping at `indent`. A key with no value takes the nested
/// block below it (deeper, or a sequence at the same column), else an empty
/// list — `PostgreSQL` writes an empty group such as `Triggers:` that way.
fn yaml_map(lines: &mut [YamlLine], i: &mut usize, indent: usize) -> Value {
    let mut map = Map::new();
    while let Some(line) = lines.get(*i) {
        if line.indent != indent || is_item(&line.text) {
            break;
        }
        let Some((key, value)) = split_key(&line.text) else {
            break;
        };
        *i += 1;
        let value = if value.is_empty() {
            match lines.get(*i) {
                Some(next)
                    if next.indent > indent || (next.indent == indent && is_item(&next.text)) =>
                {
                    let child = next.indent;
                    yaml_block(lines, i, child)
                }
                _ => Value::Array(Vec::new()),
            }
        } else {
            yaml_scalar(&value)
        };
        map.insert(key, value);
    }
    Value::Object(map)
}

/// A scalar: a JSON-escaped quoted string, a boolean, a number, or (for
/// anything else) the raw text.
fn yaml_scalar(text: &str) -> Value {
    if text.starts_with('"') {
        return serde_json::from_str::<String>(text)
            .map_or_else(|_| Value::String(text.to_string()), Value::String);
    }
    match text {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    if let Ok(n) = text.parse::<i64>() {
        return Value::Number(n.into());
    }
    text.parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map_or_else(|| Value::String(text.to_string()), Value::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain (no `ANALYZE`) nested join: Sort over a Hash Join of a Seq
    /// Scan and a Hash over an Index Scan.
    const PLAIN_JSON: &str = r#"[
      {
        "Plan": {
          "Node Type": "Sort",
          "Parallel Aware": false,
          "Async Capable": false,
          "Startup Cost": 35.51,
          "Total Cost": 35.76,
          "Plan Rows": 100,
          "Plan Width": 44,
          "Sort Key": ["o.created_at DESC", "u.name"],
          "Plans": [
            {
              "Node Type": "Hash Join",
              "Parent Relationship": "Outer",
              "Parallel Aware": false,
              "Async Capable": false,
              "Join Type": "Inner",
              "Startup Cost": 8.31,
              "Total Cost": 32.19,
              "Plan Rows": 100,
              "Plan Width": 44,
              "Inner Unique": true,
              "Hash Cond": "(o.user_id = u.id)",
              "Plans": [
                {
                  "Node Type": "Seq Scan",
                  "Parent Relationship": "Outer",
                  "Parallel Aware": false,
                  "Async Capable": false,
                  "Relation Name": "orders",
                  "Alias": "o",
                  "Startup Cost": 0.00,
                  "Total Cost": 22.70,
                  "Plan Rows": 1270,
                  "Plan Width": 20,
                  "Filter": "(total > '10'::numeric)"
                },
                {
                  "Node Type": "Hash",
                  "Parent Relationship": "Inner",
                  "Parallel Aware": false,
                  "Async Capable": false,
                  "Startup Cost": 8.29,
                  "Total Cost": 8.29,
                  "Plan Rows": 1,
                  "Plan Width": 36,
                  "Plans": [
                    {
                      "Node Type": "Index Scan",
                      "Parent Relationship": "Outer",
                      "Parallel Aware": false,
                      "Async Capable": false,
                      "Scan Direction": "Forward",
                      "Index Name": "users_pkey",
                      "Relation Name": "users",
                      "Alias": "u",
                      "Startup Cost": 0.28,
                      "Total Cost": 8.29,
                      "Plan Rows": 1,
                      "Plan Width": 36,
                      "Index Cond": "(id = 42)"
                    }
                  ]
                }
              ]
            }
          ]
        },
        "Planning Time": 0.214
      }
    ]"#;

    /// An `ANALYZE` plan with a Nested Loop whose inner Index Scan runs
    /// 120 times, a badly underestimated outer scan, and a Hash Left Join.
    const ANALYZE_JSON: &str = r#"[
      {
        "Plan": {
          "Node Type": "Hash Join",
          "Parallel Aware": false,
          "Async Capable": false,
          "Join Type": "Left",
          "Startup Cost": 12.0,
          "Total Cost": 140.0,
          "Plan Rows": 120,
          "Plan Width": 40,
          "Actual Startup Time": 0.5,
          "Actual Total Time": 10.0,
          "Actual Rows": 120,
          "Actual Loops": 1,
          "Hash Cond": "(n.customer_id = c.id)",
          "Plans": [
            {
              "Node Type": "Nested Loop",
              "Parent Relationship": "Outer",
              "Join Type": "Inner",
              "Total Cost": 120.0,
              "Plan Rows": 120,
              "Actual Total Time": 8.0,
              "Actual Rows": 120,
              "Actual Loops": 1,
              "Plans": [
                {
                  "Node Type": "Seq Scan",
                  "Parent Relationship": "Outer",
                  "Relation Name": "orders",
                  "Alias": "n",
                  "Total Cost": 20.0,
                  "Plan Rows": 2,
                  "Actual Total Time": 2.0,
                  "Actual Rows": 120,
                  "Actual Loops": 1,
                  "Filter": "(status = 'open'::text)",
                  "Rows Removed by Filter": 880
                },
                {
                  "Node Type": "Index Scan",
                  "Parent Relationship": "Inner",
                  "Index Name": "items_order_id_idx",
                  "Relation Name": "items",
                  "Alias": "items",
                  "Total Cost": 0.8,
                  "Plan Rows": 1,
                  "Actual Total Time": 0.04,
                  "Actual Rows": 1,
                  "Actual Loops": 120,
                  "Index Cond": "(order_id = n.id)"
                }
              ]
            },
            {
              "Node Type": "Hash",
              "Parent Relationship": "Inner",
              "Total Cost": 10.0,
              "Plan Rows": 400,
              "Actual Total Time": 1.0,
              "Actual Rows": 400,
              "Actual Loops": 1,
              "Plans": [
                {
                  "Node Type": "Seq Scan",
                  "Parent Relationship": "Outer",
                  "Relation Name": "customers",
                  "Alias": "c",
                  "Total Cost": 8.0,
                  "Plan Rows": 400,
                  "Actual Total Time": 0.6,
                  "Actual Rows": 400,
                  "Actual Loops": 1
                }
              ]
            }
          ]
        },
        "Planning Time": 0.3,
        "Triggers": [],
        "Execution Time": 10.4
      }
    ]"#;

    /// `EXPLAIN ANALYZE UPDATE …`: a `ModifyTable` over a Bitmap Heap Scan,
    /// with a never-executed `InitPlan`.
    const MODIFY_JSON: &str = r#"[
      {
        "Plan": {
          "Node Type": "ModifyTable",
          "Operation": "Update",
          "Relation Name": "users",
          "Alias": "users",
          "Total Cost": 12.5,
          "Plan Rows": 0,
          "Actual Total Time": 0.9,
          "Actual Rows": 0,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Result",
              "Parent Relationship": "InitPlan",
              "Subplan Name": "InitPlan 1",
              "Total Cost": 0.01,
              "Plan Rows": 1,
              "Actual Total Time": 0.0,
              "Actual Rows": 0,
              "Actual Loops": 0
            },
            {
              "Node Type": "Bitmap Heap Scan",
              "Parent Relationship": "Outer",
              "Relation Name": "users",
              "Alias": "users",
              "Total Cost": 12.4,
              "Plan Rows": 5,
              "Actual Total Time": 0.3,
              "Actual Rows": 3,
              "Actual Loops": 1,
              "Recheck Cond": "(team_id = 7)",
              "Plans": [
                {
                  "Node Type": "Bitmap Index Scan",
                  "Parent Relationship": "Outer",
                  "Index Name": "users_team_idx",
                  "Total Cost": 4.3,
                  "Plan Rows": 5,
                  "Actual Total Time": 0.1,
                  "Actual Rows": 3,
                  "Actual Loops": 1,
                  "Index Cond": "(team_id = 7)"
                }
              ]
            }
          ]
        },
        "Planning Time": 0.1,
        "Triggers": [],
        "Execution Time": 1.0
      }
    ]"#;

    /// The plain plan as `PostgreSQL` writes it with `FORMAT YAML` (the same
    /// document as [`PLAIN_JSON`]).
    const PLAIN_YAML: &str = "- Plan: \n    Node Type: \"Sort\"\n    Parallel Aware: false\n    Async Capable: false\n    Startup Cost: 35.51\n    Total Cost: 35.76\n    Plan Rows: 100\n    Plan Width: 44\n    Sort Key: \n      - \"o.created_at DESC\"\n      - \"u.name\"\n    Plans: \n      - Node Type: \"Hash Join\"\n        Parent Relationship: \"Outer\"\n        Parallel Aware: false\n        Async Capable: false\n        Join Type: \"Inner\"\n        Startup Cost: 8.31\n        Total Cost: 32.19\n        Plan Rows: 100\n        Plan Width: 44\n        Inner Unique: true\n        Hash Cond: \"(o.user_id = u.id)\"\n        Plans: \n          - Node Type: \"Seq Scan\"\n            Parent Relationship: \"Outer\"\n            Parallel Aware: false\n            Async Capable: false\n            Relation Name: \"orders\"\n            Alias: \"o\"\n            Startup Cost: 0.00\n            Total Cost: 22.70\n            Plan Rows: 1270\n            Plan Width: 20\n            Filter: \"(total > '10'::numeric)\"\n          - Node Type: \"Hash\"\n            Parent Relationship: \"Inner\"\n            Parallel Aware: false\n            Async Capable: false\n            Startup Cost: 8.29\n            Total Cost: 8.29\n            Plan Rows: 1\n            Plan Width: 36\n            Plans: \n              - Node Type: \"Index Scan\"\n                Parent Relationship: \"Outer\"\n                Parallel Aware: false\n                Async Capable: false\n                Scan Direction: \"Forward\"\n                Index Name: \"users_pkey\"\n                Relation Name: \"users\"\n                Alias: \"u\"\n                Startup Cost: 0.28\n                Total Cost: 8.29\n                Plan Rows: 1\n                Plan Width: 36\n                Index Cond: \"(id = 42)\"\n  Planning Time: 0.214";

    /// The `ModifyTable` plan in YAML, with the empty `Triggers:` group.
    const MODIFY_YAML: &str = "- Plan: \n    Node Type: \"ModifyTable\"\n    Operation: \"Update\"\n    Relation Name: \"users\"\n    Alias: \"users\"\n    Total Cost: 12.5\n    Plan Rows: 0\n    Actual Total Time: 0.9\n    Actual Rows: 0\n    Actual Loops: 1\n    Plans: \n      - Node Type: \"Result\"\n        Parent Relationship: \"InitPlan\"\n        Subplan Name: \"InitPlan 1\"\n        Total Cost: 0.01\n        Plan Rows: 1\n        Actual Total Time: 0.0\n        Actual Rows: 0\n        Actual Loops: 0\n      - Node Type: \"Bitmap Heap Scan\"\n        Parent Relationship: \"Outer\"\n        Relation Name: \"users\"\n        Alias: \"users\"\n        Total Cost: 12.4\n        Plan Rows: 5\n        Actual Total Time: 0.3\n        Actual Rows: 3\n        Actual Loops: 1\n        Recheck Cond: \"(team_id = 7)\"\n        Plans: \n          - Node Type: \"Bitmap Index Scan\"\n            Parent Relationship: \"Outer\"\n            Index Name: \"users_team_idx\"\n            Total Cost: 4.3\n            Plan Rows: 5\n            Actual Total Time: 0.1\n            Actual Rows: 3\n            Actual Loops: 1\n            Index Cond: \"(team_id = 7)\"\n  Planning Time: 0.1\n  Triggers: \n  Execution Time: 1.0";

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn plain_plan_flattens_in_pre_order_with_titles_and_details() {
        let plan = parse_json(PLAIN_JSON).unwrap();
        assert!(!plan.analyzed);
        assert!(close(plan.planning_ms, 0.214));
        assert_eq!(plan.execution_ms, None);
        let titles: Vec<String> = plan.nodes.iter().map(Node::title).collect();
        assert_eq!(
            titles,
            [
                "Sort",
                "Hash Join",
                "Seq Scan on orders o",
                "Hash",
                "Index Scan using users_pkey on users u",
            ]
        );
        let depths: Vec<usize> = plan.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, [0, 1, 2, 2, 3]);
        assert_eq!(plan.nodes[4].parent, Some(3));
        assert_eq!(plan.children(1), [2, 3]);
        assert_eq!(
            plan.nodes[0].details,
            [(
                "Sort Key".to_string(),
                "o.created_at DESC, u.name".to_string()
            )]
        );
        assert_eq!(
            plan.nodes[2].details,
            [("Filter".to_string(), "(total > '10'::numeric)".to_string())]
        );
        assert_eq!(plan.nodes[4].details[0].0, "Index Cond");
        assert!(close(plan.nodes[2].plan_rows, 1270.0));
        assert!(close(plan.nodes[2].cost, 22.70));
        // No ANALYZE: no times, no shares, no flags.
        for n in &plan.nodes {
            assert_eq!(n.exclusive_ms, None);
            assert_eq!(n.share, None);
            assert!(!n.slowest);
            assert_eq!(n.misestimate(), None);
        }
    }

    #[test]
    fn analyze_plan_accounts_exclusive_time_with_loops() {
        let plan = parse_json(ANALYZE_JSON).unwrap();
        assert!(plan.analyzed);
        assert!(close(plan.execution_ms, 10.4));
        let excl: Vec<f64> = plan.nodes.iter().map(|n| n.exclusive_ms.unwrap()).collect();
        // Hash Join 10 − (8 + 1) = 1; Nested Loop 8 − (2 + 0.04×120) = 1.2;
        // Seq Scan 2; Index Scan 0.04 × 120 loops = 4.8; Hash 1 − 0.6 = 0.4;
        // Seq Scan 0.6. Sum = 10 (the root's inclusive time).
        let want = [1.0, 1.2, 2.0, 4.8, 0.4, 0.6];
        for (got, want) in excl.iter().zip(want) {
            assert!((got - want).abs() < 1e-9, "{excl:?}");
        }
        let shares: f64 = plan.nodes.iter().filter_map(|n| n.share).sum();
        assert!((shares - 1.0).abs() < 1e-9);
        assert!(close(plan.nodes[3].share, 0.48));
        // The three slowest with at least 10%: Index Scan (48%), Seq Scan on
        // orders (20%), Nested Loop (12%); Hash Join (10%) is fourth.
        let slowest: Vec<usize> = (0..plan.nodes.len())
            .filter(|&i| plan.nodes[i].slowest)
            .collect();
        assert_eq!(slowest, [1, 2, 3]);
        assert_eq!(plan.nodes[0].title(), "Hash Join (Left)");
        assert_eq!(
            plan.nodes[3].title(),
            "Index Scan using items_order_id_idx on items"
        );
    }

    #[test]
    fn misestimates_are_flagged_both_ways_at_ten_times() {
        let plan = parse_json(ANALYZE_JSON).unwrap();
        // Seq Scan on orders: 2 estimated, 120 actual — 60× under.
        let (factor, under) = plan.nodes[2].misestimate().unwrap();
        assert!((factor - 60.0).abs() < 1e-9 && under);
        let flagged: Vec<usize> = (0..plan.nodes.len())
            .filter(|&i| plan.nodes[i].misestimate().is_some())
            .collect();
        assert_eq!(flagged, [2]);

        let over = parse_json(
            r#"[{"Plan": {"Node Type": "Seq Scan", "Plan Rows": 1000,
                "Actual Rows": 100, "Actual Loops": 1, "Actual Total Time": 1}}]"#,
        )
        .unwrap();
        let (factor, under) = over.nodes[0].misestimate().unwrap();
        assert!(
            (factor - 10.0).abs() < 1e-9 && !under,
            "exactly 10× is flagged"
        );
        let near = parse_json(
            r#"[{"Plan": {"Node Type": "Seq Scan", "Plan Rows": 1000,
                "Actual Rows": 101, "Actual Loops": 1, "Actual Total Time": 1}}]"#,
        )
        .unwrap();
        assert_eq!(near.nodes[0].misestimate(), None);
        let zero = parse_json(
            r#"[{"Plan": {"Node Type": "Seq Scan", "Plan Rows": 1,
                "Actual Rows": 0, "Actual Loops": 1, "Actual Total Time": 1}}]"#,
        )
        .unwrap();
        assert_eq!(zero.nodes[0].misestimate(), None, "1 vs 0 is not off");
    }

    #[test]
    fn modify_table_titles_its_operation_and_skips_never_executed() {
        let plan = parse_json(MODIFY_JSON).unwrap();
        let titles: Vec<String> = plan.nodes.iter().map(Node::title).collect();
        assert_eq!(
            titles,
            [
                "Update on users",
                "InitPlan 1: Result",
                "Bitmap Heap Scan on users",
                "Bitmap Index Scan using users_team_idx",
            ]
        );
        assert!(plan.nodes[1].never_executed());
        assert_eq!(plan.nodes[1].misestimate(), None);
        assert!(close(plan.nodes[1].exclusive_ms, 0.0));
        // ModifyTable 0.9 − (0 + 0.3) = 0.6 is the slowest node.
        assert!(close(plan.nodes[0].exclusive_ms, 0.6));
        assert!(plan.nodes[0].slowest);
        assert!(!plan.nodes[1].slowest, "a zero share is never slowest");
    }

    #[test]
    fn yaml_reads_into_the_same_plan_as_json() {
        let yaml = yaml_value(PLAIN_YAML).unwrap();
        let json: Value = serde_json::from_str(PLAIN_JSON).unwrap();
        assert_eq!(yaml, json);
        assert_eq!(parse_yaml(PLAIN_YAML), parse_json(PLAIN_JSON));
        assert_eq!(parse_yaml(MODIFY_YAML), parse_json(MODIFY_JSON));
        let modify = yaml_value(MODIFY_YAML).unwrap();
        assert_eq!(modify[0]["Triggers"], Value::Array(Vec::new()));
    }

    #[test]
    fn yaml_scalars_unescape_and_type() {
        assert_eq!(
            yaml_scalar(r#""a \"b\" \\ c""#),
            Value::String(r#"a "b" \ c"#.into())
        );
        assert_eq!(yaml_scalar("true"), Value::Bool(true));
        assert_eq!(yaml_scalar("42"), Value::Number(42.into()));
        assert_eq!(yaml_scalar("0.5").as_f64(), Some(0.5));
        assert_eq!(yaml_scalar("Forward"), Value::String("Forward".into()));
    }

    #[test]
    fn non_plans_are_rejected() {
        assert_eq!(parse_json("not json"), None);
        assert_eq!(parse_json("[]"), None);
        assert_eq!(parse_json(r#"[{"Plan": {"No Type": 1}}]"#), None);
        assert_eq!(parse_yaml(""), None);
        assert_eq!(
            parse_yaml("Seq Scan on users  (cost=0.00..1.10 rows=10 width=4)"),
            None
        );
        assert_eq!(
            parse_yaml("- Plan: \n    Node Type: \"Result\"\n  \"stray\""),
            None
        );
    }

    #[test]
    fn grid_rows_follow_the_nodes() {
        let plan = parse_json(ANALYZE_JSON).unwrap();
        let (headers, rows, marks) = grid(&plan);
        assert_eq!(headers.len(), 8);
        assert_eq!(rows.len(), plan.nodes.len());
        assert!(rows.iter().all(|r| r.len() == headers.len()));
        assert_eq!(rows[0][0], "Hash Join (Left)");
        assert!(rows[3][0].starts_with("    \u{2192} Index Scan"));
        assert_eq!(rows[3][1], "48.0%");
        assert_eq!(rows[3][2], "4.800");
        assert_eq!(rows[3][5], "120");
        assert_eq!(
            rows[2][7],
            "Filter: (status = 'open'::text) \u{b7} Rows Removed by Filter: 880"
        );
        assert!(marks[2].hot && marks[2].warn);
        assert!(!marks[0].hot && !marks[0].warn);
        assert_eq!(rows[0][6], "", "nothing to note on the root");
        assert!(rows[2][6].contains('\u{2191}'), "under-estimate arrow");

        let unanalyzed = parse_json(PLAIN_JSON).unwrap();
        let (headers, rows, marks) = grid(&unanalyzed);
        assert_eq!(headers.len(), 4);
        assert_eq!(rows[2][1], "1270");
        assert_eq!(rows[2][2], "22.70");
        assert!(marks.iter().all(|m| !m.hot && !m.warn));
    }

    #[test]
    fn rows_format_whole_and_fractional() {
        assert_eq!(format_rows(0.0), "0");
        assert_eq!(format_rows(1270.0), "1270");
        assert_eq!(format_rows(2.5), "2.50");
    }
}
