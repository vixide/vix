# Remote editing

**Status:** in progress (proposed 2026-10-28; T710–T714 done). Tasks T710–T719 in
[`tasks.md`](../../tasks.md). Promotes the "Remote/SSH editing" Ideas-backlog
entry, which already names Fresh's *authority* design as the model.

*Sources: the author's knowledge of Emacs TRAMP, VS Code Remote-SSH, JetBrains
Gateway, Zed remote and Fresh; no live web research for this draft.*

## Goal

Open a folder on a trusted remote machine and work on it as if it were local:
browse, edit, search, run the language server, use git, and open a terminal.
It matters when the code, data, toolchain or infrastructure can't be
reproduced locally.

## Prior art

| Tool | How it works | Trade-off |
| ---- | ------------ | --------- |
| Emacs TRAMP | Virtual file names (`/ssh:user@host:/path`); every file operation runs a remote shell command (`ssh`, `scp`, `rsync`, plus `sudo`, `docker`, `podman`, `kubernetes` methods); aggressive caching; ControlMaster | Works anywhere with a shell and nothing to install, but is slow for many small operations and fragile on odd prompts. |
| VS Code Remote-SSH | Installs a server on the remote; local UI, remote extension host; port forwarding | Fast and complete, but needs a compatible remote and a download. |
| JetBrains Gateway | Thin client plus a full remote IDE backend | Heavy; needs remote resources. |
| Zed remote | Uploads a small server binary over SSH and talks to it over stdio | Close to what a terminal editor needs. |
| Fresh | One *authority* slot owning file access and process spawn; SSH, devcontainers and Kubernetes plug in | The abstraction that makes everything else fall out. |
| `sshfs` / mounts | Kernel mount; the editor is unaware | Zero editor work, but latency hits search and LSP, and it's not portable to Windows. |

**Simplest honest answer for a terminal editor:** if the remote has Vix, `ssh
host vix` already works. This feature is for when you want your *local*
config, themes, keybindings and clipboard, with the remote's files and tools.

## Recommended design (phased)

An `Authority` trait owns every side effect: read, write, stat, list, rename,
remove, watch, spawn, and the terminal. `LocalAuthority` is today's behavior.

1. **Phase 1, shell-based SSH authority (TRAMP-style).** No remote install.
   Uses the system `ssh` with ControlMaster multiplexing and `ssh_config`
   (so ProxyJump, keys and agents just work). File ops via `cat`, `stat`,
   `find`, atomic write via temp file plus `mv`. Good enough to open and edit.
2. **Phase 2, remote agent.** `vix --remote-agent` speaks a framed protocol
   over the SSH stdio pipe (the Zed approach): batched file ops, remote
   `rg`/walk search, git, file watching, and spawning LSP servers and tasks
   on the remote with the LSP stream proxied. The agent is the same binary,
   found on the remote's `PATH` or uploaded on request (checksum-verified).
3. **Phase 3, other authorities.** Dev containers, `docker exec`, `podman`
   (links to [`../open-container/index.md`](../open-container/index.md)),
   Kubernetes, WSL.

