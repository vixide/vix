# Remote editing

**Status:** in progress (proposed 2026-10-28; T710–T712 done). Tasks T710–T719 in
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

## Safety

- Host keys are verified by `ssh` (never `StrictHostKeyChecking=no`).
- No passwords on argv or in logs; rely on keys/agent, or `SSH_ASKPASS`.
- A remote workspace is untrusted until the user trusts it (T627): no
  automatic environment activation or task execution.
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
