# Container authorities

`ContainerAuthority` (T718): the workspace inside a running Docker or Podman
container, a container in a Kubernetes pod, or a WSL distribution. Phase 3
of [`spec/remote-editing`](../../../../spec/remote-editing/index.md). It is
the SSH authority ([`spec/ssh`](../ssh/index.md)) with a different
transport: the same POSIX `sh` scripts, quoting, error codes and remote
agent ([`spec/agent`](../agent/index.md)), run through the engine's `exec`
instead of `ssh`.

**Status:** Shipped 2026-10-30 behind the crate's `ssh` feature (on in the
root `tasks` feature). Docker and Podman are tested against a fake engine
and, opt-in, a real one; Kubernetes and WSL against the fake engine only.
User surface: [`docs/remote-editing`](../../../../docs/remote-editing/index.md#containers).

## Design: one shell authority, two transports

`ShellAuthority<T: Transport>` holds everything that was `SshAuthority`
(T712, T717): every `Authority` method as a short `sh` script, the
`metadata_batch` script, the `git ls-files`/`find` walk, the `stat` flavor
probe, conflict-checked atomic saves, and the remote agent (find, upload,
start, route). A `Transport` says only how a script gets to `/bin/sh` on the
other side:

| Method | `SshTransport` | `ExecTransport` |
| --- | --- | --- |
| `name` | `ssh` | `docker`, `podman`, `kubernetes`, `wsl` |
| `label` (error messages) | the host | `docker:user@name`, `k8s:ns/pod/ctr`, `wsl:distro` |
| `shell(script, tty, env_names)` | `ssh -T`/`-t … HOST <envelope>` | the engine prefix, then `/bin/sh -c <decoder> vix <octal digits>` |
| `lost(status, stderr)` | status 255 | 125 (Docker, Podman), -1 (WSL), or an engine "no such container / can't connect" message (for `kubectl`, its message only: see below) |
| `passes_env_by_name` | no | Docker and Podman |
| `login_shell` (terminal) | `exec "${SHELL:-/bin/sh}" -l` | `$SHELL`, else `bash` if present, else `/bin/sh`, with `-l` |
| agent mode, agent binary | `SshOptions` | `ExecOptions` |

`SshAuthority` is `ShellAuthority<SshTransport>` and keeps its whole public
surface (`new`, `with_options`, `connect(interactive)`, `disconnect`,
`target`, `options`, `base_command`, `multiplexed`, `start_agent`,
`upload_agent`, `agent_status`, `watch`, …); `ContainerAuthority` is
`ShellAuthority<ExecTransport>` with `new(target, options)`, `connect()`
(probe, then the agent), `disconnect()` (stops the agent) and `target()`.
The script layer was not copied: the refactor moved the SSH specifics into
`SshTransport` and made the rest generic, so a fix to a script fixes every
transport.

The script travels the same way on every transport: three octal digits per
byte behind the fixed `DECODER` (`ssh::quote::sh_argv`). Over `ssh` a login
shell parses the envelope; through `exec` nothing parses the command line,
so `/bin/sh -c DECODER vix DIGITS` is passed as five arguments. Either way
no platform's argv quoting (Windows' for `wsl.exe` included) ever sees the
script itself, only fixed text and digits.

## Names (`container::ContainerTarget`, always built)

| Form | Meaning |
| --- | --- |
| `docker://[user@]container[/path]` | `docker exec` into a running container (name or id) |
| `podman://[user@]container[/path]` | `podman exec` |
| `k8s://namespace/pod/container[/path]` | `kubectl exec` into a pod's container (all three required; no user) |
| `wsl://[user@]distribution[/path]` | `wsl.exe -d` a WSL distribution |

The path is `%`-decoded; no path, or `/~`, is the home directory of the user
the commands run as (`~/…` below it). Names are checked when parsed, so
nothing option-shaped or shell-active ever reaches a command line: Docker,
Podman and WSL names `[A-Za-z0-9][A-Za-z0-9_.-]*`; Kubernetes names
lowercase `[a-z0-9][a-z0-9.-]*`; users (a name, a uid, `uid:gid`)
`[A-Za-z0-9_][A-Za-z0-9_.:-]*`. A bad name is `TargetError::BadName`
(`BadUser`, `BadPath`); `to_url()` round-trips.

`remote::RemoteTarget` is either an `SshTarget` or a `ContainerTarget`:
`parse`, `is_remote`, `to_url` (the session key), `label` (the status bar),
`path`/`set_path`, and, with the `ssh` feature, `connect(&ConnectOptions,
interactive)`, which builds and connects the right authority and returns it
with its agent status. The App's Open Remote dialog, `vix <name>`, the
status bar, reconnect and trust all go through it.

## Command lines

- Docker, Podman: `ENGINE exec -i [-t] [-u USER] [-e NAME]… CONTAINER
  /bin/sh -c …`.
- Kubernetes: `kubectl exec -i [-t] -n NS -c CTR POD -- /bin/sh -c …`.
- WSL: `wsl.exe -d DISTRO [-u USER] --exec /bin/sh -c …`.

`ExecOptions::program` overrides the engine (an explicit path; tests point
it at a fake). Engine context (`DOCKER_HOST`, `CONTAINER_HOST`,
`KUBECONFIG`, the current context) is inherited, never set.

`command(spec)` runs `cd -P -- <cwd> || exit 1`, the environment changes,
then `exec <program> <args…>`, every word quoted, as over SSH. **Secrets
stay off argv where the engine allows it:** Docker and Podman get `-e NAME`
for each variable whose last change sets it, with the value in the local
engine process's environment (the engine reads it from there); a variable
whose last change removes it is `unset` by the script. `kubectl exec` has
no environment option and `wsl.exe` passes only `WSLENV`-listed variables,
so for those the script exports the values, which then travel on the local
argv (as with `ssh`). `terminal(_, cwd)` is the same prefix with `-t`
(Docker, Podman, Kubernetes; `wsl.exe` uses the console it is on), its
script `cd -P -- <cwd>` then the login shell.

