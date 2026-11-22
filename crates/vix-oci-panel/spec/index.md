# OC panel

The OC menu's list panel: containers, images, volumes, networks and pods with per-row actions, confirm and input modes, built on vix-oci.

**Status:** Shipped (improvement plan T702-T704, pods from T708). Design:
[`spec/open-container`](../../../spec/open-container/index.md). Engine access
is [`vix-oci`](../../vix-oci/spec/index.md).

## Shape

`Panel` is pure state; it never starts a process. `Panel::key(key, page)`
returns a `Request` for the App to carry out: `Reload`, `Inspect`, `Logs`,
`Shell`, `OpenWorkspace`, `PortUrl(port)`, `Exec(Pending)`, `Close`, `ReadOnly`, `Invalid`, or
`None`. One panel holds all five lists and a `kind`; `Tab` / `Shift+Tab`
cycle `Container`, `Image`, `Volume`, `Network` and (only when
`Engine::has_pods`) `Pod`.

`Pending` is a fully built argv plus the `rendered` command line and a
`stream` flag (dock instead of status line).

## Keys

- All lists: arrows, PageUp/PageDown, `Tab`/`Shift+Tab` kind, `r` reload,
  `Enter`/`i` inspect, `d`/`Delete` remove (confirm), `Esc`/`q` close.
- Containers: `s` start, `x` stop, `t` restart, `p` pause or unpause, `l`
  logs, `e` shell (running only), `w` open a workspace in it (running
  only; T718, [`spec/remote-editing`](../../../spec/remote-editing/index.md)),
  `o` open the first published host port,
  `a` toggle stopped containers.
- Images: `u` run (prompt, then confirm), `g` tag, `p` pull (prompt, then
  confirm), `P` push (confirm).

## Safety

- Every `rm`, run, build, pull and push shows its exact rendered command in
  `Mode::Confirm`; only `y` or `Enter` runs it. Start, stop, restart, pause,
  unpause and tag run at once.
- Read-only mode (`oc_mode = "read-only"`): every key above that would change
  engine state answers `Request::ReadOnly` before any command is built.
  Listing, inspect, logs, shell, open-workspace and open-port remain.
- Input is checked by `vix_oci::argv::check`; a refused value is
  `Request::Invalid` and nothing runs. Run takes `IMAGE [name=N] [port=H:C]...`
  and is always detached; Build takes `FILE [tag=REF] [context=DIR]`.
- `compose_pending` and `prune_pending` build the Compose and prune commands
  as confirm-first `Pending`s for the menu entries that skip the list.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Image id display, row selection and kind cycling use `get` and `checked_*`; the old `saturating_sub` is gone and the row text no longer relies on `unwrap_or_default`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `OCI-F1` | An image id is shorter than 19 bytes, or not on a character boundary there | the whole id | The id is only shown, so showing all of it is the safe reading. |
| `OCI-F2` | There is no row at the highlighted index | the empty text | No row means nothing to show. |
| `OCI-F3` | `len - 1` on an empty list | row `0` | An empty list has no row to select. |
| `OCI-F4` | The kind list is empty or a step falls off an end | wrap to the other end; an empty list keeps the kind | Kinds cycle. |
