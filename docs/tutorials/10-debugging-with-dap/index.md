# Debugging with DAP

Vix drives real debuggers over the **Debug Adapter Protocol** (DAP) — the
same protocol VS Code uses — the same way it drives language servers over
LSP. You configure an adapter once in `config.toml`, and Vix speaks the
protocol to it: breakpoints, stepping, a live call stack, variables, and
watches, all inside the editor.

This tutorial walks through a complete session with **`debugpy`** (Python),
debugging `examples/demo-workspace/scripts/hello.py`. Vix's other
documented adapter, **`codelldb`** (for Rust/C/C++, e.g.
`examples/demo-workspace/rust-app`), is configured the same way — see
"Using `codelldb` instead" near the end for its config block.

For the condensed reference version of everything below, see
[`docs/debugger/index.md`](../../debugger/index.md) and the full protocol
detail at
[`crates/vix-dap/spec/index.md`](../../../crates/vix-dap/spec/index.md).

## Install `debugpy`

`debugpy` is a Python package, not a separate binary — install it into
whichever Python environment you'll run `hello.py` with:

```sh
pip install debugpy
```

Confirm it's importable:

```sh
python3 -m debugpy --version
```

## Configure the adapter

Debug adapters live under `[[debug_adapters]]` in `config.toml` — the same
file LSP servers go in (`~/.config/vix/config.toml` on Linux,
`~/Library/Application Support/rs.vix/config.toml` on macOS,
`%APPDATA%\vix\config\config.toml` on Windows). `debug_adapters` is empty
by default, so add an entry:

```toml
[[debug_adapters]]
adapter_id = "debugpy"
extensions = ["py"]
command = ["python", "-m", "debugpy.adapter"]
[debug_adapters.launch]
console = "internalConsole"
```

- `extensions` routes files to this adapter by extension, the same way
  `lsp_servers` entries do.
- `command` is how Vix launches the adapter — `debugpy.adapter` speaks DAP
  over stdio directly, no separate "attach" step needed.
- `[debug_adapters.launch]` is merged into the DAP `launch` request. Any
  value in it may use `{program}`, which expands to the file path being
  debugged; `program` itself defaults to the active file when you don't set
  it, which is enough for a single-script target like `hello.py`.
  `console = "internalConsole"` keeps the debuggee's output flowing into
  Vix's bottom dock rather than trying to open a separate terminal.

Save the file, then open the demo workspace:

```sh
cd examples/demo-workspace
vix scripts/hello.py
```

## Set a breakpoint

`hello.py` prints one line per CSV row and has a real, unvalidated-input
FIXME sitting right next to that loop:

```python
17	    rows = load_rows(data_path)
18
19	    print(f"{len(rows)} rows in {data_path.name}")
20	    for row in rows:
21	        print(f"  {row['name']}: {row['score']}")
22
23	    # TODO: also print the average score across all rows.
24	    # FIXME: this assumes every row has a "score" column that parses as a
25	    # number -- it doesn't validate that before use.
```

Put the cursor on line 21 (the `print(f"  {row['name']}: {row['score']}")`
line, inside the loop) and open **Run → Toggle Breakpoint**
(action id `run.toggle_breakpoint`; the Eclipse keymap binds `Ctrl+Shift+B`
to it — every other keymap only has it on the Run menu and the command
palette). The gutter marks the line with a red `●`. Toggling the same line
again clears it; breakpoints persist across Start/Stop within the session,
and toggling one while a session is already running updates it live.

## Start the session

Open **Run → Start** (`run.start`). Behind the scenes Vix launches
`python -m debugpy.adapter`, sends `initialize`, and — once the adapter's
`initialized` event comes back — sends your breakpoints (`setBreakpoints`),
then `launch`, then `configurationDone`. The status line reports "Debug
session started" and the **Debug panel** opens automatically on the right
(you can also toggle it by hand with **Run → Toggle Debug Panel**,
`run.panel`).

`hello.py` starts running, prints `4 rows in sample.csv`, then hits your
breakpoint on the first loop iteration. Vix jumps the editor to line 21 and
marks it with `▶` in the gutter — this is the `stopped` event; Vix fetches
the stack trace and the top frame's variables the moment it arrives.

## Inspect variables and the call stack

The Debug panel now shows two sections:

