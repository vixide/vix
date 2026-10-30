//! [`ShellAuthority`]: the workspace on another machine, reached through a
//! [`Transport`] and plain POSIX `sh` + coreutils on the other side, and
//! [`SshAuthority`] (T712), the one whose transport is the system `ssh`.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, BufRead as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};

use super::agent_boot::{AgentMode, AgentStatus};
use super::quote::{bytes_path, envelope, glob_escape, os_bytes, path_bytes, quote_path, sh_quote};
use super::stat::{StatFlavor, epoch_secs};
use super::target::SshTarget;
use super::transport::Transport;
use crate::agent::wire::{Expect as WireExpect, Op};
use crate::agent::{AgentClient, Fail, Watch};
use crate::{
    Arg, Authority, DirEntry, FileKind, FileStat, HARD_MAX_TEXT_BYTES, ProcessSpec, TextFile,
    TextReadError, WalkOptions, WriteConflict,
};

/// Exit statuses the remote scripts use to report an error kind; anything
/// else is reported with the remote's own error output.
pub(super) mod code {
    pub(super) const NOT_FOUND: i32 = 100;
    pub(super) const DENIED: i32 = 101;
    pub(super) const EXISTS: i32 = 102;
    pub(super) const IS_DIR: i32 = 103;
    pub(super) const NOT_DIR: i32 = 104;
    pub(super) const NOT_EMPTY: i32 = 105;
    pub(super) const CONFLICT: i32 = 106;
    pub(super) const NOT_REGULAR: i32 = 107;
    pub(super) const UNSUPPORTED: i32 = 108;
    pub(super) const TOO_LARGE: i32 = 109;
    /// The uploaded agent's SHA-256 did not match (T717).
    pub(in crate::ssh) const CHECKSUM: i32 = 110;
    /// The uploaded agent does not run, or reports another version.
    pub(in crate::ssh) const NOT_RUNNABLE: i32 = 111;
    /// No `sha256sum`, `shasum` or `openssl` to check an upload with.
    pub(in crate::ssh) const NO_HASH_TOOL: i32 = 112;
    /// `ssh` itself failed (connection, authentication, host key).
    pub(in crate::ssh) const SSH: i32 = 255;
}

/// How [`SshAuthority`] runs `ssh`. Everything else (keys, agent,
/// `ProxyJump`, host-key policy, `User`, `Port`) comes from the user's
/// `ssh_config`, which `ssh` reads as always.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SshOptions {
    /// The `ssh` program: a name looked up on `PATH`, or an explicit path.
    pub program: PathBuf,
    /// Share one connection between commands (`ControlMaster=auto`).
    /// Ignored on Windows, whose OpenSSH has no multiplexing.
    pub multiplex: bool,
    /// Where the control sockets live. `None` is a private per-user
    /// directory: `$XDG_RUNTIME_DIR/vix-ssh`, else `/tmp/vix-ssh-<uid>`.
    pub control_dir: Option<PathBuf>,
    /// How long an idle shared connection stays open (`ControlPersist`).
    pub control_persist_secs: u32,
    /// Never prompt (`BatchMode=yes`): a command that would need a password
    /// or a host-key answer fails instead of writing to the terminal under
    /// Vix's UI. [`SshAuthority::connect`] can prompt once, before the UI.
    pub batch_mode: bool,
    /// `ConnectTimeout`, in seconds.
    pub connect_timeout_secs: u32,
    /// Whether [`SshAuthority::connect`] starts the remote agent (T717).
    /// Off by default here; the App sets it from the `remote_agent`
    /// setting.
    pub agent: AgentMode,
    /// The binary [`AgentMode::Upload`] uploads. `None` is this process's
    /// own executable, when it is `vix`.
    pub agent_binary: Option<PathBuf>,
}

impl Default for SshOptions {
    fn default() -> Self {
        Self {
            program: PathBuf::from("ssh"),
            multiplex: cfg!(unix),
            control_dir: None,
            control_persist_secs: 600,
            batch_mode: true,
            connect_timeout_secs: 15,
            agent: AgentMode::Off,
            agent_binary: None,
        }
    }
}

impl SshOptions {
    /// These options with `program` as the `ssh` to run.
    #[must_use]
    pub fn with_program(mut self, program: impl Into<PathBuf>) -> Self {
        self.program = program.into();
        self
    }

    /// These options with connection sharing on or off.
    #[must_use]
    pub fn with_multiplex(mut self, multiplex: bool) -> Self {
        self.multiplex = multiplex;
        self
    }

    /// These options with control sockets in `dir`.
    #[must_use]
    pub fn with_control_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.control_dir = Some(dir.into());
        self
    }

    /// These options with the remote agent `mode` (T717).
    #[must_use]
    pub fn with_agent(mut self, mode: AgentMode) -> Self {
        self.agent = mode;
        self
    }

    /// These options uploading `binary` as the agent.
    #[must_use]
    pub fn with_agent_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.agent_binary = Some(binary.into());
        self
    }
}

/// A shell authority: the workspace on another machine (or in a container),
/// reached through a [`Transport`] that runs plain POSIX `sh` + coreutils
/// there (T712, T718).
///
/// Each operation runs one short `sh` script on the other side (`cat`,
/// `head`, `stat`, `find`, `mv`, …), so there is nothing to install there.
/// Paths are that machine's own absolute paths (`~` and `~/…` name its home
/// directory). Every path and argument is single-quoted, and the whole
/// script travels as octal digits behind a fixed decoder, so no login shell
/// or argv re-quoting can misread it. When the remote agent (T717) runs
/// there, file operations go to it instead.
///
/// [`SshAuthority`] is this over `ssh`;
/// [`crate::container::ContainerAuthority`] is this over `docker exec`,
/// `podman exec`, `kubectl exec` or `wsl.exe`.
#[derive(Debug)]
pub struct ShellAuthority<T: Transport> {
    transport: T,
    flavor: OnceLock<StatFlavor>,
    seq: AtomicU64,
    /// The running remote agent, if any (T717).
    agent: RwLock<Option<Arc<AgentClient>>>,
    agent_status: Mutex<AgentStatus>,
}

