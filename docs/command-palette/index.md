# Command Palette

The command palette is a single prompt that finds files, runs commands,
switches buffers, jumps to a line, and jumps to a symbol. Press **Ctrl+P** to
open it. A hints line at the bottom shows the available prefixes.

## Modes

A leading prefix character selects the mode:

| Prefix | Mode        | Description                                  |
| ------ | ----------- | -------------------------------------------- |
| (none) | File finder | Fuzzy search for files in your workspace       |
| `>`    | Commands    | Search and run editor commands               |
| `#`    | Buffers     | Switch between open buffers by name          |
| `:`    | Go to line  | Jump to a specific line number               |
| `@`    | Symbols     | Jump to a declaration in the current file    |

## Matching and acceptance

- Space-separated terms match independently. For example, `feat group` matches
  `features/groups/view.tsx`, `etc hosts` finds `/etc/hosts`, and `save file`
  finds `save_file.rs`.
- Press **Tab** to accept the top suggestion.
- Press **Enter** to commit the highlighted entry.

## File finder

With no prefix, type to fuzzy-match files in the workspace. Append
`:<line>[:<col>]` to a file to jump to that position after opening — for
example, `tui/src/main.rs:42:10` opens `tui/src/main.rs` and moves to line 42,
column 10.

## Go to line (`:`)

Type `:` followed by a line number. The cursor previews the target line **as
you type** and scrolls it into view. Press **Enter** to commit — recording the
original position in the jump history — or **Esc** to revert to where you were.

## Go to symbol (`@`)

Type `@` to fuzzy-filter the current file's declarations (functions, types,
classes, traits, modules, `#define`s, and so on) and press **Enter** to jump to
one. The list is a fast, offline heuristic — the same family as go-to-definition
— so it works for any language without a language server. It is also reachable
as the palette command **Go to Symbol in File**.

## Buffers (`#`)

Type `#` followed by a buffer name to fuzzy-match and switch among the open
buffers.

## Commands (`>`)

Type `>` to search and run editor commands. Palette commands share the same
action identifiers as the menu items, so a command behaves identically however
it is invoked.

## Preview, history, and resume

On a screen at least 100 columns wide, the file finder, `@`, `@@`, and
**Find In Workspace** show a preview of the highlighted result beside the
list: a few dozen numbered lines of the file around the result, its line
highlighted. The preview reads at most 256 KiB of a file (an open buffer is
previewed from its own unsaved text), and is skipped in a remote (SSH)
workspace.

With the input empty, **Up** recalls the queries you accepted earlier, newest
first, and **Down** walks back toward the newest. Typing ends the recall, and
**Up**/**Down** move the highlight again.

**Tools → Resume Last Picker** (command **Resume Last Picker**) reopens the
palette or the workspace search you closed last, in the same mode, with the
same query and the same row highlighted.

## Examples

- `README` — open the file matching `README`.
- `tui/src/main.rs:42:10` — open `tui/src/main.rs` at line 42, column 10.
- `>save` — find and run a save command.
- `#notes` — switch to the buffer matching `notes`.
- `:128` — preview and jump to line 128.
- `@parse` — jump to a declaration matching `parse` in the current file.

---

Vix™ and Vix IDE™ are trademarks.
