# Status bar panel

The bottom status bar shows the file path and dirty flag, the latest status
message, the keyway mode (Vim/Emacs), and — on the right — the language, line
ending, encoding, selection size, and cursor line:column.

**Status:** Shipped. The *formatting* of the two segments lives in the internal
`vix-status-bar-panel` crate; the host (`src/ui.rs`) gathers the live data and the
Nerd Font glyphs, calls the builders, and renders the strings. Pure string logic,
no dependencies.

## API (`vix-status-bar-panel`)

- `left_segment(mode, path, dirty, status)` → `" {mode}{path}{dirty}  —  {status}"`.
  `mode` is the keyway indicator (with trailing spacing) or empty; `dirty` is the
  unsaved-buffer glyph (with leading space) or empty.
- `info_segment(language, line_ending, selection)` →
  `"{language}  {line_ending}  UTF-8   {selection}"`. `language` is `None` for a
  non-text tab (empty result); `selection` is `(chars, lines)` rendered as
  `"Sel {chars} ({lines}L)   "`.
- `right_segment(info, line, col, calendar)` →
  `"{info}Ln {line}:Col {col}   {calendar} "`.

The bar itself is toggled with **View → Show/Hide Bottom Status**
(`show_status_bar`), and has a full-width top border separating it from the body.
