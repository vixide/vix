//! The host side: listen, authenticate, and hand each guest to the UI for
//! consent.
//!
//! A [`Listener`] runs an accept thread. Each connection gets a short-lived
//! handshake thread (at most [`MAX_HANDSHAKES`] at once, each bounded by a
//! timeout), so a stalled or hostile connection cannot block the others or the
//! UI. A connection that completes the Noise handshake and sends a valid
//! `Hello` becomes a [`Knock`]: authenticated, but **not yet admitted**. The
//! host's UI shows the guest's name and either approves it (the session then
//! sends `Welcome`) or declines it. Nothing is shared with a knock.
//!
//! Resource bounds (all enforced with counters, not queues): [`MAX_HANDSHAKES`]
//! in-flight handshakes, [`MAX_PENDING_KNOCKS`] undecided guests, a
//! 5-second handshake/`Hello` deadline, and the invite table's own cap. Spec:
//! `crates/vix-cowork-net/spec/index.md`.

use crate::error::NetError;
use crate::handshake::{accept_stream, new_keypair};
use crate::invite::{Invite, InviteTable, KEY_LEN};
use crate::link::{Link, LinkReader};
use snow::Keypair;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;
use vix_cowork::peer::sanitize_name;
use vix_cowork::wire::{ErrorCode, Message, PROTOCOL_VERSION};

/// Concurrent in-flight handshakes; further connections are dropped.
pub const MAX_HANDSHAKES: usize = 8;
/// Authenticated guests waiting for the host's decision; further ones are
/// refused with [`ErrorCode::Declined`].
pub const MAX_PENDING_KNOCKS: usize = 4;
/// Time a connection gets to finish the handshake and say `Hello`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// RAII guard for one unit of a bounded counter: decrements it on drop, so a
/// slot is released on every exit path (including panics and early returns).
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Claim a slot if fewer than `max` are in use. Increment-then-check keeps it
/// race-free without a lock: an over-limit claimant undoes its increment.
fn take_slot(counter: &Arc<AtomicUsize>, max: usize) -> Option<Slot> {
    let prev = counter.fetch_add(1, Ordering::SeqCst);
    if prev >= max {
        counter.fetch_sub(1, Ordering::SeqCst);
        None
    } else {
        Some(Slot(Arc::clone(counter)))
    }
}

/// An authenticated guest waiting for the host's decision.
#[derive(Debug)]
pub struct Knock {
    /// The name the guest asked to be shown as (already sanitized).
    pub name: String,
    /// The guest's network address, for the consent prompt.
    pub peer_addr: Option<SocketAddr>,
    link: Link,
    reader: LinkReader,
    /// Holds a pending-knock slot until the knock is approved, declined or
    /// dropped (never read; kept for its `Drop`).
    _slot: Slot,
}

/// `Slot` wraps an atomic counter with nothing useful to print.
impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Slot")
    }
}

impl Knock {
    /// Let the guest in: returns its link halves. The caller must then send
    /// the session's `Welcome` and start a reader (see
    /// [`LinkReader::spawn`]).
    #[must_use]
    pub fn approve(self) -> (Link, LinkReader) {
        (self.link, self.reader)
    }

    /// Turn the guest away with `code` and close the connection.
    pub fn decline(self, code: ErrorCode) {
        let _ = self.link.send(&Message::Error { code });
        self.link.close();
    }
}

/// The host's listening socket and its invites.
pub struct Listener {
    local: SocketAddr,
    /// This session's static public key, put into every minted invite.
    key: [u8; KEY_LEN],
    invites: Arc<InviteTable>,
    /// Tells the accept thread to exit.
    stop: Arc<AtomicBool>,
    /// Receiver for authenticated guests; `Mutex` makes `Listener: Sync`.
    knocks: Mutex<Receiver<Knock>>,
    /// The accept thread; `None` once joined.
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listener")
            .field("local", &self.local)
            .finish_non_exhaustive()
    }
}

