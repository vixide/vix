//! Repository invariant for the contributor docs (T836, `spec/test`
//! "Repository invariants"): what they say about the crate graph matches
//! `cargo metadata`, so the architecture diagram and the crate map cannot drift
//! from the workspace the way the crate map's "124 member crates" once did.
//!
//! 1. **Architecture diagram edges are real dependencies.** In every
//!    `flowchart`/`graph` Mermaid block of `docs/architecture/index.md`, an
//!    edge `a --> b` between two nodes whose labels start with a workspace
//!    crate name is a direct normal or build dependency of `a` on `b`. A
//!    `subgraph` titled `Layer N (M crates)` holds only crates of layer `N`,
//!    and the workspace has exactly `M` of them, where a crate's layer is the
//!    longest chain of workspace dependencies below it (0: none).
//! 2. **Crate names are real.** Every backticked `vix-…` name on that page and
//!    in `agents/share/crate-map.md` is a workspace member or a desktop GUI
//!    crate under `gui/`; a name with `*` matches at least one member.
//! 3. **The crate map is complete** (each member crate is named, directly or
//!    through a `*` pattern), and every "N focused `vix-*`" / "N feature
//!    crates" count in the top-level docs equals the number of member crates
//!    under `crates/`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The repository root (this test's package lives in `tui/`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Read a repository file as text.
fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The workspace's direct normal and build dependencies between members:
/// member name -> the members it depends on.
fn workspace_graph() -> BTreeMap<String, BTreeSet<String>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(repo_root())
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata JSON");
    let packages = meta["packages"].as_array().expect("packages");
    let names: BTreeSet<String> = packages
        .iter()
        .map(|p| p["name"].as_str().expect("name").to_owned())
        .collect();
    packages
        .iter()
        .map(|p| {
            let deps = p["dependencies"]
                .as_array()
                .expect("dependencies")
                .iter()
                .filter(|d| d["kind"].as_str() != Some("dev"))
                .filter_map(|d| d["name"].as_str())
                .filter(|n| names.contains(*n))
                .map(str::to_owned)
                .collect();
            (p["name"].as_str().expect("name").to_owned(), deps)
        })
        .collect()
}

/// Each member's layer: the longest chain of workspace dependencies below it.
fn layers(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeMap<String, usize> {
    fn layer(
        name: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        memo: &mut BTreeMap<String, usize>,
    ) -> usize {
        if let Some(&l) = memo.get(name) {
            return l;
        }
        let l = graph
            .get(name)
            .into_iter()
            .flatten()
            .map(|d| layer(d, graph, memo).saturating_add(1))
            .max()
            .unwrap_or(0);
        memo.insert(name.to_owned(), l);
        l
    }
    let mut memo = BTreeMap::new();
    for name in graph.keys() {
        layer(name, graph, &mut memo);
    }
    memo
}

/// The member crates under `crates/` (every workspace member but `vix-tui`).
fn member_crates(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    graph.keys().filter(|n| *n != "vix-tui").cloned().collect()
}

/// The desktop GUI crates: the directories under `gui/` named `vix-*`.
fn gui_crates() -> BTreeSet<String> {
    std::fs::read_dir(repo_root().join("gui"))
        .expect("read gui/")
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("vix-"))
        .collect()
}

/// The bodies of the `flowchart`/`graph` Mermaid blocks in `text`.
fn mermaid_flowcharts(text: &str) -> Vec<Vec<&str>> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        match current.as_mut() {
            None if trimmed == "```mermaid" => current = Some(Vec::new()),
            None => {}
            Some(_) if trimmed == "```" => {
                let block = current.take().unwrap_or_default();
                let kind = block.first().map_or_default(|l| l.trim());
                if kind.starts_with("flowchart") || kind.starts_with("graph") {
                    blocks.push(block);
                }
            }
            Some(block) => block.push(line),
        }
    }
    blocks
}

/// The node at the start of `s`: its id, its label if it has one, and the rest
/// of `s` after it.
fn node(s: &str) -> Option<(&str, Option<&str>, &str)> {
    let s = s.trim_start();
    let id_len = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if id_len == 0 {
        return None;
    }
    let (id, rest) = s.split_at(id_len);
    let close = match rest.chars().next() {
        Some('[') => ']',
        Some('(') => ')',
        Some('{') => '}',
        _ => return Some((id, None, rest)),
    };
    let end = rest.find(close)?;
    let label = rest[1..end].trim_matches('"');
    Some((id, Some(label), &rest[end.saturating_add(1)..]))
}