/// A remote machine as Vix's workspace, over the system `ssh` (T712): a
/// [`ShellAuthority`] whose transport is [`SshTransport`].
pub type SshAuthority = ShellAuthority<SshTransport>;

/// The `ssh` transport: one host, reached with the system `ssh` through a
/// shared `ControlMaster` connection.
#[derive(Debug)]
pub struct SshTransport {
    target: SshTarget,
    options: SshOptions,
    /// The control socket pattern (`<dir>/%C`), when multiplexing.
    control_path: Option<String>,
}

impl SshTransport {
    /// The `ssh` command up to the destination; `tty` asks for a remote
    /// pseudo-terminal (`-t`, the integrated terminal) instead of none
    /// (`-T`, every other operation).
    fn ssh_with(&self, batch: bool, tty: bool) -> Command {
        let mut cmd = Command::new(&self.options.program);
        cmd.arg(if tty { "-t" } else { "-T" });
        let mut opt = |o: String| {
            cmd.arg("-o").arg(o);
        };
        opt(format!("BatchMode={}", if batch { "yes" } else { "no" }));
        opt(format!(
            "ConnectTimeout={}",
            self.options.connect_timeout_secs
        ));
        if let Some(path) = &self.control_path {
            opt("ControlMaster=auto".into());
            opt(format!("ControlPath={path}"));
            opt(format!(
                "ControlPersist={}",
                self.options.control_persist_secs
            ));
        }
        if let Some(port) = self.target.port {
            cmd.arg("-p").arg(port.to_string());
        }
        if let Some(user) = &self.target.user {
            cmd.arg("-l").arg(user);
        }
        cmd.arg("--").arg(&self.target.host);
        cmd
    }
}

impl Transport for SshTransport {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn label(&self) -> String {
        self.target.host.clone()
    }

    fn shell(&self, script: &[u8], tty: bool, _env_names: &[&str]) -> Command {
        let mut cmd = self.ssh_with(self.options.batch_mode, tty);
        cmd.arg(envelope(script));
        cmd
    }

    /// `ssh`'s own failures (connection, authentication, host key) are 255.
    fn lost(&self, status: i32, _stderr: &str) -> bool {
        status == code::SSH
    }

    fn agent_mode(&self) -> AgentMode {
        self.options.agent
    }

    fn agent_binary(&self) -> Option<&Path> {
        self.options.agent_binary.as_deref()
    }
}

impl ShellAuthority<SshTransport> {
    /// An authority for `target`'s host, with default [`SshOptions`]. Nothing
    /// connects until the first operation (or [`SshAuthority::connect`]).
    ///
    /// # Errors
    /// The private control-socket directory could not be made safe.
    pub fn new(target: SshTarget) -> io::Result<Self> {
        Self::with_options(target, SshOptions::default())
    }

    /// An authority for `target`'s host, run as `options` says.
    ///
    /// # Errors
    /// The control-socket directory could not be created, or exists but is
    /// not a private (mode 0700) directory owned by this user.
    pub fn with_options(target: SshTarget, options: SshOptions) -> io::Result<Self> {
        let control_path = if options.multiplex && cfg!(unix) {
            let dir = match &options.control_dir {
                Some(dir) => dir.clone(),
                None => default_control_dir()?,
            };
            private_dir(&dir)?;
            control_pattern(&dir)
        } else {
            None
        };
        Ok(Self::over(SshTransport {
            target,
            options,
            control_path,
        }))
    }

    /// Whether commands share one connection. Off when
    /// [`SshOptions::multiplex`] is, on Windows, and when the control
    /// directory's path is too long (or oddly named) for a socket.
    #[must_use]
    pub fn multiplexed(&self) -> bool {
        self.transport.control_path.is_some()
    }

    /// The host this authority works on (its path is the workspace the
    /// caller opened; the authority itself is not tied to it).
    #[must_use]
    pub fn target(&self) -> &SshTarget {
        &self.transport.target
    }

    /// The `ssh` command up to and including the destination, with no remote
    /// command: `ssh -T -o … [-p PORT] [-l USER] -- HOST`. No secret is ever
    /// on it; authentication is `ssh`'s own (keys, agent, `SSH_ASKPASS`).
    #[must_use]
    pub fn base_command(&self) -> Command {
        self.transport
            .ssh_with(self.transport.options.batch_mode, false)
    }

    /// Open the shared connection and learn the remote `stat` flavor. With
    /// `interactive`, `ssh` may ask for a password, passphrase or host-key
    /// confirmation on the terminal, so call it before the UI takes over
    /// the screen; later commands reuse the connection without prompting.
    ///
    /// # Errors
    /// `ssh` failed to connect or authenticate (its own message), or the
    /// remote has no usable `sh`/`stat`.
    pub fn connect(&self, interactive: bool) -> io::Result<()> {
        let mut cmd = self.transport.ssh_with(!interactive, false);
        cmd.arg(envelope(super::stat::PROBE.as_bytes()));
        let out = if interactive {
            cmd.stdin(Stdio::inherit()).stderr(Stdio::inherit());
            cmd.output()?
        } else {
            cmd.stdin(Stdio::null()).output()?
        };
        let reply = finish(&self.transport, &out)?;
        self.finish_connect(&reply)
    }

    /// The options this authority runs `ssh` with.
    #[must_use]
    pub fn options(&self) -> &SshOptions {
        &self.transport.options
    }

