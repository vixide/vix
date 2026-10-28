# Todo Tree

Every `TODO`, `FIXME`, `HACK`, `XXX`, `BUG` and `NOTE` comment in your
project, gathered in one window and grouped so none are forgotten.

Open it with **Tools → Todo Tree** or from the command palette. Vix™ scans
the project's files (the ones your `.gitignore` doesn't exclude) and lists
what it finds, up to 2,000 entries.

## Reading the tree

Entries are grouped under a header that shows how many it holds.

- **By tag** (the default): one group each for `TODO`, `FIXME`, `HACK`, and
  so on. Each entry shows `file:line` and the comment's text.
- **By file**: one group per file, each entry showing its line number.

A tag only counts as a whole word, so a variable named `todos` is not a
`TODO`. A line with several tags is listed once, under the first.

## Keys

| Key | What it does |
| --- | ------------ |
| Up / Down, PageUp / PageDown | Move the highlight |
| Enter | On an entry, jump to it; on a header, fold or unfold the group |
| Space | Fold or unfold the highlighted row's group |
| `g` | Switch between grouping by tag and by file |
| `[` / `]` | Fold every group / unfold every group |
| Esc | Close |

Clicking an entry jumps to it; clicking a header folds it.

## The flat list

**Tools → TODO Finder** is still there when you just want a quick pick from
a flat list of the same entries.

## Not yet

Custom tags and patterns, limiting the scan to open files, refreshing on
save, and docking the tree beside the editor are planned.

See the specification at `crates/vix-todo-tree/spec/index.md`.