## The remote agent in a container

Exactly the SSH rules ([`spec/agent`](../agent/index.md)), through the
same code: the `remote_agent` setting picks the mode; `"installed"` uses a
`vix` of this exact version already in the container (its cache or
`PATH`); `"upload"` copies this binary in only when the container's `uname
-sm` matches this machine, and the container computes the SHA-256 before
the file is made executable. The upload goes over `exec -i`'s standard
input, not `docker cp`: one code path with SSH, the same exclusive
temporary name and 0700 checks, and it works the same for `kubectl` and
`wsl.exe` (which have no `cp` with those guarantees).

## Dev containers (App side)

`vix-oci`'s `devcontainer` module ([spec](../../../vix-oci/spec/index.md))
reads `devcontainer.json` (JSONC) and the CLI's result; the App runs
`devcontainer up --workspace-folder <root>` and opens the container it
reports as a `ContainerAuthority`. See
[`spec/remote-editing`](../../../../spec/remote-editing/index.md#containers-t718).

## Testing

`crates/vix-authority/tests/container.rs` (Unix): a fake engine script,
named by explicit path (no `PATH` or environment edits in-process; `unsafe`
is forbidden), logs its arguments, skips everything up to `/bin/sh` and runs
the rest locally, in a scratch directory standing in for the container (its
own `HOME`, `XDG_CACHE_HOME` and a `PATH` without `vix` set *in the
script*). Against it: `conformance::run` passes for all four kinds; each
engine's exact prefix; environment values passed by name and never on argv
(and exported for Kubernetes); the terminal's `-t` and working directory; a
missing container is `NotConnected` while a script's own error is not; the
agent stays off without a `vix` inside, and an upload lands verified (0700)
through `exec -i`; `RemoteTarget::connect` picks the right authority.
`a_real_docker_container_conforms` runs the suite against a real container
only when `VIX_TEST_DOCKER` is set (a running container's name, or `1` to
start and remove a throwaway `alpine`); it passed on 2026-10-30 against
Podman 5.8 behind the `docker` CLI. Unit tests cover the names (round
trips, hostile names). `tui/tests/remote_container.rs` drives the App:
Open Remote with a `docker://` name, and Reopen in Dev Container with a
fake `devcontainer` CLI.

## Kubernetes against a real cluster (T718a)

`conformance::run` passes through `kubectl exec` against a real `kind`
cluster (v0.33, Kubernetes 1.37, a pod of `alpine:3`), with
`VIX_TEST_K8S=namespace/pod/container cargo test -p vix-authority --features
ssh --test container a_real_kubernetes_container_conforms` (it starts and
removes nothing; the cluster and pod are the caller's), and in CI by the
GitHub-only `.github/workflows/k8s.yml`.

What `kubectl exec` printed and exited with (all on stderr, stdout empty),
recorded against that cluster. These are what `ExecTransport::lost` matches
(substring of stderr; `kubectl` exits 1 for all of them, so the status says
nothing and the message decides):

| Situation | Status | stderr |
| --- | --- | --- |
| missing pod | 1 | `Error from server (NotFound): pods "nopod" not found` |
| missing namespace | 1 | `Error from server (NotFound): namespaces "nons" not found` |
| missing container | 1 | `error: container nocontainer is not valid for pod vixpod out of: vixpod` |
| API server stopped | 1 | `The connection to the server 127.0.0.1:62793 was refused - did you specify the right host or port?` |
| no kubeconfig | 1 | `The connection to the server localhost:8080 was refused - ...` |
| unresolvable API host | 1 | `Unable to connect to the server: dial tcp: lookup nosuch.invalid: no such host` |
| unreachable API host, `--request-timeout` | 1 | `Unable to connect to the server: net/http: request canceled while waiting for connection (Client.Timeout exceeded while awaiting headers)` |
| unknown context | 1 | `error: context "nosuchctx" does not exist` |
| script `exit 7` | 7 | `command terminated with exit code 7` (not lost) |
| command not found in the container | 1 | `error: Internal error occurred: ... OCI runtime exec failed: ... exec: "/nonexistent": stat /nonexistent: no such file or directory` (not matched: a script failure) |

A session cut off in flight (the pod deleted, with or without a grace
period, or the node stopped, while an `exec` ran) is **not** reported by
`kubectl` as a connection error: it ends with status 137 (pod deleted) or 143
(node stopped) and only `command terminated with exit code N`, the same as
a script killed from inside. `lost` deliberately does not match those; the
next operation fails with the NotFound / connection refused texts above and
is reported lost then. The first line `kubectl` prints is not stable across
versions or locales, so the match list is best-effort, as for the other
engines.

## Not done

- WSL has only been run against the fake engine (T718b). Kubernetes ran on
  one local `kind` cluster (Podman provider, macOS arm64) and not yet on a
  CI runner, nor on a managed cluster, a non-root container user, or a pod
  with several containers.
- nerdctl has no URL form (its `exec -e NAME` behavior is unverified); the
  OC panel's **Open Workspace** refuses it with a message.
- Kubernetes and WSL environment values travel on argv (no by-name
  option); `kubectl exec`'s argv also reaches the API server's audit log.
- A container without `/bin/sh` (distroless images) can't be opened; there
  is no fallback to `docker cp` for files.
- A dev container's workspace is keyed (session, trust) by its container
  id, so a rebuilt container starts a fresh session (Reopen in Dev
  Container trusts it again). The OC panel opens by container name, which
  survives a recreate.
