# Remote editing

**Experimental.** Vix can work on a folder on another machine over SSH, with
your own settings, themes and keybindings, and the remote machine's files and
tools. Nothing needs to be installed on the remote side; if Vix is there, it
is used to go faster (see [The remote agent](#the-remote-agent)).

```sh
vix ssh://me@build-box/srv/app        # a folder: it becomes the workspace
vix ssh://build-box:2222/~/src/app    # relative to the remote home
vix /ssh:me@build-box:/srv/app/main.rs  # TRAMP's form; a file opens in its folder
```

Or, from inside Vix, **File → Open Remote…** (also in the command palette):
pick a recent remote or choose *New connection…* and type a name in either
form. Vix connects in the background (the status bar shows `⟳ host`) and then
opens the folder as the workspace. Save or close modified buffers first,
since switching workspaces closes them.

### Opening from the dialog never prompts

The editor owns the terminal, so the dialog runs `ssh` in `BatchMode`: it
cannot ask for a password, a passphrase or whether to trust a new host key.
If the host needs one of those, the connection fails with a message that
says what to do:

- run `ssh build-box` once in a terminal to accept its host key (it is saved
  in `~/.ssh/known_hosts`), or `ssh-add` to load your key into the agent; or
- start Vix with `vix ssh://…` instead, which connects before the editor
  takes over the screen and so can prompt normally.

Keys in an agent, `ssh_config` and an existing shared connection all work
from the dialog.

## While you work

- The status bar shows the host: `⇄ me@build-box`, or `⇄ me@build-box
  (restricted)` until you trust the workspace (below).
- **Saves check for changes on the remote side.** Each buffer remembers the
  file's size and modification time from when it was read or last saved.
  If the file changed since (someone else saved it, or a pull or checkout
  rewrote it), the save doesn't overwrite it; Vix asks instead: `o`
  overwrites, `c` shows the file on disk against your buffer so you can merge
  by hand and save again, `n` (or Esc) leaves both alone. Local files are
  saved as before, without this check.
- **Connection lost.** If the connection drops, the status bar shows `⚠
  host: connection lost` and a warning explains it. Your buffers stay open
  with their edits; saving is refused (not attempted and failed) and the
  check for files changed on disk pauses. **File → Reconnect Remote**
  connects again in the background; then save as usual. A file that changed
  remotely while you were away is caught by the check above.

## Trust

A remote workspace is untrusted until you say otherwise. The first time you
open one, Vix asks whether to trust it (press `y` to trust, `n` for
restricted mode; keys typed before the question appears are ignored). The
answer is remembered on your machine for that host and folder; **File →
Trust Remote Workspace…** asks again.

In restricted mode you can browse, edit, search and save, but nothing the
workspace itself defines runs: no tasks or project commands (**Project**
menu), run configurations, test lenses, debugger, language servers, project
scripts (`.vix/scripts`) or project-local Prettier (`node_modules/.bin`).
Vix has no automatic environment activation to switch off.

## How it works

Vix runs your system `ssh` for every file operation and process, so your
`~/.ssh/config` applies as usual: host aliases, `User`, `Port`, `ProxyJump`,
keys, the SSH agent and host-key checking. Vix connects once before the
editor starts, so `ssh` can ask for a password, passphrase or host-key
confirmation on the terminal; after that, every command shares that one
connection (OpenSSH's `ControlMaster`), and none of them can prompt.

On the remote side it needs only a POSIX `sh` and the usual commands: `cat`,
`head`, `stat`, `find`, `mv`, `cp`, `mkdir`, `rm`. GNU, BusyBox, BSD and macOS
versions all work, and so does a `fish` or `csh` login shell.

- **Saves are atomic**: Vix writes a temporary file next to the original and
  renames it into place, keeping the file's permissions and writing through a
  symbolic link.
- **Explorer, File → Open, the file finder, search, git and tasks** work on
  the remote machine, in the workspace. Find in Workspace, Search in
  Workspace (to dock), the TODO finder and Todo Tree, and go to definition
  run one `rg` on the remote machine (or `grep` for a plain-text search when
  `rg` isn't installed there), so a search is one round trip, not one per
  file. The file finder honors `.gitignore`
  when the folder is a git work tree. Language servers and debug adapters
  are started there too, but are not yet tested remotely (T716).
- **Settings, themes, keybindings, snippets, session and undo history** stay
  on your machine. So does the AI command-line provider, which runs where
  you are signed in to it.

## The remote agent

When the remote machine has `vix` of **exactly the same version** as yours,
Vix starts it there as a helper (`vix --remote-agent`) over the same SSH
connection, and file operations go through it instead of one shell command
each: opening, saving, listing, the file finder's walk, and the
once-a-second check for files changed on disk (all open files in one
request). Anything it can't do (a file over 64 MiB, running a program)
still uses the shell commands, and if it stops, Vix carries on with them.
The Messages drawer says when the agent is in use, or why it isn't.

Vix looks for it in `~/.cache/vix/agent/<version>/vix` (or under
`$XDG_CACHE_HOME`) and on the remote `PATH`. The `remote_agent` setting
decides the rest:

```toml
remote_agent = "installed"   # default: use one that is already there
remote_agent = "upload"      # ...and copy this vix there if there is none
remote_agent = "off"         # shell commands only
```

With `"upload"`, and only when the remote machine runs the same operating
system on the same processor architecture, Vix copies its own binary into
that cache folder (private to you, mode 0700). The remote side computes the
copy's SHA-256 (`sha256sum`, `shasum` or `openssl`) and compares it with
yours before the file is made executable, then runs it once to check its
version; on any mismatch the copy is deleted and nothing is installed. The
upload is off by default because it writes a large executable into your
remote home over what may be a slow link, which some hosts' policies don't
allow; turn it on for machines where that's fine.

## Limits

This is the first phase ([`spec/remote-editing`](../../spec/remote-editing/index.md)).

- Opening from the dialog can't answer a password, passphrase or host-key
  question (see above). There is no **Disconnect**: open another folder or
  quit.
- A dropped connection is noticed when an open, a save or the change check
  fails; until then an operation may wait for `ssh`'s 15-second connect
  timeout. Other failures (the explorer listing, search) show as ordinary
  errors.
- Trust is per remote workspace only; local workspaces keep the existing
  script-trust prompt (a general workspace-trust setting is T627).
- Without the remote agent each operation is a round trip (the
  once-a-second check for files changed on disk is one round trip for all
  open files either way). A regular-expression search on a machine
  without `rg` reads every file, one round trip each; install `ripgrep`
  there for fast search. Workspace symbol search (`@@` in the
  palette), structural replace and the Org agenda still read every file.
- Moving a file to the trash needs `gio` or `trash-put` on the remote
  machine; without one, use delete (or set `explorer_delete = "hard"`).
- The integrated terminal opens a local shell.

## Safety

- Vix never puts a password or other secret on the command line (a test
  checks that none from your settings reaches `ssh`'s arguments), and never
  turns host-key checking off: it passes no `StrictHostKeyChecking` or
  `UserKnownHostsFile` option, so your `ssh_config` and `known_hosts` decide,
  and it never accepts a new or changed host key for you. Authentication is
  `ssh`'s own (keys, the agent, `SSH_ASKPASS`).
- Every path and argument is quoted before it reaches the remote shell.
- The connection's control socket lives in a private directory
  (`$XDG_RUNTIME_DIR/vix-ssh`, or `/tmp/vix-ssh-<uid>`), which Vix refuses to
  use if anyone else can reach it.

## Testing against a real host

Vix's own tests drive the SSH support through a stand-in for `ssh`. To run
the shared file-system conformance suite against a real OpenSSH server on
your machine:

```sh
scripts/test-ssh-local
```

It needs `sshd` (on Debian or Ubuntu, `sudo apt-get install openssh-server`;
macOS ships it). The script starts a temporary `sshd` as you, not as root, on
`127.0.0.1:22022` (set `VIX_TEST_SSH_PORT` to change it), with freshly
generated keys and its own client configuration, so host-key checking stays
on and your `~/.ssh`, your SSH agent and any system `sshd` are left alone.
Everything lives in one temporary directory that is removed afterwards
(`VIX_TEST_SSH_KEEP=1` keeps it, with the server's log). The same script runs
in GitHub Actions whenever the SSH code changes.

To test against another machine instead, set `VIX_TEST_SSH_HOST` to a host
your `ssh` can reach without a prompt (`user@host` or `ssh://host:port`) and
run `cargo test -p vix-authority --features ssh --test ssh a_real_host_conforms`.
The suite creates and removes a scratch folder under `/tmp` there.
