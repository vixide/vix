# Git integration

Vix integrates with git by **shelling out to the user's `git` CLI** (so
credential helpers, SSH agents, and hooks behave exactly as on the command line)
and computing in-editor diff gutters in-process. The logic lives in the internal
`vix-git` crate; the host caches git state and renders it.

**Status:** Phases 1–2 shipped — read-only awareness, a stage/unstage/commit
changes panel, branch switching, and push/pull/fetch. Conflict resolution is a
future phase.

## Awareness (read-only)

- **Status bar** shows the current branch (with a `\u{2022}` dirty dot when the
  working tree has changes).
- **File explorer** shows a colored one-letter badge on changed tracked files:
  `M` (modified, yellow), `A` (added, green), `?` (untracked, green), `D`
  (deleted, red), `R` (renamed, cyan), `U` (conflicted, magenta).
- **Editor diff gutter** draws a colored bar in the line-number gutter for each
  line that differs from its committed (HEAD) version — green added, yellow
  modified, red deleted. Computed by diffing the buffer against its cached HEAD
  blob (`vix_git::diff_marks`, via the `similar` crate). Shown in the non-wrapped
  view; the soft-wrap gutter is a follow-up.

The cached git state (repo?, branch, changed files, HEAD blobs) refreshes at
startup, after each save, and after git actions.

## Changes panel (Git → Changes…, or the command palette)

A modal list of changed files, each showing a staged checkbox and its change
letter:

- `↑`/`↓` — move the selection.
- `Space` — toggle the selected file staged/unstaged; `s` stage, `u` unstage.
- `c` — commit: prompts for a one-line message and runs `git commit` (only when
  something is staged).
- `r` — refresh the status.
- `Esc` — close.

A left click on a row toggles its staged state.

## Branches and remotes (Git menu / command palette)

- **Switch Branch…** lists local branches in a chooser; `Enter` checks out the
  highlighted branch (`git switch`), then refreshes the cached state and the
  explorer.
- **Pull / Push / Fetch** run `git pull` / `git push` / `git fetch` through the
  async Run Command pipeline, streaming output to the bottom dock; the cached git
  state refreshes when the command finishes.

## Roadmap

- Branch creation and deletion.
- Merge-conflict resolution UI.
- Diff gutter in the soft-wrap renderer.
- Multi-line commit messages.
- Reloading open buffers after a branch switch changes files on disk.
