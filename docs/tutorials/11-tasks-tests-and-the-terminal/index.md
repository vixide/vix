# Tutorial 11: Tasks, Tests and the Terminal

Vix runs your project's commands without you leaving the editor. One
command pipeline sits under everything in this tutorial: a manual **Run
Command**, a named task, a **Project** menu command and **Run Tests** all
start a shell in the background, stream its output into the bottom dock,
and scan that output for compiler errors you can jump to. For interactive
work there is a real terminal on a pseudo-terminal, with as many tabs as you
want.

This tutorial uses the demo workspace:

```sh
cd tui/examples/demo-workspace
vix .
```

None of these commands has a key in the default Apple keymap, so the steps
use the menu bar and the command palette. Where another keymap binds a key,
the step says which keymap. The full per-keymap tables are in
[`docs/reference/`](../../reference/actions.md).

## 1. Run a one-off command

Open **Tools → Run Command…** (`tools.run_command`). A prompt titled **Run
command (shell)** opens. Type:

```sh
python3 scripts/hello.py
```

and press `Enter`. The bottom dock opens below the editor, titled
**Output**, and shows:

- a `$ python3 scripts/hello.py` line, so you can see what ran;
- the script's output, starting with `4 rows in sample.csv`;
- an `[exit 0]` line when the process ends.

The status bar says `Command exited 0`, and the notification panel records
the run (Info on exit 0, Error otherwise). The command runs in the workspace
root with stdout and stderr merged, and only one command runs at a time:
starting another while one is running reports `A command is already
running`. **Tools → Cancel Command** (`tools.cancel_command`) kills the
running one and writes `[cancelled]` to the dock.

### The bottom dock

The bottom dock keeps the last 1000 lines. While you are at the newest line
it follows new output; scroll up and it stops following, so a long build
does not pull the view away from what you are reading. Click the dock to
focus it, then:

| Key | Does |
| --- | --- |
| `Up` / `Down` | Scroll one line |
| `PageUp` / `PageDown` | Scroll one page |
| `Home` | Jump to the top (stops following) |
| `End` | Jump to the bottom (follows again) |
| `Esc` | Give focus back to the editor |

Show or hide it with **View → Layout → Show/Hide Bottom Dock**
(`view.bottom_dock`; `Ctrl+J` in the VS Code and Zed keymaps), and resize it
by dragging its top edge. Clicking a line that names a location, such as
`src/main.rs:10:22`, opens that file at that line. You will use that in
step 5.

## 2. Run a named task

Typing the same command twice is what tasks are for. The demo workspace
ships a `tasks.toml` at its root:

```toml
[[task]]
name = "build"
command = "cargo build --manifest-path rust-app/Cargo.toml"

[[task]]
name = "test"
command = "cargo test --manifest-path rust-app/Cargo.toml"

[[task]]
name = "run"
command = "cargo run --manifest-path rust-app/Cargo.toml"

[[task]]
name = "report"
command = "python3 scripts/hello.py"
```

Open **Tools → Tasks…** (`tools.tasks`). The **Run Task** chooser lists one
row per task as `name — command`. Move to `report` with `↓` and press
`Enter` (or click the row). It runs through the same pipeline as step 1:
output in the bottom dock, the result in the notification panel. `Esc`
closes the chooser without running anything.

Vix reads `<root>/tasks.toml`, and falls back to `<root>/.vix/tasks.toml`
if you would rather keep it out of the way. An entry with an empty `name` or
`command` is dropped. With no tasks at all, the status bar says `No tasks
defined (add a tasks.toml)`.

## 3. Tasks Vix finds by itself

Your project's build files already name its commands, and Vix can list them
without a `tasks.toml`. To see this, create a throwaway `Makefile` at the
demo root from your shell (Make needs a real tab before the recipe line, and
`printf` writes one):

```sh
printf 'hello:\n\tpython3 scripts/hello.py\n' > Makefile
```

Now open **Project → Run Task…** (`project.run_task`; `C-c p c x` in the
Emacs keymap). This chooser has the four `tasks.toml` tasks and a fifth row,
`make:hello — make hello`. Discovered tasks always carry their tool's
prefix, so they never clash with your own task names. The sources Vix reads
are:

