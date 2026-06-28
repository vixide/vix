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
| Capture… | `org.capture` | Open a single-line prompt; the text is inserted as a `* TODO` headline at the cursor. |
| Cycle Visibility (Fold) | `org.cycle_visibility` | Fold/unfold at the cursor (reuses the editor fold toggle). |
| Headline → Promote | `org.promote` | Remove one `*` from every headline in the subtree (refused at level 1). |
| Headline → Demote | `org.demote` | Add one `*` to every headline in the subtree. |
| Headline → Move Subtree Up | `org.move_up` | Swap the subtree with the previous sibling. |
| Headline → Move Subtree Down | `org.move_down` | Swap the subtree with the next sibling. |
| Cycle TODO | `org.cycle_todo` | Cycle the headline keyword: none → `TODO` → `DONE` → none. |
| Toggle Checkbox | `org.toggle_checkbox` | Toggle a list item's `[ ]` ⇄ `[x]`. |
| Clock In | `org.clock_in` | Insert an open `CLOCK: [now]` entry at the cursor (local time). |
| Clock Out | `org.clock_out` | Close the most recent open `CLOCK:` entry with the end time and `=> H:MM` duration. |
| Agenda Tracker | `org.agenda` | Compile `DEADLINE:`/`SCHEDULED:` items and `TODO` headlines from every `.org` file in the project into a single dated agenda buffer. |
| Time Tracker | `org.time_report` | Sum each headline's `CLOCK:` durations in the active buffer into a time-report table. |
| Export → Markdown | `org.export_markdown` | Convert the buffer to Markdown in a new tab. |
| Export → HTML | `org.export_html` | Convert the buffer to a standalone HTML document in a new tab. |

Agenda and Time Tracker output open in a new buffer. The pure builders
(`org::agenda`, `org::time_report`) are unit tested; `CLOCK:` durations are read
from the `=> H:MM` totals Org writes.

Structure commands operate on the headline/line under the cursor; the cursor
follows a moved subtree. When a command does not apply (e.g. the cursor is not on
a headline, or there is no sibling to swap with), the status bar says so.

## Roam

The **Org → Roam** submenu brings [Org-roam](https://www.orgroam.com/)-style
networked, Zettelkasten note-taking to a directory of `.org` files. A **node** is
an `.org` file with an `:ID:` property and a `#+title:`; nodes link to one another
with `[[id:<id>][Title]]` links, forming a graph. The pure logic lives in the
unit-tested `crate::roam` module; the host wires it to prompts and the filesystem.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Find Node… | `roam.node_find` | Prompt for a title; open the matching node, or create `<slug>.org` (with a fresh `:ID:`) and open it. |
| Insert Node Link… | `roam.node_insert` | Prompt for a title; insert an `[[id:…][Title]]` link at the cursor, creating the node file (without leaving the current buffer) if new. |
| Random Node | `roam.node_random` | Jump to a randomly chosen node. |
| Capture Node… | `roam.capture` | Prompt for a title and create/open a new node. |
| Backlinks | `roam.backlinks` | Compile a buffer of *linked* references (files linking to the active node's `:ID:`) and *unlinked* references (files mentioning its title). |
| Dailies → Today | `roam.dailies_today` | Open (creating if needed) today's daily note `daily/YYYY-MM-DD.org`. |
| Dailies → Capture Today… | `roam.dailies_capture` | Append a `* HH:MM …` entry to today's daily note. |
| Dailies → Go to Date… | `roam.dailies_date` | Prompt for a `YYYY-MM-DD` date and open that daily note. |
| Metadata → Add Tag… | `roam.tag_add` | Add a tag to the node's `#+filetags:` line. |
| Metadata → Add Alias… | `roam.alias_add` | Append a quoted alias to the `:ROAM_ALIASES:` property. |
| Metadata → Add Ref… | `roam.ref_add` | Append a URL / cite key to the `:ROAM_REFS:` property. |
| Graph | `roam.graph` | Build a Mermaid `flowchart` of all nodes and `[[id:…]]` links into a new buffer. |
| Sync Database | `roam.db_sync` | Re-index the project and open a sortable table of every node (title, file, tags). |

Nodes live in the project root; daily notes live in `daily/`. There is no
persistent database — *Sync Database* simply re-scans the project's `.org` files,
matching org-roam's `org-roam-db-sync` semantics in a stateless way.

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