    /// Close the shared connection (`ssh -O exit`), if there is one.
    pub fn disconnect(&self) {
        self.set_agent(None);
        let t = &self.transport;
        let Some(path) = &t.control_path else {
            return;
        };
        let mut cmd = Command::new(&t.options.program);
        cmd.arg("-o").arg(format!("ControlPath={path}"));
        if let Some(port) = t.target.port {
            cmd.arg("-p").arg(port.to_string());
        }
        if let Some(user) = &t.target.user {
            cmd.arg("-l").arg(user);
        }
        cmd.args(["-O", "exit", "--"]).arg(&t.target.host);
        let _ = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl<T: Transport> ShellAuthority<T> {
    /// A shell authority over `transport`. Nothing runs until the first
    /// operation.
    pub fn over(transport: T) -> Self {
        Self {
            transport,
            flavor: OnceLock::new(),
            seq: AtomicU64::new(0),
            agent: RwLock::new(None),
            agent_status: Mutex::new(AgentStatus::Off),
        }
    }

    /// The transport this authority runs its scripts through.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The command that runs `script` in the other side's `/bin/sh`.
    #[must_use]
    pub fn script_command(&self, script: &[u8]) -> Command {
        self.transport.shell(script, false, &[])
    }

    /// Learn the other side's `stat` flavor from a probe reply, then start
    /// the remote agent when the transport asks for it.
    fn finish_connect(&self, reply: &[u8]) -> io::Result<()> {
        let flavor = StatFlavor::from_probe(reply)
            .ok_or_else(|| io::Error::other("the remote stat command was not recognized"))?;
        let _ = self.flavor.set(flavor);
        if self.transport.agent_mode() != AgentMode::Off {
            self.start_agent();
        }
        Ok(())
    }

    /// Check that the other side answers and has a usable `sh` and `stat`,
    /// then start the remote agent when the transport's options ask for it
    /// (T717). Never prompts.
    ///
    /// # Errors
    /// The transport failed (`NotConnected`, with its message), or the other
    /// side has no usable `sh`/`stat`.
    pub fn probe(&self) -> io::Result<()> {
        let reply = self.run(super::stat::PROBE.as_bytes(), None)?;
        self.finish_connect(&reply)
    }

    /// What became of the remote agent (T717) on the last connect or
    /// [`ShellAuthority::start_agent`].
    #[must_use]
    pub fn agent_status(&self) -> AgentStatus {
        self.agent_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether file operations currently go through the remote agent.
    #[must_use]
    pub fn agent_running(&self) -> bool {
        self.agent().is_some()
    }

    /// Watch `paths` for changes on the remote side through the agent,
    /// checked every `interval` there (T717).
    ///
    /// # Errors
    /// `Unsupported` when no agent is running (the shell scripts can't
    /// watch), or the agent's refusal.
    pub fn watch(&self, paths: &[PathBuf], interval: std::time::Duration) -> io::Result<Watch> {
        match self.agent() {
            Some(a) => a.watch(paths, interval),
            None => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "watching needs the remote agent",
            )),
        }
    }

    pub(super) fn set_agent(&self, client: Option<AgentClient>) {
        *self.agent.write().unwrap_or_else(PoisonError::into_inner) = client.map(Arc::new);
    }

    pub(super) fn set_agent_status(&self, status: AgentStatus) {
        *self
            .agent_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = status;
    }

    /// Stop the remote agent, if one is running; the shell scripts take over.
    pub fn stop_agent(&self) {
        self.set_agent(None);
    }

    /// The running agent, if its connection is still up.
    fn agent(&self) -> Option<Arc<AgentClient>> {
        self.agent
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|a| a.is_alive())
            .map(Arc::clone)
    }

    /// Run `f` through the agent: `None` when there is none, or it can't do
    /// this one (the caller then runs its shell script).
    fn via<R>(&self, f: impl FnOnce(&AgentClient) -> Result<R, Fail>) -> Option<io::Result<R>> {
        let agent = self.agent()?;
        match f(&agent) {
            Ok(v) => Some(Ok(v)),
            Err(Fail::Io(e)) => Some(Err(e)),
            Err(Fail::Unavailable) => None,
        }
    }

    /// Run `script` remotely with `input` on its stdin; its stdout on
    /// success, or the error its exit status names.
    pub(super) fn run_script(&self, script: &[u8], input: Option<&[u8]>) -> io::Result<Vec<u8>> {
        self.run(script, input)
    }

    fn run(&self, script: &[u8], input: Option<&[u8]>) -> io::Result<Vec<u8>> {
        let mut cmd = self.script_command(script);
        cmd.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let out = std::thread::scope(|s| {
            if let (Some(mut w), Some(data)) = (stdin, input) {
                // A script that fails early closes its stdin; not an error.
                s.spawn(move || {
                    let _ = io::Write::write_all(&mut w, data);
                });
            }
            child.wait_with_output()
        })?;
        finish(&self.transport, &out)
    }

    /// The remote `stat` flavor, probed on first use.
    fn flavor(&self) -> io::Result<StatFlavor> {
        if let Some(f) = self.flavor.get() {
            return Ok(*f);
        }
        let reply = self.run(super::stat::PROBE.as_bytes(), None)?;
        let flavor = StatFlavor::from_probe(&reply)
            .ok_or_else(|| io::Error::other("the remote stat command was not recognized"))?;
        Ok(*self.flavor.get_or_init(|| flavor))
    }

    fn stat(&self, path: &Path, follow: bool) -> io::Result<FileStat> {
        let flavor = self.flavor()?;
        let test = if follow {
            "[ -e \"$p\" ] || exit 100\n"
        } else {
            "[ -e \"$p\" ] || [ -h \"$p\" ] || exit 100\n"
        };
        let script = Sh::new()
            .set("p", path)
            .t(test)
            .t("exec ")
            .t(flavor.command(follow))
            .t(" \"$p\"\n")
            .done();
        let out = self.run(&script, None)?;
        flavor
            .parse(&out)
            .ok_or_else(|| io::Error::other("unexpected reply from the remote stat command"))
    }

    /// A unique suffix for a temporary name.
    pub(super) fn save_token(&self) -> String {
        self.token()
    }

