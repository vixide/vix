# Snippets

**Tools → Snippets…** (action `tools.snippets`) lists the bundled snippets; the
chosen one is inserted at the cursor. Snippets may contain **tabstops** that
become navigable fields.

## Tabstops

A snippet body may contain:

- `$1`, `$2`, … — empty tabstops, visited in ascending order.
- `${1:placeholder}` — a tabstop pre-filled with `placeholder` (selected when
  reached, so typing replaces it).
- `$0` — the final cursor position (visited last).
- `\$` — a literal dollar sign.

On insert, the markers are removed, placeholders are kept, the cursor jumps to the
first tabstop (selecting its placeholder), and a session is armed:

- **Tab** advances to the next tabstop. The text the user typed at the current
  field shifts the remaining tabstops by its net length change.
- **Esc** ends the session (Tab then indents as usual).
- The session ends after the last tabstop (`$0`, or the highest-numbered one).

A snippet with no tabstops (or only `$0`) is a plain insert / caret placement.

## As implemented in Vix

`snippet_tool::parse` turns a body into `Parsed { text, stops }` (unit tested).
The host owns `SnippetSession`, `insert_snippet_body`, `snippet_goto`, and
`snippet_tab`; `editor_key` intercepts Tab/Esc while a session is active.
