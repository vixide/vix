# Tools: Insert: Markdown

Render Markdown to formatted plain-text lines for a read-only preview pane.

Also, this crate inserts small Markdown templates at the cursor
(`App::insert_markdown`).

## Preview: scroll-sync and table of contents (T206)

`Panel::open` scroll-syncs the preview to wherever the cursor was in the
source buffer: `render_full` (the version `Panel::open` actually calls, vs.
the plain `render` used by the template-insert callers and most tests) also
returns `source_lines`, each display line's originating 1-based source
line, and `toc`, the document's headings as `vix_outline_panel::Entry` rows
(`kind` the `#`-run for the level, `name` the heading text, `line` its
1-based row *in the preview*, not the source — this reuses the same
`Entry`/`Outline` type the source-file outline panel uses, just pointed at
different data). `Panel::sync_to_source_line(line)` (source line → nearest
preview line, preferring the first display line at that source line over a
trailing blank separator that happens to share it) drives the on-open sync;
`Panel::scroll_to_line(line)` (a direct preview-line jump) drives the TOC.

In the host (`App`), opening the preview (`tools.markdown_preview`) captures
the cursor's source line first, then syncs to it. While the preview is
open, `t`/`T` opens the TOC as a second overlay on top of it (the same
"overlay over an overlay" shape as the theme editor's X11 color picker);
`↑`/`↓`/`PageUp`/`PageDown` navigate it, `Enter` scrolls the preview to the
selected heading and closes the TOC (back to the preview, not the source
buffer), `Esc` closes just the TOC. A document with no headings makes `t` a
no-op with a status message.

- menu "Tools"
  - submenu "Insert"
    - submenu "Markdown"
      - menuitem "Headline 1" -> insert `# Headline 1` followed by a blank line.
      - menuitem "Headline 2" -> insert `## Headline 2` followed by a blank line.
      - menuitem "Headline 3" -> insert `### Headline 3` followed by a blank line.
      - menuitem "Link" -> insert `[Example](https://www.example.com)`.
      - menuitem "List" -> insert a three-item `- Item` bullet list.
      - menuitem "Table" -> insert a three-column header/separator/rows table.
      - menuitem "Todos" -> insert a three-item `- [ ] Todo` checklist.
