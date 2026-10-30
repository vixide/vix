# Remote agent

`vix --remote-agent` (T717): the same `vix` binary, started on the remote
machine over the SSH channel, answering file-system requests in a framed
binary protocol on its stdin and stdout. Phase 2 of
[`spec/remote-editing`](../../../../spec/remote-editing/index.md). The
shell-script `SshAuthority` ([`spec/ssh`](../ssh/index.md)) stays the
fallback for everything the agent can't do.

**Status:** Shipped 2026-10-30. `vix_authority::agent` (always built: the
codec, the agent and `AgentAuthority`); the bootstrap and routing in
`SshAuthority` need the `ssh` feature. The `remote_agent` setting picks the
mode (default `"installed"`).

## Why

Each shell operation is one `ssh` run (a process here and a shell there,
even over a shared connection), and the App's once-a-second check for files
changed on disk was one run per open file. The agent answers a request in
one write and one read on a connection that is already open, and answers a
batch of requests (all the stats of a poll) in one round trip.

## Frames

- A frame is a 4-byte big-endian payload length, then the payload. A
  payload is at most `MAX_FRAME` (64 MiB); a larger length is a protocol
  error and ends the connection. The reader reads through `take(len)`, so a
  lying length allocates only the bytes that actually arrive.
- A payload is one message: a tag byte, then fields. Integers are
  big-endian; byte strings and lists are a `u32` count then the items;
  options and enums are a tag byte; times are a tag (none / after / before
  the epoch), seconds (`u64`) and nanoseconds (`u32`, below 10⁹). Paths are
  the remote machine's raw bytes (any byte but NUL on Unix).
- The decoder trusts nothing: every count is checked against the bytes left
  (each item takes at least one byte) before anything is allocated; lists
  hold at most `MAX_BATCH` (4096) items, except a directory listing or a
  walk (bounded only by the frame); a batch holds no batch, watch or
  unwatch; an unknown tag, invalid UTF-8, an impossible time, a bad boolean
  or bytes left over are errors. A malformed frame ends the connection on
  either side; it never panics (property-tested on arbitrary bytes and
  truncations, and fuzzable through `wire::decode_stream`).

## Messages

| Message | Direction | Fields |
| --- | --- | --- |
| `Hello` | both, first | magic `VIXAGENT`, protocol (`u32`, now 1), Vix version |
| `Request` | client → agent | id (`u64`, chosen by the client), operation |
| `Reply` | agent → client | the request's id, the answer |
| `Event` | agent → client | watch id, the changed paths |

**Handshake.** The client sends its `Hello`; the agent always answers with
its own, then stops unless both the protocol and the Vix version equal its
own **exactly**. The client likewise refuses an agent of any other version
or protocol (and one that sends anything but a `Hello` first, or nothing
within 20 seconds). There is no negotiation: a different build is not used,
and the client falls back to the shell scripts.

**Operations** mirror the `Authority` trait: `Stat` (follow or not), `Read`
(at most N bytes), `ReadText` (the T571 guards: a regular file no larger
than the limit, then up to limit + 1 bytes, decoded by the client),
`Write`, `WriteAtomic` (with the conflict check: any, absent, or a version's
length and whole-second modification time), `Canonicalize`, `ReadDir`,
`CreateDirAll`, `Rename`, `RemoveFile`, `RemovePath`, `MovePath`,
`CopyRecursive`, `Trash`, `IsUnwritable`, `Walk` (the `WalkOptions`),
`Which`, `Watch`, `Unwatch`, and `Batch` (several operations answered in
order by one `Batch` reply). **Answers:** `Err` (an error code and the
remote message), `Unit`, `Stat`, `Data` (bytes and whether the file had no
more), `Path`, `Entries` (name, own kind, is-a-directory-after-links),
`Bool`, `Paths`, `MaybePath`, `Batch`.

**Errors** carry a code that the client turns back into the same
`io::ErrorKind` the local call gives (`NotFound`, `PermissionDenied`,
`AlreadyExists`, `IsADirectory`, `NotADirectory`, `DirectoryNotEmpty`,
`Unsupported`, `InvalidInput`, `InvalidData`), a `WriteConflict`, or a
`TextReadError` (`NotRegular`, `TooLarge`, `Binary`). `TooBig` means the
request or its answer would not fit in a frame.

## The agent (`agent::serve`)

- Answers each request with `LocalAuthority` on its own machine, so every
  operation has exactly the local semantics and guards (atomic,
  symlink-preserving, mode-keeping saves; the read guards; copies and
  removes that never follow a link; `PATH`-only `which`; the `ignore`-crate
  walk, which honors `.gitignore` *and* `.ignore`). `~` and `~/…` are its
  `$HOME`.
