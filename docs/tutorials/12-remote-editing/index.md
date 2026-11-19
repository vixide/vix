# Tutorial 12: Remote Editing

Vix can work on a folder that lives somewhere else: on another machine over
SSH, inside a running Docker or Podman container, in a Kubernetes pod, or in
a WSL distribution. Your settings, theme, keybindings, snippets and undo
history stay on your machine; the files, search, git, tasks, language
servers and the terminal run on the remote side.

**This feature is experimental.** It works, it is tested, and it has known
limits, listed at the end of this tutorial.

You don't need a server to follow along. The first half uses `ssh localhost`
(your own machine, reached over SSH); the second half uses a throwaway
Docker container. Each step says what to do and what you should see.

For the condensed reference, see
[`docs/remote-editing/index.md`](../../remote-editing/index.md); the design
is in [`spec/remote-editing/index.md`](../../../spec/remote-editing/index.md).

## What you need

- For the SSH part: an SSH server you can reach without typing a password
  each time. On macOS, turn on **Remote Login** in System Settings → General
  → Sharing. On Debian or Ubuntu, `sudo apt-get install openssh-server`.
  Then check that a plain `ssh` works from a terminal:

  ```sh
  ssh localhost true
  ```

  If it asks whether to trust the host key, answer `yes` once; the key is
  saved in `~/.ssh/known_hosts`. If it asks for a passphrase, run `ssh-add`
  so your key is in the agent.
- For the container part: Docker (or Podman) installed and running.

Nothing has to be installed on the remote side except a POSIX `sh` and the
usual commands (`cat`, `head`, `stat`, `find`, `mv`, `cp`, `mkdir`, `rm`).
GNU, BusyBox, BSD and macOS versions all work.

## Step 1: Make a folder to work on

Create a small folder "on the remote side". With `ssh localhost` that is
just a folder on your own disk, but Vix will only reach it through `ssh`:

```sh
mkdir -p /tmp/vix-remote-demo
printf 'first line\n' > /tmp/vix-remote-demo/notes.txt
```

## Step 2: Open it over SSH from the command line

```sh
vix ssh://localhost/tmp/vix-remote-demo
```

