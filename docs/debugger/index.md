# Debugger

Vix has an integrated debugger built on the **Debug Adapter Protocol** (DAP), the
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
`{program}` expands to the file you're debugging.

## Debugging

From the **Debug** menu (or the command palette):

- **Toggle Breakpoint** marks the cursor line with a red `●` in the gutter.
- **Start** launches the adapter for the active file. Execution runs until it hits
  a breakpoint, where the line is marked `▶` and the editor jumps there.
- **Continue**, **Step Over/Into/Out**, and **Pause** drive execution.
- The **Debug panel** (toggle it, or it opens on Start) shows the **call stack**,
  the current frame's **variables**, and your **watches**.
- **Add Watch…** keeps an expression evaluated at every stop; **Evaluate…** runs a
  one-off expression and prints the result to the bottom dock.
- **Stop** ends the session.

Program output streams to the bottom dock.

See the specification at `spec/debugger/index.md`. One debug session runs at a
time.
