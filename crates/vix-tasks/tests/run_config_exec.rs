//! Runs a rendered run-configuration plan under a real POSIX shell: a failed
//! step in a stage must stop its dependents and fail the whole plan (T817).
//! Lives here, not in `src/`, because it starts a child process.
#![cfg(unix)]

use vix_tasks::run_config::{Shell, parse, plan};

/// `(success, stdout, stderr)` of running config `c`, which depends on a
/// compound of two steps running `first` and `last`.
fn run(first: &str, last: &str) -> (bool, String, String) {
    let toml = format!(
        "[[config]]\nname=\"a\"\ncommand=\"{first}\"\n[[config]]\nname=\"b\"\ncommand=\"{last}\"\n\
         [[config]]\nname=\"g\"\ncompound=[\"a\",\"b\"]\n\
         [[config]]\nname=\"c\"\ncommand=\"echo DEP\"\nbefore=[\"g\"]\n"
    );
    let configs = parse(&toml).unwrap_or_else(|e| panic!("parse: {e}"));
    let p = plan(&configs, &[], "c").unwrap_or_else(|e| panic!("plan: {e}"));
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(p.shell_command(Shell::Posix))
        .output()
        .unwrap_or_else(|e| panic!("sh: {e}"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn all_children_succeeding_runs_the_dependent() {
    let (ok, out, _) = run("true", "true");
    assert!(ok && out.contains("DEP"));
}

#[test]
fn a_failed_first_child_blocks_the_dependent_and_is_named() {
    let (ok, out, err) = run("exit 7", "true");
    assert!(!ok && !out.contains("DEP"));
    assert!(err.contains("exit 7"), "{err}");
}

#[test]
fn a_failed_last_child_blocks_the_dependent() {
    let (ok, out, _) = run("true", "exit 7");
    assert!(!ok && !out.contains("DEP"));
}