    /// A name no other save is using, for a temp file next to its target.
    fn token(&self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        format!(
            ".vix-save-{}-{nanos}-{}",
            std::process::id(),
            self.seq.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// The atomic write, with the conflict check `expected` asks for.
    fn write_atomic_checked(
        &self,
        path: &Path,
        data: &[u8],
        expected: Expect<'_>,
    ) -> io::Result<()> {
        let flavor = self.flavor()?;
        let mut sh = Sh::new()
            .set("p", path)
            .t(RESOLVE_LINKS)
            .t("[ -d \"$p\" ] && exit 103\n\
             d=$(dirname -- \"$p\")\n\
             [ -d \"$d\" ] || exit 100\n");
        match expected {
            Expect::Any => {}
            Expect::Absent => sh = sh.t("[ -e \"$p\" ] && exit 106\n"),
            Expect::Version(stat) => {
                let mtime = stat.modified.and_then(epoch_secs);
                sh = sh
                    .t("[ -e \"$p\" ] || exit 106\ns=$(")
                    .t(flavor.command(true))
                    .t(" \"$p\") || exit 1\nset -- $s\n")
                    .t(&format!("[ \"$2\" = {} ] || exit 106\n", stat.len));
                if let Some(m) = mtime {
                    sh = sh.t(&format!("[ \"$3\" = {m} ] || exit 106\n"));
                }
            }
        }
        let script = sh
            .t("m=\nif [ -e \"$p\" ]; then m=$(")
            .t(flavor.mode_command())
            .t(" \"$p\") || exit 1; fi\n")
            .t(&format!("t=\"$d\"/{}\n", self.token()))
            .t(SAVE)
            .done();
        self.run(&script, Some(data)).map(drop)
    }

    /// The remote script that runs `spec`: `cd`, environment changes, then
    /// `exec` the program with its arguments, every word quoted.
    ///
    /// With `by_name` (a transport that passes variables by name, T718),
    /// each variable whose last change sets it is left out of the script
    /// and returned with its value instead, for the caller to pass by name
    /// (`docker exec -e NAME`, the value in the local command's
    /// environment); a variable whose last change removes it is `unset` by
    /// the script. Applying the changes in order and keeping each name's
    /// last one is the same thing, since names are independent.
    fn process_script(spec: &ProcessSpec, by_name: bool) -> io::Result<(Vec<u8>, PassedEnv)> {
        let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidInput, what.to_string());
        let program = path_bytes(Path::new(spec.program()));
        if program.is_empty() || program.starts_with(b"-") {
            return Err(invalid("invalid program name"));
        }
        let mut sh = Sh::new();
        if let Some(dir) = spec.cwd() {
            // `-P`: resolve `..` physically, as a local `chdir` does.
            sh = sh.t("cd -P -- ").path(dir).t(" || exit 1\n");
        }
        let mut passed: Vec<(String, OsString)> = Vec::new();
        for (i, (key, value)) in spec.envs().iter().enumerate() {
            let key = key
                .to_str()
                .filter(|k| is_env_name(k))
                .ok_or_else(|| invalid("invalid environment variable name"))?;
            if by_name {
                let later = spec.envs()[i + 1..].iter().any(|(k, _)| k == key);
                if later {
                    continue;
                }
                if let Some(v) = value {
                    passed.push((key.to_string(), v.clone()));
                    continue;
                }
            }
            sh = match value {
                Some(v) => sh
                    .t(key)
                    .t("=")
                    .word(&os_bytes(v))
                    .t(&format!("; export {key}\n")),
                None => sh.t(&format!("unset {key}\n")),
            };
        }
        sh = sh.t("exec ").word(&program);
        for arg in spec.get_args() {
            let (Arg::Plain(a) | Arg::Raw(a)) = arg;
            sh = sh.t(" ").word(&os_bytes(a));
        }
        Ok((sh.t("\n").done(), passed))
    }
}

/// Environment variables passed by name, with their values (T718).
type PassedEnv = Vec<(String, OsString)>;

/// What a save expects to find before it overwrites.
#[derive(Clone, Copy)]
enum Expect<'a> {
    /// Anything: no conflict check.
    Any,
    /// No file.
    Absent,
    /// A file with this length and modification time.
    Version(&'a FileStat),
}

/// Follow `$p` through symbolic links (at most 40), so a save writes the
/// link's target and keeps the link.
const RESOLVE_LINKS: &str = "n=0\n\
while [ -h \"$p\" ]; do\n\
  l=$(readlink -- \"$p\") || exit 1\n\
  case $l in /*) p=$l ;; *) p=$(dirname -- \"$p\")/$l ;; esac\n\
  n=$((n + 1)); [ \"$n\" -gt 40 ] && exit 1\n\
done\n";

/// The save itself: a temp file created exclusively (`set -C`) next to the
/// target, filled from stdin, given the old file's mode, then renamed over
/// it. Without write access to the directory, fall back to writing the
/// file in place, as the local writer does.
const SAVE: &str = "if [ -w \"$d\" ]; then\n\
  if [ -n \"$m\" ]; then (umask 077; set -C; : > \"$t\") || exit 1\n\
  else (set -C; : > \"$t\") || exit 1; fi\n\
  cat > \"$t\" || { rm -f -- \"$t\"; exit 1; }\n\
  if [ -n \"$m\" ]; then chmod \"$m\" \"$t\" || { rm -f -- \"$t\"; exit 1; }; fi\n\
  mv -f -- \"$t\" \"$p\" || { rm -f -- \"$t\"; exit 1; }\n\
else\n\
  [ -e \"$p\" ] && [ -w \"$p\" ] || exit 101\n\
  cat > \"$p\"\n\
fi\n";

/// A remote script under construction.
struct Sh(Vec<u8>);

impl Sh {
    fn new() -> Self {
        Self(Vec::new())
    }

    /// Literal script text.
    fn t(mut self, text: &str) -> Self {
        self.0.extend_from_slice(text.as_bytes());
        self
    }

    /// One quoted word.
    fn word(mut self, bytes: &[u8]) -> Self {
        self.0.extend(sh_quote(bytes));
        self
    }

    /// A quoted remote path (`~` expands).
    fn path(mut self, path: &Path) -> Self {
        self.0.extend(quote_path(path));
        self
    }

