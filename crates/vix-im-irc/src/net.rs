//! The connection (feature `net`): one background thread per account that
//! dials TCP with a timeout, wraps it in TLS (`rustls`, certificates checked
//! against `webpki-roots`) unless the account says `tls = false`, runs the
//! [`Client`] state machine, and talks to the App over std channels.
//!
//! Nothing here logs: not message text, not credentials. Reconnecting is
//! left to the App (v1).

use std::io::{self, ErrorKind as IoKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustls::pki_types::ServerName;

use crate::client::{Client, Command, Config, Event};

/// Longest line accepted from a server: 512 bytes plus 8191 for IRCv3 tags.
/// A longer line is discarded rather than buffered without bound.
const MAX_RECV_LINE: usize = 512 + 8191;

/// Where and how to connect.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// The server host name (also the TLS server name) or IP address.
    pub host: String,
    /// The TCP port (6697 is the usual TLS port, 6667 plaintext).
    pub port: u16,
    /// Use TLS. `false` sends everything in the clear; the App shows a
    /// warning in the conversation when it is off.
    pub tls: bool,
    /// Who to register as.
    pub client: Config,
    /// Limit for the TCP connect (per resolved address).
    pub connect_timeout: Duration,
    /// Socket read timeout: how often the thread wakes to check for App
    /// commands, and so how quickly [`Handle::close`] returns.
    pub poll_interval: Duration,
}

impl ConnectOptions {
    /// Options with a 10 s connect timeout and a 100 ms poll interval.
    #[must_use]
    pub fn new(host: &str, port: u16, tls: bool, client: Config) -> ConnectOptions {
        ConnectOptions {
            host: host.to_string(),
            port,
            tls,
            client,
            connect_timeout: Duration::from_secs(10),
            poll_interval: Duration::from_millis(100),
        }
    }
}

enum Request {
    Command(Command),
    Close,
}

/// The App's side of a connection.
#[derive(Debug)]
pub struct Handle {
    tx: Sender<Request>,
    rx: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Request::Command(_) => "Command",
            Request::Close => "Close",
        })
    }
}

/// Start connecting in the background. Returns at once; progress arrives as
/// events ([`Event::Connected`] on success, [`Event::Disconnected`] when the
/// connection fails or ends).
#[must_use]
pub fn connect(options: ConnectOptions) -> Handle {
    let (tx, requests) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("vix-im-irc".to_string())
        .spawn(move || run(options, &requests, &events))
        .map_err(|e| e.to_string());
    let thread = match thread {
        Ok(t) => Some(t),
        Err(reason) => {
            // No thread: report it through the same channel the App reads.
            let (events, rx2) = mpsc::channel();
            let _ = events.send(Event::Disconnected { reason });
            return Handle {
                tx,
                rx: rx2,
                thread: None,
            };
        }
    };
    Handle { tx, rx, thread }
}

impl Handle {
    /// Queue a command for the connection. `false` when the connection
    /// thread has already ended.
    #[must_use = "false means the connection has ended"]
    pub fn send(&self, command: Command) -> bool {
        self.tx.send(Request::Command(command)).is_ok()
    }

