# Session restore

Per-workspace editor session, persisted with `confy`. Vix remembers what you
had open per workspace. When you relaunch in the same directory **with no file
given on the command line**, it reopens the previous session: the open files,
the focused tab, and each tab's cursor position.

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

The session lives next to the [configuration](../../../docs/configuration/index.md) in the
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
focused = 0
# A binary tree of panes; leaves index into `files`.
[workspaces.split.tree.Split]
dir = "vertical"
ratio = 50
first.Leaf = 0
second.Leaf = 1
```

`scrolls` (the first visible line per file) and `split` (the pane-tree layout) are
optional — older session files without them still load (`#[serde(default)]`). The
split is restored only when every file reopened cleanly, so the recorded leaf
indices still line up with the tab order.

## Setting

`restore_session` (default `true`) in `config.toml` turns the feature off. With
it disabled, Vix always starts with a single empty buffer unless files are given
on the command line.

## Project task-runner state

`WorkspaceSession` also carries the private, per-user half of the `project.*`
action family's two-tier persistence (see `crates/vix-tasks/spec/index.md`
for the full picture; the shareable half is the host's `.vix/project.toml`,
outside this crate): six `project_cmd_*` fields (a resolved/edited lifecycle
command cache, one per slot: configure/compile/test/install/package/run), six
`project_history_*` fields (that slot's command run history, most-recent
last), and `project_last_command` (the most recently run project command of
any kind, for "repeat last task"). These are plain fields — not a
`vix_tasks` struct — so this crate stays a dependency-free leaf; the host
(`src/app.rs`) converts at the call site. `App` loads them lazily, only once a
`project.*` action is first used in a run, and never overwrites them on exit
without having loaded them first, so a run that never touches project features
cannot silently wipe previously saved project state.

## Script trust (T132)

`WorkspaceSession::scripts_trusted: Option<bool>` records whether this
workspace's project scripts (`.vix/scripts/*.rhai`, see
`crates/vix-script/spec/index.md`'s "Script discovery") are trusted to
load: `None` (not yet asked) is the safe default an older `session.toml`
loads as too. `Session::set_scripts_trusted(root, trusted)` is the only
way to change it — deliberately not `Session::set_workspace`, which also
bumps `visits` (a script-trust decision isn't a workspace "open") and
would otherwise require the caller to round-trip every other field on
that workspace's session just to flip this one.

## Roadmap

- Per-workspace "reopen last session" command for the disabled-by-default case.
