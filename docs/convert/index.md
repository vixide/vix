# Convert

**Tools → Convert** is a family of text-format converters: pick a source and
target format from the submenu and Vix™ rewrites the text in place. Every
converter works the same way — it transforms the current selection if you
have one, otherwise the whole buffer — and none of them open a new tab, a
dialog, or ask for options; on invalid input they leave the buffer untouched
and report the error on the status line instead. None of these actions has a
command palette entry or a default keybinding in any keymap; they are
menu-only.

There are twelve conversions, covering four format families:

- **CSV, TSV, and JSON** — six directions between the two delimited tabular
  formats and JSON, all built on the shared `vix-convert-tabular` engine.
- **JSON and YAML** — both directions.
- **JSON and TOML** — both directions.
- **Markdown and HTML** — both directions.

## CSV, TSV, and JSON

CSV → JSON, CSV → TSV, TSV → CSV, TSV → JSON, JSON → CSV, and JSON → TSV are
each a thin wrapper crate over one shared engine, `vix-convert-tabular`
(`crates/vix-convert-tabular/spec/index.md`), which owns the only CSV parser,
the only TSV parser, and the only JSON↔rows mapping in Vix. Centralizing the
logic keeps the six directions consistent with each other.

A few things the shared engine does that are worth knowing:

- **CSV** is parsed and written with RFC 4180 quoting: a field is quoted when
  it contains a comma, quote, CR, or LF, and a doubled `""` is a literal
  quote.
- **TSV** has no quoting at all — fields are just split and joined on tabs,
  so a field containing a tab or newline can't round-trip.
- Writing CSV or TSV neutralizes spreadsheet-formula injection: a field that
  a spreadsheet would read as a formula (leading `=`, `+`, `-`, `@`, a tab,
  or a CR) gets a literal `'` prefix so it imports as text, not a formula.
- **CSV/TSV → JSON**: the first row is the header; every later row becomes
  one object keyed by those headers, and every value comes out as a JSON
  *string* — `30` becomes `"30"`, not a number.
- **JSON → CSV/TSV**: the input must be a JSON array of objects. The header
  row is the union of every object's keys, in first-seen order; an object
  missing a key gets an empty cell for it. String values are written
  verbatim; other JSON values (numbers, booleans, `null`, nested
  arrays/objects) are rendered as their compact JSON text, except `null`,
  which becomes an empty cell.

Example data, shown as CSV, TSV, and JSON:

```csv
name,age
Alice,30
Bob,25
```

```tsv
name	age
Alice	30
Bob	25
```

```json
[
  { "name": "Alice", "age": "30" },
  { "name": "Bob", "age": "25" }
]
```

Each menu item below converts between two of the three representations
above; open it from the named menu path.

| Direction   | Menu path                            | Action id              | Crate spec |
| ----------- | ------------------------------------- | ----------------------- | ---------- |
| CSV → JSON  | Tools → Convert → CSV → JSON          | `tools.convert.csv.json` | `crates/vix-convert-from-csv-into-json-tool/spec/index.md` |
| CSV → TSV   | Tools → Convert → CSV → TSV           | `tools.convert.csv.tsv`  | `crates/vix-convert-from-csv-into-tsv-tool/spec/index.md` |
| TSV → CSV   | Tools → Convert → TSV → CSV           | `tools.convert.tsv.csv`  | `crates/vix-convert-from-tsv-into-csv-tool/spec/index.md` |
| TSV → JSON  | Tools → Convert → TSV → JSON          | `tools.convert.tsv.json` | `crates/vix-convert-from-tsv-into-json-tool/spec/index.md` |
| JSON → CSV  | Tools → Convert → JSON → CSV          | `tools.convert.json.csv` | `crates/vix-convert-from-json-into-csv-tool/spec/index.md` |
| JSON → TSV  | Tools → Convert → JSON → TSV          | `tools.convert.json.tsv` | `crates/vix-convert-from-json-into-tsv-tool/spec/index.md` |

## JSON and YAML

**Tools → Convert → JSON → YAML** (`tools.convert.json.yaml`) parses the
buffer as JSON and re-serializes it as YAML; scalars, arrays, and objects all
carry over, and object keys come out sorted (`serde_json`'s default map
ordering), not in their original order. See
`crates/vix-convert-from-json-into-yaml-tool/spec/index.md`.

**Tools → Convert → YAML → JSON** (`tools.convert.yaml.json`) parses the
buffer as YAML and re-serializes it as pretty-printed JSON. See
`crates/vix-convert-from-yaml-into-json-tool/spec/index.md`.

Both directions reject input that doesn't parse (invalid JSON or invalid
YAML) and guard against pathologically deep nesting rather than overflowing
the stack.

```json
{ "name": "Vix", "count": 3 }
```

```yaml
count: 3
name: Vix
```

## JSON and TOML

**Tools → Convert → JSON → TOML** (`tools.convert.json.toml`) parses the
buffer as JSON and re-serializes it as TOML. TOML documents can't have a
top-level array or scalar, so a JSON input that isn't a top-level object is
reported as an error rather than converted. See
`crates/vix-convert-from-json-into-toml-tool/spec/index.md`.

**Tools → Convert → TOML → JSON** (`tools.convert.toml.json`) parses the
buffer as TOML and re-serializes it as pretty-printed JSON. See
`crates/vix-convert-from-toml-into-json-tool/spec/index.md`.

```json
{ "name": "Vix", "count": 3 }
```

```toml
name = "Vix"
count = 3
```

## Markdown and HTML

**Tools → Convert → Markdown → HTML** (`tools.convert.markdown.html`) parses
the buffer as CommonMark with `pulldown-cmark` and renders the HTML
fragment. It currently parses plain CommonMark only — GitHub Flavored
Markdown extensions such as tables and strikethrough are not enabled, even
though the crate's spec notes GFM as an available option
(`pulldown-cmark`'s `Options::gfm()`) for a future change. See
`crates/vix-convert-from-markdown-into-html-tool/spec/index.md`.

**Tools → Convert → HTML → Markdown** (`tools.convert.html.markdown`) uses
`htmd` (a Turndown-inspired converter) to turn an HTML fragment into
Markdown, and reports an error if the HTML can't be converted. See
`crates/vix-convert-from-html-into-markdown-tool/spec/index.md`.

```markdown
# Title

a *b* c
```

```html
<h1>Title</h1>
<p>a <em>b</em> c</p>
```

---

Vix™ and Vix IDE™ are trademarks.
