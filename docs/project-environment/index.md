# Project Environment

Vix reads a project's own development environment once and gives it to the
language servers, tasks and terminals it starts, so Python and Node tooling
finds the project's interpreter and packages. A status-bar segment shows the
active source.

## Sources

Tried in this order; the first that yields variables wins.

| Source | Detected by | How it resolves | Runs project code? |
| ------ | ----------- | --------------- | ------------------ |
| direnv | `.envrc` | `direnv export json` in the workspace root | yes |
| mise | `.mise.toml`, `mise.toml`, `.tool-versions` | `mise env --json` | yes |
| venv | `.venv/pyvenv.cfg` or `venv/pyvenv.cfg` | sets `VIRTUAL_ENV`, puts the venv's `bin` (`Scripts` on Windows) first on `PATH`, removes `PYTHONHOME` | no, reads files only |

Each tool is stopped after 5 seconds. Output that is not an object of string
values is rejected whole.

## Trust

**Nothing runs or loads an environment from an untrusted workspace.** direnv
and mise execute project-controlled code, so they run only in a workspace you
have trusted. In an untrusted one Vix tells you the source was blocked and
falls back to the `.venv` source, which only reads files. Accepting the trust
prompt resolves the environment again. Commands that run on a remote host are
not given the local environment.

## Reloading

Already-running processes keep the environment they started with. After
changing `.envrc` or activating a different venv, run
**Project → Reload Environment** (`project.reload_env`); later spawns use the
new one. Restart a language server to pick it up.

See the specification at `crates/vix-project-env/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