| Tool | File | Prefix |
| --- | --- | --- |
| npm, yarn, pnpm, bun | `package.json` `scripts` | `npm:`, `yarn:`, … |
| Deno | `deno.json` / `deno.jsonc` `tasks` | `deno:` |
| Composer | `composer.json` `scripts` | `composer:` |
| just | `justfile` / `.justfile` / `Justfile` | `just:` |
| go-task | `Taskfile.yml` / `Taskfile.yaml` | `task:` |
| Rake | `Rakefile`, `*.rake`, `rakelib/*.rake` | `rake:` |
| Make | `Makefile` / `makefile` / `GNUmakefile` | `make:` |

Only files in the workspace root are read. A file Vix cannot parse adds no
tasks; it does not hide the others.

Run `make:hello`, then open **Project → Repeat Last Task**
(`project.repeat_last_task`; `C-c p c X` in the Emacs keymap). It runs the
same command again. Repeat Last Task remembers the last project command of
any kind, whether a task or one of the lifecycle commands in the next step.

Delete the `Makefile` when you are done (`rm Makefile`).

## 4. The Project menu's lifecycle commands

The top of the **Project** menu has six fixed slots: **Configure**,
**Compile**, **Test**, **Install**, **Package** and **Run**. Vix fills each
one from the project type it detects at the root (`Cargo.toml` means cargo,
`package.json` means npm, and also yarn, pnpm and bun by lockfile, plus
make, just, Python and Poetry, Go and Deno). A cargo project, for example,
gets `cargo build` for Compile, `cargo test` for Test and `cargo run` for
Run.

| Slot | Action | Emacs keymap |
| --- | --- | --- |
| Configure | `project.configure` | `C-c p c o` |
| Compile | `project.compile` | `C-c p c c` |
| Test | `project.test` | `C-c p c t` |
| Install | `project.install` | `C-c p c i` |
| Package | `project.package` | `C-c p c p` |
| Run | `project.run` | `C-c p c r` |

Open **Project → Test**. The demo root has no marker file of its own (the
Rust code is one level down, in `rust-app/`), so the status bar says `No
command configured for this project type.`

Give the root a command of its own. Create `.vix/project.toml` in the demo
workspace with:

```toml
run = "python3 scripts/hello.py"
```

The keys are the six slot names: `configure`, `compile`, `test`, `install`,
`package` and `run`. This file is meant to be committed so the whole team
gets the same commands. Now open **Project → Run**. Instead of running
straight away, Vix opens a **Run command** prompt holding
`python3 scripts/hello.py`. Edit it if you like, for example add
`; echo done`, and press `Enter`.

The command you accept is remembered for this workspace, across restarts,
and wins over both `.vix/project.toml` and the detected default from then
on. Open **Project → Run** again and the prompt holds your edited version.
To go back to the file's command, use **Project → Discard Command Cache**
(`project.discard_command_cache`). It clears only your remembered commands
and never touches `.vix/project.toml`.

Delete `.vix/project.toml` when you are done, then use **Discard Command
Cache** once more so that the next step starts clean.

## 5. Subprojects, and jumping to errors

Monorepos have several projects under one root, and `rust-app/` is one.
Open `tui/examples/demo-workspace/rust-app/src/main.rs` (in the explorer,
`rust-app/` → `src/` → `main.rs`), then **Project → Subproject → Compile**
(`project.subproject.compile`; `C-c p c m c` in the Emacs keymap). Vix finds
the nearest directory above the file that has a marker file, `rust-app/`,
detects cargo there, and opens the **Compile command** prompt with `cargo
build`. Press `Enter`. The build runs inside `rust-app/`.

**Project → Subproject** has the same six slots plus **Find File**, which
opens the palette limited to that subproject's files. From a file outside
any subproject, these report `No subproject manifest found between this file
and the project root.` The override file and the remembered commands belong
to the workspace root, though, and the subproject menu shares them: only the
detected default and the directory the command runs in change.

Now break the build. In `main.rs`, add this line straight after `let counts
= word_counts(text);`:

```rust
    let total: u32 = "all";
```

Save, and run **Project → Subproject → Compile** again. rustc prints
`error[E0308]: mismatched types` followed by `--> src/main.rs:10:22`. When
the build ends, the status bar says `Command exited 101: 1 problems (Go →
Next Error)`. Vix read the output as it streamed and found one problem.
There are four ways to get to it:

