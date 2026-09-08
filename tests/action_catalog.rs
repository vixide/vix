//! T147: every action id `App::run_action` can actually dispatch is titled
//! somewhere — by a `vix_menu::Item` leaf, by `vix_action_catalog`, or by
//! its own `palette::COMMANDS` entry.
//!
//! `App::run_action` fans out through a fixed chain of private dispatcher
//! methods (`run_file_action`, `run_edit_action`, …, each trying the next on
//! a `bool` miss); this test walks that same chain by name, brace-balancing
//! each function's body out of its source file, and collects every `"id" =>`
//! match-arm pattern it finds. A new dispatcher earns its own entry in
//! [`DISPATCHERS`] below — the list is deliberately explicit (not a blanket
//! source-wide grep) so an arm added to a `match` that *isn't* part of this
//! chain (a keymap id, a vim command char, …) never gets mistaken for an
//! action id.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// `(file relative to the workspace root, function name)`. Kept in the same
/// order `App::run_action` tries them, so a diff here reads like the
/// dispatch chain itself.
const DISPATCHERS: &[(&str, &str)] = &[
    ("src/app.rs", "run_action"),
    ("src/app.rs", "run_file_action"),
    ("src/app.rs", "run_edit_action"),
    ("src/app.rs", "run_motion_action"),
    ("src/app.rs", "run_text_tool_action"),
    ("src/app.rs", "run_convert_action"),
    ("src/app.rs", "run_format_action"),
    ("src/app.rs", "run_lsp_action"),
    ("src/app.rs", "run_search_action"),
    ("src/app.rs", "run_named_action"),
    ("src/app.rs", "run_cursor_action"),
    ("src/app.rs", "run_app_action"),
    ("src/app.rs", "run_help_action"),
    ("src/app.rs", "run_view_action"),
    ("src/app.rs", "run_project_action"),
    ("src/app.rs", "db_action"),
    ("src/app.rs", "open_edit_surface"),
    ("src/app.rs", "contacts_action"),
    ("src/app.rs", "go_action"),
    ("src/app/insert_tools.rs", "run_tools_action"),
    ("src/app/keymap.rs", "run_vim_action"),
    ("src/app/git.rs", "run_git_action"),
    ("src/app/git.rs", "run_jj_action"),
    ("src/app/org.rs", "org_action"),
    ("src/app/org.rs", "org_edit_action"),
    ("src/app/org_table.rs", "org_table_action"),
    ("src/app/roam.rs", "roam_action"),
];

/// The workspace root (this test's package is the root package).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Brace-balance `fn fn_name(...) ... { ... }` out of `text`, from its first
/// `fn fn_name(` to the matching close brace.
fn function_body<'a>(text: &'a str, fn_name: &str) -> &'a str {
    let needle = format!("fn {fn_name}(");
    let start = text
        .find(&needle)
        .unwrap_or_else(|| panic!("`fn {fn_name}` not found"));
    let bytes = text.as_bytes();
    let body_start = text[start..]
        .find('{')
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("no open brace after `fn {fn_name}`"));
    let mut depth = 0i32;
    let mut i = body_start;
    loop {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[start..=i];
                }
            }
            _ => {}
        }
        i += 1;
        assert!(i < bytes.len(), "unbalanced braces in `fn {fn_name}`");
    }
}

/// Every quoted string literal in `s`, in order. A plain scan (no escaped-
/// quote handling) — none of these action-id match patterns need it.
fn quoted_strings(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('"') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else {
            break;
        };
        out.push(&rest[..close]);
        rest = &rest[close + 1..];
    }
    out
}

/// The literal action ids matched by `"id"` (or `"a" | "b" | …`) match arms
/// in `body`. Only lines whose *pattern* side starts with a quote count —
/// this is what excludes guard arms like `a if a.starts_with("edit.") => …`
/// (pattern starts with the binding `a`, not a literal) from being read as
/// naming an action id.
fn arm_ids_in(body: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('"') {
            continue;
        }
        let Some(arrow) = trimmed.find("=>") else {
            continue;
        };
        for id in quoted_strings(&trimmed[..arrow]) {
            ids.insert(id.to_string());
        }
    }
    ids
}

/// Every action id the dispatch chain in [`DISPATCHERS`] can actually match.
fn every_dispatchable_action_id(root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (file, func) in DISPATCHERS {
        let text = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("reading {file}: {e}"));
        let body = function_body(&text, func);
        ids.extend(arm_ids_in(body));
    }
    ids
}

