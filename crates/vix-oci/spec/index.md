# OCI engine

Drive an OCI container engine (Docker, Podman, nerdctl) through its CLI: engine detection, argv builders and parsers for containers, images, volumes and networks.

**Status:** Shipped (improvement plan T700). The design is the repo-level
[`spec/open-container`](../../../spec/open-container/index.md); this is the
crate that implements its pure layer. The **OC** menu and panels on top of it
are [`vix-oci-panel`](../../vix-oci-panel/spec/index.md) and the App shell.

## Decision

Shell out to the engine's CLI with `--format json` rather than speak the
Docker Engine API over a socket: it is the one interface the engines share and
needs no HTTP or socket code.

## Layers

1. **Detection** -- `Engine` (`Docker`, `Podman`, `Nerdctl`) with `binary()`,
   `from_name()` and `has_pods()`. `detect(preferred, find)` tries the
   `oc_engine` setting first, then `docker`, `podman`, `nerdctl`, asking the
   caller-supplied `find` (name to resolved path). The crate never reads
   `PATH` itself, so detection is a pure function and the App passes its
   existing lookup. A preferred engine that is not installed falls through to
   the auto order; nothing found is `OciError::NotInstalled`.
2. **Argv builders** (`argv`) -- one function per command, returning the
   arguments after the binary. No shell is ever involved: every user-typed
   value is one argv element. `check` refuses an empty value, a leading `-`
   (an engine would read it as an option), whitespace and control characters;
   `check_path` allows spaces in paths. `render` formats a vector for display
   (single-quoting what needs it); it is shown in confirm prompts and never
   executed. `login` builds `--password-stdin`; **no builder accepts a
   password**. Covered: `list`, `inspect`, `container_op`
   (start/stop/restart/pause/unpause/rm), `logs`, `exec_shell`, `run`, `tag`,
   `pull`, `push`, `remove` (container/image/volume/network/pod), `build`,
   `compose` (up/down/logs/ps/services), `login`, `prune`.
3. **Parsers** (`parse`) -- `records` accepts Docker's NDJSON and Podman's
   single array. `containers`, `images`, `volumes`, `networks` and `pods` read
   keys case-insensitively (`ID`/`Id`/`id`), accept a name as a string or an
   array, turn Podman's port objects and Docker's `0.0.0.0:8080->80/tcp` text
   into display text plus host port numbers, and format Unix-seconds creation
   times and byte sizes. A field an engine does not report is empty, not an
   error. `pretty` formats `inspect` output.

## Dev containers (T718)

`devcontainer` reads just enough to reopen a workspace in its dev container;
the `devcontainer` CLI does the rest (Vix does not implement the Dev
Container specification). `CONFIG_FILES` is where the configuration lives
(`devcontainer.json` in `.devcontainer/`, then `.devcontainer.json`); `parse_config`
reads it as JSONC (`strip_jsonc` drops `//` and `/* */` comments and
trailing commas outside strings) and keeps `name`, `workspaceFolder` and
`remoteUser`; `up_argv(workspace, docker_path)` builds `up
--workspace-folder <dir> [--docker-path <podman>]`; `parse_up` reads the
CLI's result line (`{"outcome":"success","containerId":…}`, or the error's
`message` and `description`); `workspace_folder` picks the folder inside:
what `up` reported, else `workspaceFolder` with
`${localWorkspaceFolderBasename}` filled in, else `/workspaces/<basename>`.
The App runs it (see
[`spec/remote-editing`](../../../spec/remote-editing/index.md#containers-t718)).

## Running

With the `process` feature (default) `run(program, args, stdin)` starts the
engine with piped output, writes optional text to its standard input, and
returns stdout or a typed `OciError` (`NotInstalled`, `Io`, `Failed {code,
stderr}`, `InvalidArgument`, `Parse`). `DOCKER_HOST` and context variables are
inherited, never overridden. The root `oc` Cargo feature turns `process` on;
without it the crate is detection, builders and parsers only, and
`tui/tests/spawn_sites.rs` lists `crates/vix-oci/src/lib.rs` as the single spawn site.

## Testing

Unit tests cover detection order, hostile arguments, and every builder.
`crates/vix-oci/tests/parse.rs` parses fixtures for both output shapes of every listing.
**The fixtures were written by hand from the documented shapes, not captured
from a live engine**; re-check them against real output when an engine is
available. `crates/vix-oci/tests/fake_engine.rs` runs `run` against a shell script standing in
for `docker` (argument passing, stdin-only passwords, failure codes, missing
binary), so no real engine is ever started.
