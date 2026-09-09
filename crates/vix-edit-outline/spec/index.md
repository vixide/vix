# Outline Editor

**Status:** Shipped. The outline editor: hierarchical hide/show and
restructuring for prose text.

Vix's Tools menu offers an *Edit Outline* command that reads the active
buffer as an indented outline — each line is an item whose **level** is its
indentation depth (tabs, or two spaces per level). It behaves like code
folding, but for prose, and like a file explorer's tree: collapse an item to
hide its descendants, navigate item to item, re-indent items to change the
hierarchy, and move items (with their subtrees) up and down.

This crate is self-contained and host-agnostic: it owns the items, the
cursor, the collapse state, and an undo/redo history, and it interprets key
events itself (returning an `Outcome` telling the host when to close or
save). The host (`App` in the root `vix` crate) renders the visible items,
syncs the scroll window, and persists saves.

## Keys (the host routes them here)

- **↑ / ↓** (or `k` / `j`): move to the previous / next visible item.
- **← / →** (or `h` / `l`): close / open (collapse / expand; `←` on a leaf
  jumps to the parent, `→` on an expanded item jumps to the first child).
- **Tab / Shift+Tab** (or **Alt+→ / Alt+←**): indent / outdent the item.
- **Alt+↑ / Alt+↓**: move the item (with its subtree) up / down. Terminals
  cannot send Tab+arrow, so these "tab-up/tab-down" moves use Alt+arrows.
- **Space**: toggle collapse. **Ctrl+S**: save. **u** / **Ctrl+R**:
  undo / redo. **Esc** / **q**: close.

## Contents (`vix_edit_outline`)

- `Tree` — the outline state: items (in document order), the selected
  index, scroll position, tab-vs-space indentation (detected from the
  parsed text), dirty flag, and undo/redo history (capped at 200 steps).
  `from_text`/`to_text` round-trip the buffer; `visible()` gives the
  display-order indices with anything hidden under a collapsed ancestor
  omitted; `handle_key` is the single entry point for input.
- `Outcome` — what the host does after a key: `Consumed` (nothing further),
  `Close` (Esc/`q`), or `Save` (Ctrl+S — the host persists the buffer).

## Roadmap

- Nothing open.
