//! The private runtime directory and connection file of one kernel.

use std::fs;
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use vix_jupyter_core::{ConnectionInfo, Transport};

/// The longest IPC socket path used (`sun_path` is 104 bytes on macOS,
/// 108 on Linux, and the endpoint adds `-N`).
const MAX_IPC_PREFIX: usize = 96;

/// `n` random bytes from the OS, as lowercase hex.
pub(crate) fn random_hex(n: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(bytes.iter().fold(String::new(), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    }))
}

/// A directory only the user can enter, removed (with everything in it)
/// when dropped.
#[derive(Debug)]
pub(crate) struct RuntimeDir {
    /// The created directory, removed on drop.
    path: PathBuf,
}

impl RuntimeDir {
    /// Create `vix-jupyter-<random>` under `base`, mode `0700` on Unix.
    /// Fails rather than reuse an existing path.
    pub(crate) fn create(base: &Path) -> io::Result<Self> {
        let path = base.join(format!("vix-jupyter-{}", random_hex(8)?));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self { path })
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The connection file's path.
    pub(crate) fn connection_file(&self) -> PathBuf {
        self.path.join("kernel.json")
    }

    /// A fresh connection for `transport`, with a random 256-bit key.
    pub(crate) fn new_connection(
        &self,
        transport: Transport,
        kernel_name: &str,
    ) -> io::Result<ConnectionInfo> {
        let key = random_hex(32)?;
        match transport {
            Transport::Ipc => {
                let prefix = self.path.join("kernel");
                let prefix = prefix.to_string_lossy().into_owned();
                if prefix.len() > MAX_IPC_PREFIX {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "runtime directory path too long for a Unix socket",
                    ));
                }
                Ok(ConnectionInfo::new(
                    Transport::Ipc,
                    &prefix,
                    [1, 2, 3, 4, 5],
                    &key,
                    kernel_name,
                ))
            }
            Transport::Tcp => Ok(ConnectionInfo::new(
                Transport::Tcp,
                "127.0.0.1",
                free_ports()?,
                &key,
                kernel_name,
            )),
        }
    }

    /// Write `info` as the connection file: created new, mode `0600`.
    pub(crate) fn write_connection(&self, info: &ConnectionInfo) -> io::Result<PathBuf> {
        let path = self.connection_file();
        let _ = fs::remove_file(&path);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(info.to_json().as_bytes())?;
        file.sync_all()?;
        Ok(path)
    }

    /// Remove stale IPC socket files before a restart.
    pub(crate) fn clear_sockets(info: &ConnectionInfo) {
        if info.transport == Transport::Ipc {
            for port in 1..=5 {
                let _ = fs::remove_file(format!("{}-{port}", info.ip));
            }
        }
    }
}

impl Drop for RuntimeDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A private scratch directory for files Vix writes on a kernel's behalf,
/// such as the images it opens in a tab (T734g): `vix-jupyter-<random>`,
/// mode `0700` on Unix, never an existing path, removed with everything in
/// it when dropped.
#[derive(Debug)]
pub struct PrivateDir(RuntimeDir);

impl PrivateDir {
    /// Create one under `base` (such as [`std::env::temp_dir`]).
    ///
    /// # Errors
    /// The I/O error when the directory cannot be created.
    pub fn create(base: &Path) -> io::Result<Self> {
        RuntimeDir::create(base).map(Self)
    }

    /// The directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0.path
    }
}

