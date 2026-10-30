# OC (containers)

The **OC** menu manages local containers, images, volumes and networks without
leaving Vix. It drives whichever OCI engine you have installed -- Docker,
Podman or nerdctl -- through that engine's command line, so anything the
engine can reach (its context, `DOCKER_HOST`, rootless Podman) works as it
does in your shell. Vix itself implements none of the OCI specs.

## Engines

**OC → Engine → Detect** shows which engine will be used. The order is the
`oc_engine` setting, then `docker`, then `podman`, then `nerdctl`; the default
`"auto"` takes the first one found on `PATH`. **Choose** steps the setting
through `auto` and each installed engine. With none installed, Vix says how to
get one instead of failing silently.

## The panel

**Containers…**, **Images…**, **Volumes…**, **Networks…** and (Podman only)
**Pods…** open one panel. `Tab` / `Shift+Tab` move between the lists, the
arrows and PageUp/PageDown move through the rows, `r` reloads, `Enter` (or `i`)
opens the object's `inspect` JSON in a read-only tab, and `Esc` closes.

| List | Keys |
| ---- | ---- |
| Containers | `s` start, `x` stop, `t` restart, `p` pause or unpause, `l` logs to the bottom dock, `e` shell (an `exec -it` terminal), `w` open a workspace inside it (Docker or Podman; see [remote editing](../remote-editing/index.md#containers)), `c` copy `http://localhost:PORT` for the first published port, `a` show or hide stopped containers |
| Images | `u` run, `g` tag, `p` pull, `P` push |
| Every list | `d` or `Delete` remove, `D` prune that list |

Run asks for `IMAGE [name=NAME] [port=HOST:CTR]...` and runs detached. Build
(**OC → Build Image…**) asks for `FILE [tag=REF] [context=DIR]`, seeded with a
`Containerfile` or `Dockerfile` in the workspace root.

## Container workspaces and dev containers

**OC → Open Workspace in Container…** opens the container list; `w` on a
running container opens **File → Open Remote…** with its name filled in
(`docker://web/`), so you can work on the files inside it. **OC → Reopen in
Dev Container** builds and starts the project's dev container with the
`devcontainer` CLI (it asks first) and opens it. Both are described in
[remote editing](../remote-editing/index.md#containers).

## Compose

**OC → Compose → Up / Down / Logs** use the `compose.yaml`, `compose.yml`,
`docker-compose.yaml` or `docker-compose.yml` in the workspace root. Up and
Down show the exact command and wait for a yes; Logs streams into the bottom
dock. Output of builds, pulls, pushes, runs and Compose goes to the dock too.

## Safety

- **You see the command first.** Remove, prune, run, build, pull, push, and
  Compose Up/Down show the exact command line and run only on `y` or `Enter`.
  A prune says which kind of object it clears.
- **No shell.** Every command is an argument list; a value that starts with `-`
  or contains whitespace is refused rather than passed to the engine.
- **Read-only mode.** Set `oc_mode = "read-only"` to allow only listing,
  inspecting, logs, shell and copy-URL. Everything else answers that OC is
  read-only.
- **Passwords never go on a command line.** The login command builder uses
  `--password-stdin`. (There is no login prompt yet; sign in from a terminal.)

## Settings

| Setting | Default | Meaning |
| ------- | ------- | ------- |
| `oc_engine` | `"auto"` | `"auto"`, `"docker"`, `"podman"` or `"nerdctl"`. |
| `oc_mode` | `"full"` | `"full"` or `"read-only"`. |

## Building without it

`oc` is a Cargo capability feature (on by default, implies `tasks`). Without it
Vix never starts a container engine and the OC menu is hidden. See
[Cargo features](../../spec/cargo-features/index.md).

## See also

- The `vix-oci` and `vix-oci-panel` crate specs
  (`crates/vix-oci/spec/index.md`, `crates/vix-oci-panel/spec/index.md`) and the
  design, `spec/open-container/index.md`.
