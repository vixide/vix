# IRCv3 client (vix-im-irc)

IRCv3 client for Vix IM: line codec, registration/SASL state machine,
UTF-8-safe splitting, and an optional TLS connection thread.

This is the IRC bridge of Vix's IM feature ([`spec/instant-message/index.md`](../../../spec/instant-message/index.md),
task T751): a line codec, a pure client state machine, and — behind the
crate feature `net` — a connection thread over TLS. The App (T752) enables
`net` under its `im` capability, owns the actions and the event pump, and
converts this crate's [`Event`](#events)s into `vix-im` messages. This crate
does **not** depend on `vix-im`; it has its own event types.

## Why it is shaped this way

1. **Everything received is untrusted.** The parser is total (it never
   panics; property-tested on arbitrary input), oversized lines are dropped
   by the connection, and unknown commands are ignored. Received text is
   passed on raw: sanitizing it for display (control characters, bidi
   overrides) is `vix-im`'s job, so there is one sanitizer.
2. **Nothing sent can carry a second command.** Every outgoing line is
   built by `Line::to_wire`, which refuses CR, LF and NUL in any part. Names
   (nicks, channels, targets) are *refused*, never cleaned, when they hold a
   space, comma, a leading `:` or any control character — cleaning a name
   could send a message to someone else. Message bodies are cleaned: each
   line break starts a new `PRIVMSG`, NUL and CR are dropped. Part/quit
   reasons and topics have line breaks turned into spaces.
3. **Credentials are used once.** The SASL password is supplied by the
   caller (the App reads it from `VIX_IM_PASSWORD_<NAME>`), is wrapped in
   `Password` whose `Debug` prints `<redacted>`, is wiped (`zeroize`) right
   after the `AUTHENTICATE` reply is built, and the whole SASL config is
   dropped when negotiation ends — success, failure or a server without CAP.
   Nothing in this crate logs.
4. **No surprises for the user's privacy.** CTCP requests other than
   `ACTION` (`VERSION`, `TIME`, `PING`, ...) are ignored and never answered.

## Codec (`codec`, pure)

- `Line { tags, prefix, command, params }`. `Line::parse(&str)` accepts an
  optional `@tags`, an optional `:prefix` (`Prefix { nick, user, host }`), a
  command (upper-cased) and parameters, the last of which may be a
  `:trailing` one; runs of spaces and a trailing CR/LF are tolerated. It
  fails only with `ParseError::NoCommand`.
- `Line::to_wire()` writes a line without CRLF, adding `:` to the last
  parameter when it is empty, has a space or starts with `:`; it fails with
  `EncodeError` on CR/LF/NUL or a malformed middle parameter.
- IRCv3 tag values: `unescape_tag_value` / `escape_tag_value` (`\:` `\s`
  `\\` `\r` `\n`; an unknown escape stands for its character, a trailing
  backslash is dropped). A key with an empty value reads as no value.
- `server_time_millis("2026-10-29T12:00:00.000Z")` → UTC milliseconds since
  the epoch (fraction optional, padded or truncated to milliseconds);
  anything else → `None`.

## CTCP (`ctcp`) and ISUPPORT (`isupport`)

- `encode_action` / `decode_action` for `\x01ACTION text\x01` (closing
  `\x01` optional on input); `is_ctcp`.
- `Isupport { casemapping, nicklen, chantypes }` from `005` tokens
  (`-TOKEN` resets to the default). `CaseMapping::{Ascii, Rfc1459,
  StrictRfc1459}` with `fold` and `same`; an unknown mapping (such as
  `rfc7613`) falls back to `ascii`. Defaults: `rfc1459`, no `NICKLEN`,
  `CHANTYPES=#&`. Nicks are compared with the server's mapping, not ASCII
  lowercasing alone (Worldwide).

## Splitting (`split`, pure)

`MAX_LINE` is 512 bytes including CRLF. `privmsg_lines(target, text,
action, max_line)` returns `PRIVMSG target :chunk` lines, each fitting
`max_line` with its CRLF (and CTCP framing for actions). `split_utf8` never
cuts inside a UTF-8 character and prefers to break after the last space
that fits; the chunks concatenate back to the original text exactly. The
client passes `max_line` = 512 minus room for the `:nick!~user@host ` prefix
(host up to 63 bytes) the server adds when relaying, so other people
receive every chunk whole.

## Client state machine (`client`, pure)

`Client::new(Config { nick, user, realname, channels, sasl })`, then:

