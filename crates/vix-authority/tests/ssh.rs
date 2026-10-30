//! The SSH authority (T712) against a fake `ssh`: a shell script, named by
//! explicit path, that runs the remote command locally through a login
//! shell's `-c`, the way `sshd` does. Nothing here contacts a real host;
//! `a_real_host_conforms` does only when `VIX_TEST_SSH_HOST` names one.
#![cfg(all(unix, feature = "ssh"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use vix_authority::ssh::quote::{envelope, quote_path, sh_quote};
use vix_authority::ssh::{AgentMode, AgentStatus, SshAuthority, SshOptions, SshTarget};
use vix_authority::{Authority, ProcessSpec, WriteConflict, conformance};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// A fresh, private scratch directory for one test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-ssh-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    cleanup(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A short control directory for `dir` (a socket path must fit in 104
/// bytes, and macOS's per-user temp directory is long).
fn control_dir(dir: &Path) -> PathBuf {
    Path::new("/tmp").join(dir.file_name().unwrap())
}

/// Remove a test's scratch and control directories.
fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(control_dir(dir));
}

/// Write a fake `ssh` into `dir` that logs its arguments to `dir/argv.log`
/// and runs the remote command with `login_shell -c`.
fn fake_ssh(dir: &Path, login_shell: &str) -> PathBuf {
    fake_ssh_with(dir, login_shell, "")
}

