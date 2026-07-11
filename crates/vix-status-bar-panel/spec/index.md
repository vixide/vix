# Status Bar Panel

Formatting for the bottom status bar's two segments.

Pure string logic — the host (the `vix` app) gathers the live data (cursor,
path, dirty flag, keymap mode, language, line ending, selection) and the Nerd
Font glyphs, calls these builders, and renders the resulting strings.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior
