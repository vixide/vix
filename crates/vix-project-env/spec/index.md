# Project environment

Resolves a workspace's development environment once, and applies it to the
language servers, tasks and terminal Vix spawns (T667). Action:
`project.reload_env` (Project → Reload Environment). A status-bar segment
shows the active source.

A project's `.venv`, direnv `.envrc` or mise config otherwise goes ignored, so
Python and Node tooling finds the wrong interpreter and packages.

## Sources

Tried in this order; the first that yields variables wins.

| Source | Detected by | How it resolves | Runs project code? |
| ------ | ----------- | --------------- | ------------------ |
| direnv | `.envrc` | `direnv export json` in the root | yes |
| mise   | `.mise.toml`, `mise.toml`, `.tool-versions` | `mise env --json` in the root | yes |
| venv   | `.venv/pyvenv.cfg` or `venv/pyvenv.cfg` | `VIRTUAL_ENV` set, `<venv>/bin` (`Scripts` on Windows) prefixed onto `PATH`, `PYTHONHOME` removed | no, reads files only |

JSON from `direnv`/`mise` must be an object of string values (`null` means
"unset"); anything else, including keys with `=` or NUL bytes, is rejected
whole. Each tool is killed after 5 seconds, and its stdin is closed.

## Trust

**Nothing runs or loads an environment from an untrusted workspace.**
`direnv` and `mise` execute project-controlled code, so `resolve(root,
trusted)` calls the runner only when `trusted` is true. An untrusted
workspace with such a source reports it in `Resolved::blocked` (the app shows
a status message) and falls back to the `.venv` source, which only reads
files. The app passes the workspace-trust decision (T132/T627, stored in
`vix-session` as `scripts_trusted`, and also requiring a trusted remote
workspace: `App::workspace_trusted`). Accepting the trust prompt re-resolves
the environment. The prompt is raised only for `.vix/scripts/` or project
settings, so a workspace with just an `.envrc` stays untrusted until trusted
that way. Commands that run on a remote authority are not given the local
environment (local paths mean nothing there).

## Applying it

`set_active` publishes the resolved environment in a process-wide slot.
`apply_to(set)` hands each variable (set or remove) to the caller's own command builder, and is called where
`vix-lsp` starts a server and where the app starts a task/shell command;
`vix-terminal` applies `active_vars()` to its PTY command. Already-running
processes are unaffected; `project.reload_env` re-resolves for later spawns
(restart a language server to pick it up).

## Code

`crates/vix-project-env/src/lib.rs`: `resolve`, `resolve_with` (injectable
`Runner`, used by the tests to prove nothing runs when untrusted),
`set_active`/`apply_to`/`active_vars`; `run.rs` (the only file that starts a process, behind the `process` feature that the app's `tasks` capability enables, and listed in `tui/tests/spawn_sites.rs`). App side: `tui/src/app/project_env.rs`.
