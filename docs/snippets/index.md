# Snippets

Insert reusable boilerplate from **Tools → Snippets…** (or the command palette).
Pick a snippet and it drops in at the cursor. Snippets can carry **tabstops** —
fields you fill in and jump between with Tab.

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