- **File-system operations only.** No request starts a process; processes
  keep going through `SshAuthority::command` (one `ssh` run each, as
  before).
- A file read stops at `MAX_DATA` (64 MiB less 64 KiB); a larger file, a
  larger write, or an answer too large for a frame is `TooBig`.
- Requests are answered one at a time, in order. Watches run on their own
  threads (at most 64), each polling its paths' stats every interval
  (clamped to 0.1–60 s) and sending an `Event` with the paths whose kind,
  length or modification time changed, or that appeared or vanished. There
  is no `notify` dependency: polling `stat` on the remote is cheap, and it
  works on every file system, network mounts included.
- It exits when its input ends (the SSH channel closed), on a malformed
  frame, or after answering a mismatched `Hello`. It reads no settings and
  writes nothing but frames to stdout (`main.rs` runs it before the
  settings load, like `--mcp`).

## The client

- One connection serves every thread: requests carry ids, a reader thread
  hands each reply to the caller waiting for it and each event to its
  watch. When the connection ends, every waiting caller is released and
  the client stays dead.
- `AgentAuthority` implements `Authority` over a connected agent (a
  `Command` such as `ssh host vix --remote-agent`, or any reader and
  writer); `command` goes to the authority given to `with_commands`, or is
  `Unsupported`. (T718's `ContainerAuthority` uses the same client through
  the shared shell authority, started with `docker exec -i … vix
  --remote-agent`; see [`spec/container`](../container/index.md).) `watch(paths, interval)` returns a
  `Watch` (`changed()` drains, `next_timeout(d)` waits; dropping it sends
  `Unwatch`).
- `Authority::metadata_batch(paths)` (new, defaulting to one `metadata`
  per path) is one `Batch` of `Stat`s (in chunks of 4096).

## `SshAuthority` with the agent

`SshOptions::agent` (`AgentMode`): `Off` (the library default, and T712's
behavior), `Installed`, or `Upload`. The App sets it from the
`remote_agent` setting (`"installed"` by default; `AgentMode::from_setting`,
anything unknown is `Installed`). `connect()` then calls `start_agent()`,
which never fails the connect; `agent_status()` reports `Off`, `Running {
path, uploaded }`, `NotInstalled`, or `Unavailable(reason)`.

1. **Find.** One shell round trip prints `uname -sm` and, for the per-user
   cache (`${XDG_CACHE_HOME:-$HOME/.cache}/vix/agent/<version>/vix`; a
   relative `XDG_CACHE_HOME` is ignored) and then `command -v vix`, each
   absolute, executable candidate's `--version --json`. The first whose
   version equals this build's is started.
2. **Upload** (`Upload` only, when no candidate works). Only when the
   remote `uname -sm` names this machine's operating system and
   architecture (`same_platform`: Linux/Darwin/FreeBSD/NetBSD/OpenBSD/
   DragonFly/SunOS with x86_64/amd64, aarch64/arm64, riscv64, s390x,
   loongarch64; anything else never matches), and only this process's own
   executable when it is named `vix` (a GUI front end or a test harness is
   not an agent; `SshOptions::agent_binary` overrides it). One script,
   with the binary on stdin: `umask 077`; create the version directory and
   require it to be a real directory (not a link) owned by this user, then
   `chmod 700` it; create a temporary name there exclusively (`set -C`);
   write the bytes; compute SHA-256 **on the remote side** (`sha256sum`,
   else `shasum -a 256`, else `openssl dgst -sha256`; none of them →
   refuse) and compare it with the local hash; on a mismatch remove the
   file and refuse; only then `chmod 700`, run it once with `--version
   --json` (it must report exactly this version, else remove and refuse:
   a glibc/musl or other ABI mismatch shows up here), and rename it to
   `vix`. Nothing executable ever sits under the final name unverified, and
   no temporary file is left behind. `SshAuthority::upload_agent(bytes,
   sha256)` is that step on its own.
3. **Start.** `exec <path> --remote-agent` through the same `ssh` command
   line as every other operation (the shared `ControlMaster` connection,
   `BatchMode`), its stderr discarded; then the handshake.

