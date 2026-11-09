# Project environment

Resolves a workspace's development environment once, and applies it to the
language servers, tasks and terminal Vix spawns (T667). Action:
`project.reload_env` (Project → Reload Environment). A status-bar segment
shows the active source.

A project's `.venv`, direnv `.envrc` or mise config otherwise goes ignored, so
Python and Node tooling finds the wrong interpreter and packages.

## Sources

Tried in this order; the first that yields variables wins. User-defined detectors (below) come first, so a project can override the built-in ones.

| Source | Detected by | How it resolves | Runs project code? |
| ------ | ----------- | --------------- | ------------------ |
| direnv | `.envrc` | `direnv export json` in the root | yes |
| mise   | `.mise.toml`, `mise.toml`, `.tool-versions` | `mise env --json` in the root | yes |
| venv   | `.venv/pyvenv.cfg` or `venv/pyvenv.cfg` | `VIRTUAL_ENV` set, `<venv>/bin` (`Scripts` on Windows) prefixed onto `PATH`, `PYTHONHOME` removed | no, reads files only |
| custom | a `[[detector]]` in `.vix/env.toml` or the user-level `env.toml` | its own `command`, no shell | yes |

## User-defined detectors

A project adds its own sources in `.vix/env.toml`:

```toml
[[detector]]
name = "nix"                 # shown in diagnostics, at most 64 characters
detect = ["flake.nix"]       # the detector applies when ANY of these exists in the root
command = ["nix-env-json"]   # program and arguments, run directly (no shell) in the root
timeout_secs = 30            # optional: 1 to 60 seconds, default 5

[detector.env]               # optional: extra variables for this command only
NIX_CONFIG = "experimental-features = flakes"
```

The command prints the same JSON object `direnv export json` does (string
values, `null` = unset). Detectors are tried in file order; the first whose
files exist and whose command yields a non-empty object wins and reports as
the detector's own `name` in the status bar (`ProjectEnv::label`; `custom` is only the source's generic label). At most 16 detectors; a detector with an empty
name, no `detect` entries, an empty or NUL-containing `command`, or a `detect`
entry that is anything but plain names and `.` (absolute, `..`, or on Windows a
root or drive such as `\x` or `C:x`), or a `timeout_secs` outside 1 to 60 makes the whole file invalid and it is
ignored (never partially applied). A detector runs project-controlled code, so
it is gated exactly like `direnv`/`mise`: only for a trusted workspace;
otherwise it is reported in `Resolved::blocked` as `Source::Custom` and
nothing runs.

JSON from `direnv`/`mise`/custom detectors must be an object of string values (`null` means
"unset"); anything else, including keys with `=` or NUL bytes, is rejected
whole. `direnv` and `mise` are killed after 5 seconds; a detector after its `timeout_secs` (default 5, at most 60). Stdin is closed.

### Detector environment variables

A detector's optional `env` table adds variables to **its own command's**
environment, on top of the inherited one. They are not applied to language
servers, tasks or the terminal (only the variables the command prints are).
Values are plain strings: no shell, `$VAR` or `~` expansion. Rules, all
all-or-nothing like the rest of the file (one bad entry invalidates it):

- a name is `[A-Za-z_][A-Za-z0-9_]*`, at most 128 bytes;
- names starting with `LD_` or `DYLD_` (any case; dynamic-loader injection)
  are denied;
- a value is at most 4096 bytes with no NUL; at most 32 entries per detector;
- `PATH` is allowed deliberately: it controls how the command's own program
  is found, and a trusted workspace could run any program anyway. Everything
  else not denied is the author's responsibility.

The gate is unchanged: the command (and so its `env`) is started only for a
trusted workspace. The names (never the values, which may be secrets) of the
variables are shown after **Project → Reload Environment**
(`status.project_env_active_extra`); `ProjectEnv::command_env` carries them.

## User-level detectors

The same `[[detector]]` format in `env.toml` beside the settings file
(`~/.config/vix/env.toml` on Linux) defines detectors for every workspace.
Project detectors are tried first, so a project can override a personal one.
The file is the user's own, but a detector's `detect` files are workspace
content and its command may read the workspace, so user-level detectors are
gated by workspace trust exactly like project ones (blocked as
`Source::Custom` when untrusted, and `needs_trust` raises the trust prompt).
A missing or invalid file is ignored whole.

## Trust

**Nothing runs or loads an environment from an untrusted workspace.**
`direnv` and `mise` execute project-controlled code, so `resolve(root,
trusted)` calls the runner only when `trusted` is true. An untrusted
workspace with such a source reports it in `Resolved::blocked` (the app shows
a status message) and falls back to the `.venv` source, which only reads
files. The app passes the workspace-trust decision (T132/T627, stored in
`vix-session` as `scripts_trusted`, and also requiring a trusted remote
workspace: `App::workspace_trusted`). Accepting the trust prompt re-resolves
the environment. The trust prompt is also raised for a workspace that has an `.envrc`, a
mise config or `.vix/env.toml` (`ScriptTrustPrompt::env`), so such a
workspace no longer stays untrusted silently. Commands that run on a remote authority are not given the local
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
`set_active`/`apply_to`/`active_vars`, `parse_detectors`/`load_detectors`/`load_user_detectors`/`needs_trust` (project and user detectors), `ProjectEnv::label`, `Detector::timeout_secs`/`env`, `env_name_allowed`, `ProjectEnv::command_env`; `run.rs` (the only file that starts a process, behind the `process` feature that the app's `tasks` capability enables, and listed in `tui/tests/spawn_sites.rs`). App side: `tui/src/app/project_env.rs`.
