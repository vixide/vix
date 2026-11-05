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
# Arguments for Run → Attach (T617), sent as the DAP `attach` request:
[debug_adapters.attach]
processId = "4242"
```

`extensions` matches the active file to an adapter; `command` launches it;
`launch` is merged into the DAP `launch` request (with `{program}` expanded to the
file path; `program` defaults to the active file). `attach` (optional, T617) is
the argument table of the `attach` request **Run → Attach** sends instead of
`launch`; `{program}` expands there too, and nothing else is added, so the table
names whatever the adapter attaches by (`processId`, `port`, `connect`, …).
TOML values are strings; for `attach` a value that reads as a JSON number,
boolean, object or array is sent as that JSON (`processId = "4242"` sends the
number `4242`, `connect = '{"port": 5678}'` an object), since adapters attach by
numeric ids and ports. `launch` values stay strings.

## Commands (Run menu / `run.*`)

- **Start** / **Stop** — launch or terminate the session for the active file.
- **Attach** (`run.attach`, T617) — start the active file's adapter and send
  `attach` with its `debug_adapters.attach` table instead of `launch`.
- **Toggle Breakpoint** — add/remove a breakpoint on the cursor's line (shown as
  `●` in the gutter). Breakpoints update live on a running session.
- **Edit Breakpoint…** (`run.edit_breakpoint`, T617) — three prompts in a row,
  each seeded with the current value, set the cursor line's breakpoint
  **condition** (an expression; stop only when true), **hit condition** (e.g.
  `3` or `>= 5`, adapter-defined) and **log message** (a logpoint: print
  instead of stopping; `{expr}` interpolation is the adapter's). An empty answer
  clears that field; Esc at any prompt abandons the edit. The cursor line gains
  a breakpoint if it had none. They map to `SourceBreakpoint.condition`,
  `hitCondition` and `logMessage` in `setBreakpoints`.
- **Exception Breakpoints…** (`run.exception_filters`, T617) — a checklist of
  the exception filters the adapter advertised in its `initialize` response
  (`exceptionBreakpointFilters`); Enter toggles the highlighted one, Esc
  closes. Until the user chooses, the filters the adapter marks `default` are
  on. The choice is sent as `setExceptionBreakpoints` (during configuration and
  again on every change) and is kept for later sessions of this run of Vix.
  Without an active session, or when the adapter offers no filters, it reports
  that in the status line.
- **Function Breakpoint…** (`run.function_breakpoint`, T617) — a prompt for a
  function name; Enter adds a function breakpoint on it, or removes the one
  already there (an empty answer does nothing). They are sent as
  `setFunctionBreakpoints` (during configuration and again on every change),
  only to an adapter that advertised `supportsFunctionBreakpoints`; the status
  line says so when it did not. They are kept for later sessions of this run
  of Vix, like the exception choice.
- **Data Breakpoint on Variable** (`run.data_breakpoint`, T617) — stop when
  the memory behind the variable highlighted in the Debug panel is written (or
  read, if the adapter offers only that). The same as `b` in the panel. See
  "Data breakpoints" below.
- **Continue**, **Step Over**, **Step Into**, **Step Out**, **Pause**.
- **Add Watch…** — evaluate an expression each time execution stops.
- **Evaluate…** — a one-off expression (REPL); the result prints to the bottom
  dock.
- **Toggle Debug Panel** — the side panel showing threads, the call stack,
  variables, and watches.
- **Focus Debug Panel** (`run.focus_panel`, T617) — show the panel and give it
  the keyboard (see below).

## Debug panel (T617)

The panel is a tree in four sections: **Threads** (from the `threads`
request), **Call Stack** (the selected thread's frames), **Variables** (the
selected frame's scopes, each expandable) and **Watch**. While it has focus:

| Key | Does |
| --- | --- |
| Up / Down, PgUp / PgDn, Home / End | Move the highlight (section headings are skipped) |
| Enter | On a thread: select it (fetch its stack). On a frame: select it (fetch its scopes, jump the editor there). On a scope or variable: expand / collapse |
| Right / Left | Expand / collapse (Left on a child moves to its parent) |
| `b` | Toggle a data breakpoint on the highlighted variable (`dataBreakpointInfo`, then `setDataBreakpoints`) — only when the adapter reports `supportsDataBreakpoints` |
| `e` | Set the highlighted variable's value (`setVariable`) via a prompt seeded with its value — only when the adapter reports `supportsSetVariable` |
| Esc | Give the keyboard back to the editor |

Other chords fall through to the keymap, as for the IM panel. A variable whose
`variablesReference` is non-zero is expandable; its children are fetched with
`variables` the first time it is expanded (lazy, to any depth) and kept while
the program stays stopped. The first non-`expensive` scope is expanded
automatically. A new stop rebuilds the tree, but the nodes that were expanded
are remembered by name path (scope, then variable names) and re-expanded as
the new stop's `variables` replies arrive (`DebugView::set_scopes` and
`set_variables` return the references the host must fetch for them), so
stepping through a function keeps the structures you opened open.

## Data breakpoints (T617)

`b` in the panel, or Run → Data Breakpoint on Variable on the highlighted row,
sends `dataBreakpointInfo` with the variable's container reference and name.
The adapter's `DapEvent::DataBreakpointInfo` reply is acted on by the host: a
`dataId` that is not yet a data breakpoint is added with access type `write`
(else the first access type the adapter offers) and the whole list is sent as
`setDataBreakpoints`; one that is already there is removed; a `null` `dataId`
reports the adapter's `description` as the reason the variable cannot be
watched. Without `supportsDataBreakpoints` nothing is sent and the status line
says the adapter lacks them; with no variable highlighted it asks for one.
A data breakpoint's id only means something for one run of the program, so
they live in the session (`Dap::data_breakpoints`, `Dap::set_data_breakpoints`)
and are neither kept for the next session nor saved. They are not drawn in the
Debug panel; the status line reports each change.

## Inline values (T617)

While the program is stopped, the stopped line shows the values of the
auto-expanded scope's variables whose names occur on that line (as whole
identifiers), as end-of-line virtual text (`x = 1  y = "a"`) in its own
`VirtualTextLayer::DebugValues` layer (T614's primitive). It clears when
execution continues or the session ends.

## Behavior

- The handshake is `initialize` → (on the `initialized` event) `setBreakpoints`
  + `setExceptionBreakpoints` (when the adapter advertised filters) + `launch`
  (or `attach`) + `configurationDone`. The `initialize` response's
  capabilities that Vix uses (exception filters, `supportsSetVariable`, `supportsFunctionBreakpoints`,
  `supportsDataBreakpoints`) are
  kept for the session.
- On a `stopped` event Vix fetches `threads` and the stopped thread's stack
  trace, then the top frame's scopes and the first scope's variables, jumps the
  editor to the stop location (marked `▶` in the gutter), and updates the Debug
  panel and any watches. Selecting another thread or frame in the panel
  repeats the stack or scopes step for it.
- `evaluate` (watches and the REPL) runs in the selected frame (`frameId`).
- A breakpoint field the adapter does not support is still sent; adapters
  ignore unknown fields, and a rejection surfaces as `RequestFailed`.
- `output` events stream to the bottom dock; `terminated`/`exited` end the session.
- One session is active at a time.

## As implemented in Vix

The `dap` module is the protocol client (`Dap`/`Session`/`DebugAdapter`,
`DapEvent`, `SourceBreakpoint`, `Capabilities`, `StartMode`), reusing
`lsp_core::frame` for `Content-Length` framing. `vix_dap::view::DebugView` is
the panel's pure tree model (threads, frames, lazily loaded variable nodes,
the highlight, and the inline-values text), tested without a TTY. The host owns
breakpoints (`tui/src/app/lsp_dap.rs`: `start_debugger`/`stop_debugger`/
`toggle_breakpoint`, the Edit Breakpoint prompts, the exception-filter
checklist on the shared choice-list overlay), stepping actions, `poll_dap`
(drains events each loop), the panel's keys (`tui/src/app/debug_panel.rs`),
gutter markers (`Editor::set_breakpoints`/`set_debug_line`), and the Debug
panel drawing (`ui::draw_debug_panel`). The fast event-loop cadence engages
while `dap_busy()`.

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

## Persistent breakpoints (T617)

Line breakpoints (with their condition, hit condition and log message) and
function breakpoints are saved per workspace in the session file
(`vix-session`: `WorkspaceSession::breakpoints` / `function_breakpoints`) when
Vix exits, and restored at startup (`App::restore_breakpoints`, always, even
when files are named on the command line or `restore_session` is off). The
restored breakpoints show in the gutter once their file is open, and every
debug session started afterwards sends them with its `setBreakpoints` and
`setFunctionBreakpoints` requests, exactly as breakpoints set in this run.
The gutter of the file in view is refreshed at startup after the restore.
Exception-filter choices are about one adapter and are not saved.

## Limitations

- A single debug session at a time.
- Breakpoints are line, function and data breakpoints (no instruction
  breakpoints yet); only the line and function ones are persisted across runs
  of Vix, and data breakpoints have no row in the Debug panel.
- A remembered expansion is matched by names, so two sibling variables with
  the same name share it.

## Cargo features

- `process` (default on): starting an adapter process. Off, `start` returns `false`. The
  root `dap` feature enables it (`spec/cargo-features`, T595).

## Testing

`crates/vix-dap/tests/mock_adapter.rs` drives the client against a scripted Python mock adapter (skipped without `python3`): the initialize → launch → stopped → threads → stackTrace → scopes → variables handshake and its request order, evaluate success/failure, a rejected request surfacing as `RequestFailed`, and `terminated` ending the session. T617 extends it: breakpoint condition / hitCondition / logMessage on the wire, `setExceptionBreakpoints` with the adapter's default filters and then a changed choice, `attach` with the `attach` table instead of `launch`, thread and frame selection, lazy `variables` for a nested reference, and `setVariable`. T617's data breakpoints add `dataBreakpointInfo` (a watchable and an unwatchable variable) and `setDataBreakpoints` to the same mock. `view.rs` unit-tests the tree model, and `tui/tests/integration/debugger.rs` drives Edit Breakpoint, the exception checklist and the panel keys through the real `App` against the same kind of mock. `tui/tests/authority_remote_tools.rs` (T716)
runs a mock adapter over a fake `ssh` from **Run → Start**: it starts in the
remote workspace, is launched with the remote program path, and its stop moves
the editor there.
