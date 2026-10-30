//! Repository invariant for the reference keymaps in the repo-root
//! `keymaps/` folder: every `*.json` file there really is a VS Code
//! keybindings file.
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