/// The crate a node label names: its first word, when that is `vix-…`.
fn label_crate(label: &str) -> Option<&str> {
    let word = label
        .split(|c: char| c.is_whitespace() || c == ':')
        .next()
        .unwrap_or_default();
    word.starts_with("vix-").then_some(word)
}

/// What one Mermaid flowchart says: node labels, edges and `Layer` subgraphs.
#[derive(Default)]
struct Chart {
    /// Node id -> label.
    labels: BTreeMap<String, String>,
    /// `(from id, to id)`.
    edges: Vec<(String, String)>,
    /// `(layer, crate count, node ids inside)` for each `Layer N (M crates)` subgraph.
    layers: Vec<(usize, usize, Vec<String>)>,
}

/// Parse one flowchart body.
fn parse_chart(block: &[&str]) -> Chart {
    let mut chart = Chart::default();
    let mut open_layer: Option<usize> = None;
    for line in block.iter().skip(1) {
        let line = line.trim();
        if let Some(title) = line.strip_prefix("subgraph ") {
            open_layer = parse_layer_title(title).map(|(n, m)| {
                chart.layers.push((n, m, Vec::new()));
                chart.layers.len().saturating_sub(1)
            });
            continue;
        }
        if line == "end" {
            open_layer = None;
            continue;
        }
        // A chain `a --> b --> c`, any node possibly with a label, and an
        // edge possibly with `|text|`.
        let mut ids = Vec::new();
        for part in line.split("-->") {
            let part = part.trim_start();
            let part = match part.strip_prefix('|') {
                Some(rest) => rest.split_once('|').map_or("", |(_, after)| after),
                None => part,
            };
            if let Some((id, label, _)) = node(part) {
                if let Some(label) = label {
                    chart.labels.insert(id.to_owned(), label.to_owned());
                }
                ids.push(id.to_owned());
            }
        }
        for pair in ids.windows(2) {
            chart.edges.push((pair[0].clone(), pair[1].clone()));
        }
        if let (Some(i), Some(first)) = (open_layer, ids.first())
            && ids.len() == 1
        {
            chart.layers[i].2.push(first.clone());
        }
    }
    chart
}

/// `L3["Layer 3: … (20 crates)"]` -> `(3, 20)`.
fn parse_layer_title(title: &str) -> Option<(usize, usize)> {
    let (_, label, _) = node(title)?;
    let rest = label?.strip_prefix("Layer ")?;
    let n_len = rest.find(|c: char| !c.is_ascii_digit())?;
    let n = rest[..n_len].parse().ok()?;
    let open = rest.rfind('(')?;
    let m = rest[open.saturating_add(1)..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((n, m))
}

#[test]
fn architecture_diagram_edges_are_real_dependencies() {
    let graph = workspace_graph();
    let layer = layers(&graph);
    let guis = gui_crates();
    let text = read("docs/architecture/index.md");
    let blocks = mermaid_flowcharts(&text);
    assert!(
        !blocks.is_empty(),
        "no Mermaid flowchart in docs/architecture/index.md"
    );
    let mut problems = Vec::new();
    let mut layer_charts = 0usize;
    for block in &blocks {
        let chart = parse_chart(block);
        let crate_of = |id: &str| chart.labels.get(id).and_then(|l| label_crate(l));
        for label in chart.labels.values() {
            if let Some(name) = label_crate(label)
                && !graph.contains_key(name)
                && !guis.contains(name)
            {
                problems.push(format!("node {label:?} names no crate"));
            }
        }
        for (from, to) in &chart.edges {
            if let (Some(a), Some(b)) = (crate_of(from), crate_of(to))
                && graph.contains_key(a)
                && graph.contains_key(b)
                && !graph[a].contains(b)
            {
                problems.push(format!("edge {a} --> {b}: {a} does not depend on {b}"));
            }
        }
        for (n, m, ids) in &chart.layers {
            layer_charts = layer_charts.saturating_add(1);
            let real = layer.values().filter(|l| *l == n).count();
            if real != *m {
                problems.push(format!(
                    "Layer {n} says {m} crates, cargo metadata has {real}"
                ));
            }
            for id in ids {
                if let Some(name) = crate_of(id)
                    && layer.get(name).is_some_and(|l| l != n)
                {
                    problems.push(format!(
                        "{name} is drawn in layer {n}, it is in layer {}",
                        layer[name]
                    ));
                }
            }
        }
    }
    assert!(
        layer_charts > 0,
        "the layer diagram (subgraphs titled \"Layer N (M crates)\") is missing"
    );
    let direct = graph["vix-tui"].len();
    let members = member_crates(&graph).len();
    let claim = format!("directly on {direct} of the {members}");
    if !text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(&claim)
    {
        problems.push(format!(
            "the page should say vix-tui depends \"{claim}\" member crates"
        ));
    }
    assert!(
        problems.is_empty(),
        "docs/architecture/index.md:\n{}",
        problems.join("\n")
    );
}

/// Every backticked `vix-…` name in `text` (the whole code span).
fn backticked_crate_names(text: &str) -> BTreeSet<&str> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| {
            span.starts_with("vix-")
                && span.len() > 4
                && span
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '*')
        })
        .collect()
}

