# Split Panes

The editor area can be **split into two panes** so two buffers are visible at
once (or two places in different files). Open it from **View → Split**:

- **Vertical** — two panes side by side.
- **Horizontal** — two panes stacked top/bottom.
- **Other Pane** (`F6`) — move focus (and editing) to the other pane.
- **Unsplit** — return to a single pane.

## Behavior

- The **focused pane shows the active tab**; the other pane shows a second tab
  (the next open tab when there is one, otherwise the same buffer). Because the
  focused pane is just the active tab, all editing, search, and tab commands work
  in it exactly as without a split.
- **Switch tabs** (Next/Previous Tab, Open) and the focused pane follows; the
  other pane keeps its tab.
- **Click** a pane to focus it; **`F6`** (or View → Split → Other Pane) toggles
  focus from the keyboard.
- Each pane has its own vertical scrollbar.
- **Drag the divider** (the `│` or `─` between the panes) to change the split
  ratio (clamped to 10–90%).

## Limitations

- Exactly one split (two panes), one level deep.
- Showing the *same* buffer in both panes shares one cursor (they are the same
  tab); split is most useful with two different tabs.

## As implemented in Vix

`Editor` (in `src/editor.rs`) holds an optional `Split { dir, other,
focused_side, ratio }`; `split_pane_tabs()` returns the (left, right) tab
indices. `src/ui.rs` `draw_editor_region` lays out the two panes and the divider
(`draw_pane` renders each tab with its own scrollbar); the host routes pane
clicks (`focus_split_pane`), divider drags (`resize_split`), and the
`view.split_*` / `view.unsplit` / `view.focus_other_pane` actions.
