//! Guard for the minimal build's security goal (T595, `spec/cargo-features`): the
//! only places that may start a child process are listed here, each behind a
//! capability feature or a Cargo feature that `--no-default-features` drops.
//! A new spawn site fails this test until it is added with a reason, which is the
//! moment to ask whether it belongs behind a feature.

use std::path::{Path, PathBuf};

/// `(file, why it may spawn, what gates it out of the minimal build)`.
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "tui/src/app.rs",
        "shell/argv runner for tasks, git and jj",
        "`tasks` / `git` / `jj` features (runtime-filtered by `src/capability.rs`)",
    ),
    (
        "crates/vix-git/src/lib.rs",
        "git process runners",
        "`process` feature, enabled only by root `git`",
    ),
    (
        "crates/vix-lsp/src/lib.rs",
        "language-server process",
        "`process` feature, enabled only by root `lsp`",
    ),
    (
        "crates/vix-dap/src/lib.rs",
        "debug-adapter process",
        "`process` feature, enabled only by root `dap`",
    ),
    (
        "crates/vix-doctor/src/lib.rs",
        "git and language-server probes",
        "`probe-git` / `probe-lsp`, enabled by `git` / `lsp`",
    ),
    (
        "crates/vix-spellcheck/src/lib.rs",
        "`hunspell -D` dictionary directory lookup",
        "`hunspell` feature",
    ),
    (
        "crates/vix-terminal/src/lib.rs",
        "integrated terminal PTY",
        "optional dep of root `terminal` feature",
    ),
    (
        "crates/vix-db/src/secret.rs",
        "OS keychain helper (SQL passwords)",
        "optional dep of root `sql` feature",
    ),
    (
        "crates/vix-db/src/tunnel.rs",
        "ssh tunnel for DB connections",
        "optional dep of root `sql` feature",
    ),
    (
        "crates/vix-ai-core/src/secret.rs",
        "OS keychain helper (AI keys)",
        "optional dep of root `ai` feature",
    ),
];

/// Text that means "this file can start a process". Thread builders
/// (`thread::Builder::spawn`) are deliberately not matched.
const MARKERS: &[&str] = &[
    "Command::new",
    "process::Command",
    "use std::process",
    "portable_pty",
    "Stdio::",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn only_allowlisted_files_can_start_a_child_process() {
    // Paths are repository-relative (this test's package lives in `tui/`).
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut files = Vec::new();
    rust_files(&root.join("tui/src"), &mut files);
    for e in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        rust_files(&e.path().join("src"), &mut files);
    }
    let mut offenders = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&f).unwrap();
        let spawns = text.lines().any(|l| {
            let l = l.trim_start();
            !l.starts_with("//") && MARKERS.iter().any(|m| l.contains(m))
        });
        if spawns && !ALLOWED.iter().any(|(a, _, _)| *a == rel) {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "new child-process spawn site(s): {offenders:?}. Gate each behind a capability \
         feature, then add it to ALLOWED in tui/tests/spawn_sites.rs with the reason."
    );
}

#[test]
fn every_allowlisted_file_still_spawns() {
    // Paths are repository-relative (this test's package lives in `tui/`).
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    for (rel, _, _) in ALLOWED {
        let text = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|_| panic!("{rel} is allowlisted but missing"));
        assert!(
            MARKERS.iter().any(|m| text.contains(m)),
            "{rel} no longer spawns; drop it from ALLOWED so the list stays honest"
        );
    }
}
