# vCard Parser

A small, dependency-free vCard 4.0 ([RFC 6350](https://www.rfc-editor.org/info/rfc6350/))
parser used by the contact browser and the single-vCard view.

## As implemented in Vix

**Status:** Shipped. `vix_vcard_parser::parse(text)` returns a `Vcard` — a flat
list of `Property { name, params, value }`. It is pure (no IO); the host reads
the `.vcf` files.

What it handles:

- **Line unfolding** — a line beginning with a space or tab continues the
  previous content line (its leading whitespace removed).
- The `name;PARAM=value:VALUE` shape: the property name is uppercased and any
  group prefix is stripped (`item1.EMAIL` → `EMAIL`); parameters are parsed as
  `(key, value)` pairs, with comma-separated `TYPE` lists split apart and legacy
  bare types (`TEL;WORK:`) recorded as `TYPE`.
- **Value unescaping**: `\\`, `\n`/`\N`, `\,`, `\;`.
- `BEGIN`, `END`, and `VERSION` lines are dropped from the property list.

Accessors: `get(name)`, `all(name)`, `value(name)`, `Property::param(key)`,
`Property::types()`, and `display_name()` (the `FN`, else a `Given Family`
derived from `N`, else `"(unnamed)"`).
