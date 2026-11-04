# Session restore

Per-workspace editor session, persisted with `confy`. Vix remembers what you
had open per workspace. When you relaunch in the same directory **with no file
given on the command line**, it reopens the previous session: the open files,
the focused tab, and each tab's cursor position.

## Behavior

- **Save** — on exit (`App::on_exit`), Vix captures the current session and
  writes it to the per-workspace store. Untitled buffers and image tabs are
  skipped; only saved files are recorded. So are files matching an
  `ephemeral_paths` glob (default `**/.git/**`, T662), so `COMMIT_EDITMSG` and
  rebase todo lists never reopen at a stale cursor.
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

`acp_agent` and `acp_session` (both optional strings, T635) hold the last Agent
Client Protocol agent command started in the workspace and its session id, so
**AI → Resume Agent** works after a restart; see
[`vix-acp`](../../vix-acp/spec/index.md). Saving the workspace never forgets
them.

`scrolls` (the first visible line per file) and `split` (the pane-tree layout) are
optional — older session files without them still load (`#[serde(default)]`). The
split is restored only when every file reopened cleanly, so the recorded leaf
indices still line up with the tab order.

## Setting

`restore_session` (default `true`) in `config.toml` turns the feature off. With
it disabled, Vix always starts with a single empty buffer unless files are given
on the command line.

## Read-only loading (T731)

`Session::load` goes through `confy`, which creates a default `session.toml`
when none exists. `Session::load_existing` (and `load_existing_from` for an
explicit file) parses the file directly and never creates or writes one; a
missing or unreadable file is an empty session. The read-only `vix --mcp`
server ([`spec/mcp-server`](../../../spec/mcp-server/index.md)) uses it to
list the files Vix last had open in a workspace.

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
(`tui/src/app.rs`) converts at the call site. `App` loads them lazily, only once a
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

`Session::set_workspace` (a captured session) carries `scripts_trusted` and
`remote_trusted` forward from the entry it replaces when the new one leaves
them unset, so saving a session on exit never forgets a trust decision
(fixed 2026-10-30, T714: it used to reset `scripts_trusted`).

## Jupyter evaluation trust (T734f)

`WorkspaceSession::jupyter_trusted: Option<bool>` records the answer "always
in this workspace" to the confirmation before an Org `src` block runs in a
Jupyter kernel (`spec/jupyter`): `Some(true)` runs this workspace's blocks
without asking and lets its own kernelspecs (`.venv`) be used; `None` (the
default, and what an older file loads as) keeps asking.
`Session::set_jupyter_trusted(root, trusted)` sets it like
`set_scripts_trusted` (a minimal entry for a new root, nothing else
touched), and `Session::set_workspace` carries it forward like the other
trust decisions.

## Remote workspaces (T713, T714)

A remote workspace's key (`root`) is its canonical `ssh://` name, so its
session, project state and trust are per host and path and never collide with
a local folder of the same path. `WorkspaceSession::remote_trusted:
Option<bool>` is that workspace's trust decision (`None` = not yet asked,
which an older file loads as); `Session::set_remote_trusted(root, trusted)`
changes only it. `Session::recent_remotes` lists the remote names opened
most recently first (at most `MAX_RECENT_REMOTES`, 10, no duplicates;
`Session::push_recent_remote`) for **File → Open Remote…**. Names never carry
a password: `ssh://` names hold only user, host, port and path.

## Roadmap

- Per-workspace "reopen last session" command for the disabled-by-default case.