    /// The next event, if one is waiting. Never blocks.
    #[must_use]
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Whether the connection thread has finished (after its final
    /// [`Event::Disconnected`]).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Send `QUIT`, close the socket and wait for the thread (at most about
    /// one poll interval plus the time to write the quit line). Events still
    /// queued, including the final [`Event::Disconnected`], are returned.
    #[must_use]
    pub fn close(mut self) -> Vec<Event> {
        self.shutdown();
        self.rx.try_iter().collect()
    }

    fn shutdown(&mut self) {
        let _ = self.tx.send(Request::Close);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A plaintext or TLS stream.
enum Transport {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(s) => s.read(buf),
            Transport::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(s) => s.write(buf),
            Transport::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Transport::Plain(s) => s.flush(),
            Transport::Tls(s) => s.flush(),
        }
    }
}

impl Transport {
    fn tcp(&self) -> &TcpStream {
        match self {
            Transport::Plain(s) => s,
            Transport::Tls(s) => s.get_ref(),
        }
    }
}

fn dial(options: &ConnectOptions) -> io::Result<TcpStream> {
    let addrs: Vec<SocketAddr> = (options.host.as_str(), options.port)
        .to_socket_addrs()?
        .collect();
    let mut last = io::Error::new(IoKind::NotFound, "no address for host");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, options.connect_timeout) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn tls_config() -> io::Result<Arc<rustls::ClientConfig>> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn open(options: &ConnectOptions) -> io::Result<Transport> {
    let tcp = dial(options)?;
    tcp.set_nodelay(true)?;
    // Writes get a fixed bound so a stalled server cannot hang `close`.
    tcp.set_write_timeout(Some(Duration::from_secs(10)))?;
    if !options.tls {
        tcp.set_read_timeout(Some(options.poll_interval))?;
        return Ok(Transport::Plain(tcp));
    }
    let name = ServerName::try_from(options.host.clone())
        .map_err(|e| io::Error::new(IoKind::InvalidInput, e))?;
    let conn = rustls::ClientConnection::new(tls_config()?, name).map_err(io::Error::other)?;
    let mut stream = rustls::StreamOwned::new(conn, tcp);
    // Finish the handshake under the connect timeout, so a certificate
    // problem is reported before registration starts.
    stream
        .sock
        .set_read_timeout(Some(options.connect_timeout))?;
    while stream.conn.is_handshaking() {
        stream.conn.complete_io(&mut stream.sock)?;
    }
    stream.sock.set_read_timeout(Some(options.poll_interval))?;
    Ok(Transport::Tls(Box::new(stream)))
}

fn write_lines(stream: &mut Transport, lines: &[String]) -> io::Result<()> {
    for line in lines {
        stream.write_all(line.as_bytes())?;
        stream.write_all(b"\r\n")?;
    }
    stream.flush()
}

/// Forward a step's events; `false` when the App has dropped its receiver.
fn forward(events: &Sender<Event>, step: Vec<Event>) -> bool {
    step.into_iter().all(|e| events.send(e).is_ok())
}

fn run(options: ConnectOptions, requests: &Receiver<Request>, events: &Sender<Event>) {
    let reason = match session(options, requests, events) {
        Ok(reason) => reason,
        Err(e) => e.to_string(),
    };
    let _ = events.send(Event::Disconnected { reason });
}

/// Run one connection to its end; the `Ok` value is the disconnect reason.
fn session(
    options: ConnectOptions,
    requests: &Receiver<Request>,
    events: &Sender<Event>,
) -> io::Result<String> {
    let mut stream = open(&options)?;
    // Move the config (and its password) into the client, which wipes the
    // password once SASL is done; no copy is left behind here.
    let mut client = Client::new(options.client);
    let start = client
        .start()
        .map_err(|e| io::Error::new(IoKind::InvalidInput, e))?;
    write_lines(&mut stream, &start.send)?;

    let mut pending: Vec<u8> = Vec::new();
    let mut overlong = false;
    let mut buf = [0u8; 4096];
    loop {
        // App requests first, so a close is never stuck behind a busy read.
        loop {
            match requests.try_recv() {
                Ok(Request::Command(cmd)) => {
                    let quitting = matches!(cmd, Command::Quit(_));
                    let step = client.command(cmd);
                    write_lines(&mut stream, &step.send)?;
                    if !forward(events, step.events) {
                        return Ok("closed".to_string());
                    }
                    if quitting {
                        let _ = stream.tcp().shutdown(std::net::Shutdown::Both);
                        return Ok("quit".to_string());
                    }
                }
                Ok(Request::Close) | Err(TryRecvError::Disconnected) => {
                    let step = client.command(Command::Quit(None));
                    let _ = write_lines(&mut stream, &step.send);
                    let _ = stream.tcp().shutdown(std::net::Shutdown::Both);
                    return Ok("closed".to_string());
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        let n = match stream.read(&mut buf) {
            Ok(0) => return Ok("connection closed by server".to_string()),
            Ok(n) => n,
            Err(e) if matches!(e.kind(), IoKind::WouldBlock | IoKind::TimedOut) => continue,
            Err(e) if e.kind() == IoKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        for &byte in &buf[..n] {
            if byte == b'\n' {
                if !overlong {
                    let text = String::from_utf8_lossy(&pending);
                    let step = client.handle_line(&text);
                    write_lines(&mut stream, &step.send)?;
                    if !forward(events, step.events) {
                        return Ok("closed".to_string());
                    }
                }
                pending.clear();
                overlong = false;
            } else if pending.len() < MAX_RECV_LINE {
                pending.push(byte);
            } else {
                overlong = true;
            }
        }
    }
}
