//! The connection file (`kernel-*.json`) a kernel is started with.
//!
//! It names the transport, the address, the five channel ports and the
//! signing key. This module only models it: the transport crate writes it
//! into a fresh `0700` directory as a `0600` file, and never puts the key on
//! a command line or in a log. [`ConnectionInfo`]'s `Debug` form redacts the
//! key.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::wire::{KeyError, SIGNATURE_SCHEME, Signer};

/// Largest connection file [`ConnectionInfo::parse`] reads.
pub const MAX_CONNECTION_FILE_BYTES: usize = 16 * 1024;

/// The ZeroMQ transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// TCP; Vix binds only to loopback.
    Tcp,
    /// Unix-domain sockets; `ip` is a path prefix.
    Ipc,
}

/// One of the kernel's five channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Requests and replies (DEALER ↔ ROUTER).
    Shell,
    /// Broadcast outputs and status (SUB ↔ PUB).
    IoPub,
    /// Kernel asks the client for input (unused in v1).
    Stdin,
    /// Interrupt and shutdown, ahead of the shell queue.
    Control,
    /// Heartbeat echo.
    Heartbeat,
}

/// A parsed connection file.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// `tcp` or `ipc`.
    pub transport: Transport,
    /// TCP: the address. IPC: the socket path prefix.
    pub ip: String,
    /// Shell port (IPC: path suffix).
    pub shell_port: u16,
    /// IOPub port.
    pub iopub_port: u16,
    /// Stdin port.
    pub stdin_port: u16,
    /// Control port.
    pub control_port: u16,
    /// Heartbeat port.
    pub hb_port: u16,
    /// The HMAC key. Secret.
    pub key: String,
    /// Always `hmac-sha256` for Vix.
    pub signature_scheme: String,
    /// The kernelspec name, informational.
    #[serde(default)]
    pub kernel_name: String,
}

impl fmt::Debug for ConnectionInfo {
    /// Hand-written `Debug` that redacts the HMAC `key`, so logging a connection never leaks the signing secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionInfo")
            .field("transport", &self.transport)
            .field("ip", &self.ip)
            .field("shell_port", &self.shell_port)
            .field("iopub_port", &self.iopub_port)
            .field("stdin_port", &self.stdin_port)
            .field("control_port", &self.control_port)
            .field("hb_port", &self.hb_port)
            .field("key", &"<redacted>")
            .field("signature_scheme", &self.signature_scheme)
            .field("kernel_name", &self.kernel_name)
            .finish()
    }
}

/// Why a connection file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionError {
    /// Larger than [`MAX_CONNECTION_FILE_BYTES`].
    TooLarge,
    /// Not JSON of the right shape.
    Malformed,
    /// A signature scheme other than `hmac-sha256` (named).
    UnsupportedScheme(String),
    /// An empty key.
    EmptyKey,
}

impl fmt::Display for ConnectionError {
    /// Renders a fixed human-readable message for the error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("connection file too large"),
            Self::Malformed => f.write_str("malformed connection file"),
            Self::UnsupportedScheme(s) => write!(f, "unsupported signature scheme {s:?}"),
            Self::EmptyKey => f.write_str("connection file has an empty key"),
        }
    }
}

impl std::error::Error for ConnectionError {}

impl ConnectionInfo {
    /// A connection with `key` on the given transport, address and ports
    /// (`[shell, iopub, stdin, control, hb]`).
    #[must_use]
    pub fn new(
        transport: Transport,
        ip: &str,
        ports: [u16; 5],
        key: &str,
        kernel_name: &str,
    ) -> Self {
        let [shell_port, iopub_port, stdin_port, control_port, hb_port] = ports;
        Self {
            transport,
            ip: ip.to_owned(),
            shell_port,
            iopub_port,
            stdin_port,
            control_port,
            hb_port,
            key: key.to_owned(),
            signature_scheme: SIGNATURE_SCHEME.to_owned(),
            kernel_name: kernel_name.to_owned(),
        }
    }

    /// Parse and validate a connection file.
    ///
    /// # Errors
    ///
    /// A [`ConnectionError`]: too large, malformed, an unsupported scheme or
    /// an empty key.
    pub fn parse(json: &str) -> Result<Self, ConnectionError> {
        if json.len() > MAX_CONNECTION_FILE_BYTES {
            return Err(ConnectionError::TooLarge);
        }
        let info: Self = serde_json::from_str(json).map_err(|_| ConnectionError::Malformed)?;
        if info.signature_scheme != SIGNATURE_SCHEME {
            return Err(ConnectionError::UnsupportedScheme(info.signature_scheme));
        }
        if info.key.is_empty() {
            return Err(ConnectionError::EmptyKey);
        }
        Ok(info)
    }

