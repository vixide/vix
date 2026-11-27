# Debugger

Vix™ has an integrated debugger built on the **Debug Adapter Protocol** (DAP), the
same standard used by VS Code. You bring a debug adapter (e.g. `debugpy`,
`codelldb`); Vix drives it. Every debugger command is on the **Run** menu and
in the command palette.

For a guided first session with `debugpy`, see
[Tutorial 10: Debugging with DAP](../tutorials/10-debugging-with-dap/index.md).
This page is the reference.

## Configure an adapter

Add adapters to your `config.toml`, matched to files by extension:

```toml
[[debug_adapters]]
adapter_id = "debugpy"
extensions = ["py"]
command = ["python", "-m", "debugpy.adapter"]

[[debug_adapters]]
adapter_id = "codelldb"
extensions = ["rs", "c", "cpp"]
command = ["codelldb"]
[debug_adapters.launch]
program = "{program}"
```

The `[debug_adapters.launch]` table is merged into the adapter's launch request;
`{program}` expands to the file you're debugging. An optional
`[debug_adapters.attach]` table is what **Run → Attach** sends instead, as the
`attach` request (e.g. `processId = "4242"`; values that read as JSON numbers,
booleans, objects or arrays are sent as such).

<!-- Sources: crates/vix-dap/spec/index.md (Configuration);
tui/src/app/lsp_dap.rs (adapter_for matches the active file's extension). -->

## Start and stop

- **Run → Start** (`run.start`) starts the adapter configured for the active
  file's extension and launches it. **Run → Attach** (`run.attach`) sends the
  `attach` table instead.
- The status line says *Debugger started* and the **Debug** panel opens in the
  right dock. If the adapter cannot be started it says *Failed to start debug
  adapter*.
- With no file open it says *Open a file to debug*; with no adapter for the
  file's extension, *No debug adapter configured for this file type*.
- **Run → Stop** (`run.stop`) ends the session (*Debugger stopped*). The
  session also ends when the program exits.

Only one debug session runs at a time. Program output streams to the bottom
dock.

<!-- Sources: tui/src/app/lsp_dap.rs (run_dap_action, start_debugger,
stop_debugger); crates/vix-i18n/locales/status.yml (status.debug_started,
debug_failed, debug_no_file, debug_no_adapter, debug_stopped);
crates/vix-dap/spec/index.md (Behavior: output events, one session);
tui/tests/integration/debugger.rs
(debugger_actions_need_a_file_and_lsp_actions_run_without_a_server). -->

## Breakpoints

Vix has four kinds. Line, function and instruction breakpoints can be set
before a session starts; every new session sends them during its handshake, and changes during
a session are sent at once.

| Kind | Set with | Gutter | Kept after Vix exits |
| --- | --- | --- | --- |
| Line | **Run → Toggle Breakpoint**, **Run → Edit Breakpoint…** | `●` | Yes, per workspace |
| Function | **Run → Function Breakpoint…** | none | Yes, per workspace |
| Instruction | **Run → Instruction Breakpoint…** | none | No (kept until Vix exits) |
| Data | **Run → Data Breakpoint on Variable**, or `b` in the Debug panel | none | No (this session only) |

Exception breakpoints are a fifth switch, chosen per adapter (below).

<!-- Sources: crates/vix-dap/spec/index.md (Commands, Persistent breakpoints,
Limitations); tui/src/app/lsp_dap.rs (toggle_breakpoint, push_breakpoints,
start_debugger passes every breakpoint); tui/src/app/session.rs
(restore_breakpoints: line and function breakpoints only);
tui/tests/integration/debugger.rs
(breakpoints_persist_across_runs_and_reach_the_next_session). -->

### Line breakpoints

**Run → Toggle Breakpoint** (`run.toggle_breakpoint`) adds or removes a
breakpoint on the cursor line, drawn as a red `●` in the gutter. The Eclipse
keymap binds it to `Ctrl+Shift+B`; the other keymaps leave it to the menu
and the palette.

**Run → Edit Breakpoint…** (`run.edit_breakpoint`) asks three questions in
a row about the cursor line's breakpoint, creating it if there is none:

1. *Breakpoint condition, line N (empty: none)*: an expression; the program
   stops only when it is true.
2. *Hit count condition, line N (e.g. 3 or >= 5; empty: none)*: how the
   adapter counts hits.
3. *Log message, line N (logs instead of stopping; empty: none)*: turns the
   breakpoint into a logpoint. How `{expr}` in the message is filled in is up
   to the adapter.

Each prompt starts with the current value. An empty answer clears that field,
and `Esc` at any prompt abandons the whole edit. When you answer the last
prompt, the status line says *Breakpoint on line N updated*. Toggling the
breakpoint off removes it with all three fields.

Line breakpoints, with their condition, hit count and log message, are saved
for the workspace when Vix exits and come back at the next start, even when
you open Vix on a single file.

<!-- Sources: tui/src/app/debug_panel.rs (begin_edit_breakpoint,
accept_breakpoint_prompt); crates/vix-i18n/locales/prompt.yml
(prompt.debug_breakpoint_condition, _hit, _log); crates/vix-i18n/locales/status.yml
(status.debug_breakpoint_edited); tui/tests/integration/debugger.rs
(edit_breakpoint_sets_condition_hit_count_and_log_message);
docs/reference/keybindings-eclipse.md; crates/vix-editor-core/src/render.rs
(● a breakpoint); crates/vix-dap/spec/index.md (Persistent breakpoints:
restored "even when files are named on the command line"). -->

### Function breakpoints

**Run → Function Breakpoint…** (`run.function_breakpoint`) asks for a
*Function name to toggle a breakpoint on (empty: cancel)*. A new name adds a
breakpoint (*Function breakpoint set on NAME*), a name that already has one
removes it, and an empty answer does nothing. If the running adapter does not
support function breakpoints the status line says *This debug adapter does
not support function breakpoints*.

<!-- Sources: tui/src/app/debug_panel.rs (begin_function_breakpoint,
accept_function_breakpoint); crates/vix-i18n/locales/prompt.yml and status.yml
(prompt.debug_function_breakpoint, status.debug_function_breakpoint_added /
_removed / _unsupported); tui/tests/integration/debugger.rs
(function_breakpoint_prompt_toggles_a_named_function). -->

### Instruction breakpoints

**Run → Instruction Breakpoint…** (`run.instruction_breakpoint`) asks for a
*Memory reference of the instruction to toggle a breakpoint on, e.g. 0x4005d0
(empty: cancel)*, spelled the way your adapter spells addresses. The same
reference again removes it. With no session the breakpoint is only recorded,
and the next session sends it. If the running adapter cannot set them, the
prompt does not open and the status line says *This debug adapter does not
support instruction breakpoints*. They are not saved when Vix exits, because
addresses change from one run of a program to the next.

<!-- Sources: tui/src/app/debug_panel.rs (begin_instruction_breakpoint,
accept_instruction_breakpoint); crates/vix-i18n/locales/prompt.yml
(prompt.debug_instruction_breakpoint); tui/tests/integration/debugger.rs
(instruction_breakpoints_are_sent_by_memory_reference,
instruction_breakpoints_need_adapter_support); crates/vix-dap/spec/index.md. -->

### Data breakpoints

A data breakpoint stops when the memory behind a variable is written. While
the program is stopped, highlight a variable in the Debug panel and press `b`
(or choose **Run → Data Breakpoint on Variable**, `run.data_breakpoint`).
Vix asks the adapter whether that variable can be watched:

- If it can, the breakpoint is set on writes (or on the first kind of access
  the adapter offers, when it offers no write watch): *Data breakpoint set on
  NAME*. Pressing `b` on the same variable again removes it.
- If it cannot, the status line gives the adapter's reason.
- With no variable highlighted, the status line says *Select a variable in the
  Debug panel first*. An adapter without data breakpoint support gives *This
  debug adapter does not support data breakpoints*.

Data breakpoints last for one session only and have no row in the Debug panel;
the status line reports each change.

<!-- Sources: tui/src/app/debug_panel.rs (begin_data_breakpoint,
data_breakpoint_info_arrived: "write" else the first access type);
crates/vix-i18n/locales/status.yml (status.debug_data_breakpoint_added /
_removed / _unsupported, status.debug_data_no_variable);
tui/tests/integration/debugger.rs (data_breakpoints_are_set_from_a_panel_variable,
data_breakpoints_need_adapter_support); crates/vix-dap/spec/index.md (Data breakpoints). -->

### Exception breakpoints

**Run → Exception Breakpoints…** (`run.exception_filters`) opens the
**Exception Breakpoints** checklist of the filters your adapter offers (for
example, raised or uncaught exceptions), one `[x]` or `[ ]` row each. `Enter`
toggles the highlighted filter and keeps the list open; `Esc` closes it.
Until you change anything, the filters the adapter marks as defaults are on.
Your choice is sent at once and kept for later sessions until Vix exits.

The adapter only lists its filters once a session has started. Before that,
the status line says *No exception filters: start a debug session whose
adapter offers them*.

<!-- Sources: tui/src/app/debug_panel.rs (open_exception_filters,
show_exception_filters, toggle_exception_filter); crates/vix-i18n/locales/ui.yml
(ui.debug_exception_filters_title, ui.debug_exception_filters_hint);
crates/vix-i18n/locales/status.yml (status.debug_no_exception_filters);
tui/tests/integration/debugger.rs (exception_filters_and_the_panel_need_a_session,
panel_tree_exception_filters_set_variable_and_inline_values). -->

## Step through the program

When the program stops at a breakpoint (or after **Pause**), the editor jumps
to the stop, the line is marked `▶` in the gutter, and the status line says
*Paused (REASON)* with the adapter's reason.

- **Run → Continue** runs to the next stop.
- **Run → Step Over**, **Step Into** and **Step Out** move one step.
- **Run → Pause** stops a running program.

### Inline values

While stopped, the stopped line shows the values of the variables it mentions
at its end, for example:

```text
y = x + p.x        x = 1  p = Point
```

Only whole names count (`xx` does not show `x`), at most eight variables are
shown, and a value longer than 40 characters is cut short with `…`. The
values come from the first scope the panel expands (usually the locals). They
disappear when the program continues or the session ends.

<!-- Sources: tui/src/app/lsp_dap.rs (run.continue / step_* / pause arms,
refresh_debug_markers); crates/vix-i18n/locales/status.yml
(status.debug_stopped_at); tui/src/app/debug_panel.rs (debug_inline_values);
crates/vix-dap/src/view.rs (inline_values, INLINE_MAX = 8,
INLINE_VALUE_MAX = 40, test inline_values_match_whole_identifiers_on_the_line);
crates/vix-editor-core/src/render.rs (▶ the debugger's stopped line). -->

## The Debug panel

**Run → Toggle Debug Panel** (`run.panel`) shows or hides the **Debug** panel
in the right dock; it also opens by itself on **Start**. With no session it
reads *Not debugging. Debug → Start.* During a session it has four sections:

- **Threads**: every thread; `▶` marks the current one.
- **Call Stack**: the selected thread's frames.
- **Variables**: the selected frame's scopes, each expandable. The first
  scope that is cheap to fetch opens by itself.
- **Watch**: your watch expressions and their values.

An expandable row shows `▸` (closed) or `▾` (open), and a value follows its
name after ` = `.

**Run → Focus Debug Panel** (`run.focus_panel`) shows the panel and gives it
the keyboard:

| Key | Action |
| --- | --- |
| `↑` / `↓`, `PageUp` / `PageDown`, `Home` / `End` | Move the highlight (section headings are skipped) |
| `Enter` | On a thread: select it. On a frame: select it and jump the editor there. On a scope or variable: expand or collapse it |
| `→` / `←` | Expand / collapse (`←` on a child moves to its parent) |
| `e` | Change the highlighted variable's value (*New value for NAME*), if the adapter supports it |
| `b` | Toggle a data breakpoint on the highlighted variable |
| `Esc` | Give the keyboard back to the editor |

Keys with `Ctrl`, `Alt` or `Super`, and the function keys, still go to your
keymap, so **Run** commands bound to keys keep working while the panel has
focus.

Children of a variable are fetched the first time you open it, to any depth.
When the program stops again, the variables you had open are opened again,
matched by name, so stepping through a loop keeps your view.

<!-- Sources: tui/src/ui/docks.rs (draw_debug_panel, debug_row_line markers
▶ ▾ ▸ and " = "); crates/vix-i18n/locales/ui.yml (ui.debug, ui.debug_idle,
ui.debug_threads, ui.debug_call_stack, ui.debug_variables, ui.debug_watch);
tui/src/app/debug_panel.rs (focus_debug_panel, debug_panel_key,
debug_activate, begin_set_variable); crates/vix-i18n/locales/prompt.yml
(prompt.debug_set_variable); crates/vix-dap/spec/index.md (Debug panel:
first non-expensive scope, remembered expansion by name path). -->

## Watches and Evaluate

- **Run → Add Watch…** (`run.watch`) asks for a *Watch expression*. It is
  evaluated in the selected frame every time the program stops and listed
  under **Watch**.
- **Run → Evaluate…** (`run.repl`) asks *Evaluate (debug)* and prints the
  result of a one-off expression to the bottom dock.

<!-- Sources: tui/src/app/lsp_dap.rs ("run.watch", "run.repl" prompts);
crates/vix-i18n/locales/prompt.yml (prompt.debug_watch, prompt.debug_repl);
crates/vix-dap/spec/index.md (evaluate runs in the selected frame). -->

## Remote workspaces

In a [remote workspace](../remote-editing/index.md) the adapter runs on the
remote machine, started in the workspace root, and breakpoints and stops use
the remote paths. The debugger waits until you trust the remote workspace.
An adapter that listens on a TCP port instead of standard input and output is
not supported.

<!-- Sources: tui/src/app/lsp_dap.rs (start_debugger: set_workdir to the root
when the authority is not local); crates/vix-dap/spec/index.md (Remote
workspaces); tui/tests/authority_remote_tools.rs. -->

## Limits

- One debug session at a time.
- Data and instruction breakpoints have no row in the Debug panel, and an
  instruction breakpoint takes a memory reference only (no condition).
- A remembered expansion is matched by names, so two sibling variables with
  the same name share it.

See the specification at `crates/vix-dap/spec/index.md`.

<!-- Sources: crates/vix-dap/spec/index.md (Limitations). -->

---

Vix™ and Vix IDE™ are trademarks.
