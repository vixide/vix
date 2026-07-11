# Convert Tabular

Shared CSV/TSV/JSON conversion helpers for Vix's Tools → Convert tools.

Tabular text comes in two delimited flavors — CSV (comma-separated, with
RFC 4180 quoting) and TSV (tab-separated, no quoting) — and one structured
flavor, JSON (an array of objects). Every per-direction tool crate is a thin
wrapper over the functions here:

- [`parse_csv`] / [`write_csv`] — RFC 4180 quoting (`"` quotes, `""` escapes).
- [`parse_tsv`] / [`write_tsv`] — plain tab split/join (tabs and newlines are
assumed absent from fields, as TSV has no escape mechanism).
- [`rows_to_json`] — the first row is the header; each later row becomes an
object keyed by the headers (string values), emitted as pretty JSON.
- [`json_to_rows`] — an array of objects becomes a header row (the union of
keys, in first-seen order) followed by one row per object.

Centralizing the logic means CSV→JSON and JSON→CSV (and the TSV pair) share
exactly one parser and one mapper, so the directions stay consistent.
