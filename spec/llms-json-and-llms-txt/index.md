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
(highlights, not an exhaustive dump of all 104 crate specs), and live at the
repo root next to `README.md`/`AGENTS.md` so a crawler finds them without
knowing Vix's internal layout.

## Two copies, two link forms

The workspace-root `llms.txt`/`llms.json` use links relative to this repo
(e.g. `README.md`, a path like docs/architecture/index.md), which only
resolve inside a git checkout — cloned locally, or browsed on a forge.
Serving that exact text from vixide.github.io/llms.txt would ship links that
404 there: the site's own domain has no path like /docs/architecture/index.md.

vixide.github.io (a separate repo) therefore does not copy these files
verbatim: its website-appropriate versions — the two files under its
`static/` directory — rewrite each entry to point at wherever it actually
resolves from the site's own domain, currently
`https://github.com/vixide/vix/blob/main/...`, the same back-to-source
pattern every other page on that site already uses. `scripts/check-docs`
here gates only the workspace-root pair; when the curated map changes,
update the website copies by hand (re-verify the rewritten links resolve)
and push them separately.

## Keeping them in sync

`scripts/check-docs` gates both, alongside the other documentation checks:

- `llms.txt` is scanned like any other root markdown file, so a link it names
  that does not resolve fails the build.
- `llms.json` must parse as JSON, and its link set must equal `llms.txt`'s —
  the two are one curated map in two formats, not two maps that can drift
  apart.
- Both must stay under the 40 KB budget above.

Update both together when the curated map changes; there is no generator.
