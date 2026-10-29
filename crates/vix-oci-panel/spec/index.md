# OC panel

The OC menu's list panel: containers, images, volumes, networks and pods with per-row actions, confirm and input modes, built on vix-oci.

**Status:** Shipped (improvement plan T702-T704, pods from T708). Design:
[`spec/open-container`](../../../spec/open-container/index.md). Engine access
is [`vix-oci`](../../vix-oci/spec/index.md).

## Shape

`Panel` is pure state; it never starts a process. `Panel::key(key, page)`
returns a `Request` for the App to carry out: `Reload`, `Inspect`, `Logs`,
`Shell`, `OpenPort`, `Exec(Pending)`, `Close`, `ReadOnly`, `Invalid`, or
`None`. One panel holds all five lists and a `kind`; `Tab` / `Shift+Tab`
cycle `Container`, `Image`, `Volume`, `Network` and (only when
`Engine::has_pods`) `Pod`.

`Pending` is a fully built argv plus the `rendered` command line and a
`stream` flag (dock instead of status line).

## Keys

- All lists: arrows, PageUp/PageDown, `Tab`/`Shift+Tab` kind, `r` reload,
  `Enter`/`i` inspect, `d`/`Delete` remove (confirm), `Esc`/`q` close.
- Containers: `s` start, `x` stop, `t` restart, `p` pause or unpause, `l`
  logs, `e` shell (running only), `o` open the first published host port,
  `a` toggle stopped containers.
- Images: `u` run (prompt, then confirm), `g` tag, `p` pull (prompt, then
  confirm), `P` push (confirm).

## Safety

- Every `rm`, run, build, pull and push shows its exact rendered command in
  `Mode::Confirm`; only `y` or `Enter` runs it. Start, stop, restart, pause,
  unpause and tag run at once.
- Read-only mode (`oc_mode = "read-only"`): every key above that would change
  engine state answers `Request::ReadOnly` before any command is built.
  Listing, inspect, logs, shell and open-port remain.
- Input is checked by `vix_oci::argv::check`; a refused value is
  `Request::Invalid` and nothing runs. Run takes `IMAGE [name=N] [port=H:C]...`
  and is always detached; Build takes `FILE [tag=REF] [context=DIR]`.
- `compose_pending` and `prune_pending` build the Compose and prune commands
  as confirm-first `Pending`s for the menu entries that skip the list.
