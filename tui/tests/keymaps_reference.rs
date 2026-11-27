//! Repository invariants for keymap reference material.
//!
//! 1. Every key table a documentation page copies from a generated
//!    `docs/reference/keybindings-*.md` page (marked with a
//!    `<!-- keys-from: … -->` comment) still matches it, row for row (T827).
//! 2. Every `*.json` file in the repo-root `keymaps/` folder really is a VS
//!    Code keybindings file.
//!
//! Those files are reference material (Vix's own keymaps are hand-written
//! tables in `vix-keybindings`), so nothing else reads them and a broken one
//! fails no other test. That is how all three `visual-studio-code-for-*`
//! files sat in the tree for months as saved GitHub HTML pages instead of
//! JSON. This test parses each one as JSONC (VS Code's format: JSON with
//! `//` comments) and checks its shape, so the next bad download fails the
//! gate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::path::{Path, PathBuf};

/// The repository root (this test's package lives in `tui/`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Every `index.md` under `dir`, recursively, in a stable order. (Each
/// docs directory's `README.md` is a twin of its `index.md`, so reading only
/// `index.md` sees every page once.)
fn index_pages(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read docs dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().is_some_and(|n| n == "index.md") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// The marker a page puts before a key table it copied from a generated
/// keybindings reference page.
const KEYS_FROM: &str = "<!-- keys-from: ";

#[test]
fn copied_key_tables_match_the_generated_keybindings_references() {
    let mut tables = 0_usize;
    for page in index_pages(&repo_root().join("docs")) {
        let text = std::fs::read_to_string(&page).expect("read page");
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let Some(rest) = line.trim().strip_prefix(KEYS_FROM) else {
                continue;
            };
            let target = rest.trim_end_matches("-->").trim();
            let reference_path = page.parent().expect("page dir").join(target);
            let reference = std::fs::read_to_string(&reference_path).unwrap_or_else(|e| {
                panic!(
                    "{}: keys-from names {target}, which does not open: {e}",
                    page.display()
                )
            });
            let known: std::collections::BTreeSet<&str> =
                reference.lines().filter(|l| l.starts_with("| `")).collect();
            let table: Vec<&str> = lines
                .iter()
                .skip(at)
                .skip(1)
                .skip_while(|l| l.trim().is_empty())
                .take_while(|l| l.starts_with('|'))
                .copied()
                .collect();
            assert!(
                table.len() > 2,
                "{}: no key table after the keys-from marker for {target}",
                page.display()
            );
            assert_eq!(
                table.first().copied(),
                Some("| Key | Action | Action ID |"),
                "{}: the table after keys-from must keep the reference's header",
                page.display()
            );
            for row in table.iter().skip(2) {
                assert!(
                    known.contains(row),
                    "{}: row {row} is not in {target}; copy it again from the \
                     regenerated reference (cargo run --example list_commands \
                     -p vix-tui -- --write)",
                    page.display()
                );
            }
            tables = tables.saturating_add(1);
        }
    }
    // The migration and comparison pages (T827) carry ten such tables; a
    // lower count means a marker was dropped and its table is no longer checked.
    assert!(
        tables >= 10,
        "expected at least 10 copied key tables, found {tables}"
    );
}

/// The repo-root `keymaps/` folder (this test's package lives in `tui/`).
fn keymaps_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("keymaps")
}

/// Every `.json` file under `dir`, recursively, in a stable order.
fn json_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read keymaps dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

#[test]
fn every_reference_keymap_is_a_vscode_keybindings_array() {
    let files = json_files(&keymaps_dir());
    assert!(
        files.len() >= 3,
        "expected the VS Code Linux/macOS/Windows keymaps, found {files:?}"
    );
    for path in files {
        let text = std::fs::read_to_string(&path).expect("read keymap");
        assert!(
            !text.trim_start().starts_with('<'),
            "{} is HTML (a saved web page?), not JSON",
            path.display()
        );
        let value: serde_json::Value =
            serde_json::from_str(&vix_oci::devcontainer::strip_jsonc(&text))
                .unwrap_or_else(|e| panic!("{} is not valid JSONC: {e}", path.display()));
        let entries = value
            .as_array()
            .unwrap_or_else(|| panic!("{} is not a JSON array", path.display()));
        assert!(
            entries.len() > 100,
            "{} has only {} bindings",
            path.display(),
            entries.len()
        );
        for entry in entries {
            for field in ["key", "command"] {
                assert!(
                    entry.get(field).is_some_and(serde_json::Value::is_string),
                    "{}: binding without a string `{field}`: {entry}",
                    path.display()
                );
            }
        }
    }
}
