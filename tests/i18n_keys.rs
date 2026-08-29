//! Repository invariants for the i18n catalog: every `t!` key resolves, and
//! every call site fills exactly the placeholders its string declares.
//!
//! `rust_i18n` returns the key itself when a translation is missing, so a typo
//! or a forgotten `locales/app.yml` entry does not fail the build — it ships,
//! and the user sees `confirm.delete` where a sentence should be. (That exact
//! bug reached the file explorer's delete dialog.) This test walks the
//! workspace source, collects every `t!("…")` key, and asserts the catalog
//! defines it, so the gate catches the next one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The workspace root (this test's package is the root package).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file in the workspace, skipping build output and VCS metadata.
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                // `.cargo` is skipped because GitLab CI points CARGO_HOME at
                // `$CI_PROJECT_DIR/.cargo` (for caching — see .gitlab-ci.yml),
                // landing the registry source cache, vendored dependency
                // source included, inside the workspace this walks.
                if name == "target" || name == ".git" || name == "fuzz" || name == ".cargo" {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// The i18n keys `text` passes to `t!`, ignoring doc comments (which show the
/// macro's *shape*, e.g. `t!("key")`, rather than naming a real key).
fn keys_in(text: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("//") {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("t!(") {
            // Skip `format!(`, `write!(`, … — only a bare `t!` counts.
            let is_macro_t = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            rest = &rest[at + 3..];
            if !is_macro_t {
                continue;
            }
            let after = rest.trim_start();
            let Some(body) = after.strip_prefix('"') else {
                continue; // a computed key; nothing static to check
            };
            if let Some(end) = body.find('"') {
                keys.insert(body[..end].to_string());
            }
        }
    }
    keys
}

/// Every top-level key defined in `locales/app.yml`.
fn catalog_keys(root: &Path) -> BTreeSet<String> {
    let yaml = std::fs::read_to_string(root.join("locales/app.yml")).expect("locales/app.yml");
    let map: BTreeMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&yaml).expect("locales/app.yml parses as YAML");
    map.into_keys().collect()
}

#[test]
fn every_translation_key_used_in_code_exists_in_the_catalog() {
    let root = workspace_root();
    let catalog = catalog_keys(&root);
    assert!(catalog.len() > 1000, "catalog looks truncated");

    let mut missing: Vec<(String, String)> = Vec::new();
    for path in rust_sources(&root) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for key in keys_in(&text) {
            if !catalog.contains(&key) {
                let shown = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                missing.push((key, shown));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "these `t!` keys are not in locales/app.yml, so the UI would show the \
         raw key: {missing:#?}"
    );
}

/// The `(key, argument names)` of every `t!` call in `text`, doc comments aside.
///
/// Only calls whose argument list closes on the same line are returned: the
/// argument list is read with a paren-balanced scan (so `path.display()` does
/// not end it early), and a call continued on the next line is skipped rather
/// than reported with half its arguments.
fn calls_in(text: &str) -> Vec<(String, BTreeSet<String>)> {
    let mut calls = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("t!(") {
            let is_macro_t = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            rest = &rest[at + 3..];
            if !is_macro_t {
                continue;
            }
            let after = rest.trim_start();
            let Some(body) = after.strip_prefix('"') else {
                continue; // a computed key; nothing static to check
            };
            let Some(end) = body.find('"') else { continue };
            let key = body[..end].to_string();

            // Walk to the `)` that closes this `t!(`, skipping string literals
            // and nested calls, and keep the text at this call's own depth —
            // `path.display()` contributes `path.display`, not a stray paren.
            let mut depth = 1usize;
            let mut in_string = false;
            let mut segment = String::new();
            let mut closed = false;
            let mut chars = body[end + 1..].chars().peekable();
            while let Some(c) = chars.next() {
                if in_string {
                    if c == '\\' {
                        chars.next();
                    } else if c == '"' {
                        in_string = false;
                    }
                    continue;
                }
                match c {
                    '"' => in_string = true,
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            closed = true;
                            break;
                        }
                    }
                    _ if depth == 1 => segment.push(c),
                    _ => {}
                }
            }
            // Each `name = value` argument, comma-separated at this depth.
            let names: BTreeSet<String> = segment
                .split(',')
                .filter_map(|arg| arg.split_once('='))
                .map(|(name, _)| name.trim().to_string())
                .filter(|name| {
                    !name.is_empty()
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !name.starts_with(|c: char| c.is_ascii_digit())
                })
                .collect();
            if closed {
                calls.push((key, names));
            }
        }
    }
    calls
}

/// The `%{name}` placeholders in a catalog entry's English text.
fn placeholders(entry: &serde_yaml::Value) -> BTreeSet<String> {
    let Some(text) = entry.get("en").and_then(serde_yaml::Value::as_str) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("%{") {
        rest = &rest[at + 2..];
        if let Some(end) = rest.find('}') {
            names.insert(rest[..end].to_string());
            rest = &rest[end + 1..];
        }
    }
    names
}

#[test]
fn every_call_site_fills_the_placeholders_its_string_declares() {
    // A `%{name}` nobody fills renders literally in the UI; an argument with no
    // placeholder is silently dropped. Both shipped: `status.locale` declared
    // `%{locale}`, which `t!` reserves for choosing the target locale, so the
    // status line read `Language: %{locale}`; and `msg.git_failed` was handed an
    // `error` it had nowhere to put, so git failures lost their reason.
    let root = workspace_root();
    let yaml = std::fs::read_to_string(root.join("locales/app.yml")).expect("locales/app.yml");
    let catalog: BTreeMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&yaml).expect("locales/app.yml parses as YAML");

    let mut problems: Vec<String> = Vec::new();
    for path in rust_sources(&root) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let shown = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for (key, mut args) in calls_in(&text) {
            let Some(entry) = catalog.get(&key) else {
                continue; // missing keys are the other test's business
            };
            // `locale` selects the target locale; it is never an interpolation.
            args.remove("locale");
            let declared = placeholders(entry);
            for missing in declared.difference(&args) {
                problems.push(format!("{shown}: {key}: nothing fills %{{{missing}}}"));
            }
            for extra in args.difference(&declared) {
                problems.push(format!("{shown}: {key}: `{extra}` has no placeholder"));
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(
        problems.is_empty(),
        "i18n interpolation mismatches: {problems:#?}"
    );
}

#[test]
fn catalog_entries_are_maps_of_locale_to_string() {
    // A key whose value is a bare string (rather than `locale: text`) silently
    // fails to translate; catch the shape here.
    let root = workspace_root();
    let yaml = std::fs::read_to_string(root.join("locales/app.yml")).expect("locales/app.yml");
    let map: BTreeMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&yaml).expect("locales/app.yml parses as YAML");
    let mut bad = Vec::new();
    for (key, value) in map {
        // `_version` is rust-i18n's own catalog-format marker, not a message.
        if key.starts_with('_') {
            continue;
        }
        match value {
            serde_yaml::Value::Mapping(m) => {
                if !m.contains_key(serde_yaml::Value::String("en".into())) {
                    bad.push(format!("{key}: no `en` fallback"));
                }
            }
            _ => bad.push(format!("{key}: not a locale map")),
        }
    }
    assert!(bad.is_empty(), "malformed catalog entries: {bad:#?}");
}
