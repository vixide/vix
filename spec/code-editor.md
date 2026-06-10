# Code editor

The center editing area uses `vix-code-editor-panel`, an internal fork of
[`ratatui-code-editor`](https://crates.io/crates/ratatui-code-editor).

The fork:

- Gates each Tree-sitter grammar behind a Cargo feature so the binary links only
  the parsers it needs.
- Is theme-aware: the host configures the text, line-number, selection, and
  cursor styles, and may set a syntax color palette. The built-in themes are
  monochrome (no token colors); colors appear only under a custom theme.
- Renders a visible block cursor at the caret.
- Can render **visible whitespace** glyphs (space `·`, tab `→`, carriage return
  `␍`, line ending `¶`) in a configurable dim style; toggled by the host via
  **View → Toggle Editor Visible Whitespace**.

Capabilities: Tree-sitter syntax highlighting, undo/redo history, text selection,
system clipboard, and built-in mouse handling (click, drag-select, wheel-scroll).
