//! `vix_oci::run` against a fake engine: a shell script that records its
//! arguments and standard input. No real engine is ever started.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use vix_oci::{OciError, argv};

fn fake(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("docker");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-oci-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn run_returns_stdout_and_passes_argv_untouched() {
    let dir = scratch("argv");
    let program = fake(&dir, "for a in \"$@\"; do echo \"[$a]\"; done");
    let out = vix_oci::run(&program, &["rm".into(), "a b; echo pwned".into()], None).unwrap();
    assert_eq!(out, "[rm]\n[a b; echo pwned]\n");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn login_password_travels_on_stdin_only() {
    let dir = scratch("login");
    let program = fake(&dir, "echo \"args:$*\"; echo \"stdin:$(cat)\"");
    let args = argv::login("ghcr.io", "octocat").unwrap();
    let out = vix_oci::run(&program, &args, Some("s3cret")).unwrap();
    assert!(out.contains("stdin:s3cret"));
    assert!(!out.lines().next().unwrap().contains("s3cret"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_failing_engine_reports_code_and_stderr() {
    let dir = scratch("fail");
    let program = fake(&dir, "echo 'no such container' >&2; exit 7");
    match vix_oci::run(&program, &["stop".into(), "x".into()], None) {
        Err(OciError::Failed { code, stderr }) => {
            assert_eq!(code, Some(7));
            assert_eq!(stderr, "no such container");
        }
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_missing_program_is_not_installed() {
    let r = vix_oci::run(std::path::Path::new("/nonexistent/docker"), &[], None);
    assert!(matches!(r, Err(OciError::NotInstalled)));
}

#[test]
fn listing_through_the_fake_parses() {
    let dir = scratch("list");
    let program = fake(
        &dir,
        "echo '{\"ID\":\"abc\",\"Names\":\"web\",\"State\":\"running\"}'",
    );
    let out = vix_oci::run(&program, &argv::list(argv::Kind::Container, true), None).unwrap();
    let c = vix_oci::parse::containers(&out).unwrap();
    assert_eq!(c[0].name, "web");
    std::fs::remove_dir_all(dir).ok();
}
