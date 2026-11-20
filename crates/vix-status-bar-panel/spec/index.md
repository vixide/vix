# Status Bar Panel

Formatting for the bottom status bar's two segments.

Pure string logic — the host (the `vix` app) gathers the live data (cursor,
path, dirty flag, keymap mode, language, line ending, selection) and the Nerd
Font glyphs, calls these builders, and renders the resulting strings.

## Narrow bars

The left segment is `" {mode}{path}{dirty}  —  {status}"`; the right one
holds the remote, LSP-progress, Cowork, Jupyter, git, project-environment and
IM segments, editor info, then `Ln {line}:Col {col}` and the calendar glyph.
When both do not fit the bar's width, the host first rebuilds the right one
without the segments after git (project environment, IM, editor info), which
keeps the git segment's click rectangle in place. The left segment then gets
what is left through `fit_left_segment`: the full text if it fits; else with
the path cut to its file name; else without the path, so the status message
stays readable; and only then cut with `…`. With no message the path is
only ever cut, never dropped.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior
