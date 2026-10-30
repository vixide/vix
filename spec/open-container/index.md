# OC — Open Container Initiative menu

**Status:** implemented (T700–T708) apart from registry login and
per-service Compose logs; container workspaces and Dev Containers came with
T718 ([`spec/remote-editing`](../remote-editing/index.md#containers-t718)); see the Done notes under each task in
[`tasks.md`](../../tasks.md). User guide: [`docs/oc`](../../docs/oc/index.md).
Crates: [`vix-oci`](../../crates/vix-oci/spec/index.md) and
[`vix-oci-panel`](../../crates/vix-oci-panel/spec/index.md).

*Sources: the author's knowledge of VS Code's Container Tools extension (the
successor to the Docker extension), the OCI specs and the engines' CLIs; no
live web research for this draft. Check each CLI's current JSON output before
writing parsers.*

## Goal

An **OC** top-level menu to manage local containers, images, volumes and
networks without leaving Vix, on any OCI-compatible engine: Docker, Podman,
nerdctl, and others. It mirrors what VS Code's Container Tools offers in its
sidebar.

## The OCI pieces that matter

- **image-spec:** what an image is (layers, manifest, config).
- **runtime-spec:** how a container starts (`config.json`; runc, crun).
- **distribution-spec:** how registries serve images (push, pull, tags).

Vix does not implement these. It drives an engine that does.

## VS Code Container Tools: feature inventory

Containers (start, stop, restart, pause, remove, logs, attach shell, inspect,
open in browser by published port), images (run, run interactive, tag, push,
pull, remove, inspect, build from a Dockerfile), registries (Docker Hub,
GHCR, ACR, others; sign-in), networks, volumes, contexts, Compose
(up, down, restart, per-service logs), prune, and Dockerfile/Compose language
support. Dev Containers (`devcontainer.json`) is a separate extension and
connects to remote editing; see [`../remote-editing/index.md`](../remote-editing/index.md).

## Engine support

| Engine | Binary | Notes |
| ------ | ------ | ----- |
| Docker | `docker` | `ps --format json` prints one JSON object per line (NDJSON). |
| Podman | `podman` | Same flags, daemonless; `ps --format json` prints a single JSON array. Has **pods**. |
| nerdctl | `nerdctl` | containerd; mostly Docker-compatible flags. |
| Others | `finch`, `lima nerdctl`, `buildah`, `skopeo` | Build-only or copy-only; detected, not required. |

**Decision:** shell out to the CLI with `--format json` rather than speak the
Docker Engine API over a socket. It is the one interface all engines share, it
needs no HTTP/socket code, and it reuses `vix-terminal` and the command runner.
A socket client can come later behind a trait if listing is too slow.

Detection order: the `oc_engine` setting, then `docker`, then `podman`, then
`nerdctl`. A missing engine shows a translated install hint (same pattern as
`GitError::NotInstalled` and `TunnelError::NotInstalled`).

## Menu

**OC** → Engine ▸ (Detect, Choose), Containers…, Images…, Volumes…, Networks…,
Pods… (Podman only), Compose ▸ (Up, Down, Logs), Build Image…, Run Image…,
Registries ▸ (Login, Push, Pull), Prune…, **Open Workspace in Container…**
and **Reopen in Dev Container** (T718).

**Open Workspace in Container…** (`oc.open_workspace`) opens the container
list; `w` on a running container (Docker or Podman) fills **File → Open
Remote…**'s prompt with `docker://name/` (or `podman://`) for the path inside.
**Reopen in Dev Container** (`oc.devcontainer_reopen`) needs a
`devcontainer.json` and the `devcontainer` CLI, asks first (it runs the
project's Dockerfile and lifecycle commands), runs `devcontainer up`, and
opens the container it reports. Both land in a `ContainerAuthority`
([`vix-authority/spec/container`](../../crates/vix-authority/spec/container/index.md)).

Panels follow the Git and DB panels: a list with a detail pane, `j/k` movement
(`vix-list-state`), and per-row actions. Logs stream into the bottom dock.
**Shell** opens `exec -it <ctr> sh` in a `vix-terminal` tab.

## Architecture

- `vix-oci` — pure: engine enum, argv builders, and parsers for `ps`,
  `images`, `volume ls`, `network ls`, `inspect` (both NDJSON and array
  forms). Fixture-tested with captured real output from Docker and Podman.
- `vix-oci-panel` — the list/detail overlays.
- App shell — actions `oc.*` (one id, one `run_action` arm), menu, palette,
  `crates/vix-i18n/locales/menu.yml` and `status.yml`, and the action catalog.

## Safety

- Destructive actions (remove, prune, force) need a confirm prompt that names
  what will go.
- **Read-only mode** setting disables everything but list/inspect/logs.
- Never put registry passwords on argv: use `--password-stdin`.
- Do not auto-run images. `Run Image` shows the exact command first.
- Respect `DOCKER_HOST` / `CONTEXT` without overriding them.

## Testing

Argv builders and parsers are pure and tested with fixtures. Engine-touching
tests use a fake `docker` script on `PATH` (never a real engine in CI), the
same approach as the fake-LSP and DAP mock tests (T608).
