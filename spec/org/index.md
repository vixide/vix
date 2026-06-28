# Org

A pragmatic subset of [Org mode](https://orgmode.org/) for editing `.org`-style
outlines. The logic lives in the pure `crate::org` module (unit-tested); the
top-level **Org** menu wires it to the active buffer at the cursor line.

This is intentionally *not* a complete Org implementation — it covers the basics:
headline structure, TODO/checkbox toggling, folding, and lightweight export.

## Concepts

- **Headline**: a line of one or more leading `*` followed by a space
  (`* Top`, `** Child`). The star count is the level.
- **Subtree**: a headline plus all following lines up to the next headline of the
  same or higher level.

## Menu

The **Org** menu (`Alt+O`):

| Item | Action | Effect |
| ---- | ------ | ------ |
| Cycle Visibility (Fold) | `org.cycle_visibility` | Fold/unfold at the cursor (reuses the editor fold toggle). |
| Headline → Promote | `org.promote` | Remove one `*` from every headline in the subtree (refused at level 1). |
| Headline → Demote | `org.demote` | Add one `*` to every headline in the subtree. |
| Headline → Move Subtree Up | `org.move_up` | Swap the subtree with the previous sibling. |
| Headline → Move Subtree Down | `org.move_down` | Swap the subtree with the next sibling. |
| Cycle TODO | `org.cycle_todo` | Cycle the headline keyword: none → `TODO` → `DONE` → none. |
| Toggle Checkbox | `org.toggle_checkbox` | Toggle a list item's `[ ]` ⇄ `[x]`. |
| Export → Markdown | `org.export_markdown` | Convert the buffer to Markdown in a new tab. |
| Export → HTML | `org.export_html` | Convert the buffer to a standalone HTML document in a new tab. |

Structure commands operate on the headline/line under the cursor; the cursor
follows a moved subtree. When a command does not apply (e.g. the cursor is not on
a headline, or there is no sibling to swap with), the status bar says so.

## Insertion

Org *content* insertion (snippets, inline markers, blocks) lives under
**Tools → Insert → Org / Markers / Begin-End** — see
[`spec/tools/insert/org.md`](../tools/insert/org.md).

## Export mapping (pragmatic)

- Headlines → `#`×level (Markdown) / `<h1..6>` (HTML); `#+title:` → top heading.
- Inline: `*bold*`, `/italic/`, `_underline_`, `+strike+`, `~code~`, `=verbatim=`,
  and `[[url][desc]]` / `[[url]]` links.
- Bullet lists → Markdown `-` / HTML `<ul><li>`. Block delimiters (`#+BEGIN_…`)
  are dropped, their inner text kept.
