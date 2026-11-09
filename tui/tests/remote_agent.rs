//! T717: the remote agent end to end, with the real `vix` binary as the
//! agent and a fake `ssh` (a shell script named by explicit path that runs
//! the remote command locally, as `sshd` would). The fake gives the
//! "remote" side its own home, cache directory and a `PATH` without `vix`,
//! set in the script, never in this process. No real host is contacted.
//!
//! Covered: the checksum-verified upload into the per-user cache, the
//! handshake, the conformance suite over the agent, file operations and
//! batch stats without an `ssh` run each, a watch, a second connect reusing
//! the cached agent, and the App's change poll as one round trip with and
//! without the agent.
//!
//! The authority slot is process-wide, so this is its own test binary and
//! one `#[test]`.
#![cfg(all(unix, feature = "tasks"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

#[path = "support/script.rs"]
mod script;

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use vix::app::App;
use vix::settings::Settings;
use vix_authority::ssh::quote::sh_quote;
use vix_authority::ssh::{AgentMode, AgentStatus, SshAuthority, SshOptions, SshTarget};
use vix_authority::{Authority, conformance};

/// The real `vix` binary Cargo built for this test.
const VIX: &str = env!("CARGO_BIN_EXE_vix");

fn q(p: &Path) -> String {
    String::from_utf8(sh_quote(p.to_str().unwrap().as_bytes())).unwrap()
}

/// A fake `ssh` in `dir`: log each run (`-T`), skip the options and host,
/// and run the remote command with `/bin/sh -c` in an isolated "remote"
/// environment under `dir`.
fn fake_ssh(dir: &Path) -> PathBuf {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let script = format!(
        "#!/bin/sh\n\
         export HOME={home} XDG_CACHE_HOME={cache} PATH=/usr/bin:/bin:/usr/sbin:/sbin\n\
         printf '%s\\n' \"$1\" >> {log}\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in --) shift; break ;; -o|-p|-l) shift 2 ;; *) shift ;; esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        home = q(&home),
        cache = q(&dir.join("cache")),
        log = q(&dir.join("runs")),
    );
    let path = dir.join("fake-ssh");
    script::write(&path, &script).unwrap();
    path
}

/// How many times `ssh` has run.
fn runs(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("runs"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn options(dir: &Path, mode: AgentMode) -> SshOptions {
    SshOptions::default()
        .with_program(fake_ssh(dir))
        .with_multiplex(false)
        .with_agent(mode)
        .with_agent_binary(VIX)
}

fn target() -> SshTarget {
    SshTarget::parse("ssh://me@example.invalid/srv").unwrap()
}

/// Tick until no remote connect is running (at most a minute: the first
/// connect may upload a debug binary).
fn settle(app: &mut App) {
    let start = Instant::now();
    while app.remote_busy() && start.elapsed() < Duration::from_secs(60) {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.remote_busy(), "the connect finished");
}

#[test]
fn the_remote_agent_is_uploaded_verified_and_used() {
    let scratch = std::env::temp_dir().join(format!("vix-remote-agent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let scratch = scratch.canonicalize().unwrap();
    let cached = scratch
        .join("cache/vix/agent")
        .join(vix_authority::agent::VERSION)
        .join("vix");

    // Installed only: nothing there, nothing uploaded, shell scripts.
    let auth =
        SshAuthority::with_options(target(), options(&scratch, AgentMode::Installed)).unwrap();
    auth.connect(false).unwrap();
    assert_eq!(auth.agent_status(), AgentStatus::NotInstalled);
    assert!(!cached.exists());
    drop(auth);

    // Upload: the binary lands in the cache, checked, and runs as the agent.
    let auth = SshAuthority::with_options(target(), options(&scratch, AgentMode::Upload)).unwrap();
    auth.connect(false).unwrap();
    assert_eq!(
        auth.agent_status(),
        AgentStatus::Running {
            path: cached.clone(),
            uploaded: true
        }
    );
    assert!(auth.agent_running());
    assert_eq!(
        std::fs::read(&cached).unwrap(),
        std::fs::read(VIX).unwrap(),
        "the uploaded agent is this binary"
    );
    assert_eq!(
        std::fs::metadata(&cached).unwrap().permissions().mode() & 0o777,
        0o700
    );

    // The shared suite passes over the agent.
    let failures = conformance::run(&auth, &scratch.join("work"));
    assert_eq!(failures, Vec::<String>::new());

    // File operations and a 60-file batch stat start no ssh at all.
    let files: Vec<PathBuf> = (0..60).map(|i| scratch.join(format!("f{i}.txt"))).collect();
    let before = runs(&scratch);
    for f in &files {
        auth.write_atomic(f, b"x").unwrap();
    }
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    assert!(auth.metadata_batch(&refs).iter().all(Result::is_ok));
    assert_eq!(auth.read_to_string(&files[0]).unwrap(), "x");
    assert!(auth.read_dir(&scratch).unwrap().len() > 60);
    assert_eq!(runs(&scratch), before, "no ssh run per operation");

    // A watch reports a change made on the "remote" side.
    let watch = auth.watch(&files[..2], Duration::from_millis(100)).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    std::fs::write(&files[1], "changed").unwrap();
    let mut seen = Vec::new();
    for _ in 0..50 {
        if let Some(paths) = watch.next_timeout(Duration::from_millis(100)) {
            seen.extend(paths);
        }
        if seen.contains(&files[1]) {
            break;
        }
    }
    assert!(seen.contains(&files[1]), "{seen:?}");
    drop(watch);
    drop(auth);

    // A second connect finds the cached agent and doesn't upload again.
    let auth =
        SshAuthority::with_options(target(), options(&scratch, AgentMode::Installed)).unwrap();
    auth.connect(false).unwrap();
    assert_eq!(
        auth.agent_status(),
        AgentStatus::Running {
            path: cached,
            uploaded: false
        }
    );
    drop(auth);

    // The App: the change poll is one batch, so no ssh run with the agent
    // and exactly one without it, however many files are open.
    let proj = scratch.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let open: Vec<PathBuf> = (0..5).map(|i| proj.join(format!("m{i}.rs"))).collect();
    for f in &open {
        std::fs::write(f, "fn main() {}\n").unwrap();
    }
    for (mode, want) in [(AgentMode::Installed, 0), (AgentMode::Off, 1)] {
        let mut app = App::new(scratch.clone(), Settings::default())
            .with_session_path(scratch.join("session.toml"))
            .with_ssh_options(options(&scratch, mode));
        app.open_remote(&format!("ssh://me@example.invalid{}", proj.display()));
        settle(&mut app);
        assert_eq!(vix_authority::current().name(), "ssh");
        for f in &open {
            app.open_initial(f);
        }
        std::thread::sleep(Duration::from_millis(1100));
        let before = runs(&scratch);
        app.poll_file_changes();
        assert_eq!(runs(&scratch) - before, want, "{mode:?}");
    }

    vix_authority::install(Arc::new(vix_authority::LocalAuthority));
    let _ = std::fs::remove_dir_all(&scratch);
}