Every `Authority` method then asks the agent first and falls back to its
shell script when the agent is not running or can't do this one request
(over 64 MiB, or the connection dropped during a request that changes
nothing). A request that changes something and loses its connection midway
fails with `NotConnected` instead (it may have happened), which the App
treats as a lost connection; **File → Reconnect Remote** starts a new
agent. `metadata_batch` without the agent is one shell script per 200
paths (`for p in …; do stat …; done`), so the change poll is one round trip
either way. `command` always uses `ssh` directly. `watch(paths, interval)`
needs the agent (`Unsupported` without it). `disconnect()` stops the
agent; dropping the authority closes its input and reaps the `ssh`
process.

## Security

- **Upload is opt-in** (`remote_agent = "upload"`). The default,
  `"installed"`, never writes to the remote machine: it only runs a `vix` of
  exactly this version that is already there. Rationale: an upload puts an
  executable (tens of megabytes; well over 100 MB for a debug build) into
  the remote user's home over a link that may be slow, possibly on a shared
  or production host whose policy forbids it, and the first connect would
  wait for it. Turning it on is a decision the user makes per machine.
- **Never without the checksum.** The upload is compared against the
  local SHA-256 by the remote side before it is made executable; a
  mismatch, a missing hash tool, a binary that won't run there, or a
  wrong version installs nothing. The cache directory is created private
  (0700) and must be the user's own real directory.
- **Version-locked.** Only an agent of exactly this Vix version and
  protocol is used, checked twice (`--version --json` before starting it,
  and the handshake). A stale or foreign `vix` on the remote is not used.
- **A hostile remote** can only send frames: the client bounds every
  allocation by what arrives, refuses malformed or oversized frames
  (ending the connection, after which the shell scripts are used), ignores
  replies to unknown ids, rejects a reply of the wrong type, and drops a
  directory entry whose name is empty, `.`, `..` or contains `/`.
- **Capabilities.** The agent only touches files, as the remote user, the
  same as the shell scripts could; it starts no process and opens no
  socket. The agent runs before the T714 trust prompt: it is this build of
  Vix itself, not code from the workspace.
- A cached agent is trusted by path and version: anyone who can write the
  user's cache directory already controls the account.

## Testing

- `crates/vix-authority/src/agent/wire.rs`: every message and answer round-trips; error codes
  round-trip through `io::Error`; hostile frames (huge or lying lengths,
  counts past the end, nested batches, a watch in a batch, trailing bytes)
  are refused; property tests over arbitrary bytes, framed garbage,
  truncations and random requests.
- `crates/vix-authority/tests/agent.rs`: `AgentAuthority` over two pipes to `serve` on a thread
  passes `conformance::run` (which now checks `metadata_batch`); a
  5000-path batch; a watch; eight threads sharing one connection; another
  version or protocol refused at the handshake; garbage, a lying length or
  silence instead of a hello; a hostile reply ending the connection
  cleanly; a reply of the wrong type; the agent stopping on a malformed
  request and at end of input.
- `crates/vix-authority/tests/ssh.rs` (fake `ssh`, the "remote" given its own `HOME`,
  `XDG_CACHE_HOME` and a `PATH` without `vix` *in the script*): the shell
  batch stat is one `ssh` run for 30 paths; an upload with the wrong
  checksum, or of a binary reporting another version, installs nothing; a
  verified upload lands as `vix`, mode 0700 in a 0700 directory, with no
  temporary file; `Installed` with no `vix` stays on the shell (and passes
  the conformance suite); an uploaded binary that is no agent falls back,
  and a second connect finds the cached copy.
- `tui/tests/remote_agent.rs`: the real `vix` binary (`CARGO_BIN_EXE_vix`)
  as the agent behind the fake `ssh`: `Installed` finds nothing; `Upload`
  uploads it (byte-identical, 0700), starts it and passes the conformance
  suite; 60 writes, a 60-path batch stat, a read and a listing start no
  `ssh` run; a watch sees a change; a second connect reuses the cache; and
  the App's change poll over five open files is zero `ssh` runs with the
  agent and exactly one without.

## Not done

- The App does not subscribe to watches: its change poll stays a timer,
  now one batch per second (one round trip). Pushing events into the poll
  would remove even that.
- No `notify`-based watching on the remote side (polling only).
- Processes (tasks, git, search, LSP/DAP) still run as their own `ssh`
  command (T716 owns remote LSP/DAP/terminal); the agent spawns nothing.
- Requests are served one at a time: a long walk delays a stat issued
  meanwhile from another thread.
- No transfer of files over 64 MiB through the agent (those use the shell
  scripts), no compression, no resumable upload.
- Upload needs the same operating system and architecture; there is no
  download of a matching release build for another platform.
- Old agents in the cache (other versions) are never cleaned up.
- No request timeout once connected (as with the shell scripts, a hung
  link waits on `ssh`).
