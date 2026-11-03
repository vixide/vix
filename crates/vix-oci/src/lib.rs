//! Drive an OCI container engine through its command line.
//!
//! Vix does not implement the OCI image, runtime or distribution specs; it
//! shells out to an engine that does (Docker, Podman or nerdctl) and reads the
//! `--format json` output they share. The crate is three layers, each testable
//! alone and none of them starting a process:
//!
//! 1. [`Engine`] and [`detect`] choose the engine: the user's preference, then
//!    `docker`, `podman`, `nerdctl`, by asking a caller-supplied lookup (so the
//!    crate never reads `PATH` itself).
//! 2. The [`argv`] module builds each command as an argument vector. There is
//!    no shell anywhere: every user-typed value is one argv element, and a
//!    value that an engine could misread as an option is refused with
//!    [`OciError::InvalidArgument`]. [`argv::render`] formats a vector for
//!    display, which is how the app shows the exact command before it runs a
//!    mutating one.
//! 3. The [`parse`] module reads `ps`, `images`, `volume ls`, `network ls`,
//!    `pod ps` and `inspect` output in both the newline-delimited form Docker
//!    prints and the single-array form Podman prints.
//!
//! With the `process` feature (on by default) the crate also runs the engine:
//! [`run`] takes the resolved program path, the argument vector and optional
//! standard input (the only way a registry password ever travels), and returns
//! the output or a typed [`OciError`].
//!
//! [`devcontainer`] is a fourth, independent piece: a tolerant reader for
//! `devcontainer.json` that lets the app offer to reopen a project in its
//! container.
//!
//! The OC menu and panels built on this crate are
//! `vix-oci-panel` and the App shell
//! (`tui/src/app/oc.rs`). Owning spec: `crates/vix-oci/spec/index.md`, and the
//! repo-level `spec/open-container`.
//!
//! ```
//! use std::path::PathBuf;
//! use vix_oci::{detect, Engine};
//!
//! // Pretend only podman is installed.
//! let found = detect("", |name| (name == "podman").then(|| PathBuf::from("/bin/podman")));
//! assert_eq!(found.unwrap().engine, Engine::Podman);
//! assert_eq!(Engine::from_name("Docker"), Some(Engine::Docker));
//! ```

pub mod argv;
pub mod devcontainer;
pub mod parse;

pub use parse::{Container, Image, Network, Pod, Volume};

use std::path::PathBuf;

/// An OCI container engine whose CLI Vix can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Engine {
    /// Docker (`docker`).
    Docker,
    /// Podman (`podman`): daemonless, and the only engine with pods.
    Podman,
    /// nerdctl (`nerdctl`): the containerd CLI.
    Nerdctl,
}

impl Engine {
    /// Every engine, in detection order.
    pub const ALL: [Engine; 3] = [Engine::Docker, Engine::Podman, Engine::Nerdctl];

    /// The binary name looked up on `PATH`, which is also the setting value.
    #[must_use]
    pub fn binary(self) -> &'static str {
        match self {
            Engine::Docker => "docker",
            Engine::Podman => "podman",
            Engine::Nerdctl => "nerdctl",
        }
    }

    /// The engine whose [`binary`](Engine::binary) is `name` (case-insensitive,
    /// surrounding space ignored). `auto`, the empty string and anything unknown
    /// give `None`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Engine> {
        let name = name.trim();
        Engine::ALL
            .into_iter()
            .find(|e| e.binary().eq_ignore_ascii_case(name))
    }

    /// Whether this engine has pods (`pod ps`). Only Podman does; the App
    /// hides the Pods panel for the others.
    #[must_use]
    pub fn has_pods(self) -> bool {
        self == Engine::Podman
    }
}

/// A detected engine and where its binary lives, as returned by [`detect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// Which engine it is.
    pub engine: Engine,
    /// The resolved program to start.
    pub program: PathBuf,
}

/// Why an engine operation failed.
///
/// `Display` gives untranslated developer text; the App maps each variant to
/// a translated message and shows [`OciError::Failed`]'s `stderr` verbatim,
/// since it is the engine's own diagnosis.
#[derive(Debug)]
#[non_exhaustive]
pub enum OciError {
    /// No engine is installed (or the chosen one is not on `PATH`).
    NotInstalled,
    /// The engine could not be started or talked to for another reason.
    Io(std::io::Error),
    /// The engine ran and exited unsuccessfully.
    Failed {
        /// The exit code, when the process wasn't killed by a signal.
        code: Option<i32>,
        /// The engine's own error output, trimmed.
        stderr: String,
    },
    /// An argument was refused before running anything: it is empty, begins
    /// with `-` (so an engine could read it as an option), or holds
    /// whitespace or control characters.
    InvalidArgument(String),
    /// Engine output could not be read as the expected JSON.
    Parse(String),
}