    /// The file's JSON text.
    #[must_use]
    pub fn to_json(&self) -> String {
        // A struct of strings and integers always serializes.
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The port (IPC: path suffix) of `channel`.
    #[must_use]
    pub fn port(&self, channel: Channel) -> u16 {
        match channel {
            Channel::Shell => self.shell_port,
            Channel::IoPub => self.iopub_port,
            Channel::Stdin => self.stdin_port,
            Channel::Control => self.control_port,
            Channel::Heartbeat => self.hb_port,
        }
    }

    /// The ZeroMQ endpoint of `channel`: `tcp://ip:port`, or
    /// `ipc://ip-port` as `jupyter_client` spells it.
    #[must_use]
    pub fn endpoint(&self, channel: Channel) -> String {
        let port = self.port(channel);
        match self.transport {
            Transport::Tcp if self.ip.contains(':') => format!("tcp://[{}]:{port}", self.ip),
            Transport::Tcp => format!("tcp://{}:{port}", self.ip),
            Transport::Ipc => format!("ipc://{}-{port}", self.ip),
        }
    }

    /// Whether the kernel is reachable only from this machine: IPC, or TCP
    /// on a loopback address.
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self.transport {
            Transport::Ipc => true,
            Transport::Tcp => self
                .ip
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback()),
        }
    }

    /// A [`Signer`] for this connection's key.
    ///
    /// # Errors
    ///
    /// [`KeyError`] when the key is empty.
    pub fn signer(&self) -> Result<Signer, KeyError> {
        Signer::new(self.key.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "shell_port": 53794, "iopub_port": 53795, "stdin_port": 53796,
      "control_port": 53797, "hb_port": 53798, "ip": "127.0.0.1",
      "key": "a0436f6c-1916-498b-8eb9-e81ab9368e84", "transport": "tcp",
      "signature_scheme": "hmac-sha256", "kernel_name": "python3"
    }"#;

    #[test]
    fn parses_jupyter_client_output() {
        let info = ConnectionInfo::parse(SAMPLE).unwrap();
        assert_eq!(info.endpoint(Channel::Shell), "tcp://127.0.0.1:53794");
        assert_eq!(info.endpoint(Channel::IoPub), "tcp://127.0.0.1:53795");
        assert!(info.is_local());
        assert_eq!(ConnectionInfo::parse(&info.to_json()), Ok(info));
    }

    #[test]
    fn ipc_endpoints() {
        let info = ConnectionInfo::new(
            Transport::Ipc,
            "/run/vix/kernel",
            [1, 2, 3, 4, 5],
            "k",
            "py",
        );
        assert_eq!(info.endpoint(Channel::Control), "ipc:///run/vix/kernel-4");
        assert!(info.is_local());
    }

    #[test]
    fn remote_tcp_is_not_local() {
        let info = ConnectionInfo::new(Transport::Tcp, "10.0.0.2", [1, 2, 3, 4, 5], "k", "py");
        assert!(!info.is_local());
        let v6 = ConnectionInfo::new(Transport::Tcp, "::1", [1, 2, 3, 4, 5], "k", "py");
        assert!(v6.is_local());
        assert_eq!(v6.endpoint(Channel::Shell), "tcp://[::1]:1");
    }

    #[test]
    fn rejects_bad_files() {
        assert_eq!(ConnectionInfo::parse("{"), Err(ConnectionError::Malformed));
        let md5 = SAMPLE.replace("hmac-sha256", "hmac-md5");
        assert_eq!(
            ConnectionInfo::parse(&md5),
            Err(ConnectionError::UnsupportedScheme("hmac-md5".into()))
        );
        let empty = SAMPLE.replace("a0436f6c-1916-498b-8eb9-e81ab9368e84", "");
        assert_eq!(
            ConnectionInfo::parse(&empty),
            Err(ConnectionError::EmptyKey)
        );
        let huge = " ".repeat(MAX_CONNECTION_FILE_BYTES + 1);
        assert_eq!(ConnectionInfo::parse(&huge), Err(ConnectionError::TooLarge));
    }

    #[test]
    fn debug_redacts_the_key() {
        let info = ConnectionInfo::parse(SAMPLE).unwrap();
        let shown = format!("{info:?}");
        assert!(!shown.contains("a0436f6c"));
        assert!(shown.contains("<redacted>"));
    }
}