- `start()` → `CAP LS 302`, `NICK`, `USER <user> 0 * :<realname>`
  (refuses a bad nick or user name with `TextError::BadName`).
- `handle_line(&str)` → `Step { send, events }`.
- `command(Command)` → `Step`. A refused command sends nothing and yields
  `Event::Error { kind: InvalidCommand, .. }`.

Registration: collect `CAP LS` (multi-line `*` continuation); request
whichever of `server-time message-tags sasl account-tag` are offered (`sasl`
only with credentials and when the offered mechanisms include `PLAIN` or are
unlisted); on `ACK` with `sasl` send `AUTHENTICATE PLAIN`, answer
`AUTHENTICATE +` with base64 of `\0user\0password` in 400-byte pieces (a
final `+` when the length is a multiple of 400); `903` → `CAP END`;
`902`/`904`–`908` → `Event::Error { kind: SaslFailed }` then `CAP END`
(registration continues unauthenticated; the App shows the error). `NAK` or
nothing to request → `CAP END`. A server without CAP support may send `001`
directly. `433` before `001` appends `_` (within `NICKLEN`, replacing the
last character when full), at most 5 times; after that, or after
registration, `433` is `Event::Error { kind: NickInUse }`. `001` →
`Event::Connected { nick }` and one `JOIN` per configured or queued channel.
`PING` → `PONG` with the same parameters, in any phase.

Commands: `Join` (queued until `001`), `Part`, `Privmsg`, `Action`
(`/me`), `Nick`, `Topic` (`None` queries), `Quit`. `Part`, `Privmsg`,
`Action` and `Topic` before registration are refused (`NotRegistered`).
Our own messages are not echoed as events (`echo-message` is not
requested); the App records what it sent.

### Events

`Connected { nick }`, `Joined { channel }`, `Parted { channel, reason }`
(our own `PART`, or a `KICK` of us), `UserJoined`, `UserParted` (part or
kick), `UserQuit`, `Message { target, from, from_account, text, is_action,
time }`, `Notice { target, from, text, time }`, `Topic { channel, topic,
set_by }` (`332`, `331`, `TOPIC`), `Names { channel, nicks }` (`353` lines
gathered until `366`, status prefixes removed), `NickChanged { old, new,
is_self }`, `Error { kind, detail }` (`NickInUse`, `SaslFailed`,
`Numeric(400..=599)`, `Server` for `ERROR`, `InvalidCommand`), and
`Disconnected { reason }` (from the connection).

`from_account` is set only when the server acknowledged `account-tag`, so
a client-sent `account` tag cannot fake a verified identity; without it the
App marks the nick `(unverified)`. `time` is the `server-time` value, or
`None` (the App uses its own clock).

## Connection (`net`, feature `net`)

`net::connect(ConnectOptions { host, port, tls, client, connect_timeout,
poll_interval })` returns a `Handle` at once and does the work on a thread
named `vix-im-irc`:

- resolve, then `TcpStream::connect_timeout` per address; 10 s write
  timeout;
- with `tls = true` (the default the App uses), a `rustls` client with the
  `ring` provider and `webpki-roots` trust anchors, the handshake completed
  under the connect timeout; `tls = false` is plaintext (the App warns);
- the read timeout is `poll_interval` (default 100 ms): between reads the
  thread drains App commands, so `Handle::close` returns within about one
  interval;
- received bytes are split on LF, decoded as UTF-8 lossily, and a line
  longer than 512 + 8191 bytes is discarded.

`Handle::send(Command) -> bool` (false once the thread has ended),
`try_recv() -> Option<Event>` (never blocks), `is_finished()`, and
`close() -> Vec<Event>` (sends `QUIT`, shuts the socket, joins the thread
and returns the remaining events, ending with `Disconnected`). Dropping a
`Handle` closes it too. Every failure — resolve, connect, TLS, I/O — ends
with `Event::Disconnected { reason }`. Reconnecting is left to the App (v1).

## Tests

Unit tests per module; `crates/vix-im-irc/tests/props.rs` (proptest: the parser and the
state machine never panic, write→parse round-trips, tag escaping
round-trips, splitting is lossless and bounded, no command can inject a
second line); `crates/vix-im-irc/tests/fake_server.rs` (feature `net`): an in-process fake
server on `127.0.0.1` over plaintext exercising registration, SASL, a taken
nick, join, names, `PING`, a message round trip with `server-time` and
`account` tags, `/me`, `QUIT`, `close`, a server `ERROR`, a refused
connection, and TLS against a non-TLS peer failing cleanly. No test uses
the real network.
