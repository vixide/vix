# Overlay priority

**Status:** Shipped (T611, step 1; step 2 in part: the choosers). One table, `OVERLAYS`, orders every modal
layer from the bottom of the stack to the top. The frame draws the open rows in
table order and a key is offered to the open rows in the reverse order, so the
layer drawn on top is the layer that receives keys.

## Why

Before T611 the stacking order lived in two hand-maintained places that could
drift apart: a cascade of `is_some()` checks that drew the layers
(`tui/src/ui.rs`), and a separately ordered chain that routed keys
(`tui/src/app/keymap.rs`), plus a third copy ("keep this list in step") used to
decide whether a bracketed paste belongs to a layer. Nothing checked that the
topmost drawn layer was the one that got keys.

## The table

`tui/src/app/overlays.rs` holds `OVERLAYS: &[Overlay]`. Each `Overlay` row has:

| Field  | Meaning                                                                 |
|--------|-------------------------------------------------------------------------|
| `id`   | An `OverlayId` variant; its discriminant is the row's index (tested).   |
| `open` | `fn(&App) -> bool`: whether the layer is currently up.                  |
| `key`  | `Key::Handler(fn)`, `Key::Early`, or `Key::None` (see below).           |

| `Key` policy | Meaning                                                                         |
|--------------|---------------------------------------------------------------------------------|
| `Handler`    | The layer's key handler. It returns `false` to let the key fall through (only the integrated terminal does, for its own chords). |
| `Early`      | Jump-label mode: `App::dispatch_key` offers it the key before the table, because a miss falls through to normal handling. It still counts as capturing keys. |
| `None`       | Drawn only: the which-key popup, the LSP completion popup (its key handler runs in `dispatch_key`, which also dismisses it on any other key), and the hover tooltip (dismissed by the next key). |

Three readers, one table:

- **Drawing** - `ui::draw_overlays` walks `App::open_overlays()` forwards and
  `ui::draw_overlay` maps each `OverlayId` to its `draw_*` function. Rendering
  stays in `tui/src/ui*`.
- **Key routing** - `App::route_overlay_key` walks the same iterator in
  reverse, calling each `Handler` until one returns `true`.
- **Paste routing** - `App::overlay_capturing_keys` is "any open row that is not
  `Key::None`".

Adding an overlay is: add its row at the height it should stack, add its
`OverlayId` variant (the enum order is the table order), and add its arm to
`ui::draw_overlay`. A layer with no key policy does not compile.

## Order decisions

The table follows the former *drawing* order, because that is what the user
sees. Where the old key chain disagreed, the key order now follows the picture;
this only matters when two layers are open at once. The cases that moved:

- The calendar, clock and menu are now below search/prompt/palette and the
  panels, matching how they are drawn (previously the date/time boxes took keys
  ahead of every panel).
- Between pairs of choosers, confirmations and tool panels that can in
  principle both be open, the one drawn later now wins the key (for example a
  confirmation is drawn over the workspace-search overlay, so it now takes the
  key; the old chain offered it to the search first). The tool dialogs (color
  and unit converter, calculator, regex tester, code actions) were an
  if/else-if chain and are now stacked in that same order.
- A host's Cowork share confirmation is its own row (`CoworkConfirm`) just
  above the shared terminal, so it still answers before the terminal.

## Step 2: the `Overlay` enum (choosers)

Step 2 replaces loose `Option<_>` fields with one enum for the modals that are
mutually exclusive, so that two of them cannot be open at once at all. It is
done in slices; this slice converts the **list choosers**.

`App::chooser: Option<Chooser>` replaces fifteen fields (`branch_chooser`,
`stash_chooser`, `worktree_chooser`, `task_chooser`, `script_chooser`,
`plugin_chooser`, `location_chooser`, `capture_chooser`, `refile_chooser`,
`recent_chooser`, `encoding_chooser`, `macro_chooser`, `clipboard_chooser`,
`workspace_chooser`, `http_env_chooser`). `Chooser` has one variant per
picker, holding that picker's state. Behavior:

- Opening a chooser **replaces** any other chooser (previously the old one
  stayed in its field, hidden behind or beside the new one, and reappeared
  when the new one closed). The choosers are single-purpose list pickers
  reached from the menu bar or palette, which closes first, so no supported
  flow had two open; the change only removes the impossible state.
- Each picker still has its own row in `OVERLAYS` (same stacking height, key
  handler and draw function as before); a row's `open` test is "the slot
  holds this variant". Drawing, key routing, mouse routing and paste routing
  are unchanged.
- `Chooser`'s accessors (`branch`, `branch_mut`, `take_branch`, ...) come from
  the `ChooserSlot` trait on `Option<Chooser>` so call sites borrow only the
  `chooser` field, as the per-picker fields did.

**Remaining (later slices):** the other modals still use their own `Option`
fields. Candidates for the next slices, in rough order of fit: the read-only
info panels (`system_info`, `lsp_status`, `file_info`, `text_info`,
`media_type_panel`, `nerd_palette`, `ascii_panel`), then the edit surfaces
(`edit_table`, `edit_outline`, `edit_value`, `edit_bytes`, `edit_sql`,
`column_view`). The stacking confirmations (`confirm`, `prompt`, `unsaved`,
...) are not candidates: they legitimately open over another layer (a reword
prompt over the rebase overlay, a confirmation over workspace search).

## Tests

`tui/src/app/overlays.rs` (`tests`): row index equals id; drawing order and key
offer order are the same open rows in opposite directions; opening the
palette, help and welcome panels together and pressing `Esc` closes them
top-down (so the topmost drawn layer really takes the key); opening one
chooser while another is open replaces it (only one `Chooser` row is ever open); the set of
draw-only layers is exactly which-key, completion and hover. The existing
integration and snapshot suites cover the unchanged single-overlay behavior.

## Code

`tui/src/app/overlays.rs` (table, `route_overlay_key`,
`overlay_capturing_keys`), `tui/src/ui.rs` (`draw_overlays`, `draw_overlay*`),
`tui/src/app/keymap.rs` (the handlers that used to be inline: `help_key`,
`dialog_key`, `clock_key`, `dashboard_key`, `qrcode_key`).