- **Go → Next Error** (`go.next_error`) opens `main.rs` at line
  10, column 22, and the status bar shows `Problem 1 of 1:` with the message.
  **Go → Previous Error** (`go.prev_error`) goes the other way. Both wrap
  around at the ends of the list.
- With the cursor on that line, the message is drawn at the end of the line,
  the same way as inline LSP diagnostics. The `inline_diagnostics` setting
  (`off`, `cursor-line` (the default) or `all`) controls both.
- **Tools → Language Server → Diagnostics…** (`lsp.diagnostics`; `Ctrl+Shift+M`
  in the VS Code and Zed keymaps) is the Problems list. It shows language
  server diagnostics first, then the task problems, each tagged with the
  matcher that found it (`rustc`). `Enter` on a row jumps there.
- Click the `--> src/main.rs:10:22` line in the bottom dock.

Paths are resolved against the directory the command ran in, here
`rust-app/`, so the relative path in `--> src/main.rs:10:22` finds the
right file. Vix
recognizes rustc and cargo, gcc and clang, tsc, eslint, Python tracebacks
and pytest, and the Go toolchain. A task in `tasks.toml` can choose one,
turn matching off, or supply its own regular expression with `file` and
`line` groups:

```toml
[[task]]
name = "lint"
command = "npx eslint src"
matcher = "eslint"   # or "auto" (the default), "none", or a regex
```

Remove the `let total` line and save before you go on.

## 6. Run the tests

**Tools → Run Tests** (`tools.test`; `Ctrl+B` in the Eclipse and Sublime
keymaps) runs the `test_command` setting, which defaults to `cargo test`. It
runs in the workspace root, so for this step open Vix on the Rust project
itself:

```sh
cd tui/examples/demo-workspace/rust-app
vix .
```

Open **Tools → Run Tests**. Output streams to the bottom dock as usual, and
the **Test panel** opens on the right. When the run ends, Vix parses the
output and the panel lists each test with a `✓` (passed), `✗` (failed) or
`○` (ignored) icon. The status bar and the notification panel report the
tally: `Tests: 1 passed, 0 failed, 0 ignored`.

Now make the test fail. In `main.rs`, change the first assertion in
`counts_repeated_words` to expect `4`:

```rust
        assert_eq!(counts.get("a"), Some(&4));
```

Save and run **Tools → Run Tests** again. The tally becomes `Tests: 0 passed,
1 failed, 0 ignored`, the notification is an Error, and the panel shows
`✗ tests::counts_repeated_words`. **Click** that row: Vix opens
`main.rs` at line 37, the line cargo reported in `panicked at
src/main.rs:37:9`. Put the `3` back afterwards.

**Tools → Toggle Test Panel** (`tools.test_panel`) shows or hides the panel.
To use another runner, set `test_command` in `config.toml`
(`~/.config/vix/config.toml` on Linux,
`~/Library/Application Support/rs.vix/config.toml` on macOS,
`%APPDATA%\vix\config\config.toml` on Windows):

```toml
test_command = "pytest -v"   # or "npm test", "go test ./..."
test_width = 40              # width of the Test panel
```

The panel understands libtest output (`cargo test`) and the
`name PASSED` / `FAILED` / `SKIPPED` lines that `pytest -v` and similar
runners print. Only libtest output includes failure locations, so only
cargo rows can be clicked through to a line.

### Test at point

**Project → Test at Point** (`project.test_at_point`; `C-c p c .` in the
Emacs keymap) finds the test the cursor is in, or the nearest test above it,
and runs only that test, with no prompt. It reads the lines around the
cursor rather than a syntax tree, and supports Rust, Python, Go,
JavaScript/TypeScript, Ruby and Elixir. With the cursor inside
`counts_repeated_words`, the bottom dock shows the command it built:

```sh
cargo test -- --exact tests::counts_repeated_words
```

`--exact` compares a test's full path, so Vix builds it from the file's
place in `src/` and the `mod` blocks around the test: here `tests::`, for
the `mod tests` block in `main.rs`. Cargo reports `running 1 test`.

## 7. The integrated terminal

