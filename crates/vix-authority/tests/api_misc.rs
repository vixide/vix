//! Public-API behaviour that the larger suites do not reach: plain data
//! types, the process spec, target parsing and display, the default trait
//! methods, the fake's confinement and trash naming, and the global slot.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use vix_authority::container::pick::wsl_list_args;
use vix_authority::container::{ContainerTarget, ExecKind};
use vix_authority::remote::RemoteTarget;
use vix_authority::ssh::{SshTarget, TargetError};
use vix_authority::{
    Arg, Authority, DirEntry, FakeAuthority, FileKind, FileStat, LocalAuthority, ProcessSpec,
    TextFile, TextStream, WalkOptions, can_run, same_version,
};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-api-misc-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn file_kind_from_std_and_stat_predicates() {
    let dir = scratch("kinds");
    std::fs::write(dir.join("f"), b"abc").unwrap();
    std::fs::create_dir(dir.join("d")).unwrap();
    let kind = |p: &Path| FileKind::from_std(std::fs::symlink_metadata(p).unwrap().file_type());
    assert_eq!(kind(&dir.join("f")), FileKind::File);
    assert_eq!(kind(&dir.join("d")), FileKind::Dir);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("f"), dir.join("l")).unwrap();
        assert_eq!(kind(&dir.join("l")), FileKind::Symlink);
        // A FIFO is neither file, directory nor link.
        let fifo = dir.join("fifo");
        let status = Command::new("mkfifo").arg(&fifo).status().unwrap();
        assert!(status.success());
        assert_eq!(kind(&fifo), FileKind::Other);
    }
    let link = FileStat::new(FileKind::Symlink, 0);
    assert!(link.is_symlink() && !link.is_file() && !link.is_dir());
    let file = FileStat::new(FileKind::File, 1);
    assert!(!file.is_symlink());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stat_builders_set_every_field() {
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(5);
    let s = FileStat::new(FileKind::File, 9)
        .with_modified(Some(t))
        .with_created(Some(t))
        .with_readonly(true)
        .with_mode(Some(0o644));
    assert_eq!(s.modified, Some(t));
    assert_eq!(s.created, Some(t));
    assert!(s.readonly);
    assert_eq!(s.mode, Some(0o644));
}

#[test]
fn dir_entry_accessors() {
    let e = DirEntry::new(PathBuf::from("/a/b.txt"), FileKind::Symlink, true);
    assert_eq!(e.name(), OsStr::new("b.txt"));
    assert_eq!(e.path(), Path::new("/a/b.txt"));
    assert!(e.is_symlink());
    assert!(e.is_dir);
    // A path with no file name (the root) names itself.
    let root = DirEntry::new(PathBuf::from("/"), FileKind::Dir, true);
    assert_eq!(root.name(), OsStr::new("/"));
    assert!(!root.is_symlink());
}

#[test]
fn raw_args_are_kept_in_order_and_become_plain_args_off_windows() {
    let spec = ProcessSpec::new("echo").arg("a").raw_arg("b c").args(["d"]);
    assert_eq!(
        spec.get_args(),
        [
            Arg::Plain("a".into()),
            Arg::Raw("b c".into()),
            Arg::Plain("d".into())
        ]
    );
    #[cfg(all(feature = "process", not(windows)))]
    {
        let cmd = spec.to_local_command();
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["a", "b c", "d"]);
    }
}