- **Call Stack** — one line per frame, `<function name>` with the
  `<file>:<line>` it's stopped at in dim text. For a plain script like this
  you'll see one frame, `main`, at `hello.py:21`.
- **Variables** — the stopped frame's top-level scope, one `name = value`
  line per entry. Look for `row` — on the first stop it reads something
  like `row = {'name': 'Alice', 'role': 'engineer', 'score': '92'}`. Notice
  `'92'` is a *string*, not a number — exactly the gap the FIXME is about;
  every value straight out of `csv.DictReader` is a `str`, so any arithmetic
  on `row["score"]` needs an explicit `int()` first.

(Variable trees in Vix are one level deep — the top frame's first scope —
so nested structures show their `repr()` rather than expanding further.)

## Step through execution

With **Run → Continue** (`run.continue`) you'd run to the next breakpoint
hit — since the same line 21 breakpoint fires every loop iteration, hit
**Continue** three more times and watch `row` in the Variables list update
to Bob, then Carol, then Dave each time execution stops again.

To go line-by-line instead:

- **Step Over** (`run.step_over`) — runs the current line without entering
  any function it calls. On line 21 this just advances to the next loop
  iteration.
- **Step Into** (`run.step_into`) — steps into a called function. Put a
  breakpoint on line 17 (`rows = load_rows(data_path)`) instead, restart,
  and **Step Into** there to land inside `load_rows` itself, on the
  `with path.open(...)` line.
- **Step Out** (`run.step_out`) — runs until the current function returns,
  then stops in its caller.
- **Pause** (`run.pause`) — interrupts a running (non-stopped) session, for
  when you didn't set a breakpoint but want to break in anyway.

## Watches and the debug console

- **Run → Add Watch…** (`run.watch`) prompts for an expression and
  evaluates it every time execution stops, adding a **Watches** section to
  the Debug panel below Variables. Try `row['score']` — you'll see the
  string value re-evaluate at each subsequent stop, right alongside the
  same field already visible under Variables.
- **Run → Evaluate…** (`run.repl`) is a one-off version of the same thing —
  it prompts for an expression, evaluates it once in the current frame, and
  prints the result to the bottom dock instead of pinning it. Try
  `int(row['score']) * 2` while stopped on line 21 to confirm the fix the
  FIXME wants (`int(...)` first) is exactly what's missing today.

Program `print()` output streams to the bottom dock the same way, as
`output` events arrive — you already saw this with the `4 rows in
sample.csv` line at the top of the session.

## No conditional or logpoint breakpoints

Vix's breakpoints are a plain per-line toggle — set or cleared, nothing
else. There's no condition expression, no hit count, and no logpoint
(print-without-stopping) variant; if you want to stop only on Dave's row,
step or continue to it by hand, or temporarily edit the script's condition
directly. This is a documented limitation, not a step you're missing.

## Stop

**Run → Stop** (`run.stop`) terminates the adapter and the debuggee,
clears the call stack and variables from the Debug panel, and clears the
`▶` stop marker (your `●` breakpoints stay set for next time). Vix runs one
debug session at a time — start a second one and it replaces the first.

## Using `codelldb` instead

For a compiled target like `examples/demo-workspace/rust-app`, swap in
`codelldb` (install it via the
[CodeLLDB VS Code extension](https://github.com/vadimcn/vscode-lldb) or any
standalone build of the same binary) with its own adapter entry:

```toml
[[debug_adapters]]
adapter_id = "codelldb"
extensions = ["rs", "c", "cpp"]
command = ["codelldb"]
[debug_adapters.launch]
program = "{program}"
```

`{program}` here needs to expand to the *built binary*, not the `.rs`
source file, so run `cargo build` in `rust-app/` first and open the crate
from its `target/debug/` binary path (or point `program` at a fixed path
rather than relying on `{program}`) before hitting **Run → Start**. Every
other command in this tutorial — Toggle Breakpoint, Continue, the stepping
trio, Watches, Evaluate — works identically once the session is running;
only adapter configuration and what counts as "the program" differ between
the two.

## Closing note

That's the last tutorial in this series. If you want a refresher on
anything earlier, start back at
[Tutorial 1: Your First Session](../01-your-first-session/index.md), or
jump straight to [Getting Started](../../getting-started/index.md) for
install/config basics.

---

**Previous:** [Tutorial 9 — Make Vix Yours](../09-make-vix-yours/index.md)

---

Vix™ and Vix IDE™ are trademarks.
