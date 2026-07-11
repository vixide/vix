# Split Panes

The editor area can be **split into multiple panes** so several buffers are
visible at once (or several places in different files). Splits **nest**, so you
can build a 2x2 grid (split, then split a pane again) up to four panes. Open it
from **View → Split**:

- **Vertical** — split the focused pane into two side by side.
- **Horizontal** — split the focused pane into two stacked top/bottom.
- **Other Pane** (`F6`) — cycle focus (and editing) to the next pane.
- **Unsplit** — close the focused pane, collapsing its split.

## Behavior

- Panes form a **binary tree**: each leaf shows a tab, each internal node splits
  its area by a direction and ratio. **Vertical**/**Horizontal** split the
  **focused** pane (so repeated splits nest into a grid); the new pane shows the
  next open tab. Capped at four panes.
- The **focused pane shows the active tab**, so all editing, search, and tab
  commands work in it exactly as without a split.
- **Switch tabs** (Next/Previous Tab, Open) and the focused pane follows; the
  other panes keep their tabs.
- **Click** a pane to focus it; **`F6`** (or View → Split → Other Pane) cycles
  focus from the keyboard. First/Last Split focus the first/last pane.
- Each pane has its own vertical scrollbar.
- **Drag any divider** (the `│` or `─` between two panes) to change that split's
  ratio (clamped to 10–90%).
- **Unsplit** removes the focused pane; its sibling takes the freed space, and the
  last remaining pane returns to the unsplit state.

## Limitations

- Up to four panes.
- Showing the *same* buffer in two panes shares one cursor (they are the same
  tab); splits are most useful with different tabs.

## As implemented in Vix

The pane tree lives in `src/pane_tree.rs` (`Pane::Leaf`/`Pane::Split`, with
`layout`, `dividers`, `leaf_at`, `resize_at`, and tree surgery — unit tested).
`Editor` (in `src/editor.rs`) holds `split_root: Option<Pane>` and a
`focused_leaf` index; `set_split`/`unsplit`/`focus_other_pane`/`focus_leaf` mutate
it and `split_layout`/`split_dividers` drive rendering. `src/ui.rs`
`draw_editor_region` lays out every pane and divider (`draw_pane` renders each
tab with its own scrollbar); the host routes pane clicks (`focus_pane_at`),
divider drags (`resize_split_at`), and the `view.split_*` / `view.unsplit` /
`view.focus_other_pane` actions. The layout persists in the session as a
serialized tree (`session::PaneNode`).