    /// `name=<quoted path>` on its own line.
    fn set(self, name: &str, path: &Path) -> Self {
        self.t(name).t("=").path(path).t("\n")
    }

    fn done(self) -> Vec<u8> {
        self.0
    }
}

/// Whether `name` is a portable environment variable name.
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// The stdout of a finished remote script, or the error its status names.
fn finish(transport: &impl Transport, out: &Output) -> io::Result<Vec<u8>> {
    use io::ErrorKind as K;
    let host = transport.label();
    let Some(status) = out.status.code() else {
        return Err(io::Error::other(format!(
            "{host}: {} was terminated",
            transport.name()
        )));
    };
    if status == 0 {
        return Ok(out.stdout.clone());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let err = |kind: K, what: &str| io::Error::new(kind, format!("{host}: {what}"));
    if transport.lost(status, &stderr) {
        return Err(io::Error::new(
            K::NotConnected,
            if stderr.is_empty() {
                format!("{host}: {} failed", transport.name())
            } else {
                format!("{host}: {stderr}")
            },
        ));
    }
    Err(match status {
        code::NOT_FOUND => err(K::NotFound, "no such file or directory"),
        code::DENIED => err(K::PermissionDenied, "permission denied"),
        code::EXISTS => err(K::AlreadyExists, "already exists"),
        code::IS_DIR => err(K::IsADirectory, "is a directory"),
        code::NOT_DIR => err(K::NotADirectory, "not a directory"),
        code::NOT_EMPTY => err(K::DirectoryNotEmpty, "directory not empty"),
        code::CONFLICT => WriteConflict::error(),
        code::NOT_REGULAR => io::Error::new(K::InvalidData, TextReadError::NotRegular),
        code::TOO_LARGE => io::Error::new(K::InvalidData, TextReadError::TooLarge),
        code::UNSUPPORTED => err(K::Unsupported, "no trash command on the remote host"),
        code::CHECKSUM => err(
            K::InvalidData,
            "the uploaded agent's SHA-256 checksum did not match; nothing was installed",
        ),
        code::NOT_RUNNABLE => err(
            K::Unsupported,
            "the uploaded agent does not run on the remote host; nothing was installed",
        ),
        code::NO_HASH_TOOL => err(
            K::Unsupported,
            "no sha256sum, shasum or openssl on the remote host to check an upload",
        ),
        _ if stderr.is_empty() => io::Error::other(format!("{host}: exit status {status}")),
        _ => io::Error::other(format!("{host}: {stderr}")),
    })
}

/// `$XDG_RUNTIME_DIR/vix-ssh`, else `/tmp/vix-ssh-<uid>`: short, so the
/// socket path fits the 104-byte limit, and per-user.
fn default_control_dir() -> io::Result<PathBuf> {
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        && run.is_absolute()
        && run.is_dir()
    {
        return Ok(run.join("vix-ssh"));
    }
    Ok(PathBuf::from(format!("/tmp/vix-ssh-{}", own_uid()?)))
}

/// This process's user id, read from a private temp file it creates (no
/// `unsafe` call to `getuid`).
#[cfg(unix)]
fn own_uid() -> io::Result<u32> {
    use std::os::unix::fs::MetadataExt as _;
    let probe = vix_fileops::write_private_temp("vix-ssh-uid", b"")?;
    let uid = std::fs::metadata(&probe).map(|m| m.uid());
    let _ = std::fs::remove_file(&probe);
    uid
}

#[cfg(not(unix))]
fn own_uid() -> io::Result<u32> {
    Ok(0)
}

/// Create `dir` as 0700, or check that the existing one is a real directory
/// (not a link), owned by this user, with no group or other access. Anyone
/// who could plant a socket there could hijack the connection.
#[cfg(unix)]
fn private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    let unsafe_dir = |why: &str| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {why}", dir.display()),
        )
    };
    if !meta.is_dir() {
        return Err(unsafe_dir("the ssh control directory is not a directory"));
    }
    if meta.uid() != own_uid()? {
        return Err(unsafe_dir(
            "the ssh control directory belongs to another user",
        ));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(unsafe_dir(
            "the ssh control directory is not private (0700)",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// `ControlPath=<dir>/%C`, or `None` (no sharing) when the directory's name
/// can't be written there safely or would make the socket path too long.
fn control_pattern(dir: &Path) -> Option<String> {
    let dir = dir.to_str()?;
    // `%C` is 40 hex digits; ssh appends about 20 more while connecting;
    // Unix socket paths hold 104 bytes on macOS and the BSDs.
    if dir.len() + 64 > 104
        || dir
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\'')
    {
        return None;
    }
    Some(format!("{}/%C", dir.replace('%', "%%")))
}

impl<T: Transport> Authority for ShellAuthority<T> {
    fn name(&self) -> &'static str {
        self.transport.name()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if let Some(r) = self.via(|a| a.read_all(path)) {
            return r;
        }
        let script = Sh::new()
            .set("p", path)
            .t(READABLE)
            .t("exec cat -- \"$p\"\n")
            .done();
        self.run(&script, None)
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        if let Some(r) = self.via(|a| a.read(path, limit)) {
            return r;
        }
        let script = Sh::new()
            .set("p", path)
            .t(READABLE)
            .t(&format!("exec head -c {limit} -- \"$p\"\n"))
            .done();
        self.run(&script, None)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_text_guarded(&self, path: &Path, max_bytes: u64) -> io::Result<TextFile> {
        if let Some(r) = self.via(|a| a.read_text(path, max_bytes)) {
            return vix_fileops::decode_text(r?, max_bytes);
        }
        // The size check runs remotely, so a huge file is never sent; `head`
        // bounds the transfer even if the file grows meanwhile.
        let script = Sh::new()
            .set("p", path)
            .t("[ -e \"$p\" ] || exit 100\n[ -f \"$p\" ] || exit 107\n[ -r \"$p\" ] || exit 101\n")
            .t(&format!(
                "[ \"$(wc -c < \"$p\")\" -gt {max_bytes} ] && exit 109\n\
                 exec head -c {} -- \"$p\"\n",
                max_bytes.saturating_add(1)
            ))
            .done();
        vix_fileops::decode_text(self.run(&script, None)?, max_bytes)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(r) = self.via(|a| a.write(path, data)) {
            return r;
        }
        let parent = path.parent().unwrap_or(path);
        let script = Sh::new()
            .set("p", path)
            .set("d", parent)
            .t("[ -d \"$p\" ] && exit 103\n[ -d \"$d\" ] || exit 100\n\
                 [ -e \"$p\" ] && ! [ -w \"$p\" ] && exit 101\nexec cat > \"$p\"\n")
            .done();
        self.run(&script, Some(data)).map(drop)
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(r) = self.via(|a| a.write_atomic(path, data, WireExpect::Any)) {
            return r;
        }
        self.write_atomic_checked(path, data, Expect::Any)
    }

    fn write_atomic_if_unchanged(
        &self,
        path: &Path,
        data: &[u8],
        expected: Option<&FileStat>,
    ) -> io::Result<()> {
        let wire = expected.map_or(WireExpect::Absent, |s| WireExpect::Version(*s));
        if let Some(r) = self.via(|a| a.write_atomic(path, data, wire)) {
            return r;
        }
        self.write_atomic_checked(path, data, expected.map_or(Expect::Absent, Expect::Version))
    }

    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> io::Result<()> {
        let bytes =
            vix_fileops::encode_like(self.read_text_guarded(path, HARD_MAX_TEXT_BYTES), text)?;
        self.write_atomic(path, &bytes)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileStat> {
        if let Some(r) = self.via(|a| a.stat(path, true)) {
            return r;
        }
        self.stat(path, true)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileStat> {
        if let Some(r) = self.via(|a| a.stat(path, false)) {
            return r;
        }
        self.stat(path, false)
    }

    fn metadata_batch(&self, paths: &[&Path]) -> Vec<io::Result<FileStat>> {
        if let Some(r) = self.via(|a| a.stat_many(paths)) {
            return r.unwrap_or_else(|e| paths.iter().map(|_| Err(clone_error(&e))).collect());
        }
        self.stat_batch(paths)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if let Some(r) = self.via(|a| a.canonicalize(path)) {
            return r;
        }
        let script = Sh::new()
            .set("p", path)
            .t("[ -e \"$p\" ] || exit 100\n")
            .t(RESOLVE_LINKS)
            .t("if [ -d \"$p\" ]; then\n\
                   cd -P -- \"$p\" || exit 101; printf %s \"$(pwd -P)\"\n\
                 else\n\
                   b=$(basename -- \"$p\"); cd -P -- \"$(dirname -- \"$p\")\" || exit 101\n\
                   r=$(pwd -P); [ \"$r\" = / ] && r=; printf %s/%s \"$r\" \"$b\"\n\
                 fi\n")
            .done();
        Ok(bytes_path(&self.run(&script, None)?))
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        if let Some(r) = self.via(|a| a.read_dir(dir)) {
            return r;
        }
        let script = Sh::new()
            .set("p", dir)
            .t("[ -e \"$p\" ] || exit 100\n[ -d \"$p\" ] || exit 104\n\
                 [ -r \"$p\" ] && cd -- \"$p\" 2>/dev/null || exit 101\n\
                 for f in * .[!.]* ..?*; do\n\
                   if [ -h \"$f\" ]; then k=l; elif [ -d \"$f\" ]; then k=d\n\
                   elif [ -f \"$f\" ]; then k=f; elif [ -e \"$f\" ]; then k=o; else continue; fi\n\
                   if [ -d \"$f\" ]; then D=1; else D=0; fi\n\
                   printf '%s%s%s\\000' \"$k\" \"$D\" \"$f\"\n\
                 done\n")
            .done();
        let out = self.run(&script, None)?;
        Ok(out
            .split(|&b| b == 0)
            .filter(|rec| rec.len() > 2)
            .map(|rec| {
                let kind = match rec[0] {
                    b'l' => FileKind::Symlink,
                    b'd' => FileKind::Dir,
                    b'f' => FileKind::File,
                    _ => FileKind::Other,
                };
                DirEntry::new(dir.join(bytes_path(&rec[2..])), kind, rec[1] == b'1')
            })
            .collect())
    }

    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::CreateDirAll {
                path: path_bytes(dir),
            })
        }) {
            return r;
        }
        let script = Sh::new()
            .set("p", dir)
            .t("[ -e \"$p\" ] && ! [ -d \"$p\" ] && exit 102\nexec mkdir -p -- \"$p\"\n")
            .done();
        self.run(&script, None).map(drop)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::Rename {
                from: path_bytes(from),
                to: path_bytes(to),
            })
        }) {
            return r;
        }
        let script = Sh::new().set("a", from).set("b", to).t(MOVE).done();
        self.run(&script, None).map(drop)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::RemoveFile {
                path: path_bytes(path),
            })
        }) {
            return r;
        }
        let script = Sh::new()
            .set("p", path)
            .t("[ -e \"$p\" ] || [ -h \"$p\" ] || exit 100\n\
                 [ -d \"$p\" ] && ! [ -h \"$p\" ] && exit 103\nexec rm -f -- \"$p\"\n")
            .done();
        self.run(&script, None).map(drop)
    }

    fn remove_path(&self, path: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::RemovePath {
                path: path_bytes(path),
            })
        }) {
            return r;
        }
        // `rm -r` removes a symlink as a link; it never follows one.
        let script = Sh::new()
            .set("p", path)
            .t("[ -e \"$p\" ] || [ -h \"$p\" ] || exit 100\nexec rm -rf -- \"$p\"\n")
            .done();
        self.run(&script, None).map(drop)
    }

    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::MovePath {
                from: path_bytes(from),
                to: path_bytes(to),
            })
        }) {
            return r;
        }
        // `mv` already copies and removes across file systems.
        self.rename(from, to)
    }

    fn copy_recursive(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::CopyRecursive {
                from: path_bytes(from),
                to: path_bytes(to),
            })
        }) {
            return r;
        }
        let script = Sh::new()
            .set("a", from)
            .set("b", to)
            .t("[ -e \"$a\" ] || [ -h \"$a\" ] || exit 100\n\
                 if [ -h \"$a\" ]; then\n\
                   mkdir -p -- \"$(dirname -- \"$b\")\" && l=$(readlink -- \"$a\") || exit 1\n\
                   exec ln -s -- \"$l\" \"$b\"\n\
                 elif [ -d \"$a\" ]; then\n\
                   mkdir -p -- \"$b\" && exec cp -RP -- \"$a/.\" \"$b\"\n\
                 else\n\
                   [ -d \"$b\" ] && exit 103\n\
                   mkdir -p -- \"$(dirname -- \"$b\")\" && exec cp -P -- \"$a\" \"$b\"\n\
                 fi\nexit 1\n")
            .done();
        self.run(&script, None).map(drop)
    }

    fn trash(&self, path: &Path) -> io::Result<()> {
        if let Some(r) = self.via(|a| {
            a.simple(Op::Trash {
                path: path_bytes(path),
            })
        }) {
            return r;
        }
        let script = Sh::new()
            .set("p", path)
            .t("[ -e \"$p\" ] || [ -h \"$p\" ] || exit 100\n\
                 if command -v gio >/dev/null 2>&1; then exec gio trash -- \"$p\"; fi\n\
                 if command -v trash-put >/dev/null 2>&1; then exec trash-put -- \"$p\"; fi\n\
                 exit 108\n")
            .done();
        self.run(&script, None).map(drop)
    }

    fn is_unwritable(&self, path: &Path) -> bool {
        if let Some(r) = self.via(|a| a.is_unwritable(path)) {
            return r.unwrap_or(false);
        }
        let script = Sh::new()
            .set("p", path)
            .t("[ -f \"$p\" ] && ! [ -w \"$p\" ]\n")
            .done();
        self.run(&script, None).is_ok()
    }

    fn walk_files(&self, root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
        if let Some(r) = self.via(|a| a.walk(root, opts)) {
            return r.unwrap_or_default();
        }
        self.walk(root, opts).unwrap_or_default()
    }

    fn which(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty()
            || name.starts_with('-')
            || name.contains('/')
            || name.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return None;
        }
        if let Some(r) = self.via(|a| a.which(name)) {
            return r.ok().flatten();
        }
        let script = Sh::new()
            .t("n=")
            .word(name.as_bytes())
            .t(
                "\np=$(command -v \"$n\" 2>/dev/null) || exit 100\n\
                 case $p in /*) [ -f \"$p\" ] && [ -x \"$p\" ] && printf %s \"$p\" && exit 0 ;; esac\n\
                 exit 100\n",
            )
            .done();
        self.run(&script, None)
            .ok()
            .filter(|p| !p.is_empty())
            .map(|p| bytes_path(&p))
    }

    fn command(&self, spec: &ProcessSpec) -> io::Result<Command> {
        let by_name = self.transport.passes_env_by_name();
        let (script, passed) = Self::process_script(spec, by_name)?;
        let names: Vec<&str> = passed.iter().map(|(k, _)| k.as_str()).collect();
        let mut cmd = self.transport.shell(&script, false, &names);
        for (key, value) in &passed {
            cmd.env(key, value);
        }
        Ok(cmd)
    }

    fn shell_is_posix(&self) -> bool {
        true
    }

    /// `ssh -t … HOST <envelope>`, over the same shared connection and with
    /// the same options as every other operation, whose script changes to
    /// `cwd` and then replaces itself with the remote user's login shell
    /// (`$SHELL -l`, else `/bin/sh -l`). `shell` (the local one) is not
    /// used. A `cwd` that is gone leaves the shell in the remote home, with
    /// `cd`'s own message.
    fn terminal(&self, _shell: &str, cwd: Option<&Path>) -> io::Result<ProcessSpec> {
        let mut sh = Sh::new();
        if let Some(dir) = cwd {
            sh = sh.t("cd -P -- ").path(dir).t("\n");
        }
        let script = sh.t(self.transport.login_shell()).done();
        let cmd = self.transport.shell(&script, true, &[]);
        Ok(ProcessSpec::new(cmd.get_program()).args(cmd.get_args()))
    }
}