/// Every action id a `vix_menu::Item` leaf runs.
fn menu_action_ids() -> BTreeSet<String> {
    fn walk(items: &[vix_menu::Item], out: &mut BTreeSet<String>) {
        for it in items {
            if let Some(sub) = it.submenu {
                walk(sub, out);
            } else if !it.action.is_empty() {
                out.insert(it.action.to_string());
            }
        }
    }
    let mut out = BTreeSet::new();
    for m in vix_menu::menus() {
        walk(m.items, &mut out);
    }
    out
}

#[test]
fn every_dispatchable_action_is_titled_by_the_menu_the_catalog_or_the_palette() {
    let root = workspace_root();
    let dispatchable = every_dispatchable_action_id(&root);
    assert!(
        dispatchable.len() > 600,
        "only found {} action ids — DISPATCHERS is probably missing a function \
         (the dispatch chain had 669+ distinct ids as of T147)",
        dispatchable.len()
    );
    let menu_ids = menu_action_ids();
    let catalog_ids: BTreeSet<&str> = vix_action_catalog::CATALOG.iter().map(|a| a.id).collect();
    // A handful of `palette::COMMANDS` entries have no menu leaf of their
    // own (`nav.goto_workspace_symbol`, …) — titled there instead of in
    // the catalog (`no_catalog_entry_duplicates_a_palette_commands_entry`
    // keeps them from ever being cataloged twice).
    let palette_ids: BTreeSet<&str> = vix_palette::COMMANDS.iter().map(|(_, id)| *id).collect();

    // Three `starts_with` prefix guards match a dynamically-suffixed id
    // (`view.theme:Dark`, `script:my_script`, …), never a literal one — a
    // menu leaf or catalog entry for the bare prefix itself isn't
    // meaningful, so these are exempted rather than cataloged.
    let dynamic_prefixes = [
        "view.theme:",
        "view.locale:",
        "view.keymap:",
        "script:",
        "view.time_zone:",
    ];

    let uncovered: Vec<&str> = dispatchable
        .iter()
        .map(String::as_str)
        .filter(|id| {
            !menu_ids.contains(*id) && !catalog_ids.contains(id) && !palette_ids.contains(id)
        })
        .filter(|id| !dynamic_prefixes.iter().any(|p| id.starts_with(p)))
        .collect();
    assert!(
        uncovered.is_empty(),
        "these action ids have no menu leaf, no vix_action_catalog entry, and no \
         palette::COMMANDS entry, so F1 help/the palette would show the raw id: \
         {uncovered:#?}"
    );
}

#[test]
fn every_catalog_entry_has_no_menu_leaf_of_its_own() {
    // The catalog exists to cover the gap, not to shadow a menu label — an
    // id in both places would always show its menu label (App::action_title
    // tries the menu tree first), silently making the catalog entry dead
    // weight that could drift from the real title unnoticed.
    let menu_ids = menu_action_ids();
    let shadowed: Vec<&str> = vix_action_catalog::CATALOG
        .iter()
        .map(|a| a.id)
        .filter(|id| menu_ids.contains(*id))
        .collect();
    assert!(
        shadowed.is_empty(),
        "these vix_action_catalog ids already have a menu leaf, so the catalog \
         entry is unreachable dead weight: {shadowed:#?}"
    );
}

#[test]
fn no_catalog_entry_duplicates_a_palette_commands_entry() {
    // `App::catalog_palette_entries` (src/app/command_palette.rs) already
    // filters this out at runtime — this test guards the *reason* it has
    // to: `palette::COMMANDS` isn't purely menu-derived (T147 found
    // `nav.goto_workspace_symbol` there with no menu leaf), so a new
    // COMMANDS entry can silently start shadowing a catalog one, showing
    // the same action twice under two different labels in the palette.
    let shadowed: Vec<&str> = vix_action_catalog::CATALOG
        .iter()
        .map(|a| a.id)
        .filter(|id| vix_palette::COMMANDS.iter().any(|(_, action)| action == id))
        .collect();
    assert!(
        shadowed.is_empty(),
        "these vix_action_catalog ids are already in palette::COMMANDS — the \
         palette would show them twice under two different labels: {shadowed:#?}"
    );
}

#[test]
fn every_catalog_title_key_is_in_the_locale_catalog() {
    let root = workspace_root();
    let yaml = std::fs::read_to_string(root.join("locales/app.yml")).expect("locales/app.yml");
    let map: std::collections::BTreeMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&yaml).expect("locales/app.yml parses as YAML");

    let missing: Vec<&str> = vix_action_catalog::CATALOG
        .iter()
        .map(|a| a.title)
        .filter(|key| !map.contains_key(*key))
        .collect();
    assert!(
        missing.is_empty(),
        "these vix_action_catalog title keys are not in locales/app.yml: {missing:#?}"
    );
}
