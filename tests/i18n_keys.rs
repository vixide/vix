//! Repository invariants for the i18n catalog: every `t!` key resolves, and
//! every call site fills exactly the placeholders its string declares.
//!
//! `rust_i18n` returns the key itself when a translation is missing, so a typo
//! or a forgotten `locales/` entry does not fail the build — it ships,
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

/// Every `.yml` file under `locales/`, parsed and merged into one map — the
/// same thing `rust_i18n::i18n!` does at macro-expansion time (T148 split
/// the single `locales/app.yml` into one file per key namespace). Panics
/// (naming the offending key) if two files define the same key: rust-i18n's
/// own merge silently lets one win, which would hide a real mistake (a key
/// copy-pasted into the wrong namespace file, or moved without deleting its
/// old copy).
fn load_catalog(root: &Path) -> BTreeMap<String, serde_yaml::Value> {
    let dir = root.join("locales");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("locales/ directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yml"))
        .collect();
    paths.sort();

    let mut merged = BTreeMap::new();
    for path in paths {
        let yaml = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let map: BTreeMap<String, serde_yaml::Value> = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("{} does not parse as YAML: {e}", path.display()));
        for (key, value) in map {
            if key == "_version" {
                continue; // each file's own format marker, not a message
            }
            if merged.insert(key.clone(), value).is_some() {
                panic!(
                    "key `{key}` is defined in more than one locales/*.yml file \
                     (also seen in {})",
                    path.display()
                );
            }
        }
    }
    merged
}

/// Every top-level key defined across `locales/*.yml`.
fn catalog_keys(root: &Path) -> BTreeSet<String> {
    load_catalog(root).into_keys().collect()
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
        "these `t!` keys are not in locales/, so the UI would show the \
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
    let catalog = load_catalog(&root);

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

/// Every non-`en` locale code this branch guarantees a coverage floor for,
/// mapped to the minimum number of `locales/` entries that must
/// carry it — a ratchet (T148): bump a number up when a backfill pass
/// improves that locale's coverage, never down. `i18n_coverage_report_and_
/// floor` fails loudly on a real regression rather than shipping it
/// silently (`rust_i18n` falls back to `en` with no build error at all).
///
/// Only the 14 locales already near-universal as of T148 (`es`/`fr`/`de`/
/// `cy`/`ga`/`gd`/`pl`/`pt`/`ru`/`ar`/`hi`/`bn`/`zh`/`ja`, ~75% of all
/// entries each) get a real floor here. `tlh`/`sjn` (Klingon/Sindarin) and
/// `el`/`fa`/`id`/`it`/`ko`/`nl`/`th`/`tr`/`uk`/`vi` sit at single-digit
/// entry counts — an easter egg and an experimental seed batch
/// respectively, neither a real coverage commitment yet — so they're
/// floored at `0` (tracked in the table, never allowed to regress even
/// from near-nothing, but not held to the 14-locale bar until a future
/// task actually commits to them).
const LOCALE_FLOORS: &[(&str, usize)] = &[
    ("ar", 1705),
    ("bn", 1705),
    ("cy", 1699),
    ("de", 1699),
    ("el", 0),
    ("es", 1699),
    ("fa", 0),
    ("fr", 1699),
    ("ga", 1705),
    ("gd", 1705),
    ("hi", 1705),
    ("id", 0),
    ("it", 0),
    ("ja", 1705),
    ("ko", 0),
    ("nl", 0),
    ("pl", 1705),
    ("pt", 1705),
    ("ru", 1705),
    ("sjn", 0),
    ("th", 0),
    ("tlh", 0),
    ("tr", 0),
    ("uk", 0),
    ("vi", 0),
    ("zh", 1705),
];

/// Every catalog entry that's a real message (`load_catalog` already skips
/// `_version`; a malformed non-mapping entry is
/// `catalog_entries_are_maps_of_locale_to_string`'s job to report, so it's
/// just dropped here rather than double-reported).
fn catalog_entries(root: &Path) -> BTreeMap<String, serde_yaml::Mapping> {
    load_catalog(root)
        .into_iter()
        .filter_map(|(k, v)| match v {
            serde_yaml::Value::Mapping(m) => Some((k, m)),
            _ => None,
        })
        .collect()
}

/// How many `entries` carry each locale code seen anywhere in the catalog.
fn locale_coverage(entries: &BTreeMap<String, serde_yaml::Mapping>) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries.values() {
        for key in entry.keys() {
            if let Some(locale) = key.as_str() {
                *counts.entry(locale.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

#[test]
fn i18n_coverage_report_and_floor() {
    let root = workspace_root();
    let entries = catalog_entries(&root);
    let total = entries.len();
    assert!(total > 1000, "catalog looks truncated");
    let counts = locale_coverage(&entries);

    // A table locale-count/percentage table, most-covered first — visible
    // with `cargo test i18n_coverage_report_and_floor -- --nocapture`, or
    // in the failure output below if the ratchet catches a regression.
    let mut report: Vec<(&str, usize)> = counts.iter().map(|(l, &n)| (l.as_str(), n)).collect();
    report.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let table: String = report
        .iter()
        .map(|(locale, n)| {
            let pct = 100.0 * f64::from(u32::try_from(*n).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(total).unwrap_or(1));
            format!("  {locale:>4}  {n:>5} / {total} ({pct:5.1}%)\n")
        })
        .collect();
    println!("locales/ coverage ({total} entries):\n{table}");

    // Every locale actually present in the catalog must have a floor
    // entry — an untracked locale (a first key someone just added under a
    // brand new code) can only silently regress later if nothing here
    // ever asserts on it. `en` is exempt: it's the base language every
    // entry already must carry (`catalog_entries_are_maps_of_locale_to_
    // string` enforces that directly), not a coverage concern in the same
    // sense as a translation.
    let untracked: Vec<&str> = counts
        .keys()
        .map(String::as_str)
        .filter(|l| *l != "en" && !LOCALE_FLOORS.iter().any(|(f, _)| f == l))
        .collect();
    assert!(
        untracked.is_empty(),
        "these locale codes appear in locales/ but have no LOCALE_FLOORS \
         entry in tests/i18n_keys.rs: {untracked:#?}"
    );

    let regressed: Vec<String> = LOCALE_FLOORS
        .iter()
        .filter_map(|(locale, floor)| {
            let actual = counts.get(*locale).copied().unwrap_or(0);
            (actual < *floor).then(|| format!("{locale}: {actual} < floor {floor}"))
        })
        .collect();
    assert!(
        regressed.is_empty(),
        "locale coverage regressed below its floor in tests/i18n_keys.rs \
         (lower LOCALE_FLOORS only if the drop is deliberate, e.g. a removed \
         key, never to paper over a real regression): {regressed:#?}"
    );
}

#[test]
fn catalog_entries_are_maps_of_locale_to_string() {
    // A key whose value is a bare string (rather than `locale: text`) silently
    // fails to translate; catch the shape here.
    let root = workspace_root();
    let mut bad = Vec::new();
    for (key, value) in load_catalog(&root) {
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
