//! The workspace crate graph from `cargo metadata` (T836, T833), shared by
//! `tests/docs_crate_graph.rs`, `tests/crate_layering.rs` and the
//! `list_commands` example, which includes it with
//! `#[path = "../tests/support/crate_graph.rs"] mod crate_graph;` to write
//! `docs/reference/crate-layers.md`. Each includer uses only part of it.
#![allow(
    dead_code,
    reason = "shared by several test and example binaries, each using a part"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The repository root (the including package lives in `tui/`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// What `cargo metadata` says about the members' dependencies.
pub struct Workspace {
    /// Member name -> the members it depends on (normal and build
    /// dependencies; dev-dependencies do not count).
    pub graph: BTreeMap<String, BTreeSet<String>>,
    /// Member name -> the members it dev-depends on.
    pub dev: BTreeMap<String, BTreeSet<String>>,
    /// Member name -> every crate (workspace or not) it depends on as a normal
    /// or build dependency.
    pub all_normal: BTreeMap<String, BTreeSet<String>>,
}

/// Run `cargo metadata --no-deps --offline` on the workspace and read it.
///
/// # Panics
///
/// When `cargo metadata` cannot run or prints something unexpected: every
/// includer is a test or a generator, which should fail loudly.
pub fn workspace() -> Workspace {
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
        .unwrap_or_else(|e| panic!("run cargo metadata: {e}"));
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("metadata JSON: {e}"));
    let packages = meta["packages"].as_array().cloned().unwrap_or_default();
    let name_of = |p: &serde_json::Value| p["name"].as_str().unwrap_or_default().to_owned();
    let names: BTreeSet<String> = packages.iter().map(name_of).collect();
    let mut ws = Workspace {
        graph: BTreeMap::new(),
        dev: BTreeMap::new(),
        all_normal: BTreeMap::new(),
    };
    for p in &packages {
        let name = name_of(p);
        let (mut normal, mut dev, mut all) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let Some(dep) = d["name"].as_str() else {
                continue;
            };
            let is_dev = d["kind"].as_str() == Some("dev");
            if !is_dev {
                all.insert(dep.to_owned());
            }
            if names.contains(dep) {
                if is_dev {
                    dev.insert(dep.to_owned());
                } else {
                    normal.insert(dep.to_owned());
                }
            }
        }
        ws.graph.insert(name.clone(), normal);
        ws.dev.insert(name.clone(), dev);
        ws.all_normal.insert(name, all);
    }
    ws
}

/// Each member's layer: the longest chain of workspace dependencies below it.
pub fn layers(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeMap<String, usize> {
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

/// Member name -> how many members depend on it directly (normal or build).
pub fn dependent_counts(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = graph.keys().map(|k| (k.clone(), 0)).collect();
    for deps in graph.values() {
        for d in deps {
            if let Some(c) = counts.get_mut(d) {
                *c = c.saturating_add(1);
            }
        }
    }
    counts
}

/// The member crates under `crates/` (every workspace member but `vix-tui`).
pub fn member_crates(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    graph.keys().filter(|n| *n != "vix-tui").cloned().collect()
}

/// The desktop GUI crates: the directories under `gui/` named `vix-*`.
pub fn gui_crates() -> BTreeSet<String> {
    std::fs::read_dir(repo_root().join("gui"))
        .unwrap_or_else(|e| panic!("read gui/: {e}"))
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("vix-"))
        .collect()
}

/// GUI crate -> every crate its `gui/<crate>/Cargo.toml` names in a
/// dependency table (`[dependencies]`, `[build-dependencies]`,
/// `[dev-dependencies]` and their `[target.*]` forms), by key and by
/// `package = "…"` rename. Read from the manifests, since `gui/` is a
/// separate workspace whose `cargo metadata` would need its toolkits.
pub fn gui_dependencies() -> BTreeMap<String, BTreeSet<String>> {
    fn collect(table: &toml::Table, out: &mut BTreeSet<String>) {
        for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
            for (key, spec) in table
                .get(section)
                .and_then(toml::Value::as_table)
                .into_iter()
                .flatten()
            {
                out.insert(key.clone());
                if let Some(package) = spec.get("package").and_then(toml::Value::as_str) {
                    out.insert(package.to_owned());
                }
            }
        }
    }
    gui_crates()
        .into_iter()
        .map(|gui| {
            let path = repo_root().join("gui").join(&gui).join("Cargo.toml");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let manifest: toml::Table =
                toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            let mut deps = BTreeSet::new();
            collect(&manifest, &mut deps);
            for target in manifest
                .get("target")
                .and_then(toml::Value::as_table)
                .into_iter()
                .flatten()
            {
                if let Some(t) = target.1.as_table() {
                    collect(t, &mut deps);
                }
            }
            (gui, deps)
        })
        .collect()
}

/// `docs/reference/crate-layers.md`: every workspace member with its layer,
/// its direct workspace dependencies and how many members depend on it,
/// grouped by layer.
pub fn render_layers_report(graph: &BTreeMap<String, BTreeSet<String>>) -> String {
    use std::fmt::Write as _;
    let layer = layers(graph);
    let dependents = dependent_counts(graph);
    let top = layer.values().copied().max().unwrap_or(0);
    let mut out = String::from(
        "# Crate Layers\n\n\
         Generated by `cargo run --example list_commands -- --write` (T833) from\n\
         `cargo metadata`; do not edit by hand. `scripts/check` fails when it is stale.\n\n\
         A crate's *layer* is the longest chain of workspace dependencies below it:\n\
         layer 0 crates depend on no other workspace crate, and every crate depends\n\
         only on crates in lower layers. Dependencies are the direct normal and build\n\
         dependencies on other workspace members (dev-dependencies do not count);\n\
         *dependents* is how many members depend on the crate directly. The rules this\n\
         graph must keep are in the [architecture overview](../architecture/index.md#layering-rules),\n\
         enforced by `tui/tests/crate_layering.rs`.\n\n\
         | Layer | Crates |\n\
         |---|---|\n",
    );
    for n in (0..=top).rev() {
        let count = layer.values().filter(|l| **l == n).count();
        let _ = writeln!(out, "| {n} | {count} |");
    }
    for n in (0..=top).rev() {
        let count = layer.values().filter(|l| **l == n).count();
        let noun = if count == 1 { "crate" } else { "crates" };
        let _ = write!(
            out,
            "\n## Layer {n} ({count} {noun})\n\n\
             | Crate | Layer | Direct workspace dependencies | Dependents |\n\
             |---|---|---|---|\n"
        );
        for (name, _) in layer.iter().filter(|(_, l)| **l == n) {
            let deps = graph.get(name).cloned().unwrap_or_default();
            let deps = if deps.is_empty() {
                "none".to_owned()
            } else {
                deps.iter()
                    .map(|d| format!("`{d}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(
                out,
                "| `{name}` | {n} | {deps} | {} |",
                dependents.get(name).copied().unwrap_or(0)
            );
        }
    }
    out
}
