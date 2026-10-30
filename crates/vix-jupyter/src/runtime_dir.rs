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
    Ok(bytes
        .iter()
        .fold(String::with_capacity(n * 2), |mut out, b| {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        }))
}

/// A directory only the user can enter, removed (with everything in it)
/// when dropped.
#[derive(Debug)]
pub(crate) struct RuntimeDir {
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
    fn keys_differ() {
        let dir = RuntimeDir::create(&std::env::temp_dir()).unwrap();
        let a = dir.new_connection(Transport::Ipc, "k").unwrap();
        let b = dir.new_connection(Transport::Ipc, "k").unwrap();
        assert_ne!(a.key, b.key);
        assert!(a.ip.ends_with("kernel"));
    }
}