/// [`fake_ssh`] that runs `prelude` (shell text, e.g. `export HOME=…`)
/// before the remote command: the "remote" environment, set in the child
/// script rather than in this process.
fn fake_ssh_with(dir: &Path, login_shell: &str, prelude: &str) -> PathBuf {
    let log = dir.join("argv.log");
    let script = format!(
        "#!/bin/sh\n{prelude}\n\
         printf '%s\\n' \"$@\" >> {log}\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in\n\
             --) shift; break ;;\n\
             -o|-p|-l) shift 2 ;;\n\
             -O) printf 'control %s\\n' \"$2\" >> {log}; exit 0 ;;\n\
             *) shift ;;\n\
           esac\n\
         done\n\
         shift\n\
         exec {login_shell} -c \"$*\"\n",
        log = String::from_utf8(sh_quote(log.to_str().unwrap().as_bytes())).unwrap(),
    );
    let path = dir.join("fake-ssh");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn authority(dir: &Path, login_shell: &str) -> SshAuthority {
    let target = SshTarget::parse("ssh://me@example.invalid:2222/srv").unwrap();
    let control = control_dir(dir);
    let options = SshOptions::default()
        .with_program(fake_ssh(dir, login_shell))
        .with_control_dir(&control);
    SshAuthority::with_options(target, options).unwrap()
}

fn shell(name: &str) -> Option<String> {
    ["/bin", "/usr/bin", "/opt/homebrew/bin", "/usr/local/bin"]
        .iter()
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

#[test]
fn the_ssh_authority_conforms_against_a_fake_ssh() {
    let dir = scratch("conform");
    let auth = authority(&dir, "/bin/sh");
    let failures = conformance::run(&auth, &dir.join("work"));
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(auth.name(), "ssh");
    cleanup(&dir);
}

#[test]
fn the_command_line_carries_options_but_no_secrets() {
    let dir = scratch("argv");
    let auth = authority(&dir, "/bin/sh");
    let cmd = auth.base_command();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(auth.multiplexed());
    let ctl = control_dir(&dir);
    assert_eq!(
        args,
        [
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ControlMaster=auto",
            "-o",
            &format!("ControlPath={}/%C", ctl.display()),
            "-o",
            "ControlPersist=600",
            "-p",
            "2222",
            "-l",
            "me",
            "--",
            "example.invalid",
        ]
    );
    // The control directory was created private.
    let mode = std::fs::metadata(&ctl).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    // The remote command is one word of digits after the fixed decoder.
    let full = auth.script_command(b"echo secret-ish");
    let last = full
        .get_args()
        .last()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(last.starts_with("exec /bin/sh -c '"), "{last}");
    assert!(!last.contains("secret-ish"));
    cleanup(&dir);
}

#[test]
fn a_shared_control_directory_is_refused() {
    let dir = scratch("ctl-open");
    let ctl = dir.join("ctl");
    std::fs::create_dir(&ctl).unwrap();
    std::fs::set_permissions(&ctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let options = SshOptions::default()
        .with_program("/nonexistent/ssh")
        .with_control_dir(&ctl);
    let err = SshAuthority::with_options(SshTarget::new("h").unwrap(), options).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    // A symlink standing in for the directory is refused too.
    let link = dir.join("link");
    std::os::unix::fs::symlink(&ctl, &link).unwrap();
    std::fs::set_permissions(&ctl, std::fs::Permissions::from_mode(0o700)).unwrap();
    let options = SshOptions::default().with_control_dir(&link);
    assert!(SshAuthority::with_options(SshTarget::new("h").unwrap(), options).is_err());
    cleanup(&dir);
}

#[test]
fn a_process_spec_becomes_a_quoted_remote_script() {
    let dir = scratch("spec");
    let auth = authority(&dir, "/bin/sh");
    let cwd = dir.join("a dir with 'quotes' and $x");
    std::fs::create_dir_all(&cwd).unwrap();
    let spec = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf '%s|%s|%s|%s' \"$(pwd)\" \"$FOO\" \"${BAR-unset}\" \"$1\"")
        .arg("x")
        .arg("it's \"$HOME\" `id`; rm -rf /")
        .current_dir(&cwd)
        .env("FOO", "a 'b' $c\nd")
        .env_remove("BAR");
    let mut cmd = auth.command(&spec).unwrap();
    // Set on the local ssh process; the remote script must unset it.
    cmd.env("BAR", "local");
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let want = format!(
        "{}|a 'b' $c\nd|unset|it's \"$HOME\" `id`; rm -rf /",
        std::fs::canonicalize(&cwd).unwrap().display()
    );
    let got = String::from_utf8(out.stdout).unwrap();
    // `pwd` may report the logical path (macOS's /var vs /private/var).
    assert!(
        got == want || got.ends_with(want.split_once('|').unwrap().1),
        "{got:?}"
    );
    // Option-shaped programs and bad variable names are refused.
    assert!(auth.command(&ProcessSpec::new("-oops")).is_err());
    assert!(
        auth.command(&ProcessSpec::new("true").env("A B", "1"))
            .is_err()
    );
    cleanup(&dir);
}

#[test]
fn saves_write_through_links_keep_the_mode_and_report_conflicts() {
    let dir = scratch("save");
    let auth = authority(&dir, "/bin/sh");
    let real = dir.join("real.txt");
    std::fs::write(&real, "one\n").unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink("real.txt", &link).unwrap();
    let seen = auth.metadata(&link).unwrap();
    auth.write_atomic_if_unchanged(&link, b"two\n", Some(&seen))
        .unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "two\n");
    assert_eq!(
        std::fs::metadata(&real).unwrap().permissions().mode() & 0o777,
        0o640
    );
    // Someone else changes the file: the stale save is refused.
    std::fs::write(&real, "theirs, longer\n").unwrap();
    let err = auth
        .write_atomic_if_unchanged(&link, b"mine\n", Some(&seen))
        .unwrap_err();
    assert!(WriteConflict::is(&err), "{err}");
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "theirs, longer\n");
    // No temp files are left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".vix-save"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    cleanup(&dir);
}

