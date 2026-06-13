# Nerd Font picker

A character picker for [Nerd Font](https://www.nerdfonts.com/) glyphs. The user
opens it from the **Tools** menu → **Nerd Font Characters…**, browses a grid of icon
glyphs, and inserts one into the active editor.

## Behavior

- A small overlay shows a fixed-width grid of curated glyphs, the highlighted
  glyph's name, and a key hint.
- **Arrow keys** move the highlight within the grid (`←` `↑` `↓` `→`), clamping at
  the edges.
- **Enter** inserts the highlighted glyph at the editor cursor and **keeps the
  picker open**, so several glyphs can be picked in a row.
- **Mouse click** on a cell highlights and inserts that glyph (also leaving the
  picker open).
- **Esc** closes the picker.
- Inserting into an image tab (or when there is no editable buffer) is a no-op.

The glyphs come from the common Nerd Font ranges (Font Awesome, Devicons,
Powerline, Octicons) that virtually every patched font ships, so the picker is
useful regardless of which Nerd Font the terminal uses. A glyph a particular font
lacks simply renders as a fallback box; nothing breaks.

## As implemented in Vix

**Status:** Shipped. The glyph set and grid-selection state live in the internal
`vix-nerd-font-picker` crate (pure data, no dependencies). The host (`src/app.rs`,
`src/ui.rs`) opens the overlay on the `tools.nerd_palette` action, renders the
grid, maps clicks to cells, and inserts the chosen glyph via the editor's
`insert_str`. The grid is `8` columns wide; navigation and the mouse hit-test
share that width and the per-cell column width (`ui::NERD_CELL_W`).
