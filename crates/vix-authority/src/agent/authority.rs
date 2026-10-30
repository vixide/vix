//! [`AgentAuthority`]: every file operation through a connected agent.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use super::client::{AgentClient, Fail, Watch};
use super::wire::{Expect, Op};
use crate::ssh::quote::path_bytes;
use crate::{
    Authority, DirEntry, FileStat, HARD_MAX_TEXT_BYTES, ProcessSpec, TextFile, WalkOptions,
};

/// A workspace served by a remote agent over any byte stream (T717): the
/// agent's `stdin`/`stdout` through `ssh`, `docker exec` or, in tests, a
/// pipe to an agent thread.
///
/// Every file operation is one request (or one batch). A request whose
/// data, or whose answer, doesn't fit in a frame (over 64 MiB) fails with
/// `Unsupported`; `SshAuthority` instead runs its shell script for that one
/// operation. Processes are not the agent's job: [`Authority::command`] goes
/// to the authority given to [`AgentAuthority::with_commands`], or fails.
#[derive(Debug)]
pub struct AgentAuthority {
    client: AgentClient,
    commands: Option<Arc<dyn Authority>>,
}

/// An agent call's result as `io::Result`.
fn io<T>(r: Result<T, Fail>) -> io::Result<T> {
    r.map_err(|f| match f {
        Fail::Io(e) => e,
        Fail::Unavailable => io::Error::new(
            io::ErrorKind::Unsupported,
            "the remote agent could not do this (too large, or no longer running)",
        ),
    })
}

impl AgentAuthority {
    /// Connect to an agent whose output is `reader` and whose input is
    /// `writer`, checking its protocol and Vix version in a handshake.
    ///
    /// # Errors
    /// The agent didn't answer within 20 seconds, answered garbage, or is a
    /// different protocol or Vix version.
    pub fn connect(
        reader: impl io::Read + Send + 'static,
        writer: impl io::Write + Send + 'static,
    ) -> io::Result<Self> {
        Ok(Self {
            client: AgentClient::connect(reader, writer)?,
            commands: None,
        })
    }

    /// Start `cmd` (for example `ssh host vix --remote-agent`) and connect
    /// to it as [`AgentAuthority::connect`] does. Dropping the authority
    /// closes the agent's input and reaps the process.
    ///
    /// # Errors
    /// The process could not start, or the handshake failed.
    #[cfg(feature = "process")]
    pub fn spawn(cmd: Command) -> io::Result<Self> {
        Ok(Self {
            client: AgentClient::spawn(cmd)?,
            commands: None,
        })
    }

    /// Run processes through `authority` (the agent itself starts none).
    #[must_use]
    pub fn with_commands(mut self, authority: Arc<dyn Authority>) -> Self {
        self.commands = Some(authority);
        self
    }

    /// The agent's Vix version.
    #[must_use]
    pub fn version(&self) -> &str {
        self.client.version()
    }

    /// Whether the agent connection is still up.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.client.is_alive()
    }

    /// Watch `paths` for changes, checked on the remote side every
    /// `interval` (clamped to 0.1–60 s).
    ///
    /// # Errors
    /// The agent refused (over 64 subscriptions, or 4096 paths) or is gone.
    pub fn watch(&self, paths: &[PathBuf], interval: Duration) -> io::Result<Watch> {
        self.client.watch(paths, interval)
    }
}

impl Authority for AgentAuthority {
    fn name(&self) -> &'static str {
        "agent"
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        io(self.client.read_all(path))
    }

    fn read_limited(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        io(self.client.read(path, limit))
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_text_guarded(&self, path: &Path, max_bytes: u64) -> io::Result<TextFile> {
        vix_fileops::decode_text(io(self.client.read_text(path, max_bytes))?, max_bytes)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        io(self.client.write(path, data))
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        io(self.client.write_atomic(path, data, Expect::Any))
    }

    fn write_atomic_if_unchanged(
        &self,
        path: &Path,
        data: &[u8],
        expected: Option<&FileStat>,
    ) -> io::Result<()> {
        let expect = expected.map_or(Expect::Absent, |s| Expect::Version(*s));
        io(self.client.write_atomic(path, data, expect))
    }

    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> io::Result<()> {
        let bytes =
            vix_fileops::encode_like(self.read_text_guarded(path, HARD_MAX_TEXT_BYTES), text)?;
        self.write_atomic(path, &bytes)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileStat> {
        io(self.client.stat(path, true))
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileStat> {
        io(self.client.stat(path, false))
    }

    fn metadata_batch(&self, paths: &[&Path]) -> Vec<io::Result<FileStat>> {
        match io(self.client.stat_many(paths)) {
            Ok(stats) => stats,
            Err(e) => paths
                .iter()
                .map(|_| Err(io::Error::new(e.kind(), e.to_string())))
                .collect(),
        }
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        io(self.client.canonicalize(path))
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        io(self.client.read_dir(dir))
    }

    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        io(self.client.simple(Op::CreateDirAll {
            path: path_bytes(dir),
        }))
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        io(self.client.simple(Op::Rename {
            from: path_bytes(from),
            to: path_bytes(to),
        }))
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        io(self.client.simple(Op::RemoveFile {
            path: path_bytes(path),
        }))
    }

    fn remove_path(&self, path: &Path) -> io::Result<()> {
        io(self.client.simple(Op::RemovePath {
            path: path_bytes(path),
        }))
    }

    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()> {
        io(self.client.simple(Op::MovePath {
            from: path_bytes(from),
            to: path_bytes(to),
        }))
    }

    fn copy_recursive(&self, from: &Path, to: &Path) -> io::Result<()> {
        io(self.client.simple(Op::CopyRecursive {
            from: path_bytes(from),
            to: path_bytes(to),
        }))
    }

    fn trash(&self, path: &Path) -> io::Result<()> {
        io(self.client.simple(Op::Trash {
            path: path_bytes(path),
        }))
    }

    fn is_unwritable(&self, path: &Path) -> bool {
        self.client.is_unwritable(path).unwrap_or(false)
    }

    fn walk_files(&self, root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
        self.client.walk(root, opts).unwrap_or_default()
    }

    fn which(&self, name: &str) -> Option<PathBuf> {
        self.client.which(name).ok().flatten()
    }

    fn command(&self, spec: &ProcessSpec) -> io::Result<Command> {
        match &self.commands {
            Some(auth) => auth.command(spec),
            None => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the remote agent does not start processes",
            )),
        }
    }
}