impl std::fmt::Display for OciError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("no container engine found"),
            Self::Io(e) => write!(f, "could not run the container engine: {e}"),
            Self::Failed { code, stderr } => match code {
                Some(c) => write!(f, "engine exited with {c}: {stderr}"),
                None => write!(f, "engine was terminated: {stderr}"),
            },
            Self::InvalidArgument(a) => write!(f, "refused argument {a:?}"),
            Self::Parse(why) => write!(f, "unreadable engine output: {why}"),
        }
    }
}

impl std::error::Error for OciError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for OciError {
    /// A missing program (`NotFound` from spawn) is "not installed", not a
    /// generic I/O failure, so the UI can offer install guidance.
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotInstalled
        } else {
            Self::Io(e)
        }
    }
}

/// Choose an engine. The `preferred` setting value wins when it names an
/// engine `find` can locate; otherwise the first of `docker`, `podman`,
/// `nerdctl` that `find` locates. `find` maps a binary name to its resolved
/// path (the app passes its `PATH` lookup; tests pass a table).
///
/// # Errors
/// [`OciError::NotInstalled`] when no engine is found.
pub fn detect(
    preferred: &str,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Detected, OciError> {
    // The preferred engine is tried first; if it is not installed we fall
    // through to the normal order (it may be tried a second time, harmlessly).
    let order = Engine::from_name(preferred).into_iter().chain(Engine::ALL);
    for engine in order {
        if let Some(program) = find(engine.binary()) {
            return Ok(Detected { engine, program });
        }
    }
    Err(OciError::NotInstalled)
}

/// Run `program` with `args` and optional standard input, returning its
/// standard output. Standard error becomes the [`OciError::Failed`] text when
/// the engine exits unsuccessfully. `DOCKER_HOST`, `CONTAINER_HOST` and the
/// context variables are inherited untouched.
///
/// # Errors
/// [`OciError::NotInstalled`] if `program` does not exist, [`OciError::Io`]
/// for other spawn or pipe failures, [`OciError::Failed`] on a non-zero exit.
#[cfg(feature = "process")]
pub fn run(
    program: &std::path::Path,
    args: &[String],
    stdin: Option<&str>,
) -> Result<String, OciError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Arguments are passed as an argv vector with no shell, so quoting is
    // never an issue; `argv` has already refused option-like values.

    let mut child = Command::new(program)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // A broken pipe means the engine exited early; its status says why.
        let _ = pipe.write_all(text.as_bytes());
        // `pipe` drops here, closing stdin so the engine sees end of input
        // (a `docker login --password-stdin` would otherwise wait forever).
    }
    // `wait_with_output` drains stdout and stderr concurrently, so a chatty
    // engine cannot deadlock on a full pipe.
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(OciError::Failed {
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(names: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
        move |n| {
            names
                .contains(&n)
                .then(|| PathBuf::from(format!("/bin/{n}")))
        }
    }

    #[test]
    fn detection_order_is_docker_podman_nerdctl() {
        let d = detect("", table(&["nerdctl", "podman"])).unwrap();
        assert_eq!(d.engine, Engine::Podman);
        let d = detect("auto", table(&["nerdctl", "podman", "docker"])).unwrap();
        assert_eq!(d.engine, Engine::Docker);
    }

    #[test]
    fn the_setting_wins_when_installed_and_falls_through_when_not() {
        let d = detect("nerdctl", table(&["docker", "nerdctl"])).unwrap();
        assert_eq!(d.engine, Engine::Nerdctl);
        assert_eq!(d.program, PathBuf::from("/bin/nerdctl"));
        let d = detect("podman", table(&["docker"])).unwrap();
        assert_eq!(d.engine, Engine::Docker);
    }

    #[test]
    fn no_engine_is_not_installed() {
        assert!(matches!(
            detect("", table(&[])),
            Err(OciError::NotInstalled)
        ));
    }

    #[test]
    fn names_parse_case_insensitively() {
        assert_eq!(Engine::from_name(" Podman "), Some(Engine::Podman));
        assert_eq!(Engine::from_name("auto"), None);
        assert_eq!(Engine::from_name(""), None);
        assert!(Engine::Podman.has_pods() && !Engine::Docker.has_pods());
    }
}
