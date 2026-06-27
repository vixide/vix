# Session restore

Vix remembers what you had open per workspace. When you relaunch in the same
directory **with no file given on the command line**, it reopens the previous
session: the open files, the focused tab, and each tab's cursor position.

## Behavior

- **Save** — on exit (`App::on_exit`), Vix captures the current session and
  writes it to the per-workspace store. Untitled buffers and image tabs are
  skipped; only saved files are recorded.
- **Restore** — on startup, when no file argument is passed, `App::restore_session`
  loads the session for the current workspace root and reopens it. The fresh
  app's blank untitled buffer is dropped once at least one real file reopens. A
  status line reports how many files were restored. Files that no longer exist on
  disk are silently skipped.
- Passing a file on the command line (`vix path…`) opens those files instead and
  does **not** restore the session.

## Storage

The session lives next to the [configuration](../configuration/index.md) in the
`confy` config directory as a separate `session.toml` file, so it can be cleared
without touching preferences. It records one entry per workspace root, most
recently used first, capped at 50 workspaces. The workspace key is the
canonicalized root path, so symlinked paths map to one entry.

Schema (`vix::session`):

```toml
[[workspaces]]
root = "/home/you/project"
files = ["/home/you/project/src/main.rs", "/home/you/project/README.md"]
active = 0
cursors = [128, 0]
scrolls = [40, 0]

[workspaces.split]
dir = "vertical"
other = 1
focused_side = 0
ratio = 50
```

`scrolls` (the first visible line per file) and `split` (the two-pane layout) are
optional — older session files without them still load (`#[serde(default)]`). The
split is restored only when every file reopened cleanly, so the recorded pane
index still lines up with the tab order.

## Setting

`restore_session` (default `true`) in `config.toml` turns the feature off. With
it disabled, Vix always starts with a single empty buffer unless files are given
on the command line.

## Roadmap

- Per-workspace "reopen last session" command for the disabled-by-default case.