/// Five free loopback TCP ports. They are held together while chosen, so
/// they are distinct; another process could still take one before the
/// kernel binds it (the same race `jupyter_client` accepts).
fn free_ports() -> io::Result<[u16; 5]> {
    let listeners = (0..5)
        .map(|_| TcpListener::bind(("127.0.0.1", 0)))
        .collect::<io::Result<Vec<_>>>()?;
    let mut ports = [0u16; 5];
    for (slot, listener) in ports.iter_mut().zip(&listeners) {
        *slot = listener.local_addr()?.port();
    }
    Ok(ports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_jupyter_core::Channel;

    #[test]
    fn private_dir_and_file() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let info = dir.new_connection(Transport::Tcp, "python3").unwrap();
        assert_eq!(info.key.len(), 64);
        let path = dir.write_connection(&info).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(ConnectionInfo::parse(&text).unwrap(), info);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dmode = fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
            let fmode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(dmode, 0o700);
            assert_eq!(fmode, 0o600);
        }
        let kept = dir.path().to_path_buf();
        drop(dir);
        assert!(!kept.exists());
    }

    #[test]
    fn private_dir_is_removed_on_drop() {
        let dir = PrivateDir::create(&std::env::temp_dir()).unwrap();
        let kept = dir.path().to_path_buf();
        fs::write(kept.join("a.png"), b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&kept).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
        drop(dir);
        assert!(!kept.exists());
    }

    #[test]
    fn keys_differ() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let a = dir.new_connection(Transport::Ipc, "k").unwrap();
        let b = dir.new_connection(Transport::Ipc, "k").unwrap();
        assert_ne!(a.key, b.key);
        assert!(a.ip.ends_with("kernel"));
    }

    #[test]
    fn random_hex_is_lowercase_hex_of_double_length() {
        let a = random_hex(12).unwrap();
        assert_eq!(a.len(), 24);
        assert!(
            a.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(random_hex(0).unwrap(), "");
    }

    #[test]
    fn create_fails_when_the_base_is_missing() {
        let base = std::env::temp_dir().join(format!("vix-jupyter-nobase-{}", std::process::id()));
        assert!(RuntimeDir::create(&base).is_err());
    }

    #[test]
    fn ipc_path_that_is_too_long_for_a_socket_is_refused() {
        let base = std::env::temp_dir().join("x".repeat(120));
        // Built directly: the deep directory is never created, so dropping
        // the value removes nothing real.
        let dir = RuntimeDir {
            path: base.join("vix-jupyter-0123456789abcdef"),
        };
        let err = dir.new_connection(Transport::Ipc, "k").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        // TCP does not use the path, so it is unaffected.
        let tcp = dir.new_connection(Transport::Tcp, "k").unwrap();
        assert_eq!(tcp.ip, "127.0.0.1");
    }

    #[test]
    fn tcp_connections_get_five_distinct_loopback_ports() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let info = dir.new_connection(Transport::Tcp, "python3").unwrap();
        assert!(info.is_local());
        let mut ports: Vec<u16> = [
            Channel::Shell,
            Channel::IoPub,
            Channel::Stdin,
            Channel::Control,
            Channel::Heartbeat,
        ]
        .iter()
        .map(|c| info.port(*c))
        .collect();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(ports.len(), 5);
    }

    #[test]
    fn write_connection_replaces_an_existing_file() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let first = dir.new_connection(Transport::Tcp, "a").unwrap();
        let second = dir.new_connection(Transport::Tcp, "b").unwrap();
        let path = dir.write_connection(&first).unwrap();
        assert_eq!(path, dir.connection_file());
        let again = dir.write_connection(&second).unwrap();
        assert_eq!(again, path);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(ConnectionInfo::parse(&text).unwrap(), second);
    }

    #[test]
    fn clear_sockets_removes_only_the_five_ipc_socket_files() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let info = dir.new_connection(Transport::Ipc, "k").unwrap();
        for port in 1..=6 {
            fs::write(format!("{}-{port}", info.ip), b"s").unwrap();
        }
        RuntimeDir::clear_sockets(&info);
        for port in 1..=5 {
            assert!(!Path::new(&format!("{}-{port}", info.ip)).exists());
        }
        assert!(Path::new(&format!("{}-6", info.ip)).exists());

        // TCP connections have no socket files; nothing happens.
        let tcp = dir.new_connection(Transport::Tcp, "k").unwrap();
        RuntimeDir::clear_sockets(&tcp);
        assert!(Path::new(&format!("{}-6", info.ip)).exists());
    }
}
