# Markdown Preview

A read-only, formatted preview of the Markdown file you're editing —
headings get an underline rule, lists get bullets/numbers, block quotes a
`│` rail, code blocks stay verbatim, and links render as `text (url)`.

## Using it

**Tools → Markdown Preview** opens it, scrolled to wherever your cursor
was in the source — jump to the paragraph you were writing and preview
opens right there instead of at the top.

- `↑` / `↓` / `PageUp` / `PageDown` scroll.
- `t` opens the **table of contents**: every heading, jump to one with
  `Enter` (back to the preview, scrolled there), close it alone with `Esc`
  (leaving the preview open). A document with no headings makes `t` a
  no-op.
- `Esc` or `q` closes the preview.

See the specification at `crates/vix-markdown-preview/spec/index.md`. For
inserting Markdown boilerplate (headings, links, tables, todo lists) rather
than previewing, see [Insert](../insert/index.md).

---

Vix™ and Vix IDE™ are trademarks.
