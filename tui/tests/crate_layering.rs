//! Crate layering rules (T833, `docs/architecture/index.md` "Layering
//! rules"): facts about the workspace dependency graph that hold today and must
//! keep holding. Each failure names the offending edge.
//!
//! 1. **Nothing depends on the App shell.** No workspace member depends on
//!    `vix-tui`, as a normal, build or dev-dependency.
//! 2. **Only `vix-input` and `vix-tui` use `crossterm`.** Terminal I/O stays in
//!    the App shell and key-event conversion, so the desktop GUIs can reuse
//!    every other crate without a terminal.
//! 3. **Model and core crates know no UI surface.** A crate named `*-core` or
//!    `*-model` reaches no crate named `*-panel`, `*-picker`, `*-preview` or
//!    `*-view`, directly or through other crates.
//! 4. **GUIs never depend on each other.** No `gui/vix-*/Cargo.toml` names
//!    another GUI crate.
//! 5. **The headless MCP server knows no UI surface.** `vix-mcp-server`
//!    reaches no `*-panel`, `*-picker`, `*-preview` or `*-view` crate,
//!    directly or through other crates (T860), and no crate it reaches is
//!    in [`HEADLESS_FORBIDDEN`] (`vix-palette`, the command palette; T861).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

#[path = "support/crate_graph.rs"]
mod crate_graph;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Name -> names: a dependency map.
type Deps = BTreeMap<String, BTreeSet<String>>;

/// The App shell, which nothing may depend on.
const APP_SHELL: &str = "vix-tui";

/// The only members that may depend on `crossterm`.
const CROSSTERM_USERS: [&str; 2] = ["vix-input", "vix-tui"];

/// Crates that run without any UI, so must reach no UI surface (rule 5).
const HEADLESS: [&str; 1] = ["vix-mcp-server"];

/// UI crates without a UI-surface suffix that a [`HEADLESS`] crate must not
/// reach either (rule 5): the command palette (T861).
const HEADLESS_FORBIDDEN: [&str; 1] = ["vix-palette"];

/// Whether `name` is a model or core crate (rule 3's subjects).
fn is_model_or_core(name: &str) -> bool {
    name.ends_with("-core") || name.ends_with("-model")
}

/// Whether `name` is a UI-surface crate (what rule 3 forbids).
fn is_ui_surface(name: &str) -> bool {
    ["-panel", "-picker", "-preview", "-view"]
        .iter()
        .any(|s| name.ends_with(s))
}

/// Rule 1: edges into the App shell, normal, build or dev.
fn app_shell_dependents(graph: &Deps, dev: &Deps) -> Vec<String> {
    let mut out = Vec::new();
    for (kind, map) in [("depends", graph), ("dev-depends", dev)] {
        for (from, deps) in map {
            if deps.contains(APP_SHELL) {
                out.push(format!(
                    "{from} -> {APP_SHELL}: {from} {kind} on the App shell; \
                     move what it needs out of {APP_SHELL} into a crate under crates/"
                ));
            }
        }
    }
    out
}

/// Rule 2: edges to `crossterm` from anything but [`CROSSTERM_USERS`].
fn crossterm_outside_shell(all_normal: &Deps) -> Vec<String> {
    all_normal
        .iter()
        .filter(|(from, deps)| {
            deps.contains("crossterm") && !CROSSTERM_USERS.contains(&from.as_str())
        })
        .map(|(from, _)| {
            format!(
                "{from} -> crossterm: only {} may depend on crossterm; take \
                 vix-input's key types or let the App shell do the terminal I/O",
                CROSSTERM_USERS.join(" and ")
            )
        })
        .collect()
}

/// Rule 3: for each model/core crate, the first UI-surface crate it reaches
/// (breadth first), as the chain of edges leading there.
fn model_core_reaching_ui(graph: &Deps) -> Vec<String> {
    reaching_ui(graph, is_model_or_core, is_ui_surface, "model/core crate")
}

/// Rule 5: the same search from each [`HEADLESS`] crate.
fn headless_reaching_ui(graph: &Deps) -> Vec<String> {
    reaching_ui(
        graph,
        |n| HEADLESS.contains(&n),
        |n| is_ui_surface(n) || HEADLESS_FORBIDDEN.contains(&n),
        "headless crate",
    )
}

/// For each crate `is_subject` picks, the first crate `is_forbidden` picks
/// that it reaches (breadth first), as the chain of edges leading there;
/// `what` names the subject in the message.
fn reaching_ui(
    graph: &Deps,
    is_subject: impl Fn(&str) -> bool,
    is_forbidden: impl Fn(&str) -> bool,
    what: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    for start in graph.keys().filter(|n| is_subject(n)) {
        let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
        let mut queue = VecDeque::from([start.as_str()]);
        let mut seen = BTreeSet::from([start.as_str()]);
        while let Some(at) = queue.pop_front() {
            for dep in graph.get(at).into_iter().flatten() {
                if !seen.insert(dep.as_str()) {
                    continue;
                }
                parent.insert(dep.as_str(), at);
                if is_forbidden(dep) {
                    let mut chain = vec![dep.as_str()];
                    let mut node = dep.as_str();
                    while let Some(&p) = parent.get(node) {
                        chain.push(p);
                        node = p;
                    }
                    chain.reverse();
                    out.push(format!(
                        "{} -> {dep}: {what} {start} reaches UI crate {dep} ({})",
                        chain[chain.len().saturating_sub(2)],
                        chain.join(" -> ")
                    ));
                    queue.clear();
                    break;
                }
                queue.push_back(dep.as_str());
            }
        }
    }
    out
}

