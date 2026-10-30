# Debugger (DAP)

Debug Adapter Protocol (DAP) client: drive a debug adapter over stdio, the same
way the LSP client drives language servers. Configure
adapters with the `debug_adapters` setting; the debugger commands live under the
top-level **Run** menu (and `run.*` actions) and control a session.

## Configuration

```toml
[[debug_adapters]]
adapter_id = "debugpy"
extensions = ["py"]
command = ["python", "-m", "debugpy.adapter"]
# Extra launch arguments (values may use {program} for the file path):
[debug_adapters.launch]
console = "internalConsole"
```

`extensions` matches the active file to an adapter; `command` launches it;
`launch` is merged into the DAP `launch` request (with `{program}` expanded to the
file path; `program` defaults to the active file).

## Commands (Run menu / `run.*`)

- **Start** / **Stop** — launch or terminate the session for the active file.
- **Toggle Breakpoint** — add/remove a breakpoint on the cursor's line (shown as
  `●` in the gutter). Breakpoints update live on a running session.
- **Continue**, **Step Over**, **Step Into**, **Step Out**, **Pause**.
- **Add Watch…** — evaluate an expression each time execution stops.
- **Evaluate…** — a one-off expression (REPL); the result prints to the bottom
  dock.
- **Toggle Debug Panel** — the side panel showing the call stack, variables, and
  watches.

## Behavior

- The handshake is `initialize` → (on the `initialized` event) `setBreakpoints` +
  `launch` + `configurationDone`.
- On a `stopped` event Vix fetches the stack trace, then the top frame's scopes
  and variables, jumps the editor to the stop location (marked `▶` in the gutter),
  and updates the Debug panel and any watches.
- `output` events stream to the bottom dock; `terminated`/`exited` end the session.
- One session is active at a time.

## As implemented in Vix

The `dap` module is the protocol client (`Dap`/`Session`/`DebugAdapter`,
`DapEvent`), reusing `lsp_core::frame` for `Content-Length` framing. The host owns
breakpoints, `start_debugger`/`stop_debugger`/`toggle_breakpoint`, stepping
actions, `poll_dap` (drains events each loop), gutter markers
(`Editor::set_breakpoints`/`set_debug_line`), and the Debug panel
(`ui::draw_debug_panel`). The fast event-loop cadence engages while
`dap_busy()`.

## Remote workspaces (T710, T716)

The adapter starts through the workspace authority, so in a remote workspace it
runs on the remote machine over the `ssh` channel's stdio. Breakpoint, launch and
stack-frame paths are plain paths (`pathFormat: "path"`), the authority's own, so
the remote program path goes to `launch` as is and a stop opens the remote file
through the authority. Under a non-local authority the adapter is looked up first
(`vix_authority::can_run`; a missing one fails to start, `run.start` reports
it) and starts in the directory set by `Dap::set_workdir`, which the App sets to
the workspace root (locally it is ignored, and the adapter inherits Vix's working
directory as before). The debugger waits for trust in an untrusted remote
workspace (T714). An adapter that listens on a TCP port instead of stdio is not
supported anywhere yet; remotely it would also need a forwarded port.

## Limitations

- A single debug session at a time.
- Variable trees are shown one level deep (the top frame's first scope).

## Cargo features

- `process` (default on): starting an adapter process. Off, `start` returns `false`. The
  root `dap` feature enables it (`spec/cargo-features`, T595).

## Testing

`crates/vix-dap/tests/mock_adapter.rs` drives the client against a scripted Python mock adapter (skipped without `python3`): the initialize → launch → stopped → stackTrace → scopes → variables handshake and its request order, evaluate success/failure, a rejected request surfacing as `RequestFailed`, and `terminated` ending the session. `tui/tests/authority_remote_tools.rs` (T716)
runs a mock adapter over a fake `ssh` from **Run → Start**: it starts in the
remote workspace, is launched with the remote program path, and its stop moves
the editor there.