#[test]
fn local_stream_and_trash_free_paths() {
    let dir = scratch("local");
    let file = dir.join("t.txt");
    std::fs::write(&file, "hello\nworld\n").unwrap();
    let mut seen = String::new();
    let stream = LocalAuthority
        .stream_text_guarded(&file, 1024, &mut |s| seen.push_str(s))
        .unwrap();
    // The local authority streams: the text reached the sink.
    assert!(matches!(stream, TextStream::Streamed), "{stream:?}");
    assert_eq!(seen, "hello\nworld\n");
    // A directory link reads as a directory in `read_dir`, a file link not.
    #[cfg(unix)]
    {
        std::fs::create_dir(dir.join("sub")).unwrap();
        std::os::unix::fs::symlink(dir.join("sub"), dir.join("dl")).unwrap();
        std::os::unix::fs::symlink(&file, dir.join("fl")).unwrap();
        let entries = LocalAuthority.read_dir(&dir).unwrap();
        let find = |n: &str| entries.iter().find(|e| e.name() == OsStr::new(n)).unwrap();
        assert!(find("dl").is_symlink() && find("dl").is_dir);
        assert!(find("fl").is_symlink() && !find("fl").is_dir);
        assert!(find("sub").is_dir && !find("sub").is_symlink());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_default_stream_reads_whole_and_the_sink_stays_unused() {
    let dir = scratch("stream");
    let fake = FakeAuthority::new(&dir).unwrap();
    fake.write(Path::new("/t.txt"), b"abc").unwrap();
    let mut calls = 0;
    let stream = fake
        .stream_text_guarded(Path::new("/t.txt"), 1024, &mut |_| calls += 1)
        .unwrap();
    assert_eq!(calls, 0);
    match stream {
        TextStream::Whole(TextFile { text, .. }) => assert_eq!(text, "abc"),
        TextStream::Streamed => panic!("the default reads the whole file"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn same_version_tolerates_a_missing_time_on_one_side() {
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let timed = FileStat::new(FileKind::File, 4).with_modified(Some(t));
    let untimed = FileStat::new(FileKind::File, 4);
    assert!(same_version(Some(&timed), Some(&untimed)));
    assert!(same_version(Some(&untimed), Some(&timed)));
    let other =
        FileStat::new(FileKind::File, 4).with_modified(Some(t + Duration::from_millis(900)));
    assert!(
        same_version(Some(&timed), Some(&other)),
        "whole seconds only"
    );
    let later = FileStat::new(FileKind::File, 4).with_modified(Some(t + Duration::from_secs(1)));
    assert!(!same_version(Some(&timed), Some(&later)));
    assert!(!same_version(None, Some(&timed)));
}

#[test]
fn can_run_checks_paths_and_the_path_variable() {
    let dir = scratch("canrun");
    let fake = FakeAuthority::new(&dir).unwrap();
    fake.write(Path::new("/bin-x"), b"#!/bin/sh\n").unwrap();
    assert!(
        can_run(&fake, "/bin-x"),
        "a name with a slash is a file test"
    );
    assert!(!can_run(&fake, "/missing"));
    assert!(can_run(&fake, "sh"), "a bare name is looked up on PATH");
    assert!(!can_run(&fake, "vix-no-such-program-anywhere"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// An authority that is not local, delegating everything else to a fake, to
/// reach the trait's default `terminal`.
#[derive(Debug)]
struct NotLocal(FakeAuthority);

impl Authority for NotLocal {
    fn name(&self) -> &'static str {
        "not-local"
    }
    fn is_local(&self) -> bool {
        false
    }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        self.0.read(p)
    }
    fn read_limited(&self, p: &Path, l: u64) -> io::Result<Vec<u8>> {
        self.0.read_limited(p, l)
    }
    fn read_to_string(&self, p: &Path) -> io::Result<String> {
        self.0.read_to_string(p)
    }
    fn read_text_guarded(&self, p: &Path, m: u64) -> io::Result<TextFile> {
        self.0.read_text_guarded(p, m)
    }
    fn write(&self, p: &Path, d: &[u8]) -> io::Result<()> {
        self.0.write(p, d)
    }
    fn write_atomic(&self, p: &Path, d: &[u8]) -> io::Result<()> {
        self.0.write_atomic(p, d)
    }
    fn write_atomic_if_unchanged(
        &self,
        p: &Path,
        d: &[u8],
        e: Option<&FileStat>,
    ) -> io::Result<()> {
        self.0.write_atomic_if_unchanged(p, d, e)
    }
    fn write_text_in_file_encoding(&self, p: &Path, t: &str) -> io::Result<()> {
        self.0.write_text_in_file_encoding(p, t)
    }
    fn metadata(&self, p: &Path) -> io::Result<FileStat> {
        self.0.metadata(p)
    }
    fn symlink_metadata(&self, p: &Path) -> io::Result<FileStat> {
        self.0.symlink_metadata(p)
    }
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        self.0.canonicalize(p)
    }
    fn read_dir(&self, p: &Path) -> io::Result<Vec<DirEntry>> {
        self.0.read_dir(p)
    }
    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        self.0.create_dir_all(p)
    }
    fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.0.rename(a, b)
    }
    fn remove_file(&self, p: &Path) -> io::Result<()> {
        self.0.remove_file(p)
    }
    fn remove_path(&self, p: &Path) -> io::Result<()> {
        self.0.remove_path(p)
    }
    fn move_path(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.0.move_path(a, b)
    }
    fn copy_recursive(&self, a: &Path, b: &Path) -> io::Result<()> {
        self.0.copy_recursive(a, b)
    }
    fn trash(&self, p: &Path) -> io::Result<()> {
        self.0.trash(p)
    }
    fn is_unwritable(&self, p: &Path) -> bool {
        self.0.is_unwritable(p)
    }
    fn walk_files(&self, r: &Path, o: &WalkOptions) -> Vec<PathBuf> {
        self.0.walk_files(r, o)
    }
    fn which(&self, n: &str) -> Option<PathBuf> {
        self.0.which(n)
    }
    fn command(&self, s: &ProcessSpec) -> io::Result<Command> {
        self.0.command(s)
    }
}

#[test]
fn a_remote_authority_has_no_default_terminal() {
    let dir = scratch("term");
    let auth = NotLocal(FakeAuthority::new(&dir).unwrap());
    let err = auth.terminal("sh", None).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    assert_eq!(err.to_string(), "this workspace has no interactive shell");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_slot_hands_back_what_it_replaced() {
    let dir = scratch("slot");
    let fake: Arc<dyn Authority> = Arc::new(FakeAuthority::new(&dir).unwrap());
    let previous = vix_authority::install(Arc::clone(&fake));
    assert_eq!(vix_authority::current().name(), "fake");
    // Putting the previous one back returns the fake we installed.
    let ours = vix_authority::install(previous);
    assert_eq!(ours.name(), "fake");
    assert_eq!(vix_authority::current().name(), "local");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_fake_refuses_symlinks_that_leave_its_namespace() {
    let dir = scratch("confine");
    let outside = scratch("outside");
    std::fs::write(outside.join("secret"), b"x").unwrap();
    let fake = FakeAuthority::new(&dir).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, fake.backing().join("esc")).unwrap();
        // Following the link is refused.
        let e = fake.read(Path::new("/esc/secret")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        // So is naming a path whose parent is the link, without following it.
        for result in [
            fake.trash(Path::new("/esc/secret")),
            fake.move_path(Path::new("/esc/secret"), Path::new("/moved")),
            fake.copy_recursive(Path::new("/esc/secret"), Path::new("/copy")),
        ] {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        assert!(
            outside.join("secret").exists(),
            "nothing left the namespace"
        );
        assert!(!fake.trash_dir().join("secret").exists());
        // A walk rooted behind the link finds nothing.
        assert!(
            fake.walk_files(Path::new("/esc"), &WalkOptions::file_index(10))
                .is_empty(),
            "a walk behind the link finds nothing"
        );
        assert!(!fake.is_unwritable(Path::new("/esc/secret")));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn the_fake_numbers_files_trashed_under_the_same_name() {
    let dir = scratch("trash");
    let fake = FakeAuthority::new(&dir).unwrap();
    for body in ["one", "two", "three"] {
        fake.write(Path::new("/note.txt"), body.as_bytes()).unwrap();
        fake.trash(Path::new("/note.txt")).unwrap();
        assert!(!fake.exists(Path::new("/note.txt")));
    }
    let read = |n: &str| std::fs::read_to_string(fake.trash_dir().join(n)).unwrap();
    assert_eq!(read("note.txt"), "one");
    assert_eq!(read("note.txt.1"), "two");
    assert_eq!(read("note.txt.2"), "three");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(fake.trash_dir());
}

#[cfg(not(feature = "process"))]
#[test]
fn without_the_process_feature_no_authority_starts_a_process() {
    let dir = scratch("cmd-off");
    let fake = FakeAuthority::new(&dir).unwrap();
    let e = fake.command(&ProcessSpec::new("echo")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    let e = LocalAuthority
        .command(&ProcessSpec::new("echo"))
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "process")]
#[test]
fn the_fake_runs_commands_without_a_working_directory() {
    let dir = scratch("cmd");
    let fake = FakeAuthority::new(&dir).unwrap();
    let cmd = fake.command(&ProcessSpec::new("echo").arg("hi")).unwrap();
    assert_eq!(cmd.get_program(), "echo");
    assert!(cmd.get_current_dir().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn target_errors_have_distinct_messages() {
    let all = [
        TargetError::NotRemote,
        TargetError::MissingHost,
        TargetError::BadHost,
        TargetError::BadUser,
        TargetError::BadPort,
        TargetError::BadPath,
        TargetError::BadName,
    ];
    let messages: Vec<String> = all.iter().map(ToString::to_string).collect();
    for (i, m) in messages.iter().enumerate() {
        assert!(!m.is_empty(), "message {i} is empty");
        assert_eq!(messages.iter().filter(|o| *o == m).count(), 1, "{i}");
    }
    assert_eq!(TargetError::MissingHost.to_string(), "no host name");
    assert_eq!(TargetError::BadPort.to_string(), "invalid port");
}

#[test]
fn ssh_target_edge_cases() {
    // Port 0 means "default port".
    let t = SshTarget::new("box").unwrap().with_port(2200);
    assert_eq!(t.port, Some(2200));
    assert_eq!(t.with_port(0).port, None);
    // A bad percent escape in the user is a bad user, not a bad path.
    assert_eq!(
        SshTarget::parse("ssh://%zz@host/x"),
        Err(TargetError::BadUser)
    );
    // Junk after a bracketed IPv6 host that is not a port separator.
    assert_eq!(
        SshTarget::parse("ssh://[::1]x/p"),
        Err(TargetError::BadHost)
    );
    assert_eq!(
        SshTarget::parse("ssh://me@box:22/srv").unwrap().to_string(),
        "ssh://me@box:22/srv"
    );
}

#[test]
fn container_target_edge_cases() {
    assert_eq!(
        ContainerTarget::new(ExecKind::Kubernetes, "pod"),
        Err(TargetError::BadName)
    );
    let t = ContainerTarget::new(ExecKind::Docker, "web").unwrap();
    assert_eq!(t.to_string(), "docker://web/~");
    assert_eq!(wsl_list_args(), ["--list", "--quiet"]);
}

#[test]
fn remote_target_conversions_and_accessors() {
    let ssh = SshTarget::parse("ssh://me@box:22/srv").unwrap();
    let mut t = RemoteTarget::from(ssh);
    assert_eq!(t.path(), "/srv");
    assert_eq!(t.to_string(), "ssh://me@box:22/srv");
    t.set_path("/other".into());
    assert_eq!(t.path(), "/other");

    let c = ContainerTarget::new(ExecKind::Podman, "api").unwrap();
    let mut t = RemoteTarget::from(c);
    assert_eq!(t.path(), "~");
    assert_eq!(t.host(), "podman:api");
    assert_eq!(t.label(), "podman:api");
    t.set_path("/app".into());
    assert_eq!(t.path(), "/app");
    assert_eq!(t.to_string(), "podman://api/app");
    // A user-less, port-less SSH label is just the host.
    assert_eq!(RemoteTarget::parse("ssh://box/x").unwrap().label(), "box");
}

#[test]
fn the_local_terminal_without_a_directory_inherits_it() {
    let spec = LocalAuthority.terminal("/bin/sh", None).unwrap();
    assert_eq!(spec.program(), "/bin/sh");
    assert_eq!(spec.cwd(), None);
}

#[test]
fn a_walk_with_a_zero_limit_finds_nothing() {
    let dir = scratch("walk0");
    std::fs::write(dir.join("a.txt"), b"a").unwrap();
    let mut opts = WalkOptions::file_index(10);
    opts.limit = 0;
    assert!(
        LocalAuthority.walk_files(&dir, &opts).is_empty(),
        "a zero limit asks for nothing"
    );
    opts.limit = 10;
    assert_eq!(
        LocalAuthority.walk_files(&dir, &opts),
        vec![dir.join("a.txt")]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "ssh")]
#[test]
fn home_relative_paths_quote_to_the_home_variable() {
    use vix_authority::ssh::quote::quote_path;
    assert_eq!(quote_path(Path::new("~")), b"\"$HOME\"");
    assert_eq!(quote_path(Path::new("~/")), b"\"$HOME\"/");
    assert_eq!(quote_path(Path::new("~/a b")), b"\"$HOME\"/'a b'");
}
