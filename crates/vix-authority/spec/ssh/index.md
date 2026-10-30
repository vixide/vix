# SSH authority

`SshAuthority` (T712): the workspace on another machine, reached with the
system `ssh` and plain POSIX `sh` + coreutils on the remote side. Phase 1 of
[`spec/remote-editing`](../../../../spec/remote-editing/index.md), TRAMP-style:
nothing is installed remotely.

**Status:** Shipped 2026-10-30 behind the crate's `ssh` feature (on in the
root `tasks` feature). Experimental user surface: `vix ssh://…` on the
command line ([`docs/remote-editing`](../../../../docs/remote-editing/index.md)).

## Remote names (`ssh::SshTarget`, always built)

- `ssh://[user@]host[:port][/path]`: the user and path are `%`-decoded; an
  IPv6 host is bracketed (`ssh://[::1]:22/x`); `/~` and `/~/…` name the
  remote home; no path means the home.
- `/ssh:[user@]host[#port]:[path]` (TRAMP's form): the path is literal; an
  empty path means the home.
- Parsed into `SshTarget { user, host, port, path }`. Refused with a
  `TargetError`: an option-shaped host or user (`-…`), whitespace or control
  characters, `/` or `@` in a host, `:` in a user, a port outside 1–65535
  (digits only), a relative path, a NUL, or a bad `%` escape. `to_url()` gives
  the canonical `ssh://` form and round-trips.

## Quoting (`ssh::quote`, always built)

- `sh_quote(bytes)` single-quotes one word (`'` → `'\''`). Every remote path,
  argument and environment value goes through it as bytes, so any name
  without a NUL (spaces, quotes, `$`, newlines, invalid UTF-8) arrives intact.
  `quote_path` turns a leading `~` into `"$HOME"`.
- `envelope(script)` makes the remote command line. `sshd` hands it to the
  user's *login* shell, which may be `fish` or `csh`, where `'\''` and `!` mean
  something else. So the script travels as one word of octal digits after a
  fixed decoder: `exec /bin/sh -c '<decoder>' vix <digits>`. The decoder has no
  `\\`, `\'`, `!` or newline in it (tested), and turns the digits back into the
  script with `sed`, `tr` and `printf %b`, then `eval`s it in `/bin/sh`.
  Property tests run arbitrary bytes through both layers; a test runs the same
  write and read under every login shell installed (`sh`, `dash`, `bash`,
  `zsh`, `fish`, `tcsh`, `csh`).

## Connection

- `ssh -T -o BatchMode=yes -o ConnectTimeout=15 -o ControlMaster=auto
  -o ControlPath=<dir>/%C -o ControlPersist=600 [-p PORT] [-l USER] -- HOST
  <envelope>`. Nothing else overrides `ssh_config`, so host aliases, `User`,
  `Port`, `ProxyJump`, keys, the agent and host-key checking apply. No
  password or secret is ever on argv; authentication is `ssh`'s own (keys,
  agent, `SSH_ASKPASS`). `BatchMode` means a background operation fails
  rather than prompting under the UI.
- `connect(interactive)` opens the shared connection and probes the remote
  `stat`. Interactive, `ssh` may prompt on the terminal (BatchMode off), so it
  is called before the UI starts. `disconnect()` runs `ssh -O exit`; dropping
  the authority does not, so a second Vix on the same host keeps the
  connection until `ControlPersist` expires.
- Control sockets live in `$XDG_RUNTIME_DIR/vix-ssh`, else
  `/tmp/vix-ssh-<uid>` (the uid read from a private temp file's owner; no
  `unsafe`). The directory is created 0700; an existing one must be a real
  directory (not a link), owned by this user, with no group or other bits,
  or the authority refuses to start (`PermissionDenied`). A directory path too
  long for a 104-byte socket path, or holding whitespace or quotes, turns
  sharing off (`multiplexed()` reports it). `SshOptions` sets the program
  (an explicit path in tests), sharing, the directory, `ControlPersist`,
  `BatchMode` and `ConnectTimeout`. Windows' OpenSSH has no multiplexing.

## Operations

