# Media Types

A reference table of common **media types** (a.k.a. MIME / content types) with
their descriptions and file extensions, plus a searchable picker for inserting a
media type into the active buffer.

## Data

The table is the TSV [`media-types.tsv`](media-types.tsv) — three tab-separated
columns: **Media Type**, **Description**, **Extension** (the header row is
skipped; a row may list several comma-separated extensions, e.g. `.yaml, .yml`).
It is the single source of truth: the `crate::media_type` module embeds it with
`include_str!` and parses it once.

The list is curated from the common web media types (per MDN) plus popular
developer and modern formats (source code, config, archives, fonts, images,
audio/video). It is deliberately *common*, not the exhaustive IANA registry.

## Module (`crate::media_type`)

- `all() -> &[MediaType]` — the parsed table (`{ media_type, description, extension }`).
- `for_extension(ext) -> Option<&MediaType>` — first row whose extension list
  contains `ext` (with or without a leading dot, case-insensitive).
- `Panel` — the picker's state: a case-insensitive `query` filtering by media
  type / description / extension, plus `selected`/`scroll` over the filtered
  rows. `open_for_extension(ext)` pre-selects the row for a file extension.

## Picker (Tools → Media Types)

`tools.media_types` opens an overlay listing the table. It opens **pre-selected**
to the active file's media type when its extension is recognized.

- **Type** to filter (matches media type, description, or extension).
- **↑/↓**, **PgUp/PgDn** move; **Backspace** edits the filter.
- **Enter** (or a click) inserts the highlighted media type (e.g. `image/png`)
  at the cursor and leaves the panel open; **Esc** closes it.
