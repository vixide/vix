# Code editor

The center editing area uses `vix-editor`, Vix's fully-custom code-editor widget.
Its Tree-sitter highlighting + buffer + undo/redo *engine* was adapted from
[`ratatui-code-editor`](https://crates.io/crates/ratatui-code-editor); the widget
itself (editor state, input, mouse, and the soft-wrap renderer) is written
in-house.

The widget:

- Gates each Tree-sitter grammar behind a Cargo feature so the binary links only
  the parsers it needs.
- Is theme-aware: the host configures the text, line-number, selection, and
  cursor styles, and may set a syntax color palette. Token colors appear only
  when the active theme defines a `syntax` block.
- Renders a visible block cursor at the caret.
- Can render **visible whitespace** glyphs (space `·`, tab `→`, carriage return
  `␍`, line ending `¶`) in a configurable dim style; toggled by the host via
  **View → Editor → Show/Hide Whitespace**.
- Accepts a host-configured **indent string** (overriding its per-language
  default), so Tab inserts spaces or a tab per the `indent_style` / `tab_width`
  settings (see `docs/configuration.md`).

Capabilities: Tree-sitter syntax highlighting, undo/redo history, text selection,
system clipboard, built-in mouse handling (click, drag-select, wheel-scroll),
**line-comment toggling** (the cursor line or every line in the selection, using
the language's comment token — `//`, `#`, `--` — as one undoable edit), and
**bracket matching** (the partner of the bracket at/just before the cursor is
highlighted; no pair auto-insertion). It also exposes its language, line ending
(LF/CRLF), and selection span for the host's status bar.

Editing comforts (each a single undoable edit): **select all** (`Ctrl+A`),
**duplicate line** (`Ctrl+D`), **delete line** (`Ctrl+K`), **move line up/down**
(`Alt+↑`/`Alt+↓`), **jump to the matching bracket** (`Ctrl+]`), and **auto-indent
on Enter** (the new line carries the previous line's leading whitespace).

## Soft wrap

**View → Editor → Show/Hide Soft Wrap** (the `soft_wrap` setting) wraps long logical lines
across several screen rows instead of scrolling horizontally. A shared
visual-row layout drives the renderer, cursor positioning, vertical scroll, and
mouse hit-testing.

## Roadmap

- **Display tab width.** Literal tabs render as a single column; rendering them as
  `tab_width` columns needs matching changes in the render and the grapheme-width
  helpers.