One short `sh` script per operation; the script's exit status names the error
kind (100 `NotFound`, 101 `PermissionDenied`, 102 `AlreadyExists`, 103
`IsADirectory`, 104 `NotADirectory`, 105 `DirectoryNotEmpty`, 106 conflict,
107/109 `TextReadError::NotRegular`/`TooLarge`, 108 `Unsupported`), and `ssh`'s
own 255 is `NotConnected` with its message. Other failures carry the remote's
error output.

| Operation | Remote side |
| --- | --- |
| `read`, `read_limited` | `cat`, `head -c N` |
| `read_text_guarded` | regular-file and size checks (`wc -c`) remotely, then `head -c max+1`; decoded locally with `vix_fileops::decode_text` |
| `write` | `cat >` |
| `write_atomic` | follow symlinks (`readlink`, at most 40), exclusive temp file (`set -C`, umask 077 when replacing) in the same directory, `chmod` to the old mode, `mv -f`; without a writable directory, write in place like the local writer |
| `write_atomic_if_unchanged` | the same, after checking the target's length and modification time (whole seconds) in the same script |
| `metadata`, `symlink_metadata` | `stat -L`/`stat`: GNU/BusyBox `-c '%f %s %Y %W'` or BSD/macOS `-f '%p %z %m %B'`, picked by a one-time probe |
| `canonicalize` | follow links, then `cd -P` and `pwd -P` |
| `read_dir` | glob `* .[!.]* ..?*`, kind and followed-dir flag per entry, NUL-separated |
| `create_dir_all`, `remove_file`, `remove_path` | `mkdir -p`, `rm -f`, `rm -rf` (a link is removed, never followed) |
| `rename`, `move_path` | `mv -f`, refusing to move *into* an existing directory (`std::fs::rename` semantics: an empty target directory is replaced) |
| `copy_recursive` | `ln -s` for a link, `cp -RP src/. dst` for a tree, `cp -P` for a file |
| `trash` | `gio trash` or `trash-put` if present, else `Unsupported` |
| `is_unwritable` | `[ -f ] && ! [ -w ]` |
| `walk_files` | `git ls-files -co --exclude-standard` (filtered to real files) when ignore files are honored and the root is a git work tree, else `find -mindepth 1 -maxdepth D … -prune -o -type f -print0`; read as a stream and cut off at the limit; depth, hidden and pruned names re-applied locally |
| `which` | `command -v`, accepting only an absolute, executable file; option-shaped names or names with `/` are refused |
| `command` | see below |

`write_text_in_file_encoding` reads with the guard and encodes with
`vix_fileops::encode_like`, then writes atomically.

`metadata_batch` (T717) is one script per 200 paths: `for p in …; do` a
`stat` line (`o…`), `n` for a missing path or `e` for a failed `stat`;
`done`. So the App's change poll is one `ssh` run, not one per open file.

**With the remote agent** (T717, [`spec/agent`](../agent/index.md)), when
`SshOptions::agent` is not `Off` and `connect()` found or uploaded a `vix`
of this exact version, every operation in this table except `command` goes
to the agent first, and falls back to the script above only when the agent
is gone or the request is too large for a frame.

### Processes

`command(spec)` returns a `Command` running `ssh … HOST <envelope>` whose
script is: `cd -P -- <cwd> || exit 1` (T715: `-P` resolves `..` physically, as a local `chdir` does), then `KEY=<value>; export KEY` or
`unset KEY` for each environment change in order, then `exec <program>
<args…>`, every word quoted. An option-shaped program or an invalid variable
name is refused (`InvalidInput`). `Arg::Raw` is an ordinary argument (the
remote is POSIX). The caller configures stdio and spawns as before; anything
it sets on the `Command` itself (environment, working directory, process
group) applies to the local `ssh`, not the remote program, so call sites put
those in the spec. Stopping the local `ssh` closes the channel; a remote
program that ignores its closed stdin/stdout may keep running (T716/T717).

## T710 leftovers decided here

- **`no_tty_prompts`** (`tui/src/app.rs`) now adds its environment
  (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, `GIT_SSH_COMMAND`) to the
  `ProcessSpec`, so it reaches the remote side; the process group
  (`detach_from_tty`) still goes on the local `Command`. There is no remote
  terminal to prompt on (`ssh -T`).
