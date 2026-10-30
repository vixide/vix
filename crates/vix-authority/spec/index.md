# Authority

The authority slot: one trait that owns Vix's workspace file access and process spawning, with local, SSH and container implementations and a temp-dir-backed fake.

It lets a remote machine stand in for the local one. The cross-cutting design is
[`spec/remote-editing`](../../../spec/remote-editing/index.md); this crate is
its first piece (T710, T711).

**Status:** Shipped (T710, T711; finished 2026-10-29; T712, T715, T716 and T717 2026-10-30).
`LocalAuthority` is installed by default. `SshAuthority` (the `ssh`
feature, [`spec/ssh`](ssh/index.md)) is installed by the experimental
`vix ssh://…` command line and by **File → Open Remote…** (T713; the App
side is in [`spec/remote-editing`](../../../spec/remote-editing/index.md)).
The remote agent (`vix --remote-agent`, T717, [`spec/agent`](agent/index.md))
speeds it up when a `vix` of the same version is on the remote machine.
`ContainerAuthority` (T718, [`spec/container`](container/index.md)) is the
same shell authority over `docker exec`, `podman exec`, `kubectl exec` or
`wsl.exe`, installed by `vix docker://…` (`podman://`, `k8s://`, `wsl://`),
the same dialog, and the OC menu.

## API

- `Authority` (trait, `Send + Sync + Debug`). One method per operation the
  App used to call directly, with the same semantics and errors:
  `read`, `read_to_string`, `read_text_guarded` (the T571 guards and T661
  encoding detection), `stream_text_guarded` (the same, handing plain UTF-8
  to a sink block by block so opening a large file needs no file-sized copy,
  T749; the default reads the whole file and returns `TextStream::Whole`,
  and only `LocalAuthority` streams), `read_to_string_guarded`, `write` (plain, truncating),
  `write_atomic`, `write_text_in_file_encoding`, `metadata`,
  `symlink_metadata`, `exists`/`is_dir`/`is_file`, `canonicalize`,
  `read_dir`, `create_dir_all`, `rename`, `remove_file`, `remove_path`,
  `move_path`, `copy_recursive`, `trash`, `is_unwritable`, `walk_files`,
  `which`, and `command`; and `write_atomic_if_unchanged` (T712), an atomic
  write that refuses with a `WriteConflict` (`WriteConflict::is`) when the
  file's length or modification time (whole seconds) no longer matches the
  `FileStat` the caller saw, or it appeared or vanished (`same_version`).
  The default checks with `metadata` first; the SSH authority checks and
  writes in one remote script.