/// Whether `name` matches `pattern`, where `*` stands for any text.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    for (i, part) in parts.iter().enumerate() {
        if i.saturating_add(1) == parts.len() {
            return rest.ends_with(part);
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at.saturating_add(part.len())..],
            None => return false,
        }
    }
    rest.is_empty()
}

#[test]
fn crate_names_in_contributor_docs_are_real() {
    let graph = workspace_graph();
    let guis = gui_crates();
    let mut problems = Vec::new();
    for rel in ["docs/architecture/index.md", "agents/share/crate-map.md"] {
        let text = read(rel);
        for name in backticked_crate_names(&text) {
            let known = if name.contains('*') {
                graph.keys().any(|n| glob_matches(name, n))
            } else {
                graph.contains_key(name) || guis.contains(name)
            };
            if !known {
                problems.push(format!("{rel}: `{name}` is not a workspace or GUI crate"));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn crate_map_names_every_member_and_counts_match() {
    let graph = workspace_graph();
    let members = member_crates(&graph);
    let map = read("agents/share/crate-map.md");
    let patterns: Vec<&str> = backticked_crate_names(&map)
        .into_iter()
        .filter(|n| n.contains('*') && *n != "vix-*")
        .collect();
    let missing: Vec<&String> = members
        .iter()
        .filter(|m| {
            !map.contains(&format!("`{m}`")) && !patterns.iter().any(|p| glob_matches(p, m))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "agents/share/crate-map.md does not name: {missing:?}"
    );

    let count = members.len();
    let mut problems = Vec::new();
    for rel in [
        "AGENTS.md",
        "CLAUDE.md",
        "CONTRIBUTING.md",
        "README.md",
        "index.md",
        "llms.txt",
        "agents/share/crate-map.md",
        "docs/architecture/index.md",
        "docs/development.md",
    ] {
        let text = read(rel).split_whitespace().collect::<Vec<_>>().join(" ");
        for claim in crate_count_claims(&text) {
            if claim != count {
                problems.push(format!(
                    "{rel} says {claim} member crates, crates/ has {count}"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The numbers in "N `vix-*`", "N focused `vix-*`", "N focused crates" and
/// "N feature crates" (whitespace already collapsed to single spaces).
fn crate_count_claims(text: &str) -> Vec<usize> {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let Ok(n) = word.trim_start_matches(['(', '*']).parse::<usize>() else {
            continue;
        };
        let next = words.get(i.saturating_add(1)).copied().unwrap_or_default();
        let after = words.get(i.saturating_add(2)).copied().unwrap_or_default();
        let is_claim = next.starts_with("`vix-*`")
            || (matches!(next, "focused" | "feature")
                && (after.starts_with("`vix-*`") || after.starts_with("crates")));
        if is_claim {
            out.push(n);
        }
    }
    out
}

#[test]
fn glob_and_claim_helpers() {
    assert!(glob_matches(
        "vix-convert-from-*-into-*-tool",
        "vix-convert-from-csv-into-json-tool"
    ));
    assert!(!glob_matches(
        "vix-convert-from-*-into-*-tool",
        "vix-convert-tabular"
    ));
    assert!(glob_matches("vix-edit-*", "vix-edit-table"));
    assert_eq!(
        crate_count_claims("over **147 focused `vix-*` member crates**"),
        vec![147]
    );
    assert_eq!(
        crate_count_claims("the 124 `vix-*` member crates, 7 GUI crates"),
        vec![124]
    );
    assert_eq!(
        crate_count_claims("Layer 3 (20 crates)"),
        Vec::<usize>::new()
    );
    assert_eq!(
        parse_layer_title(r#"L3["Layer 3: features (20 crates)"]"#),
        Some((3, 20))
    );
}
