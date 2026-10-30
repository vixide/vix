# Jupyter kernel transport

Jupyter kernel transport: kernelspec discovery, kernel launch with a private
connection file, and a signed ZeroMQ client on a worker thread.

It starts Jupyter kernels and talks to them (T734b). The protocol itself (codec,
messages, connection file, kernelspecs, outputs) is in the pure
[`vix-jupyter-core`](../../vix-jupyter-core/spec/index.md), re-exported as
`vix_jupyter::core`. The feature design is in
[`spec/jupyter`](../../../spec/jupyter/index.md).

## Features

| Feature | Default | Adds |
| ------- | ------- | ---- |
| `process` | yes | `KernelHandle`, `LaunchOptions`, `LaunchError`: launching kernels and the ZeroMQ client (`zeromq`, `tokio`, `getrandom`, `jiff`, and `nix` on Unix). |

Without `process` the crate has only kernelspec discovery and the event
types: no sockets, no processes. Vix's `jupyter` build feature
(`tui/Cargo.toml`) turns `process` on; it is not in `full`.

## Discovery (always built)

- `discover::path_env()` snapshots the environment into a `PathEnv`:
  `JUPYTER_PATH`, `JUPYTER_DATA_DIR`, the home directory, `XDG_DATA_HOME`,
  `APPDATA`, `PROGRAMDATA`, and the Python prefixes from `python_prefixes()`:
  `VIRTUAL_ENV`, `CONDA_PREFIX`, then the prefix of the first `python3` or
  `python` on `PATH` (found without running it).
- `discover::kernelspecs(env)` reads `kernels/*/kernel.json` under every
  directory of `vix_jupyter_core::kernelspec::kernel_dirs(env)`. A name found
  earlier hides the same name later on, as in Jupyter. Within a directory,
  specs are sorted by name. At most 256 entries are read per directory, each
  `kernel.json` is read only up to its 64 KiB cap, and bad files are skipped.

Whether a project-local spec (such as one in `.venv`) may be used is the
App's trust decision (T734f); discovery only reports what is installed.

## Launch (`process`)

`KernelHandle::launch(spec, options)`:

1. creates `vix-jupyter-<16 hex>` under `LaunchOptions::runtime_base` (the OS
   temporary directory by default), mode `0700` on Unix, failing rather than
   reusing an existing path;
2. picks the connection: on Unix, IPC with the socket prefix
   `<dir>/kernel` and ports 1–5 (refused if the path is too long for a Unix
   socket); on Windows, TCP on `127.0.0.1` with five free ports. The key is
   256 bits from the OS RNG, as hex;
3. writes `<dir>/kernel.json` with `create_new`, mode `0600` on Unix;
4. runs `spec.argv_for(file)` with the spec's `env`, stdin and stdout closed,
   stderr piped (the last 4 KiB are kept for the death report), in
   `LaunchOptions::cwd` if set, and on Unix in its own process group so a
   terminal Ctrl+C never reaches it. The key is in the file, never on argv.

`KernelHandle::connect(info, options)` attaches to a kernel someone else
started, from its connection file. It is refused (`LaunchError::NotLocal`)
unless `info.is_local()`: IPC, or TCP on loopback. Vix does not own that
process.

A `LaunchError` (`Io`, `Spawn`, `Runtime`, `Connection`, `NotLocal`) is
returned only for failures before the kernel runs. A missing kernel program
is `Spawn` (the App shows an install hint). Everything after that arrives as
events.

## The worker

Each handle has one thread, `vix-jupyter`, running a `current_thread` tokio
runtime (the `vix-db` pattern). It connects a SUB socket (IOPub, subscribed
to everything) first, then DEALER sockets for shell and control. The
heartbeat and stdin channels are not used: the worker watches the process
instead (every 100 ms), and v1 sends `allow_stdin: false`.

While connecting it also watches for the process exiting, the start timeout
(`LaunchOptions::start_timeout`, 30 s by default) and a kill from the handle,
so dropping a handle never waits out the timeout. Requests sent meanwhile are
queued and sent once connected. It then sends `kernel_info_request`; the
kernel must answer within the start timeout.

