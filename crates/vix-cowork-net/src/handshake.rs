//! The Noise handshake: `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s`.
//!
//! - **N**: the initiator (guest) has no static key.
//! - **K**: the responder's (host's) static key is known to the initiator in
//!   advance, from the invite, so the guest authenticates the host: a man in
//!   the middle without the host's private key cannot complete the handshake.
//! - **psk0**: the invite's secret is mixed in before the first message, so
//!   the host authenticates the guest as holder of an unspent invite; a
//!   stranger who reaches the port cannot get past the first message.
//!
//! Both sides get forward secrecy from fresh ephemeral keys. The connection
//! opens with a 13-byte plaintext preamble (magic, version, public token id)
//! so the host knows which secret to use; it carries nothing secret.
//!
//! The preamble id is also bound into the Noise *prologue*, so a man in the
//! middle who rewrites the id breaks the handshake rather than redirecting it.
//! After the two handshake messages both sides switch to a *stateless*
//! transport state: the caller supplies explicit nonces (see `link`), which
//! lets the send and receive halves live on different threads. Failures on the
//! host side are silent (the socket is dropped) so scanners learn nothing.
//! Spec: `crates/vix-cowork-net/spec/index.md`.

use crate::error::NetError;
use crate::invite::{ID_LEN, Invite, InviteTable};
use crate::link::{Link, LinkReader, read_frame, split, write_frame};
use snow::{Builder, Keypair};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// The Noise protocol name; fixes pattern, DH, cipher and hash.
const PARAMS: &str = "Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s";
/// First four bytes of every connection; lets the host reject non-Cowork
/// traffic (HTTP probes, port scanners) before spending any crypto.
const MAGIC: &[u8; 4] = b"VXCW";
/// The transport framing version, sent in the preamble.
pub const WIRE_VERSION: u8 = 1;
/// A first handshake message is about 48 bytes; anything near this is not one.
const MAX_HANDSHAKE_FRAME: usize = 1024;

/// The Noise prologue: a protocol label plus the public token id. Both sides
/// must agree on it byte for byte or the handshake fails authentication.
fn prologue(id: [u8; ID_LEN]) -> Vec<u8> {
    let mut p = b"vix-cowork/1".to_vec();
    p.extend_from_slice(&id);
    p
}

/// A Noise builder for [`PARAMS`]. Parsing the constant cannot realistically
/// fail; it is mapped to [`NetError::Crypto`] rather than unwrapped.
fn builder() -> Result<Builder<'static>, NetError> {
    let params = PARAMS.parse().map_err(|_| NetError::Crypto)?;
    Ok(Builder::new(params))
}

/// A fresh static keypair for one hosted session.
pub(crate) fn new_keypair() -> Result<Keypair, NetError> {
    Ok(builder()?.generate_keypair()?)
}

/// Connect to the host named in `invite` and complete the handshake.
///
/// # Errors
///
/// [`NetError::Io`] / [`NetError::Timeout`] if the host cannot be reached;
/// [`NetError::Crypto`] if the host is not the one the invite pins or the
/// invite is spent (the host simply closes the connection).
pub fn connect(invite: &Invite, timeout: Duration) -> Result<(Link, LinkReader), NetError> {
    let addr = invite
        .addr
        .to_socket_addrs()?
        .next()
        .ok_or(NetError::BadInvite)?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    // Preamble: 4 magic + 1 version + 8 token id = 13 bytes, plaintext.
    let mut pre = Vec::with_capacity(13);
    pre.extend_from_slice(MAGIC);
    pre.push(WIRE_VERSION);
    pre.extend_from_slice(&invite.id);
    std::io::Write::write_all(&mut stream, &pre)?;

    let pro = prologue(invite.id);
    let mut hs = builder()?
        .remote_public_key(&invite.host_key)?
        .psk(0, &invite.secret)?
        .prologue(&pro)?
        .build_initiator()?;
    // NK message 1 (`-> e, es`) carries no payload, so it is tiny.
    let mut buf = [0u8; 1024];
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(&mut stream, &buf[..n])?;

    // Message 2 (`<- e, ee`) completes the handshake; a wrong host key or a
    // spent invite makes the host close instead, which shows up here as an
    // error from `read_frame`/`read_message`.
    let reply = read_frame(&mut stream)?;
    if reply.len() > MAX_HANDSHAKE_FRAME {
        return Err(NetError::Protocol);
    }
    let mut payload = [0u8; 1024];
    hs.read_message(&reply, &mut payload)?;
    if !hs.is_handshake_finished() {
        return Err(NetError::Protocol);
    }
    let state = hs.into_stateless_transport_mode()?;
    split(state, &stream)
}

/// Answer one incoming connection. On success the invite is consumed.
///
/// A failed handshake is charged against the token (`record_failure`), so
/// three bad attempts revoke it. The token is consumed only *after* the
/// handshake succeeds, so a stranger who knows only the public id cannot burn
/// someone's invite without holding the secret.
///
/// # Errors
///
/// Any failure, after which the caller drops the socket without a reply: a
/// scanner or a wrong-secret guest learns nothing about which part failed.
pub(crate) fn accept_stream(
    mut stream: TcpStream,
    keypair: &Keypair,
    invites: &InviteTable,
    timeout: Duration,
) -> Result<(Link, LinkReader), NetError> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    // Cheap checks first: wrong magic or version never reaches the crypto.
    let mut pre = [0u8; 13];
    std::io::Read::read_exact(&mut stream, &mut pre)?;
    if &pre[..4] != MAGIC || pre[4] != WIRE_VERSION {
        return Err(NetError::Protocol);
    }
    let mut id = [0u8; ID_LEN];
    id.copy_from_slice(&pre[5..]);
    let secret = invites.lookup(&id).ok_or(NetError::UnknownInvite)?;

    // Run the handshake in a closure so every failure path funnels through the
    // single `record_failure` below.
    let result: Result<snow::StatelessTransportState, NetError> = (|| {
        let pro = prologue(id);
        let mut hs = builder()?
            .local_private_key(&keypair.private)?
            .psk(0, &secret)?
            .prologue(&pro)?
            .build_responder()?;
        let first = read_frame(&mut stream)?;
        if first.len() > MAX_HANDSHAKE_FRAME {
            return Err(NetError::Protocol);
        }
        let mut payload = [0u8; 1024];
        hs.read_message(&first, &mut payload)?;
        let mut buf = [0u8; 1024];
        let n = hs.write_message(&[], &mut buf)?;
        write_frame(&mut stream, &buf[..n])?;
        if !hs.is_handshake_finished() {
            return Err(NetError::Protocol);
        }
        Ok(hs.into_stateless_transport_mode()?)
    })();
    match result {
        Ok(state) => {
            // Only now is the token spent; a concurrent holder of the same
            // invite that also completed loses here.
            if !invites.consume(&id) {
                return Err(NetError::UnknownInvite);
            }
            split(state, &stream)
        }
        Err(e) => {
            invites.record_failure(&id);
            Err(e)
        }
    }
}