#[test]
fn errors_keep_their_kinds() {
    use std::io::ErrorKind as K;
    let dir = scratch("errors");
    let auth = authority(&dir, "/bin/sh");
    let missing = dir.join("missing");
    assert_eq!(auth.read(&missing).unwrap_err().kind(), K::NotFound);
    assert_eq!(auth.metadata(&missing).unwrap_err().kind(), K::NotFound);
    assert_eq!(auth.read(&dir).unwrap_err().kind(), K::IsADirectory);
    assert_eq!(auth.read_dir(&missing).unwrap_err().kind(), K::NotFound);
    std::fs::write(dir.join("f"), "x").unwrap();
    assert_eq!(
        auth.read_dir(&dir.join("f")).unwrap_err().kind(),
        K::NotADirectory
    );
    assert_eq!(auth.remove_file(&dir).unwrap_err().kind(), K::IsADirectory);
    std::fs::create_dir_all(dir.join("full/x")).unwrap();
    std::fs::create_dir_all(dir.join("other")).unwrap();
    assert_eq!(
        auth.rename(&dir.join("other"), &dir.join("full"))
            .unwrap_err()
            .kind(),
        K::DirectoryNotEmpty
    );
    let guarded = auth.read_text_guarded(&dir, 10).unwrap_err();
    assert_eq!(
        vix_authority::TextReadError::from_io(&guarded),
        Some(vix_authority::TextReadError::NotRegular)
    );
    std::fs::write(dir.join("big"), "0123456789ab").unwrap();
    let big = auth.read_text_guarded(&dir.join("big"), 10).unwrap_err();
    assert_eq!(
        vix_authority::TextReadError::from_io(&big),
        Some(vix_authority::TextReadError::TooLarge)
    );
    std::fs::write(dir.join("bin"), b"a\0b").unwrap();
    let bin = auth.read_text_guarded(&dir.join("bin"), 10).unwrap_err();
    assert_eq!(
        vix_authority::TextReadError::from_io(&bin),
        Some(vix_authority::TextReadError::Binary)
    );
    assert_eq!(auth.which("-x"), None);
    assert_eq!(auth.which("a/b"), None);
    cleanup(&dir);
}

#[test]
fn odd_names_survive_listing_and_walking() {
    let dir = scratch("names");
    let auth = authority(&dir, "/bin/sh");
    let odd = [
        "sp ace",
        "new\nline",
        "it's",
        "$HOME",
        "-dash",
        "*",
        ".hidden",
        "é",
    ];
    for name in odd {
        std::fs::write(dir.join(name), name).unwrap();
    }
    let mut listed: Vec<String> = auth
        .read_dir(&dir)
        .unwrap()
        .iter()
        .map(|e| e.name().to_string_lossy().into_owned())
        .filter(|n| odd.contains(&n.as_str()))
        .collect();
    listed.sort();
    let mut want: Vec<String> = odd.iter().map(ToString::to_string).collect();
    want.sort();
    assert_eq!(listed, want);
    for name in odd {
        assert_eq!(auth.read_to_string(&dir.join(name)).unwrap(), name);
    }
    let walked = auth.walk_files(&dir, &vix_authority::WalkOptions::file_index(100));
    assert!(walked.contains(&dir.join("new\nline")));
    assert!(
        !walked.contains(&dir.join(".hidden")),
        "hidden files are skipped"
    );
    cleanup(&dir);
}

#[test]
fn every_login_shell_runs_the_same_script() {
    let tricky = "a 'b' \"c\" $d `e` \\f !g\nh #i %j";
    for name in ["sh", "dash", "bash", "zsh", "fish", "tcsh", "csh"] {
        let Some(login) = shell(name) else { continue };
        let dir = scratch(name);
        let auth = authority(&dir, &login);
        let file = dir.join(tricky);
        auth.write_atomic(&file, tricky.as_bytes())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), tricky, "{name}");
        assert_eq!(auth.read_to_string(&file).unwrap(), tricky, "{name}");
        cleanup(&dir);
    }
}

