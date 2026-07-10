# Snippets

Insert reusable boilerplate two ways:

- **Tools → Snippets…** (or the command palette) opens a searchable picker —
  type to filter by name, prefix, or description, then Enter to insert.
- **Prefix expansion** — type a snippet's prefix (e.g. `fn`) and press **Tab**;
  the word is replaced by the snippet body.

Snippets can carry **tabstops** — fields you fill in and jump between with Tab.

## Your own snippets (JSON files)

Beyond the bundled set, Vix™ loads snippet files from three places (modeled on VS
Code's JSON snippets):

- **Global** — `~/.config/vix/global/snippets/snippets.json` (always available).
- **Media-type** — `~/.config/vix/media-types/<type>/<subtype>/snippets/snippets.json`,
  e.g. `…/media-types/text/rust/snippets/snippets.json` for Rust files. Applies
  only to buffers of that media type.
- **Project** — `config/snippets/snippets.json` under the project root (set the
  path with the `project_snippets` setting).

Each file is a JSON object of `"Name": { "prefix": …, "body": …, "description": … }`
entries. `prefix` and `body` may be a string or an array of strings (joined by
newlines). For example:

```json
{
  "Function": {
    "prefix": "fn",
    "body": ["fn ${1:name}(${2}) -> ${3:()} {", "\t$0", "}"],
    "description": "A Rust function"
  }
}
```

(The files are JSON, not XML; TextMate's interpolated shell code and `\u` escapes
are not supported.) See the full reference at `spec/snippets/index.md`.

## Tabstops

When a snippet has fields, inserting it places the cursor on the first one and
selects any placeholder text, so you can just start typing:

- **Tab** moves to the next field.
- **Esc** leaves snippet mode (Tab goes back to indenting).
- The last field (`$0`) is where the cursor ends up.

For example, the **Rust function** snippet expands to:

```
fn name() -> () {

}
```

with `name` selected first; Tab then moves to the parameters, the return type, and
finally the body.

## Writing tabstops

Snippet bodies use a small syntax:

| Marker            | Meaning                                          |
| ----------------- | ------------------------------------------------ |
| `$1`, `$2`, …     | A field, visited in order                        |
| `${1:placeholder}`| A field pre-filled with selectable text          |
| `$0`              | The final cursor position                        |
| `\$`              | A literal dollar sign                            |

See the specification at `spec/snippets/index.md`. For one-shot generated text
(UUIDs, dates, language boilerplate) see [Insert](../insert/index.md).

---

Vix™ and Vix IDE™ are trademarks.