- `metadata_batch(paths)` (T717) stats many paths, in order. The default
  calls `metadata` for each; `SshAuthority` answers them in one round trip
  (the agent's batch, or one shell script per 200 paths) and
  `AgentAuthority` in one batch. The App's change poll uses it.
- `is_local()` (T715) says whether the authority's paths are this
  machine's and a read is a cheap local call: `true` for `LocalAuthority`,
  `false` by default (so a new authority counts as remote), for
  `FakeAuthority` and for `SshAuthority`. When it is `false`, the App's
  project-wide searches run `rg` (else `grep`) through `command` instead of
  reading every file ([`vix-workspace-search`'s
  spec](../../vix-workspace-search/spec/index.md#remote-search-t715)).
- `shell_is_posix()` (T716) says whether `command` runs on a POSIX system,
  so a shell command line for it is `sh -c` rather than `cmd /C`: this
  machine's platform by default, always `true` for `SshAuthority` (so a
  Windows client's tasks, run configurations and **Run Command…** run `sh`
  on its remote).
- `terminal(shell, cwd)` (T716) returns the `ProcessSpec` the integrated
  terminal puts on a **local** pseudo-terminal: for `LocalAuthority`,
  `shell` in `cwd` (exactly the old behavior); for `FakeAuthority`, `shell`
  in `cwd`'s backing directory (the backing root when `cwd` is `None`); for
  `SshAuthority`, `ssh -t` over the shared connection into the remote login
  shell in `cwd` ([`spec/ssh`](ssh/index.md#the-terminal-t716)). `cwd` is
  an authority path, `None` meaning the home directory on that side. The
  default serves a local authority and refuses (`Unsupported`) for any
  other, so a new remote authority must say how its terminal works.
- `can_run(auth, program)` (T716): whether `program` can start under
  `auth` — `which` for a bare name, `is_file` for one with a `/`. The
  language-server and debug-adapter spawns call it under a non-local
  authority, so a server missing on the remote side fails to start as it
  would locally (an `ssh` that starts and then reports "not found" would
  look like a crash and be respawned).
- `current()` returns the installed authority (the local one while the slot
  is empty); `install(authority)` replaces it and returns the previous one.
  The slot is process-wide, like Fresh's single authority: one Vix process
  works against one machine at a time.
- `FileStat` (kind, length, modified/created times, read-only flag) and
  `DirEntry` (path, own kind *not* following a link, `is_dir` *following*
  one) are owned values, so a remote authority can build them from a `stat`
  reply. `FileKind` is `File`/`Dir`/`Symlink`/`Other`.
- `WalkOptions` (depth, hidden, ignore files, pruned directory names, limit);
  `WalkOptions::file_index(limit)` is the fuzzy file finder's walk.
- `ProcessSpec` describes a process as data (program, arguments, working
  directory, environment sets and removals; `raw_arg` for Windows `cmd /C`
  scripts). `Authority::command(&spec)` turns it into a `std::process::Command`
  the caller then configures (stdio, process group, creation flags) and
  spawns, exactly as before. A remote authority returns a `Command` that runs
  `ssh` (or `docker exec`, …) instead.
- `LocalAuthority` delegates to `vix-fileops` (`write_atomic`,
  `read_text_guarded`, `remove_path`, `copy_recursive`, `move_path`,
  `trash_path`, `is_unwritable`, `which_on_path`) and to `std::fs`, and walks
  with the `ignore` crate. So every existing safety property holds unchanged:
  atomic, symlink-preserving, mode-preserving saves (hard links and ownership
  are not carried over, as before), the size/FIFO/binary read guards, copies
  and deletes that never follow a link, and `PATH`-only program lookup.
- `FakeAuthority` (T711) maps a virtual absolute namespace (`/a/b`) onto a
  backing directory, refuses any path or symlink that escapes it
  (`PermissionDenied`), moves "trashed" paths into a sibling trash directory
  rather than the user's real trash, runs processes locally with the working
  directory mapped into the backing directory, and records every call
  (`calls()` returns `Call { op, path }` values, `clear_calls()`). The trash
  is the sibling `<backing>.trash`; `backing()`, `trash_dir()`,
  `to_backing()` and `to_virtual()` expose the mapping to tests.
- `ssh` (T712): `SshTarget`/`TargetError` parse `ssh://user@host:port/path`
  and `/ssh:user@host#port:/path`; `ssh::quote` is the shell quoting; both
  are always built. `SshAuthority` and `SshOptions` need the `ssh` feature
  (which implies `process`). See [`spec/ssh`](ssh/index.md).
- `ssh::ShellAuthority<T: Transport>` (T718): the shell-script authority,
  generic over how a script reaches `/bin/sh` on the other side.
  `SshAuthority` is `ShellAuthority<SshTransport>`; `ContainerAuthority` is
  `ShellAuthority<ExecTransport>`. `ssh::quote::sh_argv` is the encoded
  script as an argument vector for a transport that runs no shell first.
- `container` (T718): `ContainerTarget` and `ExecKind` (always built) parse
  `docker://`, `podman://`, `k8s://` and `wsl://` names; `ContainerAuthority`,
  `ExecTransport` and `ExecOptions` need the `ssh` feature. See
  [`spec/container`](container/index.md).
- `remote` (T718): `RemoteTarget`, any remote name (`Ssh` or `Container`),
  with `parse`, `to_url`, `label` and `path`; with the `ssh` feature,
  `connect(&ConnectOptions, interactive)` returns a `Connection` (the
  authority and its agent status). The App uses only this.
- `agent` (T717, always built): the remote agent's frame codec
  (`agent::wire`), the agent itself (`agent::serve`, `serve_stdio`, what
  `vix --remote-agent` runs) and `AgentAuthority`, an authority over a
  connected agent. `ssh::AgentMode`, `ssh::AgentStatus` and
  `SshAuthority::{start_agent, agent_status, upload_agent, watch}` need the
  `ssh` feature. See [`spec/agent`](agent/index.md).
- `conformance` (T711) is the shared behavioral suite: `conformance::run(&auth,
  scratch)` exercises every trait method under `scratch` (an authority path
  that must not exist yet) and returns the failures. It runs against
  `LocalAuthority` and `FakeAuthority` in the crate's integration test, and
  against `SshAuthority` over a fake `ssh` in `crates/vix-authority/tests/ssh.rs` (and over a real
  host when `VIX_TEST_SSH_HOST` is set), and against `AgentAuthority` and
  the agent-backed `SshAuthority` (T717), and against `ContainerAuthority`
  for all four kinds over a fake engine in `crates/vix-authority/tests/container.rs` (and a
  real Docker/Podman container when `VIX_TEST_DOCKER` is set, T718). It
  includes the conflict check and `metadata_batch`.

## What goes through the authority

The workspace: what the user opens, edits, browses, searches and builds.
The rule: a path that comes from the workspace root, the file index, an open
buffer, or a file the user named relative to them goes through
`vix_authority::current()`; so does every process that works on the
workspace.

| Area | Call sites |
| ---- | ---------- |
| Editor | open (canonicalize, `is_unwritable`, guarded read), reload from disk, reopen with encoding, save (`write_atomic`) — `vix-editor`; App save-all/replace-in-files writes and the apply-file-plan canonicalization, the external-change mtime poll, large-file open checks, Save As guards (dangling link, overwrite), reopen-closed-tab, recent-file canonicalization, session tab restore and the session key, jump-to-location checks (bottom dock, test results, debug stops, code-lens run directory) |
| Explorer | the tree listing (`vix-left-dock`), copy/cut/paste/delete/trash, rename, new file/folder |
| File browser | **File → Open…** listing (`vix-file-browser-panel`) |
| Search | the fuzzy file finder index (`walk_files`) and workspace-symbol reads; under a non-local authority, workspace search, search to dock, the TODO finder/Todo Tree and go-to-definition run `rg`/`grep` through `command` (T715), and otherwise read each file |
| Workspaces | the `.vix-workspace` file load/save and its folder containment check (`vix-workspace`), switching projects, adding a folder |
| Tasks | project-type detection, discovered-task manifests, `tasks.toml` and `.vix/run.toml` (`vix-tasks`), `.vix/project.toml`, `.editorconfig` |
| Org | `id:` link following, archive to `<file>_archive`, capture into project files, the agenda over the file index (and `org_agenda_files`), agenda TODO cycling; every Org-roam node, daily note, rename and backlink read (all under the workspace root) |
| IM | a message's code reference (`im_inside_root`: exists, is a file, canonical path inside the root), the share-path and thread-path root-relative fallbacks, and the anchored review threads under `<root>/.vix/im/threads/*.json` (list, read, atomic write, id-collision probe) — they are committed workspace files |
| HTTP client | `.vix/http-env.json` and the active-environment file (`vix-http-client` `env`), `{{$dotenv}}` and `< file` request bodies (`rest`). The request itself is sent from this machine. |
| Other | Cowork's path confinement, coverage-report path matching (`vix-coverage`), LSP diagnostic path keying (`vix-lsp`), jj/git repo probes for **init**, the OC build/compose file lookup, Prettier's project-local `node_modules/.bin` lookup |
| Processes | the task/workspace-search/shell-filter shell (`spawn_shell`, `run_shell_filter`; `sh -c` or `cmd /C` by `shell_is_posix`), direct argv runs (`spawn_argv`: git/jj/oc commands, program resolved with `Authority::which`), every `vix-git` runner (`git_command`), `vix-git-graph`, LSP servers and DAP adapters (looked up with `can_run` and started in the workspace when not local, T716), the Prettier formatter (and its working-directory check), the integrated terminal's process (`terminal`, T716) |

Library crates that only receive text or paths from the App (parsers,
panels, `vix-org`, `vix-im`'s resolvers, …) don't touch the authority; the
App reads through it and hands them the result.

## What stays local, and why

These call sites keep using `std::fs`/`std::process` on purpose. They are
about the person at the keyboard, not the workspace, so they must stay on the
machine running the UI even when the workspace is remote.

- **Configuration and per-user data**: settings (`vix-settings`),
  keybindings (`vix-keybindings` user bindings), macros (`vix-macros`),
  snippets (`vix-snippets`), themes and the theme editor's **Save As**
  (`vix-theme-model`, `picker_panels`), the personal dictionary and Hunspell
  dictionaries (`vix-spellcheck`, App `load_user_words`/`persist_user_word`),
  the settings bundle export/import (`vix-settings-bundle`), script discovery
  under the config directory (`vix-script`), AI/FHIR/HL7 profile and secret
  files.
- **Personal data in configured directories**: the contacts browser and
  vCard view (`contacts_dir`, which falls back to the workspace root — a
  remote workspace will want the per-path routing below).
- **State Vix keeps about the session**: session save/restore storage
  (`vix-session`), the undo store and Local History (`vix-undo-store`), IM
  logs, drafts and attachments (`tui/src/app/im.rs`; these are the user's
  chat history, not workspace files), the DB workbench's
  query history and saved queries.
- **Temp files and credentials**: `write_private_temp` and the files AI and
  git hand to a local CLI (`ai.rs` removes its prompt temp file), the
  tutorial's scratch copies under the temp directory (`tutor.rs`),
  `password_command`/keyring helpers (`vix-ai-core`, `vix-db`), Azure AD
  token commands (`vix-http-client` `aad`), the DB SSH tunnel (it *is* an
  ssh client), `gpg-agent.conf` in the user's GnuPG home (`vix-git`'s
  signing diagnosis), `vix-git`'s own `PATH` lookup for `git` (cached once).
- **Local clients**: the DB workbench (`vix-db`) connects from this machine,
  so its CSV import/export paths are this machine's; the MCP server
  (`vix-mcp-server`, `vix --mcp`) is a separate process an AI agent starts
  next to the files it serves and opens SQLite with an in-process library, so
  on a remote host it runs there (T717) rather than through an authority.
- **Diagnostics and integrations with their own transport**: `vix --doctor`
  (checks the local install), the OCI engine CLI's own runner and engine
  lookup (`vix-oci`, `oc.rs`'s `which_on_path`; the OC panel manages the
  engine from this machine, and a container *workspace* goes through
  `ContainerAuthority`, T718), the `devcontainer` CLI (it builds on this
  machine's engine), the
  integrated terminal's PTY itself (always local; what runs on it comes from
  `Authority::terminal`, T716), Hunspell as an external checker.
- **Implementation and tooling**: `vix-fileops` (the local implementation's
  engine), `vix-authority` itself, and the action catalog's source scan
  (`vix-action-catalog` `dispatch_scan`, used by tests and the docs
  generator).
- **Tests and examples**: every `#[cfg(test)]` module, `tests/`, `benches/`,
  `examples/`, which set up their own temp trees.

Measured on 2026-10-29 (`\bfs::name(` call expressions outside comments and
`#[cfg(test)] mod tests`, in `tui/src` and `crates/*/src`): **146** call sites
in **37** files before T710 (the spec's earlier 428/38 counted test code), plus
**16** `Command::new` in **13** files. After T710, **60** call sites in **21**
files remain outside `vix-fileops`, `vix-authority` and test helpers, every
one in the list above; the only non-test `Command::new` left are the
credential helpers, the DB tunnel, `--doctor`, Azure AD, OCI and Hunspell.
Workspace `Path::{exists, is_file, is_dir, canonicalize, symlink_metadata}`
probes in the App were routed too (they are filesystem calls that the
`fs::` count misses).

### Not done (T710 follow-ups)

- ~~`tui/src/app/im.rs` resolves a message's code reference locally~~ —
  routed 2026-10-30 (T712 step 1), together with the anchored-thread files.
- ~~The AI `cli` provider's local prompt temp file~~ — T712 pins the
  provider to `LocalAuthority` (it is the user's own, locally signed-in
  tool).
- ~~`vix-git`'s cached `git` path and `no_tty_prompts`~~ — T712: the cached
  path is used only locally; `no_tty_prompts` puts its environment in the
  `ProcessSpec` (so it reaches a remote side) and only the process group on
  the local `Command`.

## Testing

The crate's `conformance` integration test runs `conformance::run` against
`LocalAuthority` (a fresh temp directory) and `FakeAuthority`. The fake's own
tests check path mapping, escape refusal (relative, `..` above `/`, a symlink
out), call recording and the trash directory; `the_slot_*` checks the default.
`tui/tests/authority_fake.rs` (its own test binary, because the slot is
process-wide) installs a `FakeAuthority` and drives the App's explorer
listing, open and save against `/proj`, a path that exists only inside it,
and starts an IM review thread whose file lands in the fake's
`/proj/.vix/im/threads`.

## Next

T712 shipped the SSH authority ([`spec/ssh`](ssh/index.md), with its own
*Not done* list). Still open from this slice:

- Per-path routing. The slot is one authority; opening a local file (say,
  the settings file) while connected needs a routing authority that sends
  `ssh://` paths to the remote and the rest to `LocalAuthority`. It fits the
  trait as-is.
- ~~The external-change poll stats every open file each second; over SSH it
  wants batching (the remote agent, T717).~~ Done (T717): one
  `metadata_batch` per poll, one round trip with or without the agent.
