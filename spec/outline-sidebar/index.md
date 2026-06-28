# Outline Sidebar

A persistent code-outline dock (distinct from the modal Outline overlay) that
stays open beside the editor and follows the cursor. Toggled by **View → Layout →
Outline Sidebar** (action `view.outline_dock`); persisted in `show_outline_dock`
with width `outline_width`.

## Behavior

- Docks on the right of the editor (after the message drawer).
- Lists the active buffer's symbols using the same scanner as go-to-symbol and the
  modal outline (`palette::symbols`): a kind prefix (`fn`, `struct`, …) plus name.
- **Follows the cursor**: the highlight tracks the symbol the cursor is inside
  (`Outline::select_nearest`), and the list rescans when the buffer changes.
- **Click** a row to jump to that symbol.
- Empty (no recognized symbols) shows the "outline empty" hint.

## As implemented in Vix

`App::refresh_outline_dock` (called each event loop) rebuilds `outline_dock` only
when the active tab or its revision changes (cached in `outline_dock_key`), then
re-selects the nearest symbol. `ui::draw_outline_dock` renders the dock and records
`layout.outline_dock`; clicks route through `App::outline_dock_click`. Reuses the
`outline_panel::Outline`/`Entry` types from the modal
[outline panel](../../docs/outline-panel/index.md).
