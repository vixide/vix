# Org-Roam

Vix™ includes an [Org-roam](https://www.orgroam.com/)-style layer on top of
[Org mode](../org/index.md): networked, Zettelkasten-style note-taking over a
directory of `.org` files. Open the **Org → Roam** (or **Org → Node**) menu
with a buffer of Org text.

A *node* is an `.org` file carrying an `:ID:` property and a `#+title:`.
Nodes link to one another with `[[id:<id>][Title]]` links; the set of nodes
and links forms a graph you can search, insert into, and visualize.

## Finding, creating, and linking nodes

- **Find Node…** (`roam.node_find`) prompts for a title and opens the
  matching node, or offers to create one if none exists.
- **Insert Node Link…** (`roam.node_insert`) prompts for a title and inserts
  a `[[id:...][Title]]` link to that node at the cursor — finding or creating
  it — without leaving the buffer you're editing. A freshly created node file
  is written to disk but not opened.
- **Random Node** (`roam.node_random`) jumps to a random existing node.
- **Complete Link…** (`roam.link_complete`), and typing `[[` directly in an
  Org buffer, opens a completion popup of node titles; accepting one inserts
  the link. See `crates/vix-roam/spec/wiki-link-completion/index.md`.
- **Capture Node…** (`roam.capture`) prompts for a title and creates (or
  opens) a node without needing to already know whether it exists.

## Backlinks

- **Backlinks** (`roam.backlinks`) lists the nodes that link to the current
  one.
- **Live Backlinks** (`roam.backlinks_follow`) is a toggle that fills the
  bottom dock with the active node's linked and unlinked references, and
  refreshes automatically as you move between nodes. See
  `crates/vix-roam/spec/live-backlinks/index.md`.

## Dailies

**Org → Roam → Dailies** manages one daily note per day:

- **Today** (`roam.dailies_today`) opens (or creates) today's daily note.
- **Capture Today…** (`roam.dailies_capture`) prompts for a quick note to
  drop into today's daily note.
- **Go to Date…** (`roam.dailies_date`) opens (or creates) the daily note for
  a date you type.
- **Calendar…** (`roam.dailies_calendar`) opens the month-grid calendar in
  dailies mode: pressing Enter on a day opens or creates that day's daily
  note instead of inserting a date string. See
  `crates/vix-roam/spec/dailies-calendar/index.md`.

## Node metadata

**Org → Roam → Metadata** edits the current node's file-level property
drawer and `#+filetags:` line:

- **Add Tag…** (`roam.tag_add`)
- **Add Alias…** (`roam.alias_add`)
- **Add Ref…** (`roam.ref_add`)

## Graph and database

- **Graph** (`roam.graph`) compiles the node/link graph into a Mermaid
  diagram in a new tab.
- **Sync Database** (`roam.db_sync`) rebuilds the node index from the
  project's `.org` files.

## Org → Node

**Org → Node** is a second menu over the same node infrastructure, styled
after `org-node`: fast, ID-based nodes that can be whole files *or* subtrees.
It reuses Find/Insert Link/Random/Backlinks from Roam and adds its own
operations:

- **Insert Transclusion…** (`node.insert_transclusion`)
- **Nodeify Entry** (`node.nodeify`) — give the headline at the cursor an
  `:ID:` so it becomes a node in place.
- **Extract Subtree to Node** (`node.extract_subtree`) — move a subtree into
  its own node file.
- **List Dead Links** (`node.dead_links`) — find `id:` links with no matching
  node.
- **Rename File by Title** (`node.rename_by_title`) — rename a node's file to
  match its `#+title:`.
- **Rebuild Cache** (`node.reset`) — the same rebuild as Sync Database, run
  from the Node menu.

None of these actions have default keybindings; all run from the **Org**
menu or the command palette.

## Implementation

`vix-roam` is the pure, testable core — parsing a node's title/id, building
new node and daily-note skeletons, editing the property drawer and
`#+filetags:`, and compiling backlinks/graph/index views — with no
filesystem or editor dependency of its own. The host (`App`, mostly
`src/app/roam.rs`) wires these functions to the menu, prompting for input and
reading/writing the actual `.org` files under the project root.

See the crate spec at `crates/vix-roam/spec/index.md`, and its sub-specs
`crates/vix-roam/spec/dailies-calendar/index.md`,
`crates/vix-roam/spec/live-backlinks/index.md`, and
`crates/vix-roam/spec/wiki-link-completion/index.md`. For the underlying
headline/TODO/agenda
features these notes are written in, see [Org](../org/index.md).

---

Vix™ and Vix IDE™ are trademarks.
