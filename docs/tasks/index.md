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

See the specification at `crates/vix-tasks/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
