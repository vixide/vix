# Cowork transport

Encrypted, authenticated TCP transport for Vix Cowork: Noise handshake, invite
tokens, framed messages. It carries the `vix-cowork` wire messages between a
host and its guests and decides nothing about what they mean; permissions and
sharing live in `vix-cowork`. The design is in
[`spec/cowork`](../../../spec/cowork/index.md).

## Security properties

- **No plaintext, no anonymous peers.** The only unencrypted bytes on a
  connection are a 13-byte preamble (`VXCW`, version, a public token id). Every
  later byte is a Noise transport frame. There is no unencrypted mode and no
  way to connect without an invite.
- **Mutual authentication.** The handshake is
  `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s`. *NK*: the guest knows the host's
  static public key in advance (it is in the invite) and pins it, so a man in
  the middle cannot pass as the host. *psk0*: the invite's 32-byte secret is
  mixed in before the first message, so only a holder of an unspent invite can
  complete the handshake.
- **Forward secrecy.** Both sides use fresh ephemeral keys. The host's static
  key is generated per hosted session and never stored.
- **Invites** are random (token id and secret from the OS), **single-use**
  (spent only when a handshake completes, atomically), **short-lived** (the
  caller picks the time to live), **capped** (16 outstanding) and **revoked
  after 3 failed handshakes** against the same token. Closing the listener or
  `revoke_invites` voids them all. The invite string is
  `vix-cowork:1:<base64url(host key | id | secret)>@host:port`; its `Debug`
  form never prints the secret.
- **Silent failure.** A bad preamble, an unknown or spent token or a failed
  handshake closes the socket without a reply, so a scanner learns nothing.
  The `Crypto` error carries no detail.
- **Host consent.** An authenticated guest is only a `Knock` (name and
  address). The host's UI approves or declines it; nothing is sent to it until
  approval, and the guest waits at most its consent timeout.
- **Denial-of-service bounds.** At most 8 handshakes run at once, each
  bounded by a 5 second timeout; at most 4 authenticated guests wait for a
  decision; frames are at most 65,535 bytes and a message is reassembled only
  up to `MAX_MESSAGE_BYTES` (16 MB); nonces are strictly counted so a dropped,
  reordered or replayed frame fails authentication.

## API

- `Listener::bind(addr)` — listen (an accept thread plus a short handshake
  thread per connection). `new_invite(advertise, ttl)`, `try_knock()`,
  `revoke_invites()`, `close()`. Binding a loopback address keeps the session
  on the machine; exposing it on a LAN is the host's explicit choice.
- `Knock { name, peer_addr }` — `approve()` yields the guest's `(Link,
  LinkReader)`; `decline(code)` tells it why and closes. Names are
  sanitized (control characters, escape sequences dropped).
- `join(invite, name, connect_timeout, consent_timeout)` — the guest side;
  returns the link halves and the host's `Welcome`, or `NetError::Refused`.
- `Link::send(&Message)` (cloneable, serialized), `Link::send_encoded(&[u8])`
  (an already encoded payload, for a message family this build does not
  model yet), `Link::close()`,
  `LinkReader::recv()` (blocking), `LinkReader::spawn(tag, tx)` (a forwarding
  thread ending in `LinkEvent::Closed`).

## Reserved messages

A decoded payload in a reserved tag range (`vix_cowork::wire::
RESERVED_CHAT_TAGS`, kept for session chat, T756) is skipped by
`LinkReader::recv` and the link stays open, so a newer peer's chat cannot end
an older peer's session. Any other undecodable payload still ends the link.

## Framing

`[u16 big-endian length][Noise ciphertext]`. A logical message is split into
chunks of at most 60,000 bytes; each chunk's plaintext is `[flag][bytes]` with
`flag = 1` on the last chunk, and each uses the next nonce.

## Not done

Reconnect and resync after a dropped connection (a guest rejoins with a new
invite today), NAT traversal or a relay, TLS or WebSocket carriage, and rate
limiting by source address beyond the global handshake cap.

## Tests

Unit tests for invites. `crates/vix-cowork-net/tests/loopback.rs` runs over real loopback sockets:
a complete host and guest session including a multi-frame snapshot; decline;
an unapproved guest receiving nothing; wrong secret, revoked, spent and
expired invites; a pinned-key mismatch; garbage and fake-preamble
connections; a recording proxy proving neither names, file names nor
document text appear on the wire; and a reserved (chat) message being skipped
while the session carries on.
