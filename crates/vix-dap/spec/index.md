# Debugger (DAP)

Vix debugs via the **Debug Adapter Protocol**: it drives an external debug adapter
over stdio, the same way the LSP client drives language servers. Configure
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

## Limitations

- A single debug session at a time.
- Variable trees are shown one level deep (the top frame's first scope).
