//! The container authorities (T718) against a fake engine: a shell script,
//! named by explicit path (never `PATH` or environment edits in this
//! process), that logs its arguments, skips the engine's own options up to
//! the `/bin/sh` that runs inside, and runs that locally, in a scratch
//! directory standing in for the container. Nothing here starts a real
//! engine; `a_real_docker_container_conforms` does only when
//! `VIX_TEST_DOCKER` is set.
#![cfg(all(unix, feature = "ssh"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vix_authority::container::{ContainerAuthority, ContainerTarget, ExecKind, ExecOptions};
use vix_authority::remote::{ConnectOptions, RemoteTarget};
use vix_authority::ssh::quote::sh_quote;
use vix_authority::ssh::{AgentMode, AgentStatus};
use vix_authority::{Arg, Authority, ProcessSpec, conformance};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-container-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn q(path: &Path) -> String {
    String::from_utf8(sh_quote(path.to_str().unwrap().as_bytes())).unwrap()
}

/// A fake engine in `dir`: the "container" has its own home and cache
/// under `dir` and a `PATH` without `vix`; a container named `gone` is
/// answered the way Docker answers for a missing one.
fn fake_engine(dir: &Path) -> PathBuf {
    let script = format!(
        "#!/bin/sh\n\
         export HOME={home} XDG_CACHE_HOME={cache} PATH=/usr/bin:/bin:/usr/sbin:/sbin\n\
         printf '%s\\n' \"$@\" >> {log}\n\
         printf 'run\\n' >> {runs}\n\
         for a; do\n\
           [ \"$a\" = /bin/sh ] && break\n\
           if [ \"$a\" = gone ]; then\n\
             echo 'Error response from daemon: No such container: gone' >&2; exit 1\n\
           fi\n\
         done\n\
         while [ $# -gt 0 ] && [ \"$1\" != /bin/sh ]; do shift; done\n\
         exec \"$@\"\n",
        home = q(&dir.join("home")),
        cache = q(&dir.join("cache")),
        log = q(&dir.join("argv.log")),
        runs = q(&dir.join("runs.log")),
    );
    let path = dir.join("fake-engine");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn authority(dir: &Path, name: &str, options: ExecOptions) -> ContainerAuthority {
    let target = ContainerTarget::parse(name).unwrap();
    ContainerAuthority::new(target, options.with_program(fake_engine(dir)))
}

fn args_of(cmd: &std::process::Command) -> Vec<String> {
    cmd.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn runs(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("runs.log")).map_or(0, |s| s.lines().count())
}

#[test]
fn every_kind_conforms_against_a_fake_engine() {
    for (name, kind) in [
        ("docker://web/srv", "docker"),
        ("podman://me@web/srv", "podman"),
        ("k8s://dev/web/app/srv", "kubernetes"),
        ("wsl://Ubuntu/srv", "wsl"),
    ] {
        let dir = scratch("conform");
        let auth = authority(&dir, name, ExecOptions::default());
        auth.connect().unwrap();
        assert_eq!(auth.name(), kind);
        assert!(!auth.is_local());
        assert!(auth.shell_is_posix());
        let failures = conformance::run(&auth, &dir.join("work"));
        assert_eq!(failures, Vec::<String>::new(), "{name}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn each_engine_gets_its_own_exec_command_line() {
    let dir = scratch("argv");
    let prefix = |name: &str, tty: bool, env: &[&str]| {
        let auth = ContainerAuthority::new(
            ContainerTarget::parse(name).unwrap(),
            ExecOptions::default(),
        );
        let cmd = auth.transport().prefix(tty, env);
        (
            cmd.get_program().to_string_lossy().into_owned(),
            args_of(&cmd),
        )
    };
    assert_eq!(
        prefix("docker://me@web/x", false, &["FOO"]),
        (
            "docker".into(),
            vec!["exec", "-i", "-u", "me", "-e", "FOO", "web"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        prefix("podman://web/x", true, &[]).1,
        ["exec", "-i", "-t", "web"]
    );
    assert_eq!(
        prefix("k8s://dev/web/app/x", false, &[]),
        (
            "kubectl".into(),
            ["exec", "-i", "-n", "dev", "-c", "app", "web", "--"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        prefix("wsl://me@Ubuntu/x", false, &[]),
        (
            "wsl.exe".into(),
            ["-d", "Ubuntu", "-u", "me", "--exec"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        )
    );
    // The script itself travels encoded behind `/bin/sh -c <decoder>`.
    let auth = authority(&dir, "docker://web/x", ExecOptions::default());
    let args = args_of(&auth.script_command(b"echo 'secret path'\n"));
    let sh = args.iter().position(|a| a == "/bin/sh").unwrap();
    assert_eq!(args[sh + 1], "-c");
    assert_eq!(args[sh + 3], "vix");
    assert!(args.iter().all(|a| !a.contains("secret")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn docker_passes_environment_values_by_name_never_on_argv() {
    let dir = scratch("env");
    let auth = authority(&dir, "docker://web/x", ExecOptions::default());
    let cwd = dir.join("a 'proj'");
    std::fs::create_dir_all(&cwd).unwrap();
    let spec = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf '%s|%s|%s' \"$(pwd -P)\" \"$TOKEN\" \"${BAR-unset}\"")
        .current_dir(&cwd)
        .env("TOKEN", "first")
        .env("TOKEN", "s3cr3t value")
        .env("BAR", "x")
        .env_remove("BAR");
    let mut cmd = auth.command(&spec).unwrap();
    let args = args_of(&cmd);
    assert!(args.windows(2).any(|w| w == ["-e", "TOKEN"]), "{args:?}");
    assert!(!args.iter().any(|a| a.contains("s3cr3t")), "{args:?}");
    assert!(!args.iter().any(|a| a == "BAR"), "a removal is not passed");
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let want = format!(
        "{}|s3cr3t value|unset",
        std::fs::canonicalize(&cwd).unwrap().display()
    );
    assert_eq!(String::from_utf8(out.stdout).unwrap(), want);
    // kubectl has no `-e`: the script exports it instead.
    let k8s = authority(&dir, "k8s://dev/web/app/x", ExecOptions::default());
    let out = k8s
        .command(
            &ProcessSpec::new("sh")
                .arg("-c")
                .arg("printf %s \"$A\"")
                .env("A", "1 2"),
        )
        .unwrap()
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"1 2");
    assert!(auth.command(&ProcessSpec::new("-oops")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_terminal_is_a_login_shell_in_the_workspace() {
    let dir = scratch("terminal");
    let auth = authority(&dir, "docker://web/x", ExecOptions::default());
    let cwd = dir.join("proj");
    std::fs::create_dir_all(&cwd).unwrap();
    let spec = auth.terminal("/local/zsh", Some(&cwd)).unwrap();
    assert_eq!(spec.cwd(), None);
    let args: Vec<String> = spec
        .get_args()
        .iter()
        .map(|a| match a {
            Arg::Plain(a) | Arg::Raw(a) => a.to_string_lossy().into_owned(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(&args[..3], ["exec", "-i", "-t"], "{args:?}");
    let mut cmd = spec.to_local_command();
    cmd.env("SHELL", "/bin/sh")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"pwd -P\nexit\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim_end(),
        std::fs::canonicalize(&cwd).unwrap().display().to_string()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_container_is_a_lost_connection() {
    let dir = scratch("gone");
    let auth = authority(&dir, "docker://gone/x", ExecOptions::default());
    let err = auth.connect().unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotConnected, "{err}");
    assert!(err.to_string().contains("docker:gone"), "{err}");
    // A script's own failure is not.
    let ok = authority(&dir, "docker://web/x", ExecOptions::default());
    let err = ok.read(&dir.join("no such file")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_vix_inside_the_shell_scripts_stay() {
    let dir = scratch("agent-none");
    let auth = authority(
        &dir,
        "docker://web/x",
        ExecOptions::default().with_agent(AgentMode::Installed),
    );
    auth.connect().unwrap();
    assert_eq!(auth.agent_status(), AgentStatus::NotInstalled);
    assert!(!auth.agent_running());
    assert!(!dir.join("cache/vix").exists(), "Installed never uploads");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_upload_into_the_container_is_checksum_verified() {
    let dir = scratch("upload");
    // A stand-in `vix`: right version, but no agent.
    let fake = dir.join("fake-vix");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\ncase $1 in --version) printf '{{\"name\":\"vix\",\"version\":\"%s\",\"capabilities\":[]}}\\n' '{}' ;; *) exit 3 ;; esac\n",
            vix_authority::agent::VERSION
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let auth = authority(
        &dir,
        "podman://web/x",
        ExecOptions::default()
            .with_agent(AgentMode::Upload)
            .with_agent_binary(&fake),
    );
    auth.connect().unwrap();
    // Uploaded through `exec -i`'s stdin, verified, but not an agent.
    assert!(
        matches!(auth.agent_status(), AgentStatus::Unavailable(_)),
        "{:?}",
        auth.agent_status()
    );
    let landed = dir
        .join("cache/vix/agent")
        .join(vix_authority::agent::VERSION)
        .join("vix");
    assert_eq!(
        std::fs::read(&landed).unwrap(),
        std::fs::read(&fake).unwrap()
    );
    assert_eq!(
        std::fs::metadata(&landed).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // A wrong checksum installs nothing new.
    let err = auth.upload_agent(b"other", &"0".repeat(64)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remote_targets_connect_to_the_right_authority() {
    let dir = scratch("connect");
    let target = RemoteTarget::parse("docker://web/srv").unwrap();
    let options =
        ConnectOptions::default().with_exec(ExecOptions::default().with_program(fake_engine(&dir)));
    let before = runs(&dir);
    let c = target.connect(&options, false).unwrap();
    assert_eq!(c.authority.name(), "docker");
    assert_eq!(c.agent, AgentStatus::Off);
    assert!(runs(&dir) > before, "the connect probed the container");
    assert!(c.authority.is_dir(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn kinds_name_their_programs() {
    assert_eq!(ExecKind::Docker.program(), "docker");
    assert_eq!(ExecKind::Kubernetes.scheme(), "k8s");
}

/// Opt-in only: `VIX_TEST_DOCKER=<running container>` runs the suite in
/// that container; `VIX_TEST_DOCKER=1` starts a throwaway `alpine` one
/// (`docker run -d --rm alpine sleep 600`) and removes it afterwards.
/// Nothing runs without it.
#[test]
fn a_real_docker_container_conforms() {
    let Some(value) = std::env::var_os("VIX_TEST_DOCKER") else {
        return;
    };
    let value = value.to_string_lossy().into_owned();
    let (name, started) = if value == "1" {
        let out = std::process::Command::new("docker")
            .args(["run", "-d", "--rm", "alpine", "sleep", "600"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8(out.stdout).unwrap().trim().to_string(),
            true,
        )
    } else {
        (value, false)
    };
    let target = ContainerTarget::new(ExecKind::Docker, &name).unwrap();
    let auth = ContainerAuthority::new(target, ExecOptions::default());
    auth.connect().unwrap();
    let failures = conformance::run(
        &auth,
        Path::new(&format!("/tmp/vix-t718-{}", std::process::id())),
    );
    assert!(vix_authority::can_run(&auth, "sh"));
    if started {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &name])
            .output();
    }
    assert_eq!(failures, Vec::<String>::new());
}
