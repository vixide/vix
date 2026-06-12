# Git integration

Vix integrates with git by **shelling out to the user's `git` CLI** (so
credential helpers, SSH agents, and hooks behave exactly as on the command line)
and computing in-editor diff gutters in-process. The logic lives in the internal
`vix-git` crate; the host caches git state and renders it.

**Status:** Phase 1 shipped — read-only awareness plus a stage/unstage/commit
changes panel. Push/pull and conflict resolution are future phases.

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

## Roadmap

- Push / pull / fetch.
- Branch switching and creation.
- Merge-conflict resolution UI.
- Diff gutter in the soft-wrap renderer.
- Multi-line commit messages.
