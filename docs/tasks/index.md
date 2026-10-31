# Tasks

Define your project's common commands once and run them from a menu instead of
retyping them. Vix™ reads a `tasks.toml` from the workspace root (or
`.vix/tasks.toml`) and lists the tasks under **Tools → Tasks…** (also in the
command palette).

## Defining tasks

```toml
[[task]]
name = "build"
command = "cargo build"

[[task]]
name = "test"
command = "cargo test --all"

[[task]]
name = "serve docs"
command = "python3 -m http.server -d docs"
```

Each `[[task]]` needs a `name` (shown in the chooser) and a `command` (a shell
command line). Entries missing either are ignored.

## Running a task

Open **Tools → Tasks…**, highlight a task with `↑` / `↓`, and press `Enter` (or
click it). The command runs through the same pipeline as **Run Command**: output
streams to the bottom dock and the completion is recorded in the
[notification panel](../notification-panel/index.md) (Info on success, Error on a
non-zero exit). If no `tasks.toml` is found, Vix says so in the status bar.

## Problems in task output

While a command runs — a task, a **Project** menu command, **Tools → Test**, or
a plain **Run Command** — Vix reads its output for compiler errors, lint
findings and failing tests, like VS Code's problem matchers or Emacs's
`next-error`. When it finishes, the status bar says how many problems it found.
Then:

- **Go → Next Error** and **Go → Previous Error** open each problem's file at
  its line and column, in the order the tool printed them, across files. At the
  last problem Next starts again from the first (and Previous from the last).
  The status bar shows "Problem 2 of 5" and the message.
- **Tools → Language Server → Diagnostics…** (the Problems list) shows them too, after the
  language servers' diagnostics, tagged with the tool that found them.
- The open file shows the message at the end of the problem's line, in the same
  way as inline diagnostics (the `inline_diagnostics` setting: `off`,
  `cursor-line` or `all`).
- Clicking a line in the bottom dock jumps to the location it names, including
  rustc's `  --> src/main.rs:4:18` lines and Python's `File "app.py", line 3`
  frames.

Vix recognizes rustc and cargo, gcc and clang, tsc, eslint, Python tracebacks
and pytest, and the Go toolchain. A task can pick one, turn matching off, or
give its own regular expression:

```toml
[[task]]
name = "lint"
command = "npx eslint src"
matcher = "eslint"          # or "rustc", "gcc", "tsc", "python", "go",
                            # "auto" (the default) or "none"

[[task]]
name = "check"
command = "./check.sh"
matcher = '^(?P<file>[^:]+):(?P<line>\d+): (?P<severity>\w+): (?P<message>.*)$'
```

A regular expression must have `file` and `line` groups; `column`, `severity`
and `message` are optional. If it is invalid, Vix says so and uses the built-in
matchers instead.

File names are taken relative to the folder the command ran in. Vix never opens
a file just because a command printed its name: it opens one only when you go
to that problem, and only if it exists.

See the specification at `crates/vix-tasks/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
