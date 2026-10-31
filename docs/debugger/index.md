# Debugger

Vix™ has an integrated debugger built on the **Debug Adapter Protocol** (DAP), the
same standard used by VS Code. You bring a debug adapter (e.g. `debugpy`,
`codelldb`); Vix drives it.

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

## Debugging

From the **Run** menu (or the command palette):

- **Toggle Breakpoint** marks the cursor line with a red `●` in the gutter.
- **Edit Breakpoint…** sets that line's breakpoint **condition**, **hit count**
  and **log message** (a logpoint prints instead of stopping), one prompt each.
- **Exception Breakpoints…** toggles the exception filters the adapter offers
  (its defaults start ticked).
- **Start** launches the adapter for the active file; **Attach** attaches with
  the adapter's `attach` table instead. Execution runs until it hits a
  breakpoint, where the line is marked `▶`, the editor jumps there, and the
  line shows the values of the variables it mentions.
- **Continue**, **Step Over/Into/Out**, and **Pause** drive execution.
- The **Debug panel** (toggle it, or it opens on Start) shows the **threads**,
  the **call stack**, the selected frame's **variables** and your **watches**.
  **Focus Debug Panel** gives it the keyboard: Enter selects a thread or frame
  or expands a variable (children load on demand, to any depth), Left
  collapses, `e` sets a variable's value, Esc returns to the editor.
- **Add Watch…** keeps an expression evaluated at every stop; **Evaluate…** runs a
  one-off expression and prints the result to the bottom dock.
- **Stop** ends the session.

Program output streams to the bottom dock.

See the specification at `crates/vix-dap/spec/index.md`. One debug session runs at a
time.

---

Vix™ and Vix IDE™ are trademarks.