#[test]
fn the_decoder_runs_in_every_posix_sh() {
    // The envelope always `exec`s `/bin/sh`, which is dash on Debian and
    // Ubuntu, bash elsewhere, and ksh/mksh on some BSDs: try each one here.
    use std::os::unix::ffi::OsStrExt as _;
    let script = b"printf '%s|' \"$0\" 'it'\\''s' \"$(printf 'a\\tb')\"";
    let digits = envelope(script).rsplit(' ').next().unwrap().to_string();
    for name in ["sh", "dash", "bash", "ksh", "mksh"] {
        let Some(sh) = shell(name) else { continue };
        let out = Command::new(&sh)
            .arg("-c")
            .arg(std::ffi::OsStr::from_bytes(
                vix_authority::ssh::quote::DECODER.as_bytes(),
            ))
            .arg("vix")
            .arg(&digits)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "vix|it's|a\tb|",
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// How many `ssh` runs the fake has logged (each starts with `-T`).
fn ssh_runs(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| *l == "-T")
        .count()
}

#[test]
fn a_batch_stat_is_one_round_trip_without_the_agent() {
    let dir = scratch("batch");
    let auth = authority(&dir, "/bin/sh");
    auth.connect(false).unwrap();
    let files: Vec<PathBuf> = (0..30).map(|i| dir.join(format!("f {i}'s"))).collect();
    for f in files.iter().step_by(3) {
        std::fs::write(f, "abc").unwrap();
    }
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    let before = ssh_runs(&dir);
    let stats = auth.metadata_batch(&refs);
    assert_eq!(ssh_runs(&dir) - before, 1, "one ssh run for 30 stats");
    for (i, s) in stats.iter().enumerate() {
        if i % 3 == 0 {
            assert_eq!(s.as_ref().unwrap().len, 3);
        } else {
            assert_eq!(s.as_ref().unwrap_err().kind(), std::io::ErrorKind::NotFound);
        }
    }
    assert!(!auth.agent_running());
    assert_eq!(auth.agent_status(), AgentStatus::Off);
    cleanup(&dir);
}

/// A stand-in for a `vix` binary: answers `--version --json` with
/// `version` and nothing else (it is not an agent).
fn fake_vix(dir: &Path, version: &str) -> (PathBuf, Vec<u8>) {
    let body = format!(
        "#!/bin/sh\ncase $1 in --version) printf '{{\"name\":\"vix\",\"version\":\"%s\",\"capabilities\":[]}}\\n' '{version}' ;; *) exit 3 ;; esac\n"
    )
    .into_bytes();
    let path = dir.join(format!("fake-vix-{version}"));
    std::fs::write(&path, &body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (path, body)
}

/// An authority whose "remote" has its own home and cache under `dir`
/// and a `PATH` without any `vix`.
fn isolated_authority(dir: &Path, options: SshOptions) -> SshAuthority {
    let prelude = format!(
        "export HOME={home} XDG_CACHE_HOME={cache} PATH=/usr/bin:/bin:/usr/sbin:/sbin",
        home = String::from_utf8(sh_quote(dir.join("home").to_str().unwrap().as_bytes())).unwrap(),
        cache =
            String::from_utf8(sh_quote(dir.join("cache").to_str().unwrap().as_bytes())).unwrap(),
    );
    std::fs::create_dir_all(dir.join("home")).unwrap();
    let target = SshTarget::parse("ssh://me@example.invalid/srv").unwrap();
    let options = options
        .with_program(fake_ssh_with(dir, "/bin/sh", &prelude))
        .with_control_dir(control_dir(dir));
    SshAuthority::with_options(target, options).unwrap()
}

fn sha256_of(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Everything left in the agent cache directory.
fn cache_contents(dir: &Path) -> Vec<String> {
    let agent = dir
        .join("cache/vix/agent")
        .join(vix_authority::agent::VERSION);
    std::fs::read_dir(agent)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_upload_with_the_wrong_checksum_installs_nothing() {
    let dir = scratch("upload-bad");
    let auth = isolated_authority(&dir, SshOptions::default());
    let (_, body) = fake_vix(&dir, vix_authority::agent::VERSION);
    let wrong = sha256_of(b"something else");
    let err = auth.upload_agent(&body, &wrong).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
    assert!(
        cache_contents(&dir).is_empty(),
        "{:?}",
        cache_contents(&dir)
    );
    // Not a hash at all: refused before anything is sent.
    let before = ssh_runs(&dir);
    assert_eq!(
        auth.upload_agent(&body, "nope").unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert_eq!(ssh_runs(&dir), before);
    cleanup(&dir);
}

#[test]
fn an_upload_of_another_version_installs_nothing() {
    let dir = scratch("upload-other");
    let auth = isolated_authority(&dir, SshOptions::default());
    let (_, body) = fake_vix(&dir, "0.0.0-other");
    let err = auth.upload_agent(&body, &sha256_of(&body)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Unsupported, "{err}");
    assert_eq!(cache_contents(&dir), Vec::<String>::new());
    cleanup(&dir);
}

#[test]
fn a_verified_upload_lands_in_a_private_cache() {
    let dir = scratch("upload-ok");
    let auth = isolated_authority(&dir, SshOptions::default());
    let (_, body) = fake_vix(&dir, vix_authority::agent::VERSION);
    let path = auth.upload_agent(&body, &sha256_of(&body)).unwrap();
    let want = dir
        .join("cache/vix/agent")
        .join(vix_authority::agent::VERSION)
        .join("vix");
    assert_eq!(path, want);
    assert_eq!(std::fs::read(&want).unwrap(), body);
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&want), 0o700);
    assert_eq!(mode(want.parent().unwrap()), 0o700);
    assert_eq!(cache_contents(&dir), ["vix"], "no temp file left");
    cleanup(&dir);
}

#[test]
fn without_a_vix_there_the_shell_scripts_stay() {
    let dir = scratch("agent-none");
    let auth = isolated_authority(&dir, SshOptions::default().with_agent(AgentMode::Installed));
    auth.connect(false).unwrap();
    assert_eq!(auth.agent_status(), AgentStatus::NotInstalled);
    assert!(!auth.agent_running());
    // Everything still works.
    let failures = conformance::run(&auth, &dir.join("work"));
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(
        cache_contents(&dir),
        Vec::<String>::new(),
        "Installed never uploads"
    );
    cleanup(&dir);
}

#[test]
fn an_uploaded_binary_that_is_no_agent_falls_back() {
    let dir = scratch("agent-fake");
    let (fake, _) = fake_vix(&dir, vix_authority::agent::VERSION);
    let options = SshOptions::default()
        .with_agent(AgentMode::Upload)
        .with_agent_binary(&fake);
    let auth = isolated_authority(&dir, options);
    auth.connect(false).unwrap();
    // Uploaded and verified, but it doesn't speak the protocol.
    assert!(
        matches!(auth.agent_status(), AgentStatus::Unavailable(_)),
        "{:?}",
        auth.agent_status()
    );
    assert!(!auth.agent_running());
    assert_eq!(cache_contents(&dir), ["vix"]);
    assert!(auth.is_dir(&dir));
    // A second connect finds the cached copy (and again can't use it).
    let again = isolated_authority(&dir, SshOptions::default().with_agent(AgentMode::Installed));
    assert_eq!(again.start_agent(), auth.agent_status());
    cleanup(&dir);
}

#[test]
fn a_real_host_conforms() {
    // Opt-in only (T719 adds a CI job with a local sshd): never contact a
    // real host unless asked to.
    let Some(host) = std::env::var_os("VIX_TEST_SSH_HOST") else {
        return;
    };
    let host = host.to_string_lossy().into_owned();
    let target = if SshTarget::is_remote(&host) {
        SshTarget::parse(&host).unwrap()
    } else {
        SshTarget::parse(&format!("ssh://{host}")).unwrap()
    };
    let auth = SshAuthority::new(target).unwrap();
    auth.connect(false).unwrap();
    let scratch = PathBuf::from(format!("/tmp/vix-authority-ssh-{}", std::process::id()));
    let failures = conformance::run(&auth, &scratch);
    auth.disconnect();
    assert_eq!(failures, Vec::<String>::new());
}

/// Run `script` through `/bin/sh -c`, returning its stdout.
fn sh(script: &[u8]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(std::ffi::OsStr::from_bytes(script))
        .output()
        .unwrap();
    out.stdout
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Any word without a NUL comes back exactly once it's quoted.
    #[test]
    fn quoted_words_round_trip(word in proptest::collection::vec(1u8..=255, 0..64)) {
        let mut script = b"printf %s ".to_vec();
        script.extend(sh_quote(&word));
        prop_assert_eq!(sh(&script), word);
    }

    /// The same holds for paths, and for anything sent in the envelope.
    #[test]
    fn enveloped_scripts_round_trip(word in proptest::collection::vec(1u8..=255, 1..48)) {
        use std::os::unix::ffi::OsStrExt as _;
        let path = Path::new(std::ffi::OsStr::from_bytes(&word));
        let mut inner = b"printf %s ".to_vec();
        if word.starts_with(b"~") {
            // `~` expands remotely; compare only the literal case here.
            inner.extend(sh_quote(&word));
        } else {
            inner.extend(quote_path(path));
        }
        let out = sh(envelope(&inner).as_bytes());
        prop_assert_eq!(out, word);
    }
}
