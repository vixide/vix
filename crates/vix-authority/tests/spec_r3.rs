//! Round-3 tests for the pure, always-compiled parts: the process spec's
//! accessors, the container kinds, shell quoting and the SSH envelope.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use vix_authority::container::{ContainerTarget, ExecKind};
use vix_authority::ssh::quote;
use vix_authority::{Arg, ProcessSpec};

#[test]
fn process_spec_records_everything_it_is_given() {
    let spec = ProcessSpec::new("tool")
        .arg("one")
        .args(["two", "three"])
        .raw_arg("\"quoted\"")
        .current_dir("/work")
        .env("KEEP", "1")
        .env_remove("DROP");
    assert_eq!(spec.program(), OsStr::new("tool"));
    assert_eq!(
        spec.get_args(),
        &[
            Arg::Plain("one".into()),
            Arg::Plain("two".into()),
            Arg::Plain("three".into()),
            Arg::Raw("\"quoted\"".into()),
        ]
    );
    assert_eq!(spec.cwd(), Some(Path::new("/work")));
    assert_eq!(
        spec.envs(),
        &[
            (OsString::from("KEEP"), Some(OsString::from("1"))),
            (OsString::from("DROP"), None),
        ]
    );
    let moved = spec.clone().with_cwd(Some(PathBuf::from("/elsewhere")));
    assert_eq!(moved.cwd(), Some(Path::new("/elsewhere")));
    assert_eq!(moved.get_args(), spec.get_args(), "only the cwd changes");
    assert_eq!(moved.with_cwd(None).cwd(), None, "the cwd can be cleared");
}

#[test]
fn every_exec_kind_names_its_scheme_program_and_label() {
    let table = [
        (ExecKind::Docker, "docker", "docker", "docker"),
        (ExecKind::Podman, "podman", "podman", "podman"),
        (ExecKind::Kubernetes, "k8s", "kubectl", "kubernetes"),
        (ExecKind::Wsl, "wsl", "wsl.exe", "wsl"),
    ];
    assert_eq!(ExecKind::ALL.len(), table.len());
    for (kind, scheme, program, name) in table {
        assert_eq!(kind.scheme(), scheme);
        assert_eq!(kind.program(), program);
        assert_eq!(kind.name(), name);
        assert!(ExecKind::ALL.contains(&kind));
        assert!(ContainerTarget::is_container(&format!("{scheme}://x")));
    }
    assert!(!ContainerTarget::is_container("ssh://host"));
}

#[test]
fn container_urls_round_trip_for_every_kind() {
    for url in [
        "docker://app/srv/www",
        "podman://me@db/var/lib",
        "k8s://prod/web-1/main/etc",
        "wsl://root@Ubuntu/home/me",
    ] {
        let target = ContainerTarget::parse(url).unwrap();
        assert_eq!(target.to_url(), url, "{url}");
        assert_eq!(ContainerTarget::parse(&target.to_url()).unwrap(), target);
    }
    let kube = ContainerTarget::parse("k8s://prod/web-1/main").unwrap();
    assert_eq!(kube.path, "~");
    assert_eq!(kube.label(), "k8s:prod/web-1/main");
    assert_eq!(
        ContainerTarget::parse("wsl://me@Ubuntu").unwrap().label(),
        "wsl:me@Ubuntu"
    );
}

#[test]
fn shell_quoting_keeps_every_word_one_literal_word() {
    assert_eq!(quote::sh_quote(b"plain"), b"'plain'");
    assert_eq!(quote::sh_quote(b"it's"), b"'it'\\''s'");
    assert_eq!(quote::sh_quote_str("a b"), "'a b'");
    assert_eq!(quote::glob_escape("a*b?[c]\\"), "a\\*b\\?\\[c]\\\\");
    // `~` and `~/rest` expand against the remote home; others are quoted whole.
    assert_eq!(quote::quote_path(Path::new("~")), b"\"$HOME\"");
    assert_eq!(quote::quote_path(Path::new("~/")), b"\"$HOME\"/");
    assert_eq!(quote::quote_path(Path::new("~/a b")), b"\"$HOME\"/'a b'");
    assert_eq!(quote::quote_path(Path::new("/etc/x")), b"'/etc/x'");
    assert_eq!(
        quote::bytes_path(&quote::path_bytes(Path::new("/a/b"))),
        Path::new("/a/b")
    );
    assert_eq!(quote::os_bytes(OsStr::new("é")), "é".as_bytes());
}

#[test]
fn envelope_and_argv_carry_the_script_as_octal_digits_only() {
    let script = b"echo 'hi'; exit 3";
    let env = quote::envelope(script);
    let digits = script.iter().fold(String::new(), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{b:03o}");
        acc
    });
    assert_eq!(
        env,
        format!("exec /bin/sh -c '{}' vix {digits}", quote::DECODER)
    );
    let argv = quote::sh_argv(script);
    assert_eq!(argv[0], "/bin/sh");
    assert_eq!(argv[1], "-c");
    assert_eq!(argv[2], quote::DECODER);
    assert_eq!(argv[3], "vix");
    assert_eq!(argv[4], digits);
    assert!(argv[4].bytes().all(|b| (b'0'..=b'7').contains(&b)));
}
