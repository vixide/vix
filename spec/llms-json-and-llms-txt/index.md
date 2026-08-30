# llms.json and llms.txt

Create AI guidance helper files at the repo root:

- `llms.json` -> JSON
- `llms.txt` -> markdown text

Purpose: Provide AI tools with a clean, curated map of its most important content.

Help large language models (LLMs) read, understand, and cite a site's documentation or resources without getting bogged down.

File size: < 40k bytes.

## Format

`llms.txt` follows the [llms.txt convention](https://llmstxt.org/): an H1
title, a blockquote summary, optional context paragraphs, then H2 sections
each holding a bullet list of markdown links, one per line, each followed by
an optional `: notes` description. An `## Optional` section marks links a
shorter-context read can skip.

`llms.json` is its structured twin — `{name, summary, description,
repository, version, license, sections: [{title, links: [{title, url,
notes}]}]}` — for tools that want to parse rather than scan markdown.

Both are curated by hand, at the same altitude as [`docs/index.md`](../../docs/index.md)
(highlights, not an exhaustive dump of all 102 crate specs), and live at the
repo root next to `README.md`/`AGENTS.md` so a crawler finds them without
knowing Vix's internal layout.

## Keeping them in sync

`scripts/check-docs` gates both, alongside the other documentation checks:

- `llms.txt` is scanned like any other root markdown file, so a link it names
  that does not resolve fails the build.
- `llms.json` must parse as JSON, and its link set must equal `llms.txt`'s —
  the two are one curated map in two formats, not two maps that can drift
  apart.
- Both must stay under the 40 KB budget above.

Update both together when the curated map changes; there is no generator.