- **`vix-git`'s cached `git` path** is used only under the local authority;
  any other authority runs the bare `git`, which its own side resolves on its
  `PATH` (a POSIX `exec` never searches the working directory).
- **The AI `cli` provider** always runs on this machine through
  `LocalAuthority`: it is the user's own tool, signed in locally, and its
  prompt is a local private temp file. Under a remote authority it runs in the
  local temp directory instead of the (remote) workspace root.

## Testing

`crates/vix-authority/tests/ssh.rs` (Unix): a fake `ssh` script, named by
explicit path (no `PATH` editing), skips the options and host and runs the
remote command with a login shell's `-c`. Against it: `conformance::run`
passes; the exact argv (options, `-p`, `-l`, `--`, no script text in clear);
the control directory is 0700 and a shared or linked one is refused; a
`ProcessSpec` with a quoted `cwd`, hostile arguments and environment values
round-trips, and `env_remove` removes a variable the local process had;
saves write through links, keep the mode, refuse a stale version and leave
no temp files; error kinds; odd names (newline, quote, `$`, `*`, leading
`-`, non-ASCII) list, read and walk; every installed login shell, and the decoder under every installed `/bin/sh` candidate (`dash`, `bash`, `ksh`, `mksh`); property
tests for both quoting layers. `a_real_host_conforms` runs the suite against
the host in `VIX_TEST_SSH_HOST` (`user@host` or a remote name) and does
nothing without it. [`scripts/test-ssh-local`](../../../../scripts/test-ssh-local) (T719) runs
it against a throwaway non-root `sshd` on `127.0.0.1` with generated keys, its own
`ssh_config` and `known_hosts` (host-key checking on) and an `ssh` wrapper on `PATH` that
adds `-F`, never touching `~/.ssh`; `.github/workflows/ssh.yml` runs that script on
`ubuntu-latest` when this crate changes, or on demand (GitHub only; see
[`spec/ci`](../../../../spec/ci/index.md)). `tui/tests/authority_ssh.rs`
installs an `SshAuthority` over the fake `ssh` and drives the App's open and
save. Unit tests cover the URL forms, both `stat` flavors' replies and the
walk filters.

## Host keys and secrets (T714)

Vix passes no host-key option (`StrictHostKeyChecking`,
`UserKnownHostsFile`, `GlobalKnownHostsFile`) and no authentication option,
so `ssh_config` and `known_hosts` decide, and every command after the
interactive first connect of `vix ssh://…` runs with `BatchMode=yes`: an
unknown or changed host key, or a key that needs a passphrase, fails instead
of being accepted or asked for under the UI. **File → Open Remote…** connects
with `BatchMode=yes` too, and its error says to run `ssh host` once (or
`ssh-add`) or to start with `vix ssh://…`. Vix never writes `known_hosts`.
Nothing secret is ever on argv: `SshAuthority` builds argv only from the
target (user, host, port) and its `SshOptions`, never from settings;
`tui/tests/remote_ui.rs` checks every argv of a session against
password-like values planted in the settings.

## Not done (later tasks)

- ~~The App's save does not call `write_atomic_if_unchanged`~~ — T713 wires
  it for a remote workspace, with a per-tab disk stamp and an overwrite /
  compare / cancel prompt.
- ~~Connection-lost state, reconnect, status-bar host, the Open Remote
  dialog; trust prompt~~ — T713, T714 (see
  [`spec/remote-editing`](../../../../spec/remote-editing/index.md)).
- Per-path routing (a local file opened while a remote workspace is
  installed still goes to the remote); remote search with `rg` (T715);
  remote LSP/DAP path mapping and a remote terminal (T716).
- ~~Batching: the external-change poll stats each open file over its own round
  trip (T717's agent).~~ Done (T717): `metadata_batch`, one script (or one
  agent batch) per poll.
- The remote `PATH` is that of a non-interactive `sh` (profile files are not
  read). The git-based walk applies git's ignore rules only (not `.ignore`
  files); the `find` walk outside a work tree applies none.
- Modification times are whole seconds; hard links, ownership, ACLs and
  extended attributes are not carried over by a save (as locally).
