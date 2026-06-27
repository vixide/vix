# Git integration

Vix integrates with git by **shelling out to the user's `git` CLI** (so
credential helpers, SSH agents, and hooks behave exactly as on the command line)
and computing in-editor diff gutters in-process. The logic lives in the internal
`git` crate; the host caches git state and renders it.

**Status:** Phases 1–2 shipped — read-only awareness, a stage/unstage/commit
changes panel, branch switching, and push/pull/fetch. Conflict resolution is a
future phase.

## Awareness (read-only)

- **Status bar** shows the current branch (with a `\u{2022}` dirty dot when the
  working tree has changes). **Clicking it opens the Git Changes panel.**
- **File explorer** shows a colored one-letter badge on changed tracked files:
  `M` (modified, yellow), `A` (added, green), `?` (untracked, green), `D`
  (deleted, red), `R` (renamed, cyan), `U` (conflicted, magenta).
- **Editor diff gutter** draws a colored bar in the line-number gutter for each
  line that differs from its committed (HEAD) version — green added, yellow
  modified, red deleted. Computed by diffing the buffer against its cached HEAD
  blob (`vix_git::diff_marks`, via the `similar` crate). Drawn in both the
  non-wrapped and soft-wrap views (in soft-wrap, the bar appears on a changed
  line's first visual row).

The cached git state (repo?, branch, changed files, HEAD blobs) refreshes at
startup, after each save, and after git actions.

## Changes panel (Git → Changes…, or the command palette)

A modal list of changed files, each showing a staged checkbox and its change
letter:

- `↑`/`↓` — move the selection.
- `Space` — toggle the selected file staged/unstaged; `s` stage, `u` unstage.
- `c` — commit: prompts for a message and runs `git commit` (only when something
  is staged). The prompt is **multi-line**: `Alt+Enter` inserts a newline, plain
  `Enter` submits, so subject + body messages are supported.
- `r` — refresh the status.
- `Esc` — close.

A left click on a row toggles its staged state.

## Blame (Git → Blame Line, action `git.blame`)

Annotates the **cursor's current line** with its `git blame` attribution in the
status bar: `L<line>: <short-hash> <author>, <YYYY-MM-DD> · <commit summary>`.
Lines not yet committed report `L<line>: not committed yet`.

Blame runs `git blame --line-porcelain -L <n>,<n>` for the single line, invoked
from the file's own directory so git resolves the repository itself (robust to
symlinked workspace roots such as macOS `/var` → `/private/var`). The porcelain
output is parsed by `vix_git::parse_blame_porcelain` into a `BlameLine`
(`hash`, `author`, `date`, `summary`); the authored date is rendered in the
author's own time zone via a dependency-free epoch→civil-date conversion.

## Branches and remotes (Git menu / command palette)

- **Switch Branch…** lists local branches in a chooser; `Enter` checks out the
  highlighted branch (`git switch`), then refreshes the cached state and the
  explorer, and reloads every **clean** open buffer from disk (dirty buffers are
  left untouched) via `Editor::reload_clean_from_disk`, reporting the count.
- **Pull / Push / Fetch** run `git pull` / `git push` / `git fetch` through the
  async Run Command pipeline, streaming output to the bottom dock; the cached git
  state refreshes when the command finishes.

## Roadmap

- Merge-conflict resolution UI.
- Persistent inline (end-of-line) blame annotations, not just the status bar.