Go back to the demo root (`cd ..`, then `vix .`). Open **Tools → Terminal →
Show/Hide Terminal** (`tools.terminal`; ``Ctrl+` `` in the VS Code, Zed and
Sublime keymaps, `Ctrl+T` in the TTT keymap). Your shell (`$SHELL`, else
`/bin/sh`; `cmd.exe` on Windows) starts on a real pseudo-terminal in the
workspace root, drawn over the editor area. Full-screen programs, colors
and the mouse all work.

While the terminal is shown, it gets every key except its own chords:

| Key | Does |
| --- | --- |
| `Ctrl+]` | Hide the terminal; it keeps running |
| `Shift+PgUp` / `Shift+PgDn` | Page through the scrollback |
| `Shift+Home` / `Shift+End` | Oldest kept line / back to the live screen |
| `Ctrl+PgUp` / `Ctrl+PgDn` | Previous / next terminal |
| `Ctrl+\` | Toggle key capture: let `Ctrl` and `Alt` chords reach the editor instead |

Try it:

1. In the terminal, run `grep -rn TODO scripts`. One line comes back,
   starting with `scripts/hello.py:23:`.
2. **Ctrl+click** that `scripts/hello.py:23` text. The terminal hides and
   the editor opens `scripts/hello.py` at line 23. Ctrl+click reads
   `path`, `path:line` and `path:line:col`, and falls back to the same
   problem parser as the bottom dock, so compiler errors and Python
   tracebacks work too.
3. Open `tasks.toml`, select the text `python3 scripts/hello.py` on the
   `report` task's line, and choose **Tools → Terminal → Send Selection to
   Terminal** (`terminal.send_selection`). The terminal opens and runs it.
   With no selection it sends the current line.
4. **Tools → Terminal → New Terminal** (`terminal.new`) opens a second
   shell. A row of numbered tabs appears at the top; `Ctrl+PgDn` and
   `Ctrl+PgUp` switch between them.
5. Type `exit` in one of them. The tab stays, marked `exited`, with its
   output. Press `Enter` to start the shell again in place, or any other
   key to close the tab.

The rest of the **Tools → Terminal** submenu: **Next Terminal** /
**Previous Terminal**, **Copy Terminal Text** (the selection, or the whole
visible screen), **Restart Terminal** (a fresh shell in the same tab, screen
and scrollback kept), **Toggle Terminal Key Capture** and **Close
Terminal**. Dragging with the left mouse button selects text and copies it
when you let go. Each terminal keeps 2000 lines of scrollback; the
`terminal_scrollback` setting changes that, up to 50 000.

Use the terminal for interactive work (a REPL, `git rebase -i`, `htop`), and
Run Command or a task for commands whose output you want to keep, scan for
problems and jump from.

## Where to go next

- [Tasks](../../tasks/index.md): `tasks.toml`, problem matchers and custom
  matcher expressions.
- [Test Runner](../../test-runner/index.md) and
  [Integrated Terminal](../../terminal/index.md): the user guides for
  steps 6 and 7.
- [Docks and layout](../../bottom-dock/index.md): the bottom dock, and the
  other two docks around the editor.
- [IDE parity](../../ide-parity/index.md): run configurations in
  `.vix/run.toml` (**Project → Run Configuration…**), which chain tasks,
  set `cwd` and `env`, and run several commands together.
- [Tutorial 10: Debugging with DAP](../10-debugging-with-dap/index.md): when
  a failing test needs a debugger rather than a print statement.
- The specs behind this page:
  [`vix-tasks`](../../../crates/vix-tasks/spec/index.md),
  [`vix-test-runner`](../../../crates/vix-test-runner/spec/index.md),
  [`vix-terminal`](../../../crates/vix-terminal/spec/index.md) and
  [`vix-bottom-dock`](../../../crates/vix-bottom-dock/spec/index.md).
- [Emacs keybindings](../../reference/keybindings-emacs.md): the full
  `C-c p c` chord family.

---

**Previous:** [Tutorial 10 — Debugging with DAP](../10-debugging-with-dap/index.md)
**Next:** [Tutorial 12 — Remote Editing](../12-remote-editing/index.md)

---

Vix™ and Vix IDE™ are trademarks.
