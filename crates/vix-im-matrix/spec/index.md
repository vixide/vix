# Matrix bridge (vix-im-matrix)

Matrix bridge for Vix IM: the JSON-lines protocol spoken with the
`vix-matrix` helper, and an optional thread that starts and pumps the helper.

This is Vix's side of the Matrix bridge chosen in the T758 spike
([`spec/instant-message/matrix.md`](../../../spec/instant-message/matrix.md)):
Matrix runs in a separate program, `vix-matrix` (the
[`matrix/`](../../../matrix/Cargo.toml) workspace, on `matrix-sdk` with its
SQLite store and vodozemac E2EE), so `matrix-sdk` never enters `vix`'s own
dependency graph. The helper depends on this crate (without `net`) for the
protocol types, so both sides share one definition. This crate does **not**
depend on `vix-im`; the App (`tui/src/app/im_matrix.rs`, feature `matrix`)
turns replies into `vix-im` messages.

## Protocol (`protocol`, pure)

One JSON object per line, UTF-8. Every object carries `"v": 1`
(`PROTOCOL_VERSION`) and a `"type"` naming the variant in `snake_case`;
unknown fields are ignored, so a newer helper can add fields.

**Vix → helper (`Request`):**

| `type` | Fields | Meaning |
| --- | --- | --- |
| `login` | `account`, `homeserver`, `user`, `password`? | Resume the session saved for `account`; only without one, log in with `password`; with neither, `error` `no_session` |
| `send` | `room`, `body`, `kind` (`text` default, `emote`, `notice`) | Send to a joined room |
| `join` | `room` (id `!…` or alias `#…`) | Join |
| `leave` | `room` | Leave |
| `rooms` | — | List the joined rooms again |
| `logout` | — | Sign this device out, delete its store and keyring entries, then exit |
| `quit` | — | Stop, say `closed`, exit; the session stays saved |

**Helper → Vix (`Reply`):** `ready` (`helper` version, `protocol`; always
first), `logged_in` (`user_id`, `device_id`, `restored`), `room` (`id`,
`name`, `encrypted`, `direct`), `left` (`id`), `message` (`room`,
`event_id`, `sender`, `sender_name`?, `time` in UTC ms, `body`, `kind`,
`encrypted`, `verified`: the sending device is cross-signed by its owner),
`undecryptable` (`room`, `event_id`, `sender`, `time`), `error` (`key`,
`detail`?), `closed` (always last).

`error` carries an `ErrorKey` (`login_failed`, `no_session`,
`not_logged_in`, `network`, `send_failed`, `join_failed`, `leave_failed`,
`unknown_room`, `store`, `keyring`, `bad_request`; and, made by Vix's own
side, `protocol`, `helper_missing`, `helper_exited`), never an English
sentence: the App translates `msg.im_matrix_error_<key>`. `detail` is
untrusted extra text, such as a server error code.

**The password** travels once, inside `login` on the helper's stdin. It is
never put in argv or the environment, where other processes could read it.
`Password`'s `Debug` prints `<redacted>` and its bytes are wiped on drop.

## Codec (`codec`, pure)

`encode_request`/`decode_request` and `encode_reply`/`decode_reply`.
Encoding never writes a raw line break (JSON escapes control characters in
strings), so a body cannot start a second message. Decoding never panics
and refuses, as `DecodeError`: a line over `MAX_LINE` (1 MiB), anything that
isn't a JSON object, a missing or different `"v"`, and an unknown or
malformed message. A trailing CR/LF is ignored.

## The helper process (`net`, feature `net`)

- `find_helper(setting, beside, path)`: the `im.matrix_helper` setting when
  set (and only if that file exists), else `vix-matrix` (plus the platform's
  executable suffix) beside the running `vix`, else the first one on `PATH`.
- `Handle::spawn(&SpawnOptions)` always returns a handle. It starts the
  program with no arguments, piped stdin and stdout, and stderr dropped or
  appended to `stderr_log` (the App sets it from `VIX_MATRIX_LOG`). A thread
  named `vix-im-matrix-in` writes requests and a thread named
  `vix-im-matrix-out` reads replies, so the App never blocks on the helper.
  A missing program ends at once with `error` `helper_missing` and `closed`,
  and a program that won't start with `helper_exited` and `closed`.
- The reader turns a line that does not decode into `error` `protocol` and
  skips an overlong line with one. If stdout ends without `closed`, it adds
  `error` `helper_exited` and `closed`.
- `send(Request) -> bool` (false once ended), `try_recv()` (never blocks),
  `is_finished()`, and `close() -> Vec<Reply>`: send `quit`, close stdin,
  wait up to 3 s, kill the helper if it hangs, and return the remaining
  replies. Dropping a handle closes it too, so no helper outlives its handle.
- `Handle::pair()` returns a handle wired to an in-memory `Peer` instead of
  a process, for the App's tests.

Starting the helper is a process spawn, so `crates/vix-im-matrix/src/net.rs`
is in `tui/tests/spawn_sites.rs`, gated on `matrix`.

## Tests

Unit tests per module; `crates/vix-im-matrix/tests/props.rs` (proptest: decoding
never panics, requests and replies round-trip on one line);
`crates/vix-im-matrix/tests/fake_helper.rs` (feature `net`, `harness = false`):
the test binary copies itself to `vix-matrix-fake` and runs that copy as a
fake helper. It covers `ready` first, a request before login, a login with a
right, wrong or missing password, a body with line breaks coming back whole,
a non-protocol line, a crash without `closed`, `close`, and a hung helper
killed on close. The fake also exits with an error if a password ever appears
in its argv or environment. No test contacts a Matrix server.
