# Matrix helper (vix-matrix)

The `vix-matrix` program: a Matrix client that Vix starts as a child process
and drives over JSON lines on stdin and stdout. It is built on
[`matrix-sdk`](https://github.com/matrix-org/matrix-rust-sdk) 0.19 with its
SQLite store and end-to-end encryption (Olm/Megolm through vodozemac).

This is the helper half of the Matrix bridge of Vix's IM feature (task
T793). The decision to run Matrix out of process, and why, is
[`spec/instant-message/matrix.md`](../../../spec/instant-message/matrix.md).
Vix's half, and the protocol itself, is the `vix-im-matrix` crate
([`crates/vix-im-matrix/spec/index.md`](../../../crates/vix-im-matrix/spec/index.md)).
The helper depends on that crate for the protocol types and the codec and
never redefines them.

## Why it is shaped this way

1. **Its own workspace.** `matrix/` is a separate Cargo workspace with its
   own lockfile, like `gui/`. `matrix-sdk-sqlite` needs `libsqlite3-sys`
   0.38 and the DB workbench's `sqlx` needs it below 0.38, and Cargo allows
   one `links = "sqlite3"` package per lockfile. The root workspace never
   builds the helper, and the helper links nothing of Vix but
   `vix-im-matrix`'s protocol types.
2. **stdout is the protocol.** Only reply lines are written there, by one
   writer thread. Every line is flushed. `Ready` is always the first line
   and `Closed` always the last. Diagnostics go to stderr.
3. **Secrets stay out of logs and argv.** No message body, access token,
   key, passphrase or password is logged, printed or passed as a process
   argument. `tracing` is capped at `warn` whatever `RUST_LOG` says,
   because the SDK's debug output can include event contents.
4. **Testable without a server or a keyring.** The request loop takes its
   keyring (a `SecretStore` trait) and data directory as parameters. The
   tests run it over in-memory pipes, an in-memory keyring and a scratch
   directory. No test contacts a Matrix server, creates an account or
   touches the OS keyring.

## Building

```sh
cargo build --release --manifest-path matrix/Cargo.toml
# put matrix/target/release/vix-matrix beside `vix`, on PATH, or at im.matrix_helper
```

TLS is rustls with the `ring` provider, installed as the process default
at startup. `matrix-sdk`'s default features are off, which keeps its
`rustls-aws-lc-rs` feature, and so aws-lc, out of the build. SQLite is
built from source (`bundled-sqlite`), so a binary does not depend on the
system's SQLite version.

Measured on 2026-11-10 (aarch64-apple-darwin, Apple M4 Max, rustc 1.99.1,
`[profile.release]` with `opt-level = "z"`, thin LTO and strip, sources
prefetched): a clean release build takes **79 s** wall clock, and the
stripped binary is **28.3 MB** (28,334,880 bytes; it links only system
frameworks, `libiconv` and `libSystem`). The lockfile has **400** packages
(all targets), of which **289** are in the macOS normal and build tree. Linux
and Windows builds are unmeasured.

## Input and output

- Requests arrive on stdin, one JSON object per line
  (`vix_im_matrix::decode_request`). A line longer than
  `vix_im_matrix::MAX_LINE` is discarded without being buffered whole.
  A blank line is ignored. A line that is not UTF-8, is too long, or does
  not decode gets `Error { key: bad_request }` and the helper keeps reading.
  Its `detail` names the kind of failure only (`not_utf8`, `too_long`,
  `not_an_object`, `version`, `invalid`). serde's message is never passed
  on because it can quote the line, and the line can hold a password.
- End of input is handled like `Quit`.
- Replies leave on stdout, one `vix_im_matrix::encode_reply` line each.

## Store and keyring

- **Data directory**: Vix's own (`vix_settings::Settings::data_dir`: the
  platform data directory plus `vix`), or `$VIX_MATRIX_DATA_DIR` when that
  is set and not empty (for packaging and tests).
- **Store directory**: `<data dir>/matrix/<account>/`, where `<account>` is
  the `[[im.accounts]]` name escaped as Vix escapes IM history file names.
  ASCII letters, digits, `-`, `_` and a `.` after the first byte are kept,
  and every other byte becomes `%XX`. An account name therefore cannot name
  `..`, a separator or an absolute path, and no two names share a
  directory. An empty account name is refused (`bad_request`). On Unix the
  directory is created with mode `0700`.
- **The store**: `matrix-sdk`'s SQLite state, crypto and event-cache
  stores, encrypted with a random passphrase (32 bytes from the OS, as hex).
- **The keyring**: one entry per account, service `vix-matrix`, user = the
  account name. It holds one JSON object with the homeserver as configured
  and as discovered, the user id, the device id, the access token and the
  store passphrase. The account password is never stored. macOS uses the
  login Keychain and Windows the Credential Manager, through `keyring-core`
  1 (`apple-native-keyring-store`, `windows-native-keyring-store`). Linux
  runs `secret-tool` (`lookup` / `store` / `clear`), with the arguments as
  argv, no shell, and the secret on stdin. Other platforms have no keyring,
  so Login answers `keyring`.

