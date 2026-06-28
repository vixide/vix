# Snippets

Reusable text templates inserted at the cursor, with **tabstops** for navigable
fields. Vix's snippets draw on the TextMate
(<https://macromates.com/textmate/manual/snippets>) and VS Code
(<https://code.visualstudio.com/docs/editing/userdefinedsnippets>) models, but the
snippet **files are JSON, not XML**, and two TextMate features are intentionally
**omitted**: interpolated shell code (`` `…` ``) and `\u` case-folding escapes.

There are two ways to use a snippet:

1. **Picker** — **Tools → Snippets…** (`tools.snippets`) lists every in-scope
   snippet; type to filter by name, prefix, or description; Enter inserts it.
2. **Prefix expansion** — type a snippet's **prefix** then press **Tab**; the word
   is replaced by the snippet body (and a tabstop session begins).

## File format (JSON)

A snippet file is a JSON object whose **keys are snippet names** and whose values
describe each snippet — the VS Code shape:

```json
{
  "Function": {
    "prefix": "fn",
    "body": [
      "fn ${1:name}(${2}) -> ${3:()} {",
      "\t$0",
      "}"
    ],
    "description": "A Rust function"
  },
  "Print": {
    "prefix": ["pr", "println"],
    "body": "println!(\"${1:{}}\", ${2:value});$0"
  }
}
```

- **`prefix`** — a string, or an array of strings. Typing a prefix and pressing
  Tab expands the snippet. Optional (a snippet with no prefix is picker-only).
- **`body`** — a string, or an array of strings joined with newlines. Required.
- **`description`** — optional human description, shown in the picker.

Unknown fields (e.g. VS Code's `scope`) are ignored. A malformed file is skipped
with a warning rather than aborting; well-formed snippets in other files still
load.

## Tabstops (body syntax)

The body uses the TextMate/VS Code tabstop syntax:

- `$1`, `$2`, … — empty tabstops, visited in ascending order on Tab.
- `${1:placeholder}` — a tabstop pre-filled with `placeholder` (selected when
  reached, so typing replaces it).
- `$0` — the final cursor position (visited last).
- `\$` — a literal dollar sign.
- `\t` / `\n` in JSON strings are a literal tab / newline in the body.

Interpolated shell code and `\u` escapes are **not** supported.

## Scopes and file locations

Snippets are gathered from four sources and merged (later sources add to, and may
shadow by name, earlier ones):

| Scope | Location | Applies to |
| ----- | -------- | ---------- |
| Bundled | built into Vix | always |
| Global | `<config>/global/snippets/snippets.json` | always |
| Media-type | `<config>/media-types/<type>/<subtype>/snippets/snippets.json` | buffers of that media type |
| Project | `<project root>/<project_snippets>` (default `config/snippets/snippets.json`) | the open project |

`<config>` is Vix's config directory (e.g. `~/.config/vix/`). The media-type
segment is the buffer's media type (see [media-types](../media-types/index.md));
for example a Rust source file resolves to
`media-types/text/rust/snippets/snippets.json`. Vix also accepts the
`x-`-prefixed form (`text/x-rust`), so either spelling works.

Examples:

- `~/.config/vix/global/snippets/snippets.json`
- `~/.config/vix/media-types/text/plain/snippets/snippets.json`
- `~/.config/vix/media-types/text/rust/snippets/snippets.json`
- `<project>/config/snippets/snippets.json`

The project file is configurable with the **`project_snippets`** setting (a path
relative to the project root).

## As implemented in Vix

- `crate::snippets` loads and merges the JSON files (`parse_json`, scope/path
  resolution, `load_scoped`) into `Snippet { name, prefixes, body, description,
  scope }`, and provides the picker's filter state. Pure parsing/merging is unit
  tested.
- `crate::snippet_tool::parse` turns a body into `Parsed { text, stops }` (the
  tabstop engine), shared by both the picker and prefix expansion.
- The host (`App`) builds the in-scope library for the active buffer's media
  type, renders the searchable picker, expands a typed prefix on Tab, and drives
  the tabstop session (`insert_snippet_body`, `snippet_tab`).
