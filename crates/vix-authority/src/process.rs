//! [`ProcessSpec`]: what to run, described as data, so an authority can decide
//! *where* it runs (here, over SSH, in a container) before a `Command` exists.
//!
//! A [`std::process::Command`] cannot be inspected or rewritten after it is
//! built, so a remote authority could not re-target it. `ProcessSpec` is the
//! plain-data form call sites build instead; [`crate::Authority::command`]
//! turns it into a `Command` (locally via `ProcessSpec::to_local_command`, or
//! wrapped in `ssh`/`docker exec`/… by the remote authorities). Owning spec:
//! `crates/vix-authority/spec/index.md`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// One command-line argument.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Arg {
    /// An ordinary argument, escaped by the platform as `Command::arg` does.
    Plain(OsString),
    /// Text appended to the Windows command line unescaped
    /// (`CommandExt::raw_arg`), for `cmd /C` scripts that carry their own
    /// quoting; an ordinary argument everywhere else.
    Raw(OsString),
}

/// A process to start: program, arguments, working directory and environment
/// changes. Stdio and spawning stay with the caller, on the `Command` that
/// [`crate::Authority::command`] builds from this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSpec {
    /// The executable, as given (looked up on the authority's `PATH` if bare).
    program: OsString,
    /// Arguments in command-line order, each plain or raw.
    args: Vec<Arg>,
    /// Working directory, in the authority's own path space; `None` inherits.
    cwd: Option<PathBuf>,
    /// Environment edits applied in order: `Some(v)` sets, `None` removes.
    env: Vec<(OsString, Option<OsString>)>,
}

impl ProcessSpec {
    /// A spec that runs `program` with no arguments, in the inherited working
    /// directory and environment.
    ///
    /// # Examples
    /// ```
    /// use vix_authority::ProcessSpec;
    /// let spec = ProcessSpec::new("git").arg("status").args(["--short"]);
    /// assert_eq!(spec.program(), "git");
    /// assert_eq!(spec.get_args().len(), 2);
    /// assert!(spec.cwd().is_none());
    /// ```
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    /// Append one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(Arg::Plain(arg.as_ref().to_owned()));
        self
    }

    /// Append several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| Arg::Plain(a.as_ref().to_owned())));
        self
    }

    /// Append text to the Windows command line unescaped (see [`Arg::Raw`]).
    #[must_use]
    pub fn raw_arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(Arg::Raw(arg.as_ref().to_owned()));
        self
    }

    /// Run in `dir`.
    #[must_use]
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Set the environment variable `key` to `value`.
    #[must_use]
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_owned(), Some(value.as_ref().to_owned())));
        self
    }

    /// Remove the environment variable `key`.
    #[must_use]
    pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        self.env.push((key.as_ref().to_owned(), None));
        self
    }

    /// The program to run.
    #[must_use]
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// The arguments, in order.
    #[must_use]
    pub fn get_args(&self) -> &[Arg] {
        &self.args
    }

    /// The working directory, if one was set.
    #[must_use]
    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// The environment changes, in order: `Some` sets, `None` removes.
    #[must_use]
    pub fn envs(&self) -> &[(OsString, Option<OsString>)] {
        &self.env
    }

    /// This spec with its working directory replaced by `cwd` (or cleared).
    #[must_use]
    pub fn with_cwd(mut self, cwd: Option<PathBuf>) -> Self {
        self.cwd = cwd;
        self
    }

    /// The `Command` that runs this spec on the local machine: exactly what
    /// the call sites built by hand before T710. Only built with the `process`
    /// feature (T595); without it no `Command` is ever constructed.
    #[cfg(feature = "process")]
    #[must_use]
    pub fn to_local_command(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(&self.program);
        for arg in &self.args {
            match arg {
                Arg::Plain(a) => {
                    cmd.arg(a);
                }
                Arg::Raw(a) => {
                    // `raw_arg` exists only on Windows; elsewhere a raw
                    // argument is just a plain one.
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt as _;
                        cmd.raw_arg(a);
                    }
                    #[cfg(not(windows))]
                    cmd.arg(a);
                }
            }
        }
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        for (key, value) in &self.env {
            match value {
                Some(v) => cmd.env(key, v),
                None => cmd.env_remove(key),
            };
        }
        cmd
    }
}

#[cfg(all(test, feature = "process"))]
mod tests {
    use super::*;

    #[test]
    fn the_local_command_carries_every_part_of_the_spec() {
        let spec = ProcessSpec::new("git")
            .arg("status")
            .args(["--short", "--branch"])
            .current_dir("/tmp")
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env_remove("GIT_DIR");
        let cmd = spec.to_local_command();
        assert_eq!(cmd.get_program(), "git");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["status", "--short", "--branch"]);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/tmp")));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("GIT_LITERAL_PATHSPECS"), Some(OsStr::new("1")))));
        assert!(envs.contains(&(OsStr::new("GIT_DIR"), None)));
    }
}