impl Listener {
    /// Listen on `addr` with a fresh static key (valid for this session
    /// only). Bind to a loopback address unless the host chose to expose the
    /// session; the handshake protects either way.
    ///
    /// # Errors
    ///
    /// When the socket cannot be bound.
    pub fn bind(addr: SocketAddr) -> Result<Self, NetError> {
        let socket = TcpListener::bind(addr)?;
        // Non-blocking so the accept loop can poll `stop` and shut down.
        socket.set_nonblocking(true)?;
        let local = socket.local_addr()?;
        let keypair = new_keypair()?;
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&keypair.public);
        let invites = Arc::new(InviteTable::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let thread = {
            let invites = Arc::clone(&invites);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || accept_loop(&socket, &keypair, &invites, &stop, &tx))
        };
        Ok(Self {
            local,
            key,
            invites,
            stop,
            knocks: Mutex::new(rx),
            thread: Some(thread),
        })
    }

    /// The address actually bound (useful after binding port 0).
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Mint a single-use invite valid for `ttl`, to be handed to one guest
    /// out of band. `advertise` is the `host:port` the guest should dial
    /// (the bound address, or the host's LAN name or tunnel endpoint).
    ///
    /// # Errors
    ///
    /// When too many invites are outstanding.
    pub fn new_invite(&self, advertise: &str, ttl: Duration) -> Result<Invite, NetError> {
        let (id, secret) = self.invites.mint(ttl)?;
        Ok(Invite {
            addr: advertise.to_string(),
            host_key: self.key,
            id,
            secret,
        })
    }

    /// Revoke every outstanding invite.
    pub fn revoke_invites(&self) {
        self.invites.revoke_all();
    }

    /// The next authenticated guest awaiting a decision, without blocking.
    #[must_use]
    pub fn try_knock(&self) -> Option<Knock> {
        self.knocks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_recv()
            .ok()
    }

    /// Stop listening and revoke all invites. Guests already admitted keep
    /// their links.
    pub fn close(mut self) {
        self.shutdown();
    }

    /// Stop the accept thread and revoke invites. Idempotent: `close` and
    /// `Drop` may both run it.
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.invites.revoke_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The accept thread body. Polls a non-blocking socket (sleeping 20 ms when
/// idle, 100 ms after a hard error) until `stop`, spawning one handshake thread
/// per connection within the [`MAX_HANDSHAKES`] bound. The `Keypair` is copied
/// into an `Arc` because the handshake threads outlive this borrow.
fn accept_loop(
    socket: &TcpListener,
    keypair: &Keypair,
    invites: &Arc<InviteTable>,
    stop: &AtomicBool,
    tx: &Sender<Knock>,
) {
    let handshakes = Arc::new(AtomicUsize::new(0));
    let pending = Arc::new(AtomicUsize::new(0));
    let key = Arc::new(Keypair {
        private: keypair.private.clone(),
        public: keypair.public.clone(),
    });
    while !stop.load(Ordering::SeqCst) {
        match socket.accept() {
            Ok((stream, _)) => {
                let Some(hs_slot) = take_slot(&handshakes, MAX_HANDSHAKES) else {
                    continue; // over the limit: drop the connection
                };
                // Accepted sockets inherit non-blocking mode on some
                // platforms; the handshake wants timeouts instead.
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                let (invites, key, tx, pending) = (
                    Arc::clone(invites),
                    Arc::clone(&key),
                    tx.clone(),
                    Arc::clone(&pending),
                );
                std::thread::spawn(move || {
                    let _hs_slot = hs_slot;
                    let Ok((link, mut reader)) =
                        accept_stream(stream, &key, &invites, HANDSHAKE_TIMEOUT)
                    else {
                        return;
                    };
                    let _ = reader.set_timeout(Some(HANDSHAKE_TIMEOUT));
                    // The guest must speak first, within the same deadline.
                    let Ok(Message::Hello { version, name }) = reader.recv() else {
                        link.close();
                        return;
                    };
                    if version != PROTOCOL_VERSION {
                        let _ = link.send(&Message::Error {
                            code: ErrorCode::Version,
                        });
                        link.close();
                        return;
                    }
                    // Too many undecided guests: refuse early rather than queue.
                    let Some(slot) = take_slot(&pending, MAX_PENDING_KNOCKS) else {
                        let _ = link.send(&Message::Error {
                            code: ErrorCode::Declined,
                        });
                        link.close();
                        return;
                    };
                    // The host may take a while to decide; the reader waits
                    // indefinitely from here on.
                    let _ = reader.set_timeout(None);
                    // The guest chose `name`; sanitize before any UI shows it.
                    let knock = Knock {
                        name: sanitize_name(&name),
                        peer_addr: link.peer_addr(),
                        link,
                        reader,
                        _slot: slot,
                    };
                    let _ = tx.send(knock);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// What a guest receives from [`join`]: both link halves plus the host's
/// `Welcome`, already read.
#[derive(Debug)]
pub struct Joined {
    /// Send half.
    pub link: Link,
    /// Receive half.
    pub reader: LinkReader,
    /// The host's `Welcome` message.
    pub welcome: Message,
}

/// Join a session from an invite: connect, authenticate, say `Hello`, and wait
/// up to `consent_timeout` for the host to approve.
///
/// # Errors
///
/// A connection or authentication failure (see
/// [`connect`](crate::connect)), [`NetError::Refused`] if the host declined or
/// speaks another protocol version, or [`NetError::Timeout`] if the host never
/// decided.
pub fn join(
    invite: &Invite,
    name: &str,
    connect_timeout: Duration,
    consent_timeout: Duration,
) -> Result<Joined, NetError> {
    let (link, mut reader) = crate::handshake::connect(invite, connect_timeout)?;
    link.send(&Message::Hello {
        version: PROTOCOL_VERSION,
        name: name.to_string(),
    })?;
    // The host's human may take a while; wait up to `consent_timeout` only for
    // the decision, then go back to waiting indefinitely.
    reader.set_timeout(Some(consent_timeout))?;
    let welcome = reader.recv()?;
    reader.set_timeout(None)?;
    match welcome {
        Message::Welcome { .. } => Ok(Joined {
            link,
            reader,
            welcome,
        }),
        Message::Error { code } => Err(NetError::Refused(code)),
        _ => Err(NetError::Protocol),
    }
}