Vix runs your system `ssh` and connects **before** the editor takes over the
screen, so if `ssh` needs to ask for a password, a passphrase or a host-key
confirmation, it can ask here as usual. Every later command shares this one
connection (OpenSSH's `ControlMaster`), so none of them prompts again.

Other forms of the same name:

```sh
vix ssh://me@build-box/srv/app          # a user and an absolute path
vix ssh://build-box:2222/~/src/app      # a port, and a path under the remote home
vix /ssh:me@build-box:/srv/app/main.rs  # TRAMP's form; a file opens in its folder
```

Because Vix uses your system `ssh`, your `~/.ssh/config` applies: host
aliases, `User`, `Port`, `ProxyJump`, keys and the SSH agent all work.

## Step 3: Decide whether to trust the workspace

The first time you open a remote workspace, Vix asks:

> Trust ssh://localhost/…? Only a trusted workspace runs
> its own code: tasks, run configurations, tests, the debugger, language
> servers and project tools. (y: trust, n: restricted mode)

Press `n` for now. The status bar shows `⇄ localhost (restricted)`.

In restricted mode you can still browse, edit, search and save, and git and
commands you type yourself (**Run Command…**) still run. What stays off is
everything the workspace itself defines:

- tasks and project commands (the **Project** menu), run configurations and
  test lenses;
- the debugger (**Run → Start**, **Run → Attach**);
- language servers;
- project-local Prettier (`node_modules/.bin`).

Project scripts (`.vix/scripts`) never load for a remote workspace, trusted
or not.

Try one: open **Project → Test** (`project.test`). The status line says *This remote
workspace is not trusted: its own code does not run (File → Trust Remote
Workspace…)*.

Now trust it: open **File → Trust Remote Workspace…** (action id
`file.trust_remote`) and press `y`. The `(restricted)` marker goes away. The
answer is remembered on your machine for that host and folder, so you are
not asked again next time; **File → Trust Remote Workspace…** asks again
whenever you want to change it.

## Step 4: Edit and save

Open `notes.txt` from the Explorer or **File → Open**, add a line, and save.
Saves on a remote workspace are atomic: Vix writes a temporary file next to
the original and renames it into place, keeping the file's permissions.

From a separate terminal, confirm the change landed:

```sh
cat /tmp/vix-remote-demo/notes.txt
```

The Explorer, **File → Open**, the file finder, Find in Workspace, git and
tasks all work on the remote side. Workspace search runs one `rg` there (or
`grep` for a plain-text search when `rg` is missing), so a search is one
round trip rather than one per file.

## Step 5: See a save conflict

Each remote buffer remembers the file's size and modification time from
when it was read or last saved. If the file changes on the remote side
behind your back, Vix won't overwrite it silently.

1. In Vix, add another line to `notes.txt` but **don't save yet**.
2. In a separate terminal, change the file on the "remote" side:

   ```sh
   ssh localhost "printf 'changed elsewhere\n' >> /tmp/vix-remote-demo/notes.txt"
   ```

3. Back in Vix, save.

Instead of saving, Vix asks:

> …notes.txt changed on disk since you opened it. o: overwrite it, c:
> compare, n: cancel

- `o` overwrites the remote file with your buffer.
- `c` shows the file on disk against your buffer, so you can merge by hand
  and save again.
- `n` (or Esc) leaves both alone.

Local files are saved as before, without this check.

## Step 6: Open a terminal on the remote side

Open **Tools → Terminal → Show/Hide Terminal** (`tools.terminal`). Vix runs
`ssh -t` over the same shared connection, and the remote side starts your
login shell (`$SHELL`) in the workspace folder. `Ctrl+]` hides it again.

In an untrusted workspace the terminal still opens, but in your remote home
folder instead of the workspace, so nothing the workspace defines (a shell
hook, an `.envrc`) runs until you `cd` there yourself.

## Step 7: Language servers on the remote side

Once the workspace is trusted, language servers and debug adapters run on
the remote machine, where the code and its toolchain are. Configure them in
`config.toml` exactly as in
[Tutorial 5](../05-setting-up-lsp/index.md) and
[Tutorial 10](../10-debugging-with-dap/index.md); Vix looks each `command` up
on the remote machine's `PATH`. A server that isn't installed there shows
as failed in **View → LSP Status…**.

One catch: servers, adapters and tasks see the `PATH` of a non-interactive
`sh` on the remote machine, which does not read your shell profile. A tool
under `~/.cargo/bin` may not be found. If so, configure its full path:

```toml
[[lsp_servers]]
language_id = "rust"
extensions = ["rs"]
command = ["/home/me/.cargo/bin/rust-analyzer"]
```

The terminal is a login shell, so it has your usual `PATH`.

## Step 8: Open a remote folder from inside Vix

You don't have to start Vix with a remote name. From a running Vix, open
**File → Open Remote…** (`file.open_remote`, also in the command palette).
It lists your recent remotes, newest first, under a **New connection…**
row. Pick `ssh://localhost/tmp/vix-remote-demo` from the list, or choose
**New connection…** and type a name in either form.

The status bar shows `⟳ localhost` while Vix connects in the background,
then the folder opens as the workspace. Switching workspaces closes your
buffers, so Vix refuses to switch while a buffer has unsaved edits: save or
close them first.

**The dialog never prompts.** The editor owns the terminal, so this
connection runs `ssh` in `BatchMode`: it cannot ask for a password, a
passphrase or whether to trust a new host key. If the host needs one of
those, the connection fails with a message that says what to do: run
`ssh <host>` once in a terminal to accept its key, run `ssh-add`, or start
Vix with `vix ssh://…` instead, which can prompt. Keys in an agent,
`ssh_config` and an existing shared connection all work from the dialog.

## Step 9: Lose the connection and reconnect

If the connection drops, the status bar shows
`⚠ localhost: connection lost — File → Reconnect Remote`. Your buffers stay
open with their edits. Saving is refused (not attempted), and the check for
files changed on disk pauses.

When the host is reachable again, open **File → Reconnect Remote**
(`file.reconnect_remote`). Vix connects in the background; nothing is
reloaded or lost, and you save as usual. A file that changed remotely while
you were away is caught by the save check from Step 5.

There is no **Disconnect** command: to leave a remote workspace, open
another folder or quit.

## Step 10: Work inside a container

The same works inside a running container. Vix runs each command through
the engine's `exec` instead of `ssh`, so the container needs only a POSIX
`sh` and the usual commands. Start a throwaway Alpine container with a
folder in it:

```sh
docker run -d --name vix-demo alpine sleep infinity
docker exec vix-demo sh -c 'mkdir -p /work && echo hello > /work/hello.txt'
```

Open it:

```sh
vix docker://vix-demo/work
```

A container connection never prompts. You get the same trust question as in
Step 3 (every container workspace starts untrusted), and then the status
bar shows `⇄ docker:vix-demo`. Everything from Steps 3 to 9 applies: atomic,
conflict-checked saves, search, git, tasks and language servers inside the
container, and **Tools → Terminal → Show/Hide Terminal** opens a login shell
inside (`$SHELL`, else `bash`, else `sh`).

The other container forms:

```sh
vix podman://node@api/~/src     # Podman, as user node, under its home
vix k8s://dev/web-7f9c/app/srv   # namespace/pod/container in Kubernetes
vix wsl://Ubuntu/home/me/src     # a WSL distribution (from Windows)
```

**File → Open Remote…** takes all of these names too. It also lists the
containers of running Kubernetes pods (a **Kubernetes container…** row, for
the current context's namespace) when `kubectl` is installed, and the
installed WSL distributions (**WSL distribution…**) when `wsl.exe` is. Pick
one and the prompt is filled in with its name and `~`; change the path and
press Enter.

If a container stops, it shows as a lost connection; start it again and use
**File → Reconnect Remote**.

When you are done with the demo container:

```sh
docker rm -f vix-demo
```

## Step 11: Pick a container from the OC menu

You can also start from the container list. Open **OC → Open Workspace in
Container…** (`oc.open_workspace`). The status line says *Select a running
container and press w to open a workspace in it*. Highlight a running Docker
or Podman container and press `w`: the Open Remote prompt opens, filled in
with `docker://<name>/` (or `podman://…`). Finish the path inside the
container and press Enter.

## Step 12: Reopen a project in its dev container

If a local project has a `devcontainer.json` in its `.devcontainer` folder
(or a `.devcontainer.json` at its root), **OC → Reopen in Dev Container**
(`oc.devcontainer_reopen`) opens it inside that container. It needs the
[`devcontainer` CLI](https://github.com/devcontainers/cli):

```sh
npm install -g @devcontainers/cli
```

Vix tells you if it isn't installed. Building a dev container runs the
project's own Dockerfile, features and lifecycle commands, so Vix asks
first (`y` to build, `n` to cancel). Then it runs
`devcontainer up --workspace-folder` on the project in the background (with
`--docker-path podman` when the `oc_engine` setting, or what is installed,
is Podman), and opens the workspace folder the CLI reports, as the
`remoteUser` the configuration names. Because you just agreed to run the
project's code, this workspace is trusted at once.

Vix doesn't implement the Dev Container specification itself: the CLI does
the work, and Vix reads `devcontainer.json` only for `workspaceFolder` and
`remoteUser` as fallbacks.

## Step 13: Speed it up with the remote agent

Without help, every file operation is one shell command over the shared
connection. If the remote machine has `vix` of **exactly the same version**
as yours, Vix starts it there as a helper, `vix --remote-agent`, and sends
file operations to it instead: opening, saving, listing, the file finder's
walk, and the once-a-second check for files changed on disk (all open files
in one request). Anything it can't do (a file over 64 MiB, running a
program) still uses shell commands, and if the agent stops, Vix carries on
without it.

`vix --remote-agent` is not for you to run by hand; Vix starts it over the
connection. With `ssh localhost` the remote `vix` is the same binary as
yours, so if `vix` is on the `PATH` a non-interactive `sh` sees, the agent
is used. Open the Messages drawer (**View Messages**, `view.messages`, in
the command palette) to see a line saying file operations go through the
remote agent, or why it isn't in use.

Vix looks for the agent in `~/.cache/vix/agent/<version>/vix` (or under
`$XDG_CACHE_HOME`) and on the remote `PATH`. The `remote_agent` setting in
`config.toml` decides the rest:

```toml
remote_agent = "installed"   # default: use one that is already there
# remote_agent = "upload"    # ...and copy this vix there if there is none
# remote_agent = "off"       # shell commands only
```

With `"upload"`, and only when the remote machine runs the same operating
system on the same processor architecture, Vix copies its own binary into
that cache folder. The remote side checks the copy's SHA-256 before it is
made executable; on any mismatch the copy is deleted. Upload is off by
default because it writes a large executable into your remote home over
what may be a slow link. The setting applies to containers too.

## Current limits

Remote editing is experimental. Today:

- Opening from **File → Open Remote…** can't answer a password, passphrase
  or host-key question (Step 8). There is no **Disconnect**.
- A dropped connection is noticed when an open, a save or the change check
  fails; until then an operation may wait for `ssh`'s 15-second connect
  timeout.
- Trust is per remote workspace only; local workspaces keep the existing
  script-trust prompt.
- Without the remote agent, each operation is a round trip. A
  regular-expression search on a machine without `rg` reads every file, one
  round trip each; install `ripgrep` there. Workspace symbol search (`@@` in
  the palette), structural replace and the Org agenda still read every file.
- Moving a file to the trash needs `gio` or `trash-put` on the remote
  machine; without one, delete instead (or set `explorer_delete = "hard"`).
- Debug adapters that talk over a TCP port instead of standard input and
  output aren't supported, locally or remotely.
- Stopping a task or closing the terminal ends the local `ssh`; a remote
  program that ignores the closed connection may keep running there.
- A container without `/bin/sh` (a "distroless" image) can't be opened.
  nerdctl containers can't be opened by name yet. Kubernetes has been run
  against one local cluster; WSL has been tested only against a stand-in
  for `wsl.exe`.

## Where to go next

- [`docs/remote-editing/index.md`](../../remote-editing/index.md): the full
  user guide, including safety (no secrets on the command line, host-key
  checking always left to `ssh`) and how to run Vix's own SSH tests against
  a local `sshd` with `scripts/test-ssh-local`.
- [`docs/oc/index.md`](../../oc/index.md): the OC menu for containers,
  images, volumes and networks.
- [`docs/terminal/index.md`](../../terminal/index.md): the integrated
  terminal.
- [`docs/cli/index.md`](../../cli/index.md): every command-line form,
  including `vix ssh://…` and `--remote-agent`.
- [`crates/vix-authority/spec/index.md`](../../../crates/vix-authority/spec/index.md):
  how every file operation and process goes through one "authority", with
  the [SSH](../../../crates/vix-authority/spec/ssh/index.md),
  [container](../../../crates/vix-authority/spec/container/index.md) and
  [agent](../../../crates/vix-authority/spec/agent/index.md) details.

---

**Previous:** [Tutorial 11 — Tasks, Tests and the Terminal](../11-tasks-tests-and-the-terminal/index.md)
**Next:** [Tutorial 13 — Scripting with Rhai](../13-scripting-with-rhai/index.md)

---

Vix™ and Vix IDE™ are trademarks.
