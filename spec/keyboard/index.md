# Keyboard Navigation

**Status:** Partly shipped. The keyboard-shortcut overlay (**Help → Shortcuts**,
`F1`) is now a **searchable browser**: just type to filter the list. For the full
set of shortcuts Vix supports, see `docs/keybindings/index.md`.

## Searchable shortcut browser (shipped)

Open with **Help → Shortcuts** or `F1`. A search field sits at the top:

- **Type** to filter the rows live by key combo or (translated) description;
  matching is case-insensitive and substring-based.
- **Backspace** edits the query; an empty query shows every shortcut.
- **Esc** / **F1** close the overlay (and clear the filter).

Rendering filters `keyboard_shortcut_panel::ROWS` against `App::help_filter`.

## Roadmap

- **Key-recording lookup**: press a key combination and see which binding(s) it
  triggers (reverse lookup), with a Tab toggle between text and key-record modes.
- Scrolling/Home/End navigation when the filtered list overflows the overlay.
- Including plugin-registered command names once a plugin system exists.