/// A copy of `e` (kind and message) for each path of a failed batch.
fn clone_error(e: &io::Error) -> io::Error {
    io::Error::new(e.kind(), e.to_string())
}

/// Paths per [`SshAuthority::stat_batch`] script, so the command line stays
/// well under any `ARG_MAX`.
const STAT_BATCH: usize = 200;

impl<T: Transport> ShellAuthority<T> {
    /// [`Authority::metadata_batch`] without the agent: one script per
    /// [`STAT_BATCH`] paths, printing one line per path (`o` and the stat,
    /// `n` for a missing path, `e` for a failed `stat`).
    fn stat_batch(&self, paths: &[&Path]) -> Vec<io::Result<FileStat>> {
        let mut out = Vec::with_capacity(paths.len());
        for chunk in paths.chunks(STAT_BATCH) {
            match self.stat_chunk(chunk) {
                Ok(stats) => out.extend(stats),
                Err(e) => out.extend(chunk.iter().map(|_| Err(clone_error(&e)))),
            }
        }
        out
    }

    fn stat_chunk(&self, paths: &[&Path]) -> io::Result<Vec<io::Result<FileStat>>> {
        let flavor = self.flavor()?;
        let mut sh = Sh::new().t("for p in");
        for p in paths {
            sh = sh.t(" ").path(p);
        }
        let script = sh
            .t("; do\n  if [ -e \"$p\" ]; then\n    s=$(")
            .t(flavor.command(true))
            .t(
                " \"$p\" 2>/dev/null) && printf 'o%s\\n' \"$s\" || printf 'e\\n'\n  \
                else printf 'n\\n'; fi\ndone\n",
            )
            .done();
        let reply = self.run(&script, None)?;
        let lines: Vec<&[u8]> = reply.split(|&b| b == b'\n').collect();
        let host = self.transport.label();
        Ok((0..paths.len())
            .map(|i| match lines.get(i).and_then(|l| l.split_first()) {
                Some((b'o', rest)) => flavor.parse(rest).ok_or_else(|| {
                    io::Error::other("unexpected reply from the remote stat command")
                }),
                Some((b'n', _)) => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{host}: no such file or directory"),
                )),
                _ => Err(io::Error::other(format!("{host}: stat failed"))),
            })
            .collect())
    }

    /// [`Authority::walk_files`]: `git ls-files` when ignore files are
    /// honored and `root` is in a git work tree (git's own ignore rules),
    /// else `find`. Read as a stream and cut off at the limit.
    fn walk(&self, root: &Path, opts: &WalkOptions) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        if opts.limit == 0 {
            return Ok(out);
        }
        let mut prune: Vec<String> = opts.prune_dirs.iter().map(|d| glob_escape(d)).collect();
        if opts.skip_hidden {
            prune.push(".*".into());
        }
        let mut find = String::from("printf 'f\\000'\nexec find . -mindepth 1");
        if let Some(depth) = opts.max_depth {
            let _ = write!(find, " -maxdepth {depth}");
        }
        if !prune.is_empty() {
            find.push_str(" \\(");
            for (i, name) in prune.iter().enumerate() {
                if i > 0 {
                    find.push_str(" -o");
                }
                find.push_str(" -name ");
                find.push_str(&String::from_utf8_lossy(&sh_quote(name.as_bytes())));
            }
            find.push_str(" \\) -prune -o");
        }
        find.push_str(" -type f -print0\n");
        let mut sh = Sh::new()
            .set("r", root)
            .t("[ -d \"$r\" ] || exit 100\ncd -- \"$r\" || exit 101\n");
        if opts.honor_ignore_files {
            sh = sh.t(
                "if command -v git >/dev/null 2>&1 && \
                 git rev-parse --is-inside-work-tree >/dev/null 2>&1; then\n\
                   printf 'g\\000'\n\
                   git ls-files -z --cached --others --exclude-standard | \
                   xargs -0 sh -c 'for f; do [ -f \"$f\" ] && ! [ -h \"$f\" ] && printf \"%s\\000\" \"$f\"; done' sh\n\
                   exit 0\n\
                 fi\n",
            );
        }
        let script = sh.t(&find).done();
        let mut child = self
            .script_command(&script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let Some(stdout) = child.stdout.take() else {
            return Ok(out);
        };
        let mut records = io::BufReader::new(stdout).split(0);
        let _marker = records.next();
        for rec in records {
            let Ok(rec) = rec else { break };
            let rel = rec.strip_prefix(b"./").unwrap_or(&rec);
            let rel = bytes_path(rel);
            if keep(&rel, opts) {
                out.push(root.join(rel));
                if out.len() >= opts.limit {
                    let _ = child.kill();
                    break;
                }
            }
        }
        let _ = child.wait();
        Ok(out)
    }
}