**Search and git (T715, done 2026-10-30).** Under a non-local authority
(`Authority::is_local` is `false`) the App's project-wide scans run one
`rg --json` (else, for a literal query, `grep -rnF`) on the remote through
`Authority::command` instead of a read per file, re-check every line with
the local regex and keep only indexed files, so results match the local
search; a regex without `rg` falls back to per-file reads. Every git and jj
invocation already ran through the authority (T710); `vix-git` now derives
the repository top level from `--show-cdup` rather than git's own absolute
`--show-toplevel`, so it stays an authority path. Details:
[`vix-workspace-search`](../../crates/vix-workspace-search/spec/index.md#remote-search-t715),
[`vix-git`](../../crates/vix-git/spec/index.md#where-git-runs-t710-t715).

Path form: `ssh://user@host[:port]/abs/path` on the command line and in the
UI; TRAMP-style `/ssh:host:/path` accepted as an alias.

## The hard part: the local-only model

A grep finds **428** `std::fs` call sites across **38** files, plus
`Command::new` in many crates. All of them must go through an authority, or be
explicitly marked local-only (settings, themes, session, undo store, which
always live on the machine running the UI). This is the bulk of the work and
should land first, on its own, with no behavior change (T710).

Done 2026-10-29 (T710, T711): the `vix-authority` crate owns the slot, and its
spec lists every routed area and every local-only exception with its reason.
(The 428/38 count above included test code; outside tests it was 146 call
sites in 37 files.)

Done 2026-10-30 (T712): phase 1's `SshAuthority`
([`crates/vix-authority/spec/ssh`](../../crates/vix-authority/spec/ssh/index.md)),
with an experimental `vix ssh://user@host/path` command line. It is tested
against a fake `ssh` script named by path, rather than one on `PATH`, since
changing the environment in-process is `unsafe` under edition 2024.

## The user surface (T713, T714)

Done 2026-10-30. The App side lives in `tui/src/app/remote_session.rs`.

- **File → Open Remote…** (`file.open_remote`, menu and palette) shows the
  recent remote names (`vix_session::Session::recent_remotes`, newest first,
  at most 10) under a *New connection…* row, which opens a prompt
  (`PromptKind::OpenRemote`) for an `ssh://…` or `/ssh:…` name. The connect
  runs on a background thread; `App::poll_remote` (every tick) installs the
  authority and re-roots the workspace (the old session is saved first,
  under its own key). It is refused while a buffer has unsaved edits or
  another connect is running.
- **Prompting, decided:** the TUI owns the terminal, so the dialog connects
  with `BatchMode=yes` (`SshAuthority::connect(false)`) and never prompts.
  Suspending the TUI for an interactive `ssh` was rejected: a passphrase or
  host-key prompt in the middle of a session, on a terminal the editor had
  to give up and take back, is fragile and easy to answer by accident. A
  failure says how to fix it: run `ssh host` once (accept the key into
  `known_hosts`) or `ssh-add`, or start with `vix ssh://…`, the one path that
  can prompt, because it connects before the UI starts.
- **Status bar:** `⇄ user@host:port`, with *(restricted)* while untrusted,
  `⟳ host` while connecting, and `⚠ host: connection lost — File → Reconnect
  Remote` after a loss.
- **Connection lost:** an operation that fails with `NotConnected` (`ssh`'s
  exit 255) during an open, a save or the external-change poll marks the
  connection lost once (a warning in Messages and the status segment). Then
  saves are refused before any `ssh` runs, the change poll stops (each try
  would wait for the connect timeout), and buffers stay as they are.
  **File → Reconnect Remote** (`file.reconnect_remote`) connects again in the
  background and installs a fresh authority; nothing is reloaded or lost.
- **Conflict-checked saves:** the App turns on `vix_editor`'s
  `Flags::CHECK_CONFLICTS` for a remote workspace. Each tab then keeps a
  `DiskStamp` (the file's stat before it was read, and after each save or
  reload), and a save goes through `Authority::write_atomic_if_unchanged`.
  A `WriteConflict` opens a confirmation (`ConfirmKind::SaveConflict`): `o`
  overwrites (`Editor::save_active_overwriting`), `c` opens a diff of the
  file on disk against the buffer, `n`/Esc keeps both. **Local files are not
  checked** (the flag stays off): a prompt where a save used to just work is
  a behavior change, and the external-change poll already warns about a
  dirty buffer whose file changed locally.
- **Session key:** a remote workspace's session, project state and trust
  are keyed by its canonical `ssh://` name, never its bare path.

**Trust (T714).** A remote workspace is untrusted until the user trusts it.
T627 (per-project settings and general workspace trust) is not done, so this
is the minimal version, for remote workspaces only:

- `App::adopt_remote` (shared by the dialog and `vix ssh://…`) reads
  `WorkspaceSession::remote_trusted` for the `ssh://` name; when unset it asks
  (`ConfirmKind::TrustRemote`: `y` trusts, `n`/Esc is restricted mode; keys
  within 250 ms of the prompt opening are ignored, as T655 does for scripts).
  The answer is persisted per host and path in the local session file.
  **File → Trust Remote Workspace…** (`file.trust_remote`) asks again.
- Until trusted: the `project.*` task and command actions (not the cache or
  run-configuration *edit* actions), `run.start` (the debugger),
  `lsp.code_lens_run` and every test lens, `lsp.restart` and `script.run` are
  refused with a status message (one check in `App::run_action`); language
  servers are off (`Lsp` is rebuilt with `enabled = false`, and rebuilt again
  when trusted); and Prettier ignores the project's `node_modules/.bin`.
  Project scripts (`.vix/scripts`) never load for a remote workspace, trusted
  or not, since their discovery reads this machine's files (later work).
  There is no automatic environment activation in Vix to gate.
- Git, search and user-typed commands (**Run Command…**, pipe through a
  command) still run: the user starts each one. Git on the remote side
  applies its own `safe.directory` ownership check.

## Safety

- Host keys are verified by `ssh` (never `StrictHostKeyChecking=no`). Vix
  passes no host-key option at all, so `ssh_config` and `known_hosts` decide;
  every connection after the first runs with `BatchMode=yes`, so an unknown
  or changed key fails rather than being accepted, and Vix never writes
  `known_hosts` (T714).
- No passwords on argv or in logs; rely on keys/agent, or `SSH_ASKPASS`.
  `tui/tests/remote_ui.rs` plants password-like values in the settings (the
  AI key command, a DB connection's password command) and checks that none
  appears in any `ssh` argv during an open, saves, a lost connection and a
  reconnect (T714).
- A remote workspace is untrusted until the user trusts it (T714, above;
  general workspace trust is T627): no task execution, language servers or
  project tools.
- Writes are atomic and detect remote change by mtime+size before saving,
  reusing the external-change guard.
- A "connection lost" state is visible, buffers stay dirty-safe, and
  reconnect doesn't lose edits.
- All user text via `t!`.

## What stays local

Config, themes, keybindings, clipboard (OSC 52 or local), UI, undo store,
session. What goes remote: files, search, git, LSP, DAP, tasks, terminal.

## Testing

A fake authority backed by a temp dir proves the trait. The SSH authority is
tested against a local `sshd` only in an opt-in CI job; unit tests use a fake
`ssh` script on `PATH` (the same technique as the DAP mock).