/// Rule 4: a GUI manifest naming another GUI crate.
fn guis_depending_on_guis(gui_deps: &Deps) -> Vec<String> {
    let guis: BTreeSet<&str> = gui_deps.keys().map(String::as_str).collect();
    let mut out = Vec::new();
    for (from, deps) in gui_deps {
        for dep in deps {
            if dep != from && guis.contains(dep.as_str()) {
                out.push(format!(
                    "gui/{from} -> {dep}: a desktop GUI depends on another GUI; \
                     share code through vix-tui or a crate under crates/"
                ));
            }
        }
    }
    out
}

#[test]
fn the_workspace_keeps_its_layering_rules() {
    let ws = crate_graph::workspace();
    let gui_deps = crate_graph::gui_dependencies();
    assert!(
        ws.graph.contains_key(APP_SHELL),
        "no {APP_SHELL} in cargo metadata"
    );
    assert!(!gui_deps.is_empty(), "no GUI crate found under gui/");
    let mut problems = app_shell_dependents(&ws.graph, &ws.dev);
    problems.extend(crossterm_outside_shell(&ws.all_normal));
    problems.extend(model_core_reaching_ui(&ws.graph));
    problems.extend(headless_reaching_ui(&ws.graph));
    problems.extend(guis_depending_on_guis(&gui_deps));
    assert!(
        problems.is_empty(),
        "crate layering rules broken (docs/architecture/index.md, \"Layering rules\"):\n{}",
        problems.join("\n")
    );
}

#[test]
fn every_gui_reaches_the_app_shell() {
    // Rule 4's input is read from manifests; make sure it reads them.
    for (gui, deps) in crate_graph::gui_dependencies() {
        assert!(
            deps.contains("vix") || deps.contains(APP_SHELL),
            "gui/{gui}/Cargo.toml names no vix (vix-tui) dependency: {deps:?}"
        );
    }
}

/// A dependency map from `(from, [to…])` pairs.
fn deps(pairs: &[(&str, &[&str])]) -> Deps {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.iter().map(|s| (*s).to_owned()).collect()))
        .collect()
}

#[test]
fn each_rule_names_the_offending_edge() {
    let graph = deps(&[
        ("vix-tui", &["vix-a-model"]),
        ("vix-a-model", &["vix-mid"]),
        ("vix-mid", &["vix-x-panel"]),
        ("vix-x-panel", &["vix-tui"]),
        ("vix-ok-core", &["vix-mid2"]),
        ("vix-mid2", &[]),
    ]);
    let dev = deps(&[("vix-mid2", &["vix-tui"])]);
    assert_eq!(
        app_shell_dependents(&graph, &dev)
            .iter()
            .map(|p| p.split(':').next().unwrap())
            .collect::<Vec<_>>(),
        ["vix-x-panel -> vix-tui", "vix-mid2 -> vix-tui"]
    );
    let ui = model_core_reaching_ui(&graph);
    assert_eq!(ui.len(), 1, "{ui:?}");
    assert!(
        ui[0].starts_with("vix-mid -> vix-x-panel: model/core crate vix-a-model")
            && ui[0].ends_with("(vix-a-model -> vix-mid -> vix-x-panel)"),
        "{}",
        ui[0]
    );

    let headless = deps(&[
        ("vix-mcp-server", &["vix-md", "vix-ok-core"]),
        ("vix-md", &["vix-md-preview"]),
        ("vix-md-preview", &[]),
        ("vix-ok-core", &[]),
    ]);
    let hl = headless_reaching_ui(&headless);
    assert_eq!(hl.len(), 1, "{hl:?}");
    assert!(
        hl[0].starts_with("vix-md -> vix-md-preview: headless crate vix-mcp-server")
            && hl[0].ends_with("(vix-mcp-server -> vix-md -> vix-md-preview)"),
        "{}",
        hl[0]
    );
    assert_eq!(headless_reaching_ui(&graph), Vec::<String>::new());
    let palette = deps(&[
        ("vix-mcp-server", &["vix-palette"]),
        ("vix-palette", &[]),
        ("vix-ok-core", &["vix-palette"]),
    ]);
    let pl = headless_reaching_ui(&palette);
    assert_eq!(pl.len(), 1, "{pl:?}");
    assert!(
        pl[0].starts_with("vix-mcp-server -> vix-palette: headless crate vix-mcp-server"),
        "{}",
        pl[0]
    );
    assert_eq!(model_core_reaching_ui(&palette), Vec::<String>::new());

    let all = deps(&[
        ("vix-input", &["crossterm"]),
        ("vix-tui", &["crossterm"]),
        ("vix-db", &["crossterm", "ratatui"]),
    ]);
    let ct = crossterm_outside_shell(&all);
    assert_eq!(ct.len(), 1);
    assert!(ct[0].starts_with("vix-db -> crossterm:"), "{}", ct[0]);

    let guis = deps(&[("vix-egui", &["vix", "vix-iced"]), ("vix-iced", &["vix"])]);
    assert_eq!(
        guis_depending_on_guis(&guis)
            .iter()
            .map(|p| p.split(':').next().unwrap())
            .collect::<Vec<_>>(),
        ["gui/vix-egui -> vix-iced"]
    );
}
