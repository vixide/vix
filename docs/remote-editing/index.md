# Remote editing

**Experimental.** Vix can work on a folder on another machine over SSH, with
your own settings, themes and keybindings, and the remote machine's files and
tools. Nothing is installed on the remote side.

```sh
vix ssh://me@build-box/srv/app        # a folder: it becomes the workspace
vix ssh://build-box:2222/~/src/app    # relative to the remote home
vix /ssh:me@build-box:/srv/app/main.rs  # TRAMP's form; a file opens in its folder
```

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

## Limits

This is the first phase ([`spec/remote-editing`](../../spec/remote-editing/index.md)).

- There is no **Open Remote…** dialog yet, no remote indicator in the status
  bar, and no "connection lost" state: if the connection drops, operations
  fail with ordinary file errors until you restart Vix.
- Each operation is a round trip, so the once-a-second check for files
  changed on disk is slower than locally. A regular-expression search on a
  machine without `rg` reads every file, one round trip each; install
  `ripgrep` there for fast search. Workspace symbol search (`@@` in the
  palette), structural replace and the Org agenda still read every file.
- Moving a file to the trash needs `gio` or `trash-put` on the remote
  machine; without one, use delete (or set `explorer_delete = "hard"`).
- The integrated terminal opens a local shell.

## Safety

- Vix never puts a password on the command line and never turns host-key
  checking off; authentication is `ssh`'s own.
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
