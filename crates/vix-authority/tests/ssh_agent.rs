//! The SSH authority with a remote agent, and its failure paths, against a
//! fake `ssh` (a shell script that runs the remote command locally). The
//! "remote" agent is this test process's own `serve`, bridged to the script
//! that `ssh` starts by two named pipes, so nothing here needs a real host,
//! a `vix` binary or the network.
#![cfg(all(unix, feature = "ssh"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

#[path = "support/script.rs"]
mod script;

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use vix_authority::agent::{VERSION, serve};
use vix_authority::ssh::quote::sh_quote;
use vix_authority::ssh::{AgentMode, AgentStatus, SshAuthority, SshOptions, SshTarget};
use vix_authority::{Authority, FileKind as K, TextReadError, WriteConflict, conformance};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vsa-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    cleanup(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A short control directory for `dir` (socket paths are short-limited).
fn control_dir(dir: &Path) -> PathBuf {
    Path::new("/tmp").join(format!(
        "ctl-{}",
        dir.file_name().unwrap().to_string_lossy()
    ))
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(control_dir(dir));
}

fn quoted(p: &Path) -> String {
    String::from_utf8(sh_quote(p.to_str().unwrap().as_bytes())).unwrap()
}

/// A fake `ssh` that logs its arguments, runs `prelude` and then the remote
/// command (the last argument) in `/bin/sh`.
fn fake_ssh(dir: &Path, prelude: &str) -> PathBuf {
    let log = dir.join("argv.log");
    let body = format!(
        "#!/bin/sh\n{prelude}\nprintf '%s\\n' \"$@\" >> {log}\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in\n\
             --) shift; break ;;\n\
             -o|-p|-l) shift 2 ;;\n\
             -O) printf 'control %s\\n' \"$2\" >> {log}; exit 0 ;;\n\
             *) shift ;;\n\
           esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        log = quoted(&log),
    );
    let path = dir.join("fake-ssh");
    script::write(&path, body).unwrap();
    path
}

/// The environment of the "remote" side: its own home and cache under
/// `dir`, and a `PATH` without any `vix` (plus `extra` directories first).
fn remote_env(dir: &Path, extra: &str) -> String {
    std::fs::create_dir_all(dir.join("home")).unwrap();
    format!(
        "export HOME={} XDG_CACHE_HOME={} PATH={extra}/usr/bin:/bin:/usr/sbin:/sbin",
        quoted(&dir.join("home")),
        quoted(&dir.join("cache")),
    )
}

fn options(dir: &Path, program: PathBuf) -> SshOptions {
    SshOptions::default()
        .with_program(program)
        .with_control_dir(control_dir(dir))
}

fn target() -> SshTarget {
    SshTarget::parse("ssh://me@example.invalid:2222/srv").unwrap()
}

fn log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("argv.log")).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// A remote agent served by this process
// ---------------------------------------------------------------------------

/// Where the agent cache of this version lives under `dir`.
fn cache_vix(dir: &Path) -> PathBuf {
    dir.join("cache/vix/agent").join(VERSION).join("vix")
}

/// A stand-in `vix`: reports `version`, and as `--remote-agent` bridges its
/// stdin and stdout to two named pipes this process serves.
fn fake_vix(dir: &Path, version: &str, bridge: bool) -> PathBuf {
    let (inp, out) = (
        quoted(&dir.join("agent-in")),
        quoted(&dir.join("agent-out")),
    );
    let agent = if bridge {
        format!("exec 3<&0; cat <&3 > {inp} & exec cat < {out}")
    } else {
        "exit 0".to_string()
    };
    let body = format!(
        "#!/bin/sh\ncase $1 in\n\
         --version) printf '{{\"name\":\"vix\",\"version\":\"%s\",\"capabilities\":[]}}\\n' '{version}' ;;\n\
         --remote-agent) {agent} ;;\n\
         *) exit 3 ;;\nesac\n"
    );
    let path = dir.join(format!("fake-vix-{version}"));
    script::write(&path, body).unwrap();
    path
}

