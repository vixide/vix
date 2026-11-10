# Overlay priority

**Status:** Shipped (T611, step 1; step 2 complete: the choosers, info panels and edit surfaces). One table, `OVERLAYS`, orders every modal
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
- The Git changes panel (`GitPanel`) sits below `Prompt` and `Confirm`,
  beside the rebase overlay (T792). Its Discard (`d`) and smart-commit (`c`
  with nothing staged, T577) confirmations open while the panel stays up.
  T611 first put the panel above `Confirm`, its old drawing height, so the
  confirmation was drawn under the panel and `y`/`n`/Esc went to the panel:
  Discard never discarded and the smart-commit question could not be
  declined. Before T611 the key chain had happened to offer `Confirm` first.

## Step 2: the `Overlay` enum (choosers)

Step 2 replaces loose `Option<_>` fields with one enum for the modals that are
mutually exclusive, so that two of them cannot be open at once at all. It is
done in slices. Slice 1 converted the **list choosers**; slice 2 converted the **info panels**; slice 3 converted the **edit surfaces** (below).

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

### Slice 2: the info panels

`App::info: Option<InfoPanel>` replaces seven fields (`system_info`,
`lsp_status`, `file_info`, `text_info`, `media_type_panel`, `nerd_palette`,
`ascii_panel`); `InfoPanel` has one variant per panel, in
`tui/src/app/info_panel.rs`, with typed access through the `InfoSlot` trait on
`Option<InfoPanel>` (`nerd_palette()`, `nerd_palette_mut()`,
`open_nerd_palette(p)`, `close_nerd_palette()`, and the same for each panel).
Behavior:

- Opening an info panel **replaces** any other info panel. Previously a second
  one could be opened beneath or beside the first and the first reappeared when
  the second closed. These panels are opened from the menu bar or palette,
  which closes first, so no supported flow had two open; the change removes the
  impossible state. (Observable only through the tests: opening, say, the ASCII
  table while the Nerd palette is open now leaves only the ASCII table.)
- `close_*` closes a panel only if it is the open one, so a stale close never
  dismisses a different panel.
- Each panel keeps its own row in `OVERLAYS` (same height, key handler and
  draw function); a row is open when the slot holds its variant. Key, mouse
  and draw routing are unchanged.
- The panels' layout rectangles (`Layout::nerd_palette`, ...) are unchanged.

### Slice 3: the edit surfaces

`App::edit: Option<EditSurface>` replaces six fields (`edit_table`,
`edit_outline`, `edit_value`, `edit_bytes`, `edit_sql`, `column_view`);
`EditSurface` has one variant per editor, in `tui/src/app/edit_surface.rs`,
with typed access through the `EditSlot` trait on `Option<EditSurface>`
(`edit_table()`, `edit_table_mut()`, `open_edit_table(e)`,
`close_edit_table()`, and the same for each editor). Behavior:

- Opening an edit surface **replaces** any other edit surface. Each is opened
  from the menu bar or palette, which closes first, so no supported flow had
  two open; the change removes the impossible state.
- `close_*` closes an editor only if it is the open one.
- Each editor keeps its own row in `OVERLAYS` (same height, key handler and
  draw function); a row is open when the slot holds its variant. Key, mouse
  and draw routing and the layout rectangles are unchanged. The byte editor's
  `edit_bytes_file` (the file it edits directly) stays a separate `App` field.

With this slice step 2 is complete. The stacking confirmations (`confirm`,
`prompt`, `unsaved`, ...) are not candidates: they legitimately open over
another layer (a reword prompt over the rebase overlay, a confirmation over
workspace search).

## Tests

`tui/src/app/overlays.rs` (`tests`): row index equals id; drawing order and key
offer order are the same open rows in opposite directions; opening the
palette, help and welcome panels together and pressing `Esc` closes them
top-down (so the topmost drawn layer really takes the key); opening one
chooser while another is open replaces it (only one `Chooser` row is ever open); opening one info panel while another is open replaces it (only one `InfoPanel` row is ever open), and closing a panel that is not the open one is a no-op; opening one edit surface while another is open replaces it (only one `EditSurface` row is ever open), and closing an editor that is not the open one is a no-op; the set of
draw-only layers is exactly which-key, completion and hover. The existing
integration and snapshot suites cover the unchanged single-overlay behavior.

## Code

`tui/src/app/overlays.rs` (table, `route_overlay_key`,
`overlay_capturing_keys`), `tui/src/ui.rs` (`draw_overlays`, `draw_overlay*`),
`tui/src/app/keymap.rs` (the handlers that used to be inline: `help_key`,
`dialog_key`, `clock_key`, `dashboard_key`, `qrcode_key`).