Every received message goes through `wire::decode` with
`LaunchOptions::limits`: size caps and the HMAC check come before any JSON is
parsed. A message that fails, or whose content does not have its type's
shape, is dropped and reported as `KernelEvent::Rejected`. IOPub is a
broadcast, so outputs are kept only when their parent request is from this
handle's session.

## The handle

| Method | Effect |
| ------ | ------ |
| `execute(code) -> RequestId` | Sends `execute_request` and returns its id at once (`<session>_<n>`). |
| `interrupt()` | SIGINT to a launched kernel whose `interrupt_mode` is `signal` (Unix); otherwise `interrupt_request` on control. |
| `restart()` | Launched: `Restarting`, `shutdown_request` (`restart: true`) on control, wait up to `shutdown_grace` (2 s), kill if still running, remove stale IPC socket files, start the same argv on the same connection file, reconnect; then `Restarted` and `Ready`. Connected: sends `shutdown_request` with `restart: true` only. |
| `shutdown()` | `shutdown_request` on control; a launched kernel is killed if still running after the grace period; a connected one is given the grace period to reply. Then `Dead(Shutdown)`. |
| `events()` | The `std::sync::mpsc::Receiver<KernelEvent>` the App polls with `try_recv` on its tick. |
| `Drop` | Kills a launched kernel at once, joins the thread, removes the runtime directory. |

`KernelEvent`:

- `State(KernelState)`: `Starting` (each start), `Busy`/`Idle` (IOPub
  `status`), `Restarting`, `Dead`.
- `Ready(KernelInfoReply)`: after each start and restart.
- `Output { request, output }`: one per IOPub output of a request.
- `Reply { request, status, execution_count }`: after the request's
  outputs. The shell reply and IOPub are separate sockets, so the reply is
  held until IOPub's `idle` for that request arrives (at most 1 s, in case
  IOPub dropped it; at most 256 tracked).
- `Restarted`.
- `Rejected(DecodeError)`: a dropped message.
- `Dead(DeathReason)`: the last event. `Shutdown`, `Exited { code, stderr }`
  (the stderr tail with ANSI removed), `StartTimeout`, or `Transport(text)`.
  The runtime directory is removed before it is sent.

`RequestId`, `KernelEvent`, `KernelState` and `DeathReason` are built without
`process` too, so App code can name them.

## Private scratch directory

`PrivateDir::create(base)` (`process`) makes a `vix-jupyter-<random>`
directory under `base`, mode `0700` on Unix and never an existing path, and
removes it with everything in it when dropped; `path()` names it. The App
writes the images it opens in a tab there (T734g), so a kernel's image
never lands in a shared, predictable temporary path.

## Testing

- `crates/vix-jupyter/tests/fake_kernel.rs`: an in-process fake kernel built from the same
  `zeromq` crate (ROUTER for shell and control, PUB for IOPub) speaking the
  signed protocol, over TCP and (Unix) IPC. It covers `kernel_info`, a
  stream, an error, a PNG display, a message signed with the wrong key
  (rejected), another session's output (ignored), interrupt over control,
  and shutdown. Also: a remote connection is refused, a missing kernel binary
  is `Spawn`, nothing listening gives `Dead` and a prompt drop, and (Unix) a
  launched "kernel" script finds its connection file `0600` with the key in
  it and reports its exit code and stderr.
- `crates/vix-jupyter/tests/ipykernel_smoke.rs`: runs only when `VIX_JUPYTER_PYTHON` names a
  Python with `ipykernel`. Covers a print, a multi-line cell, an error,
  stderr, a PNG, `input()` raising, SIGINT interrupt of a sleep, restart
  clearing state, and shutdown. It passed against ipykernel 7.4.0 (Python 3.11) in a throwaway
  virtual environment on 2026-10-30 (ready in about 0.4 s). CI does not run
  it.
- `runtime_dir` unit tests check the `0700`/`0600` modes and that the
  directory is removed; `discover` tests check precedence and caps.