/// Whether a walked path (relative to the root) passes `opts`' depth,
/// hidden and pruned-name rules (`git ls-files` applies none of them).
fn keep(rel: &Path, opts: &WalkOptions) -> bool {
    let names: Vec<_> = rel
        .components()
        .map(std::path::Component::as_os_str)
        .collect();
    if opts.max_depth.is_some_and(|d| names.len() > d) {
        return false;
    }
    !names.iter().any(|n| {
        let n = n.to_string_lossy();
        (opts.skip_hidden && n.starts_with('.')) || opts.prune_dirs.iter().any(|p| *p == n)
    })
}

/// `$p` must be an existing, readable non-directory.
const READABLE: &str =
    "[ -e \"$p\" ] || exit 100\n[ -d \"$p\" ] && exit 103\n[ -r \"$p\" ] || exit 101\n";

/// `std::fs::rename` semantics on top of `mv`: replace a file or an empty
/// directory, never move *into* an existing directory.
const MOVE: &str = "[ -e \"$a\" ] || [ -h \"$a\" ] || exit 100\n\
[ \"$a\" -ef \"$b\" ] && exit 0\n\
if [ -d \"$b\" ] && ! [ -h \"$b\" ]; then\n\
  if [ -d \"$a\" ] && ! [ -h \"$a\" ]; then rmdir -- \"$b\" 2>/dev/null || exit 105\n\
  else exit 103; fi\n\
fi\n\
exec mv -f -- \"$a\" \"$b\"\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_names_are_checked() {
        assert!(is_env_name("GIT_DIR") && is_env_name("_x1"));
        assert!(
            !is_env_name("1X") && !is_env_name("A-B") && !is_env_name("") && !is_env_name("A B")
        );
    }

    #[test]
    fn walk_filters_match_the_local_walk() {
        let opts = WalkOptions::file_index(10);
        assert!(keep(Path::new("src/main.rs"), &opts));
        assert!(!keep(Path::new(".git/config"), &opts));
        assert!(!keep(Path::new("a/target/x"), &opts));
        assert!(!keep(Path::new("node_modules/x.js"), &opts));
        let deep: PathBuf = (0..13).map(|i| i.to_string()).collect();
        assert!(!keep(&deep, &opts));
    }

    #[test]
    fn control_paths_stay_short_and_literal() {
        assert_eq!(
            control_pattern(Path::new("/tmp/vix-ssh-501")).as_deref(),
            Some("/tmp/vix-ssh-501/%C")
        );
        assert_eq!(
            control_pattern(Path::new("/run/a%b")).as_deref(),
            Some("/run/a%%b/%C")
        );
        assert_eq!(control_pattern(Path::new("/tmp/a b")), None);
        assert_eq!(
            control_pattern(&PathBuf::from(format!("/{}", "x".repeat(60)))),
            None
        );
    }
}
