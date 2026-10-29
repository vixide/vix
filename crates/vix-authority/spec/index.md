# Authority

The authority slot: one trait that owns Vix's workspace file access and process spawning, with a local implementation and a temp-dir-backed fake.

It lets a remote machine stand in for the local one. The cross-cutting design is
[`spec/remote-editing`](../../../spec/remote-editing/index.md); this crate is
its first piece (T710, T711).

**Status:** Shipped (T710, T711). `LocalAuthority` is the only production
implementation and is installed by default; nothing installs another one yet.
No user-visible change.

## API

- `Authority` (trait, `Send + Sync + Debug`). One method per operation the
  App used to call directly, with the same semantics and errors:
  `read`, `read_to_string`, `read_text_guarded` (the T571 guards and T661
  encoding detection), `read_to_string_guarded`, `write` (plain, truncating),
  `write_atomic`, `write_text_in_file_encoding`, `metadata`,
  `symlink_metadata`, `exists`/`is_dir`/`is_file`, `canonicalize`,
  `read_dir`, `create_dir_all`, `rename`, `remove_file`, `remove_path`,
  `move_path`, `copy_recursive`, `trash`, `is_unwritable`, `walk_files`,
  `which`, and `command`.
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
  (`calls()`, `clear_calls()`).
- `conformance` (T711) is the shared behavioral suite: `conformance::run(&auth,
  scratch)` exercises every trait method under `scratch` (an authority path
  that must not exist yet) and returns the failures. It runs against
  `LocalAuthority` and `FakeAuthority` in the crate's integration test; T712's SSH
  authority runs the same function.

## What goes through the authority

The workspace: what the user opens, edits, browses, searches and builds.

| Area | Call sites |
| ---- | ---------- |
| Editor | open (canonicalize, `is_unwritable`, guarded read), reload from disk, reopen with encoding, save (`write_atomic`) — `vix-editor`; App save-all/replace-in-files writes, the external-change mtime poll, large-file open checks |
| Explorer | the tree listing (`vix-left-dock`), copy/cut/paste/delete/trash, rename, new file/folder |
| File browser | **File → Open…** listing (`vix-file-browser-panel`) |
| Search | the fuzzy file finder index (`walk_files`) and symbol search reads |
| Tasks | project-type detection, discovered-task manifests, `.vix/project.toml`, `.editorconfig` |
| Processes | the task/workspace-search/shell-filter shell (`spawn_shell`, `run_shell_filter`), direct argv runs (`spawn_argv`: git/jj/oc commands), every `vix-git` runner, `vix-git-graph`, LSP servers, DAP adapters, the Prettier formatter |

## What stays local, and why

These call sites keep using `std::fs`/`std::process` on purpose. They are
about the person at the keyboard, not the workspace, so they must stay on the
machine running the UI even when the workspace is remote.

- **Configuration and per-user data**: settings (`vix-settings`),
  keybindings, macros, snippets, themes, the user dictionary and Hunspell
  dictionaries (`vix-spellcheck`), the settings bundle export/import, script
  discovery under the config directory (`vix-script`), `.http` environment
  files, the action catalog/tutor data, AI/FHIR/HL7 profile and secret files.
- **State Vix keeps about the session**: session save/restore
  (`vix-session`), the undo store and Local History (`vix-undo-store`), IM
  logs and drafts, the DB workbench's query history and saved queries, Org
  agenda/capture/Roam directories configured in settings (the user's own
  notes, wherever the workspace is).
- **Temp files and credentials**: `write_private_temp` and the files AI and
  git hand to a local CLI, `password_command`/keyring helpers
  (`vix-ai-core`, `vix-db`), Azure AD token commands (`vix-http-client`),
  the DB SSH tunnel (it *is* an ssh client).
- **Diagnostics and integrations with their own transport**: `vix --doctor`
  (checks the local install), the OCI engine CLI (T718 owns containers), the
  integrated terminal's PTY (T716 owns a remote terminal), Hunspell as an
  external checker.
- **Tests and examples**: every `#[cfg(test)]` module, `tests/`, `benches/`,
  `examples/`, which set up their own temp trees.

Measured on 2026-10-29 (`\bfs::name(` call expressions outside comments and
`#[cfg(test)] mod tests`, in `tui/src` and `crates/*/src`): **146** call sites
in **37** files before T710 (the spec's earlier 428/38 counted test code), plus
**16** `Command::new` in **13** files. After T710 the routed sites call the
authority instead; what remains is the local-only list above. `vix-fileops`
itself (26 sites) is the local implementation's engine, not a call site.

## Testing

The crate's `conformance` integration test runs `conformance::run` against `LocalAuthority` (a
fresh temp directory) and `FakeAuthority`. The fake's own tests check path
confinement, call recording and the trash directory; `the_slot_*` checks the
default and `install`.

## Next (T712)

An `SshAuthority` implementing the trait with `ssh` + ControlMaster: file ops
via `cat`/`stat`/`find`/`mv`, `command` returning an `ssh host -- …` command
with the spec's arguments shell-quoted, and the conformance suite run against
a fake `ssh` on `PATH`. Open questions it inherits from this slice:

- Per-path routing. The slot is one authority; opening a local file (say,
  the settings file) while connected needs a routing authority that sends
  `ssh://` paths to the remote and the rest to `LocalAuthority`. It fits the
  trait as-is.
- `vix-git` caches the `git` path from the local `PATH` (`git_program`); a
  remote authority must resolve it remotely (`Authority::which`).
- `no_tty_prompts` sets environment and a process group on the *local*
  `Command`; over SSH those must reach the remote side.
- The external-change poll stats every open file each second; over SSH it
  wants batching (the remote agent, T717).
