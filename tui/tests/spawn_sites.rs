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
        "tui/src/app/doc_links.rs",
        "`open`/`xdg-open` for an LSP document link to a web or mail URL (T621; argv only, no shell)",
        "`tasks` feature (without it external links report that they could not be opened)",
    ),
    (
        "tui/src/app/remote_search.rs",
        "`rg`/`grep` for project-wide search under a non-local authority (T715; argv only, no shell)",
        "`tasks` feature (without it the scan stays in-process)",
    ),
    (
        "crates/vix-authority/src/lib.rs",
        "the `Authority::command` signature (T710): builds no process itself",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/process.rs",
        "`ProcessSpec::to_local_command`, the one place an authority builds a local `Command`",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/local.rs",
        "`LocalAuthority::command` (returns `Unsupported` without `process`)",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/fake.rs",
        "`FakeAuthority::command` (T711): defers to `LocalAuthority::command`",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/ssh/authority.rs",
        "`SshAuthority` (T712): every remote operation runs the system `ssh` (argv only, no local shell)",
        "`ssh` feature (implies `process`), enabled only by root `tasks`",
    ),
    (
        "crates/vix-authority/src/ssh/transport.rs",
        "the `Transport::shell` signature (T718): builds no process itself",
        "`ssh` feature (implies `process`), enabled only by root `tasks`",
    ),
    (
        "crates/vix-authority/src/container/exec.rs",
        "`ContainerAuthority` (T718): every operation runs `docker`/`podman`/`kubectl exec` or `wsl.exe --exec` (argv only, no local shell)",
        "`ssh` feature (implies `process`), enabled only by root `tasks`",
    ),
    (
        "crates/vix-authority/src/agent/client.rs",
        "`AgentClient::spawn` (T717): starts the remote agent's transport (`ssh … vix --remote-agent`) from a caller-built `Command`",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/agent/authority.rs",
        "`AgentAuthority::spawn` (T717, `process` only) and `command`, which defers to the authority given to `with_commands`",
        "`process` feature, enabled only by root `tasks` and each spawning crate's `process`",
    ),
    (
        "crates/vix-authority/src/ssh/agent_boot.rs",
        "`SshAuthority::start_agent` (T717): starts `vix --remote-agent` over the same `ssh` command line (argv only)",
        "`ssh` feature (implies `process`), enabled only by root `tasks`",
    ),
    (
        "crates/vix-http-client/src/aad.rs",
        "`az account get-access-token` for {{$aadToken}} (argv array, no shell)",
        "`http` feature (vix-http-client is an optional dependency of the root)",
    ),
    (
        "crates/vix-git/src/lib.rs",
        "git process runners",
        "`process` feature, enabled only by root `git`",
    ),
    (
        "crates/vix-oci/src/lib.rs",
        "container-engine CLI runner for the OC menu (argv only, no shell)",
        "`process` feature, enabled only by root `oc`",
    ),
    (
        "crates/vix-project-env/src/run.rs",
        "direnv/mise, only ever for a trusted workspace (T667)",
        "`process` feature, enabled only by root `tasks`",
    ),
    (
        "crates/vix-acp/src/process.rs",
        "the ACP coding agent (T635), started only for a trusted workspace from the user's `ai_agents` setting (argv, no shell)",
        "`process` feature, enabled only by root `ai`",
    ),
    (
        "crates/vix-acp/src/terminal.rs",
        "an ACP agent's terminal command (T635), started only after the user answered yes to a prompt naming the full command line, in a trusted workspace (argv, no shell)",
        "`process` feature, enabled only by root `ai`",
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
        "crates/vix-format-tool/src/run.rs",
        "the user's own Prettier (format document / on save)",
        "`process` feature, enabled only by root `tasks`",
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
    (
        "crates/vix-jupyter/src/kernel.rs",
        "launches a Jupyter kernel from its kernelspec argv (T734b; key only in the 0600 connection file)",
        "`process` feature, enabled only by root `jupyter` (opt-in, not in `full`)",
    ),
    (
        "crates/vix-jupyter/src/worker.rs",
        "relaunches the kernel on restart and kills it on shutdown (T734b)",
        "`process` feature, enabled only by root `jupyter` (opt-in, not in `full`)",
    ),
    (
        "crates/vix-im-matrix/src/net.rs",
        "the `vix-matrix` helper for Matrix accounts (T793): no arguments, no extra environment, the password only over stdin",
        "`net` feature, enabled only by root `matrix` (opt-in, not in `full`)",
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