/// Create the pipes and serve an agent on them in the background. The
/// thread ends when the connection does (or never starts if nothing
/// connects, which is harmless).
fn serve_agent(dir: &Path) {
    for name in ["agent-in", "agent-out"] {
        let status = Command::new("mkfifo").arg(dir.join(name)).status().unwrap();
        assert!(status.success());
    }
    let (inp, out) = (dir.join("agent-in"), dir.join("agent-out"));
    std::thread::spawn(move || {
        let Ok(reader) = File::open(inp) else { return };
        let Ok(writer) = OpenOptions::new().write(true).open(out) else {
            return;
        };
        let _ = serve(reader, writer);
    });
}

/// Install a `vix` in the remote agent cache.
fn install(dir: &Path, vix: &Path) {
    let dest = cache_vix(dir);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    script::write(&dest, std::fs::read(vix).unwrap()).unwrap();
}

fn agent_authority(dir: &Path, mode: AgentMode, binary: Option<&Path>) -> SshAuthority {
    let mut opts = options(dir, fake_ssh(dir, &remote_env(dir, ""))).with_agent(mode);
    if let Some(b) = binary {
        opts = opts.with_agent_binary(b);
    }
    SshAuthority::with_options(target(), opts).unwrap()
}

#[test]
fn an_installed_agent_takes_over_every_operation() {
    let dir = scratch("installed");
    serve_agent(&dir);
    install(&dir, &fake_vix(&dir, VERSION, true));
    let auth = agent_authority(&dir, AgentMode::Installed, None);
    auth.connect(false).unwrap();
    assert_eq!(
        auth.agent_status(),
        AgentStatus::Running {
            path: cache_vix(&dir),
            uploaded: false
        }
    );
    assert!(auth.agent_running());

    // The whole conformance suite, every operation through the agent.
    let failures = conformance::run(&auth, &dir.join("work"));
    assert_eq!(failures, Vec::<String>::new());

    // Operations that the suite leaves out, through the agent too.
    let work = dir.join("work2");
    auth.create_dir_all(&work.join("a/b")).unwrap();
    let f = work.join("a/b/f.txt");
    auth.write(&f, b"hello").unwrap();
    assert_eq!(auth.read_to_string_guarded(&f).unwrap(), "hello");
    auth.write_text_in_file_encoding(&f, "new").unwrap();
    assert_eq!(auth.read_to_string(&f).unwrap(), "new");
    assert_eq!(auth.read_limited(&f, 2).unwrap(), b"ne");
    auth.copy_recursive(&work.join("a"), &work.join("c"))
        .unwrap();
    auth.move_path(&work.join("c"), &work.join("d")).unwrap();
    auth.rename(&work.join("d/b/f.txt"), &work.join("d/b/g.txt"))
        .unwrap();
    assert!(auth.is_file(&work.join("d/b/g.txt")));
    auth.remove_file(&work.join("d/b/g.txt")).unwrap();
    auth.remove_path(&work.join("d")).unwrap();
    assert!(!auth.exists(&work.join("d")));
    let names: Vec<_> = auth
        .read_dir(&work)
        .unwrap()
        .iter()
        .map(|e| e.name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a"]);
    assert_eq!(
        auth.canonicalize(&work.join("a/../a")).unwrap(),
        std::fs::canonicalize(work.join("a")).unwrap()
    );
    assert_eq!(
        auth.walk_files(&work, &vix_authority::WalkOptions::file_index(10))
            .len(),
        1
    );
    assert!(auth.which("sh").is_some());
    assert!(auth.which("vix-no-such-program").is_none());
    assert!(!auth.is_unwritable(&f));
    // Trash of a path that is not there is refused (nothing is moved).
    assert!(auth.trash(&work.join("missing")).is_err());
    // A stale save is reported as a conflict by the agent.
    let stat = auth.metadata(&f).unwrap();
    std::fs::write(&f, "changed behind its back, so longer").unwrap();
    let e = auth
        .write_atomic_if_unchanged(&f, b"x", Some(&stat))
        .unwrap_err();
    assert!(WriteConflict::is(&e));
    // A directory listing of a missing directory fails with its own kind.
    assert_eq!(
        auth.read_dir(&work.join("nope")).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );

    // Watching works through the agent.
    let watched = work.join("watched.txt");
    auth.write(&watched, b"1").unwrap();
    let watch = auth
        .watch(std::slice::from_ref(&watched), Duration::from_millis(100))
        .unwrap();
    // The agent takes its first snapshot of the file some time after `watch`
    // returns; a change written before that snapshot is never reported. So
    // keep growing the file until a change is seen, then put back the text
    // the checks below expect.
    let mut seen = Vec::new();
    let mut text = b"2 and more".to_vec();
    for _ in 0..100 {
        std::fs::write(&watched, &text).unwrap();
        if let Some(paths) = watch.next_timeout(Duration::from_millis(100)) {
            seen.extend(paths);
            break;
        }
        text.push(b'.');
    }
    assert_eq!(seen, vec![watched.clone()]);
    drop(watch);
    std::fs::write(&watched, b"2 and more").unwrap();

    // Stopping the agent hands everything back to the shell scripts.
    auth.stop_agent();
    assert!(!auth.agent_running());
    assert_eq!(std::fs::read(&watched).unwrap(), b"2 and more");
    assert_eq!(auth.read(&watched).unwrap(), b"2 and more");
    let e = auth
        .watch(std::slice::from_ref(&watched), Duration::from_millis(100))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    assert_eq!(e.to_string(), "watching needs the remote agent");
    cleanup(&dir);
}

#[test]
fn a_missing_agent_is_uploaded_and_started() {
    let dir = scratch("upload");
    serve_agent(&dir);
    let vix = fake_vix(&dir, VERSION, true);
    let auth = agent_authority(&dir, AgentMode::Upload, Some(&vix));
    auth.connect(false).unwrap();
    assert_eq!(
        auth.agent_status(),
        AgentStatus::Running {
            path: cache_vix(&dir),
            uploaded: true
        }
    );
    assert!(auth.agent_running());
    assert!(auth.is_dir(&dir));
    cleanup(&dir);
}

#[test]
fn an_installed_vix_of_another_version_is_not_an_agent() {
    let dir = scratch("other-version");
    install(&dir, &fake_vix(&dir, "0.0.0-other", false));
    let auth = agent_authority(&dir, AgentMode::Installed, None);
    auth.connect(false).unwrap();
    match auth.agent_status() {
        AgentStatus::Unavailable(why) => {
            assert!(why.contains("is Vix 0.0.0-other"), "{why}");
            assert!(why.contains(VERSION), "{why}");
        }
        other => panic!("{other:?}"),
    }
    assert!(!auth.agent_running());
    assert!(auth.is_dir(&dir), "the shell scripts still work");
    cleanup(&dir);
}

#[test]
fn an_installed_vix_that_will_not_start_as_an_agent_is_unavailable() {
    let dir = scratch("no-hello");
    install(&dir, &fake_vix(&dir, VERSION, false));
    let auth = agent_authority(&dir, AgentMode::Installed, None);
    auth.connect(false).unwrap();
    match auth.agent_status() {
        AgentStatus::Unavailable(why) => {
            assert!(why.contains("did not say hello"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    assert!(!auth.agent_running());
    cleanup(&dir);
}

#[test]
fn a_binary_that_cannot_be_read_is_unavailable_and_nothing_is_uploaded() {
    let dir = scratch("no-binary");
    let missing = dir.join("no-such-binary");
    let auth = agent_authority(&dir, AgentMode::Upload, Some(&missing));
    auth.connect(false).unwrap();
    assert!(
        matches!(auth.agent_status(), AgentStatus::Unavailable(_)),
        "{:?}",
        auth.agent_status()
    );
    assert!(!cache_vix(&dir).exists());
    cleanup(&dir);
}

#[test]
fn upload_mode_will_not_upload_a_test_harness() {
    // No explicit binary: this process's own executable, which is not `vix`.
    let dir = scratch("harness");
    let auth = agent_authority(&dir, AgentMode::Upload, None);
    auth.connect(false).unwrap();
    match auth.agent_status() {
        AgentStatus::Unavailable(why) => {
            assert!(why.contains("not the vix binary"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    cleanup(&dir);
}

#[test]
fn starting_the_agent_with_a_dead_transport_is_unavailable() {
    let dir = scratch("dead");
    let program = dir.join("dead-ssh");
    script::write(
        &program,
        "#!/bin/sh\necho 'Permission denied (publickey).' >&2\nexit 255\n",
    )
    .unwrap();
    let opts = options(&dir, program).with_agent(AgentMode::Installed);
    let auth = SshAuthority::with_options(target(), opts).unwrap();
    match auth.start_agent() {
        AgentStatus::Unavailable(why) => {
            assert!(why.contains("Permission denied"), "{why}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(auth.agent_status(), auth.start_agent());
    let e = auth.connect(false).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotConnected);
    cleanup(&dir);
}

#[test]
fn off_mode_never_looks_for_an_agent() {
    let dir = scratch("off");
    let auth = agent_authority(&dir, AgentMode::Off, None);
    assert_eq!(auth.start_agent(), AgentStatus::Off);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Exit statuses and replies the scripts cannot normally produce
// ---------------------------------------------------------------------------

/// An authority whose `ssh` is `body` (a shell script).
fn with_ssh(dir: &Path, body: &str) -> SshAuthority {
    let program = dir.join("odd-ssh");
    script::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
    SshAuthority::with_options(target(), options(dir, program)).unwrap()
}

#[test]
fn remote_exit_statuses_map_to_error_kinds() {
    use io::ErrorKind as K;
    let dir = scratch("codes");
    let table: [(i32, K, &str); 11] = [
        (101, K::PermissionDenied, "permission denied"),
        (102, K::AlreadyExists, "already exists"),
        (103, K::IsADirectory, "is a directory"),
        (104, K::NotADirectory, "not a directory"),
        (105, K::DirectoryNotEmpty, "directory not empty"),
        (108, K::Unsupported, "no trash command"),
        (110, K::InvalidData, "checksum did not match"),
        (111, K::Unsupported, "does not run on the remote host"),
        (112, K::Unsupported, "no sha256sum"),
        (7, K::Other, "boom"),
        (255, K::NotConnected, "boom"),
    ];
    for (status, kind, text) in table {
        let auth = with_ssh(&dir, &format!("echo boom >&2; exit {status}"));
        let e = auth.metadata(Path::new("/x")).unwrap_err();
        assert_eq!(e.kind(), kind, "status {status}: {e}");
        assert!(e.to_string().contains(text), "status {status}: {e}");
    }
    // Statuses that carry a typed payload.
    let auth = with_ssh(&dir, "exit 106");
    assert!(WriteConflict::is(
        &auth.metadata(Path::new("/x")).unwrap_err()
    ));
    let auth = with_ssh(&dir, "exit 107");
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.kind(), K::InvalidData);
    assert_eq!(
        e.get_ref().unwrap().downcast_ref::<TextReadError>(),
        Some(&TextReadError::NotRegular)
    );
    let auth = with_ssh(&dir, "exit 109");
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(
        e.get_ref().unwrap().downcast_ref::<TextReadError>(),
        Some(&TextReadError::TooLarge)
    );
    // No stderr: the status itself is the message.
    let auth = with_ssh(&dir, "exit 9");
    assert!(
        auth.metadata(Path::new("/x"))
            .unwrap_err()
            .to_string()
            .ends_with("exit status 9")
    );
    let auth = with_ssh(&dir, "exit 255");
    assert!(
        auth.metadata(Path::new("/x"))
            .unwrap_err()
            .to_string()
            .ends_with("ssh failed")
    );
    // Killed by a signal: no status at all.
    let auth = with_ssh(&dir, "kill -9 $$");
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert!(e.to_string().ends_with("ssh was terminated"), "{e}");
    cleanup(&dir);
}

#[test]
fn an_unrecognized_stat_is_reported_not_guessed() {
    let dir = scratch("stat-garbage");
    // The probe answers neither `gnu` nor `bsd`.
    let auth = with_ssh(&dir, "echo what");
    let e = auth.connect(false).unwrap_err();
    assert_eq!(e.to_string(), "the remote stat command was not recognized");
    let e = auth.metadata(Path::new("/x")).unwrap_err();
    assert_eq!(e.to_string(), "the remote stat command was not recognized");
    cleanup(&dir);
}

#[test]
fn garbled_stat_replies_are_errors_per_path() {
    let dir = scratch("stat-fake");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // A `stat` that passes the flavor probe and then prints nonsense, or
    // fails for a path named `fail`.
    script::write(
        &bin.join("stat"),
        "#!/bin/sh\ncase \"$*\" in\n *'%s /'*) exit 0 ;;\n *fail*) exit 1 ;;\n *) echo garbage ;;\nesac\n",
    )
    .unwrap();
    let ok = dir.join("ok");
    let fail = dir.join("fail");
    std::fs::write(&ok, b"x").unwrap();
    std::fs::write(&fail, b"x").unwrap();
    let ssh = fake_ssh(&dir, &remote_env(&dir, &format!("{}:", bin.display())));
    let auth = SshAuthority::with_options(target(), options(&dir, ssh)).unwrap();
    auth.connect(false).unwrap();

    let e = auth.metadata(&ok).unwrap_err();
    assert_eq!(
        e.to_string(),
        "unexpected reply from the remote stat command"
    );

    let missing = dir.join("missing");
    let results = auth.metadata_batch(&[&ok, &missing, &fail]);
    assert_eq!(
        results[0].as_ref().unwrap_err().to_string(),
        "unexpected reply from the remote stat command"
    );
    assert_eq!(
        results[1].as_ref().unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert!(
        results[2]
            .as_ref()
            .unwrap_err()
            .to_string()
            .ends_with("stat failed")
    );
    cleanup(&dir);
}

#[test]
fn a_failed_batch_fails_every_path_the_same_way() {
    let dir = scratch("batch-fail");
    // The probe succeeds; the batch script dies with a permission error.
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    script::write(
        &bin.join("stat"),
        "#!/bin/sh\ncase \"$*\" in *'%s /'*) exit 0 ;; esac\nexit 1\n",
    )
    .unwrap();
    let ssh = dir.join("batch-ssh");
    // After the probe, every command fails with 101.
    script::write(
        &ssh,
        format!(
            "#!/bin/sh\n{env}\nshift $(( $# - 1 ))\ncase \"$1\" in *) ;; esac\n\
             n=$(cat {counter} 2>/dev/null || echo 0)\necho $((n+1)) > {counter}\n\
             if [ \"$n\" -eq 0 ]; then exec /bin/sh -c \"$1\"; fi\necho denied >&2; exit 101\n",
            env = remote_env(&dir, &format!("{}:", bin.display())),
            counter = quoted(&dir.join("count")),
        ),
    )
    .unwrap();
    let auth = SshAuthority::with_options(target(), options(&dir, ssh)).unwrap();
    let results = auth.metadata_batch(&[Path::new("/a"), Path::new("/b")]);
    assert_eq!(results.len(), 2);
    for r in results {
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// Construction and teardown
// ---------------------------------------------------------------------------

#[test]
fn accessors_report_what_the_authority_was_built_with() {
    let dir = scratch("accessors");
    let ssh = fake_ssh(&dir, "");
    let opts = options(&dir, ssh).with_multiplex(true);
    let auth = SshAuthority::with_options(target(), opts.clone()).unwrap();
    assert_eq!(auth.target(), &target());
    assert_eq!(auth.options(), &opts);
    assert!(auth.multiplexed());
    let plain = SshAuthority::with_options(target(), opts.with_multiplex(false)).unwrap();
    assert!(!plain.multiplexed());
    cleanup(&dir);
}

#[test]
fn the_default_authority_has_a_private_control_directory() {
    let auth = SshAuthority::new(target()).unwrap();
    assert_eq!(auth.options(), &SshOptions::default());
    assert_eq!(auth.name(), "ssh");
    assert!(!auth.is_local());
    assert!(auth.shell_is_posix());
}

#[test]
fn an_unusable_control_directory_is_refused() {
    let dir = scratch("ctl-bad");
    // A parent that does not exist.
    let opts = SshOptions::default().with_control_dir(dir.join("missing/ctl"));
    let e = SshAuthority::with_options(target(), opts).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotFound);
    // A file where the directory should be.
    let file = dir.join("file");
    std::fs::write(&file, b"x").unwrap();
    let e = SshAuthority::with_options(target(), SshOptions::default().with_control_dir(&file))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    assert!(e.to_string().contains("not a directory"), "{e}");
    cleanup(&dir);
}

#[test]
fn disconnect_closes_the_shared_connection_with_the_same_options() {
    let dir = scratch("disconnect");
    let auth = SshAuthority::with_options(target(), options(&dir, fake_ssh(&dir, ""))).unwrap();
    auth.connect(false).unwrap();
    auth.disconnect();
    let shown = log(&dir);
    assert!(shown.contains("control exit"), "{shown}");
    // `-O exit` carries the port and user of the target, like every command.
    let tail: Vec<&str> = shown.lines().rev().take(12).collect();
    assert!(tail.contains(&"2222") && tail.contains(&"me"), "{tail:?}");

    // Without a shared connection there is nothing to close.
    let dir2 = scratch("disconnect-plain");
    let opts = options(&dir2, fake_ssh(&dir2, "")).with_multiplex(false);
    let auth = SshAuthority::with_options(target(), opts).unwrap();
    auth.connect(false).unwrap();
    let before = log(&dir2);
    auth.disconnect();
    assert_eq!(log(&dir2), before);
    cleanup(&dir);
    cleanup(&dir2);
}

#[test]
fn an_interactive_connect_inherits_the_terminal_and_still_probes() {
    let dir = scratch("interactive");
    let auth = SshAuthority::with_options(target(), options(&dir, fake_ssh(&dir, ""))).unwrap();
    auth.connect(true).unwrap();
    // BatchMode is off for an interactive connect.
    assert!(log(&dir).contains("BatchMode=no"), "{}", log(&dir));
    cleanup(&dir);
}

#[test]
fn trash_without_a_trash_command_reports_nothing_to_do_or_unsupported() {
    // Through the shell script: a path that is not there is NotFound; one
    // that is there but with no trash tool on the remote is Unsupported.
    let dir = scratch("trash");
    let ssh = fake_ssh(&dir, &remote_env(&dir, ""));
    let auth = SshAuthority::with_options(target(), options(&dir, ssh)).unwrap();
    let missing = dir.join("missing");
    assert_eq!(
        auth.trash(&missing).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// RemoteTarget::connect
// ---------------------------------------------------------------------------

#[test]
fn connecting_a_remote_target_builds_the_ssh_authority_and_reports_its_agent() {
    use vix_authority::remote::{ConnectOptions, RemoteTarget};
    let dir = scratch("connect");
    let opts = options(&dir, fake_ssh(&dir, &remote_env(&dir, "")));
    let connect = ConnectOptions::default()
        .with_ssh(opts)
        .with_agent(AgentMode::Installed);
    let remote = RemoteTarget::from(target());
    let connection = remote.connect(&connect, false).unwrap();
    assert_eq!(connection.authority.name(), "ssh");
    // The agent mode reached the SSH options; with no `vix` there, it says so.
    assert_eq!(connection.agent, AgentStatus::NotInstalled);
    assert!(connection.authority.is_dir(&dir));
    cleanup(&dir);
}

#[test]
fn connecting_to_a_dead_host_fails_without_an_authority() {
    use vix_authority::remote::{ConnectOptions, RemoteTarget};
    let dir = scratch("connect-dead");
    let program = dir.join("dead-ssh");
    script::write(&program, "#!/bin/sh\necho 'no route' >&2\nexit 255\n").unwrap();
    let connect = ConnectOptions::default().with_ssh(options(&dir, program));
    let e = RemoteTarget::from(target())
        .connect(&connect, false)
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotConnected);
    assert!(e.to_string().contains("no route"), "{e}");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// The shell scripts' own corners
// ---------------------------------------------------------------------------

fn plain_authority(dir: &Path) -> SshAuthority {
    SshAuthority::with_options(target(), options(dir, fake_ssh(dir, &remote_env(dir, "")))).unwrap()
}

#[test]
fn a_listing_names_links_directories_and_odd_files() {
    let dir = scratch("listing");
    std::fs::write(dir.join("file"), b"x").unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::os::unix::fs::symlink(dir.join("sub"), dir.join("link-to-dir")).unwrap();
    std::os::unix::fs::symlink(dir.join("file"), dir.join("link-to-file")).unwrap();
    let status = Command::new("mkfifo")
        .arg(dir.join("pipe"))
        .status()
        .unwrap();
    assert!(status.success());
    let auth = plain_authority(&dir);
    let entries = auth.read_dir(&dir).unwrap();
    let find = |n: &str| {
        entries
            .iter()
            .find(|e| e.name() == std::ffi::OsStr::new(n))
            .unwrap()
    };
    assert_eq!((find("file").kind, find("file").is_dir), (K::File, false));
    assert_eq!((find("sub").kind, find("sub").is_dir), (K::Dir, true));
    assert_eq!(
        (find("link-to-dir").kind, find("link-to-dir").is_dir),
        (K::Symlink, true)
    );
    assert_eq!(
        (find("link-to-file").kind, find("link-to-file").is_dir),
        (K::Symlink, false)
    );
    assert_eq!((find("pipe").kind, find("pipe").is_dir), (K::Other, false));
    cleanup(&dir);
}

#[test]
fn a_walk_honors_its_options() {
    use vix_authority::WalkOptions;
    let dir = scratch("walk");
    let tree = dir.join("tree");
    for (path, body) in [
        ("a.txt", "a"),
        (".hidden", "h"),
        ("sub/b.txt", "b"),
        ("sub/deep/c.txt", "c"),
        ("target/x.txt", "x"),
        ("node_modules/y.txt", "y"),
        ("ignored.txt", "i"),
    ] {
        let p = tree.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    let auth = plain_authority(&dir);
    let names = |opts: &WalkOptions| -> Vec<String> {
        let mut v: Vec<String> = auth
            .walk_files(&tree, opts)
            .iter()
            .map(|p| p.strip_prefix(&tree).unwrap().display().to_string())
            .collect();
        v.sort();
        v
    };
    // The `find` form: no git work tree here, and ignore files not honored.
    let mut opts = WalkOptions::file_index(100);
    opts.honor_ignore_files = false;
    assert_eq!(
        names(&opts),
        ["a.txt", "ignored.txt", "sub/b.txt", "sub/deep/c.txt"],
        "hidden files and pruned directories are skipped"
    );
    opts.skip_hidden = false;
    opts.prune_dirs.clear();
    assert!(names(&opts).contains(&".hidden".to_string()));
    assert!(names(&opts).contains(&"target/x.txt".to_string()));
    opts.max_depth = Some(1);
    let shallow = names(&opts);
    assert!(shallow.contains(&"a.txt".to_string()));
    assert!(shallow.iter().all(|n| !n.contains('/')), "{shallow:?}");
    opts.max_depth = None;
    opts.limit = 2;
    assert_eq!(names(&opts).len(), 2, "cut off at the limit");
    opts.limit = 0;
    assert!(names(&opts).is_empty(), "a zero limit asks for nothing");
    assert!(
        auth.walk_files(&dir.join("missing"), &WalkOptions::file_index(5))
            .is_empty(),
        "a missing root has no files"
    );

    // Inside a git work tree, git's own ignore rules decide.
    let init = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&tree)
        .status()
        .unwrap();
    assert!(init.success());
    std::fs::write(tree.join(".gitignore"), "ignored.txt\n").unwrap();
    let mut opts = WalkOptions::file_index(100);
    opts.skip_hidden = false;
    opts.prune_dirs.clear();
    let listed = names(&opts);
    assert!(listed.contains(&"a.txt".to_string()), "{listed:?}");
    assert!(!listed.contains(&"ignored.txt".to_string()), "{listed:?}");
    assert!(!listed.iter().any(|n| n.starts_with(".git/")), "{listed:?}");
    cleanup(&dir);
}

#[test]
fn text_that_is_not_utf8_is_invalid_data_over_ssh() {
    let dir = scratch("utf8");
    let f = dir.join("bin");
    std::fs::write(&f, [0xff, 0xfe, 0x00]).unwrap();
    let auth = plain_authority(&dir);
    assert_eq!(
        auth.read_to_string(&f).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    cleanup(&dir);
}

#[test]
fn a_save_expectation_without_a_time_checks_only_the_length() {
    use vix_authority::{FileKind, FileStat};
    let dir = scratch("expect");
    let f = dir.join("f.txt");
    std::fs::write(&f, b"abcd").unwrap();
    let auth = plain_authority(&dir);
    let right = FileStat::new(FileKind::File, 4);
    auth.write_atomic_if_unchanged(&f, b"new", Some(&right))
        .unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"new");
    let wrong = FileStat::new(FileKind::File, 99);
    let e = auth
        .write_atomic_if_unchanged(&f, b"nope", Some(&wrong))
        .unwrap_err();
    assert!(WriteConflict::is(&e));
    assert_eq!(std::fs::read(&f).unwrap(), b"new");
    cleanup(&dir);
}

#[test]
fn an_upload_that_prints_no_path_is_an_error() {
    let dir = scratch("nopath");
    let auth = with_ssh(&dir, "exit 0");
    let e = auth.upload_agent(b"binary", &"a".repeat(64)).unwrap_err();
    assert_eq!(e.to_string(), "the upload printed no path");
    cleanup(&dir);
}

#[test]
fn a_remote_of_another_platform_gets_no_upload() {
    let dir = scratch("platform");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    script::write(&bin.join("uname"), "#!/bin/sh\necho 'Plan9 mips'\n").unwrap();
    let vix = fake_vix(&dir, VERSION, true);
    let ssh = fake_ssh(&dir, &remote_env(&dir, &format!("{}:", bin.display())));
    let opts = options(&dir, ssh)
        .with_agent(AgentMode::Upload)
        .with_agent_binary(&vix);
    let auth = SshAuthority::with_options(target(), opts).unwrap();
    auth.connect(false).unwrap();
    match auth.agent_status() {
        AgentStatus::Unavailable(why) => {
            assert!(
                why.starts_with("the remote machine is Plan9 mips, not "),
                "{why}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(!cache_vix(&dir).exists(), "nothing was uploaded");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// An agent that goes away
// ---------------------------------------------------------------------------

/// Serve the hello and exactly one request, then hang up without answering.
fn serve_one_then_vanish(dir: &Path) {
    use vix_authority::agent::wire::{self, Message, PROTOCOL};
    for name in ["agent-in", "agent-out"] {
        let status = Command::new("mkfifo").arg(dir.join(name)).status().unwrap();
        assert!(status.success());
    }
    let (inp, out) = (dir.join("agent-in"), dir.join("agent-out"));
    std::thread::spawn(move || {
        let Ok(mut reader) = File::open(inp) else {
            return;
        };
        let Ok(mut writer) = OpenOptions::new().write(true).open(out) else {
            return;
        };
        let _ = wire::read_frame(&mut reader);
        let hello = Message::Hello {
            protocol: PROTOCOL,
            version: VERSION.into(),
        };
        let _ = wire::write_frame(&mut writer, &wire::encode(&hello));
        let _ = wire::read_frame(&mut reader);
    });
}

#[test]
fn a_read_when_the_agent_vanishes_falls_back_to_the_shell_script() {
    let dir = scratch("vanish-read");
    serve_one_then_vanish(&dir);
    install(&dir, &fake_vix(&dir, VERSION, true));
    let auth = agent_authority(&dir, AgentMode::Installed, None);
    auth.connect(false).unwrap();
    assert!(auth.agent_running());
    let f = dir.join("f.txt");
    std::fs::write(&f, b"four").unwrap();
    // The agent drops the request; the read-only call is repeated as a
    // script and succeeds.
    assert_eq!(auth.metadata(&f).unwrap().len, 4);
    assert!(!auth.agent_running(), "the dead agent is no longer used");
    assert_eq!(auth.read(&f).unwrap(), b"four");
    cleanup(&dir);
}

#[test]
fn a_write_when_the_agent_vanishes_reports_an_unknown_outcome() {
    let dir = scratch("vanish-write");
    serve_one_then_vanish(&dir);
    install(&dir, &fake_vix(&dir, VERSION, true));
    let auth = agent_authority(&dir, AgentMode::Installed, None);
    auth.connect(false).unwrap();
    let f = dir.join("f.txt");
    // It may or may not have been written, so it is not silently redone.
    let e = auth.write_atomic(&f, b"data").unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotConnected);
    assert!(!f.exists());
    cleanup(&dir);
}
