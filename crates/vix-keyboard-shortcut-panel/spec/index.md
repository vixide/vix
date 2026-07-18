# Keyboard Shortcuts

**Status:** Shipped. The keyboard-shortcut overlay (**Help → Keyboard
Shortcuts…**, `F1`) is a **searchable, sortable table of every active
shortcut**, in two columns:

- **Action** — the translated action name (e.g. "Command Palette");
- **Shortcut** — the key combo, shown verbatim (e.g. `Ctrl P`).

## Contents (all active shortcuts)

The host assembles the rows from every active source, deduplicated on
(action, keys) with the first source winning:

1. the curated global rows (`ROWS` — key combo + i18n description);
2. **every menu-item accelerator**, walking all menus and submenus (the
   action name is the item's translated label);
3. the **active keymap's chord tables** — the Spacemacs `SPC` leader map
   (shown as `SPC f f`, …) when the keymap is `spacemacs`, and the Emacs
   `Ctrl X` map (shown as `Ctrl X Ctrl F`, …) when it is `emacs`. An action
   with no menu item shows its action id, as the which-key popup does.

## Interaction

- **Type** to filter rows live, case-insensitively, against both columns.
- **Backspace** edits the query; an empty query shows every shortcut.
- **Click a column header** to sort that column ascending; **click it again**
  to flip to descending. Clicking the other header starts ascending there.
  Until a header is clicked the rows keep their natural (source) order.
- **↑/↓**, **PgUp/PgDn**, and the mouse wheel scroll the list.
- **Esc** / **F1** close the overlay.

## Module (`crate::keyboard_shortcut_panel`)

- `Row` / `ROWS` — the curated static rows (combo + i18n description key).
- `Shortcut` — one assembled row: translated `action` + verbatim `keys`.
- `Column` — `Action | Keys`.
- `Panel` — the overlay state: `rows`, `query`, `sort`
  (`None` = natural order, else column + ascending flag), `scroll`.
  `matches()` filters and orders; `toggle_sort(col)` implements the header
  click cycle; `clamp_scroll(view_h)` keeps the scroll in range.

## Roadmap

- **Key-recording lookup**: press a key combination and see which binding(s)
  it triggers (reverse lookup).
- Vi/modal per-mode motion tables (currently code-driven, so not listed).
- Including plugin-registered command names once a plugin system exists.