## Login

1. If a session is already running, `bad_request` ("already logged in").
2. Read the account's keyring entry. If the keyring fails, `keyring`.
3. The saved session is used only if it is for the same user (the full id,
   or its local part) and the same homeserver as configured (compared
   without the scheme or a trailing slash, ignoring case). It is resumed
   over the existing store. A missing store is a `store` error: recreating
   it would give the device a new, untrusted identity.
4. Otherwise, or when resuming fails and a password was given, log in with
   the password. Without a password the answer is `no_session`. A fresh
   login first deletes whatever store the account had, creates a new
   encrypted one, and logs in with the device display name "Vix" (server
   discovery accepts a server name or a URL). It then saves the session to
   the keyring. If that save fails, `keyring` is sent and the session runs
   anyway, but it cannot be resumed next time.
5. Send `LoggedIn` (`restored` tells resume from new login), then a `Room`
   for every joined room, then start the background sync.

Login errors: a server refusal is `login_failed` with the server's error
code as `detail`. An unreachable server or an invalid homeserver is
`network`. A store that cannot be created or opened is `store`.

## Sync

The sync runs in the background as repeated `/sync` long polls (30 s).

- **Rooms**: a joined room that has not been announced yet, or whose name,
  encryption or direct flag changed, gets a `Room`. All rooms are checked
  again when global account data (`m.direct`) changes. A room in the
  response's `left` section that had been announced gets a `Left`. That
  covers leaving from another client, kicks and bans.
- **Messages**: `m.room.message` events with `msgtype` `m.text`, `m.emote`
  or `m.notice` become `Message`, with the plain `body` only (a formatted
  body is dropped). `encrypted` is set when the event was decrypted.
  `verified` is true only when the sending device is cross-signed by its
  owner: the SDK's `Verified` state, or `Unverified(UnverifiedIdentity)`
  (signed by an identity we have not verified ourselves). A changed identity
  (a verification violation), an unsigned device or an unknown device give
  `false`, and so does every unencrypted message. `sender_name` is the
  sender's display name in the room, from the store. `time` is the origin
  server timestamp (UTC, ms). Other event types and other message types
  (images, files) are skipped. An edit arrives as its fallback body
  (`* new text`).
- **Undecryptable**: an encrypted event with no key gives `Undecryptable`.
  An event that becomes decryptable later, when a key arrives, is not sent
  again in this slice.
- **Our own echoes** are skipped: an event from our user with
  `unsigned.transaction_id` (only the sending device sees one), or one
  decrypted as sent by this device.
- **First sync**: after a new login, the first sync's timeline (history from
  before this device existed) is not sent. Only rooms are announced. After a
  resume, the first sync carries what arrived while the helper was not
  running, and it is sent.
- **Failures**: the first failure of an outage sends `network` (with the
  server's error code, if any). Sync then retries with backoff from 1 s,
  doubling up to 60 s. A revoked access token (`M_UNKNOWN_TOKEN`) stops
  the sync, deletes the keyring entry so the next Login uses the password,
  and sends `login_failed` with detail `M_UNKNOWN_TOKEN`.

## Requests

| Request | Before login | After login |
| --- | --- | --- |
| `login` | see [Login](#login) | `bad_request` |
| `send` | `not_logged_in` | sends `m.text` / `m.emote` / `m.notice` with a plain body; no reply on success, `send_failed` (with the error code) on failure; a room id that does not parse, or names no joined room, is `unknown_room` |
| `join` | `not_logged_in` | joins by id or alias, then sends `Room`; `join_failed` on failure |
| `leave` | `not_logged_in` | leaves, then sends `Left`; `unknown_room` or `leave_failed` |
| `rooms` | `not_logged_in` | a `Room` for every joined room |
| `logout` | `not_logged_in` | stops sync, logs this device out on the server, deletes the store directory and the keyring entry, then `Closed` and exit; each failed step sends its error (`network`, `store`, `keyring`), and the local clean-up runs even when the server cannot be reached |
| `quit` | `Closed`, exit 0 | stops sync, `Closed`, exit 0; the session stays saved |

## Not in this slice

Device verification (SAS emoji, QR), server-side key backup and recovery,
re-sending events that become decryptable later, media and files, SSO and
OAuth login, refresh tokens, threads, reactions, edits as edits and
redactions, invites, typing notifications and read receipts, and shipping
the helper through dist, Homebrew, npm or the Linux packages. The helper is
a source build, and its crate is published on crates.io (`publish = true`,
after `vix-im-matrix`).

## License

`Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only`, the same set as `vix`.
`matrix-sdk` and its sub-crates, `vodozemac`, `matrix-pickle`, `ring`,
`prost`, `backon`, `bytesize` and `sync_wrapper` are Apache-2.0 only, which
rules out GPL-2.0-only ([`spec/license`](../../../spec/license/index.md)).
Every other linked crate allows all five.
