# Todo Tree

The comment tags in a project (`TODO`, `FIXME`, `HACK`, `XXX`, `BUG`, `NOTE`)
gathered into a tree you can fold and jump from, so none are forgotten. Opened
from **Tools → Todo Tree**; the flat **TODO Finder** remains for a quick pick.

## Scanning

`scan(file, text, tags)` returns an `Item { tag, file, line, col, text }` for
each line holding a tag as a whole word (via `vix_textops::tag_column`, so
`todos` is not a `TODO`). A line reports one tag, the first in `tags` order.
`text` is the trimmed line cut to 120 characters. `DEFAULT_TAGS` is the list.
The app scans the files of its file index, which honors `.gitignore`, skipping
unreadable and oversized files, and stops at 2,000 items.

## The tree

`Panel` sorts its items by file then line and groups them either **by tag**
(known tags first in `DEFAULT_TAGS` order, others alphabetical) or **by file**
(alphabetical). `rows()` yields each `Row::Group { label, count, collapsed }`
followed by its `Row::Item(index)` rows unless the group is folded. Folds are
remembered per grouping.

Navigation uses `vix_list_state` (up, down, page, direct select, keep visible).

| Key | Action |
| --- | ------ |
| Up / Down / PageUp / PageDown | move the highlight |
| Enter | on an item, jump to it; on a header, fold or unfold it |
| Space | fold or unfold the group of the highlighted row |
| `g` | switch between grouping by tag and by file |
| `[` / `]` | fold all / unfold all |
| Esc | close |

A click selects a row; a click on an item jumps, on a header folds.

## Actions

`tools.todo_tree` (Tools menu, a `Tools → Find` neighbor of `tools.todo_finder`).

## Not yet

Custom tags and regexes from settings, a workspace-versus-open-files toggle,
refresh on save, and a docked (non-overlay) placement.
