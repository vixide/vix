# Emmet Expansion

A pragmatic subset of [Emmet](https://emmet.io/) abbreviation expansion:
type a compact abbreviation, then expand it into indented HTML in place.

## Using it

Type an abbreviation ending at the cursor — the contiguous non-whitespace run
immediately before it — then choose **Edit → Emmet Expand** or the command
palette. Vix™ replaces that run with the expanded HTML.

If the run doesn't parse as Emmet, nothing is replaced and the status line
says so instead.

## Supported syntax

The expander covers the common operators and modifiers:

| Syntax     | Meaning                                    |
| ---------- | ------------------------------------------- |
| `>`        | child                                       |
| `+`        | sibling                                     |
| `*N`       | multiply (repeat N times)                   |
| `#id`      | id attribute                                |
| `.class`   | class attribute (`.a.b` for multiple)       |
| `{text}`   | literal text content                        |
| `$`        | numbering inside text within a multiplied element |

Grouping with `()` is **not** supported — an abbreviation that needs it
doesn't parse, and nothing is replaced.

A single abbreviation is also capped at 100,000 expanded elements, so a
runaway multiply like `div*900000000` fails to parse rather than hanging or
exhausting memory.

## Example

With the cursor right after `ul>li.item$*3`, choosing **Edit → Emmet
Expand** replaces it with:

```html
<ul>
  <li class="item1"></li>
  <li class="item2"></li>
  <li class="item3"></li>
</ul>
```

## Notes

Expansion is pure logic in `crate::emmet::expand` (editor action
`edit.emmet_expand`), with no dependency on file type — it works in any
buffer. See the specification at `crates/vix-emmet/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
