# Tasks

A workspace can define named shell commands in `tasks.toml`; **Tools → Tasks…**
(action `tools.tasks`, also in the command palette) lists them and runs the
chosen one through the async Run Command pipeline.

## File

Loaded from `<root>/tasks.toml`, falling back to `<root>/.vix/tasks.toml`:

```toml
[[task]]
name = "build"
command = "cargo build"

[[task]]
name = "test"
command = "cargo test"
```

Tasks with an empty `name` or `command` are dropped. If neither file exists or
parsing fails, the chooser reports `status.no_tasks`.

## Behavior

- The chooser is a list of `name — command` rows; `↑`/`↓` move, `Enter` (or a
  row click) runs the highlighted task, `Esc` cancels.
- Running a task calls `App::run_command`, so output streams to the bottom dock
  and the completion posts to the notification panel (Info on exit 0, Error
  otherwise) — exactly like a manual Run Command.

## As implemented in Vix

The `tasks` module (`Task`, `load`, `parse`) reads and validates `tasks.toml`
(via the `toml` crate). The host owns `TaskChooser`, `open_tasks`, `tasks_key`,
`tasks_mouse`, and `run_selected_task`; `ui::draw_task_chooser` renders it with
the shared list chooser.
