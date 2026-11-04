# Branch review

Review a branch, a commit range, a commit or a stash as one multi-file diff,
with line comments that are saved per repository and exported as Markdown.

**Status:** Mostly shipped (improvement plan T665). Shipped: the multi-file
buffer, hunk navigation, saved line comments, the Markdown/JSON export,
stage/discard of the hunk under the cursor, staging every hunk at once (a
whole stash), `r` in the stash chooser, re-diffing after a stage or discard,
and `\ No newline at end of file` markers. Not shipped: the two-pane view
(T660's model could be added later) and discarding a whole review at once.

## Actions

All eight are in **Git** and in the command palette.

| Action | Menu item | Does |
| --- | --- | --- |
| `git.review` | Review Range… | Prompts for a range and opens it in a read-only tab titled `review <range>`. |
| `git.review_comment` | Comment on Review Line… | Prompts for a comment on the diff line under the cursor, pre-filled with the existing one. Empty text removes it. On a comment row it edits that comment. Elsewhere it only says so in the status line. |
| `git.review_next` | Next Review Hunk | Moves to the next `@@` header, wrapping to the first. |
| `git.review_prev` | Previous Review Hunk | Moves to the previous `@@` header, wrapping to the last. |
| `git.review_export` | Export Review Comments | Writes the open review's comments to `.vix/review/<range>.md` and `.json`. |
| `git.review_stage_hunk` | Stage Review Hunk | Stages the hunk under the cursor: `Review::hunk_patch` rebuilds a one-hunk patch (file headers use `/dev/null` for a new or deleted file) and `vix_git::apply_patch(.., ApplyTarget::Index)` feeds it to `git apply --cached`. A binary file, or a cursor outside a hunk, only says so in the status line. |
| `git.review_stage_all` | Stage All Review Hunks | `Review::full_patch` joins every hunk of every text file (one header per file, binary files skipped) and `apply_patch(.., ApplyTarget::Index)` stages it in one call; for a stash review that is the whole stash. Says so in the status line when there is nothing to stage. |
| `git.review_discard_hunk` | Discard Review Hunk… | Same patch, after a yes/no (`ConfirmKind::GitReviewDiscard`), through `git apply --reverse` on the working tree. |

The next, previous, comment, export, stage, stage-all and discard actions work only in the review tab
(they say so in the status line elsewhere).

After a successful stage (one hunk or all) or discard, the host runs the
range's diff again, `Review::replace_diff` swaps the files in (comments are
kept) and the buffer is redrawn with the cursor row kept. If git cannot diff
the range any more the buffer is left as it was. A committed range does not
change when the index does, so the re-diff matters for ranges that involve the
working tree or a moved ref.

A line that git marks `\ No newline at end of file` carries
`DiffLine::no_newline`; the buffer shows the marker on its own row right after
the line, and `hunk_patch`/`full_patch` write it back so `git apply` accepts
the hunk.

## Ranges

What the user types is checked like any ref name (no leading `-`, no
whitespace or control characters) and passed after `--end-of-options`.

- `a..b` and `a...b` run `git diff`.
- A single revision (a commit, a tag, `stash@{0}`) runs `git show` without the
  commit header (with `-m --first-parent`, so a stash or merge is diffed
  against its first parent), so it shows what that revision changed.

Prefixes are forced to `a/` and `b/` (`vix_git::review_diff`) so the output
parses whatever the user's git config says about diff prefixes. An empty diff opens nothing.

## The buffer

One line of title, then for each changed file a blank row, a `=== path`
heading and its hunks exactly as git printed them (`@@` header, then lines
with a leading space, `+` or `-`). A binary file shows `(binary file)`. Each
comment is shown on its own row right under its line, as `    >> text`.

The tab is read-only. Adding or removing a comment re-renders the buffer and
keeps the cursor row. [`Rendered::targets`] maps every buffer row to what it
shows (file, hunk, diff line, comment), which is how the cursor row finds the
line to comment on.

## Comments

A comment is anchored to a file, a **side** and a line number on that side: a
removed line counts on the old file, every other line on the new one. It keeps
the line's text (`code`) so an export reads without the diff. One comment per
line; commenting again replaces it.

### Storage

`.vix/review/<name>.json` under the workspace root, where `<name>` is
`store_name(range)`: every character other than a letter, digit, `.`, `-` or
`_` becomes `_`, and leading dots are dropped (so `main..feature/x` is
`main..feature_x`). The file holds the range and the comments. It is written
on every change through the filesystem authority, and loaded when the same
range is reviewed again. Line numbers belong to the diff as it was when the
comment was written; comments whose line is no longer in the diff are kept in
the file and the export, but are not shown in the buffer.

### Export

`to_markdown` groups the comments by file, ordered by line, as
`- **Line 12**: text` (`(old)` after the line number for the old side), each
followed by its code line in a fenced block, so it can be pasted into a pull
request or an agent prompt. `to_json` is `{"range": .., "comments": [..]}`,
the same shape as the storage file.

## As implemented in Vix

- `crates/vix-review/src/lib.rs`: `parse` (unified diff to `FileDiff`/`Hunk`/
  `DiffLine`), `Review` (`render`, `set_comment`, `comment_text`,
  `line_text`, `to_markdown`, `to_json`, `load_json`), `Rendered`
  (`next_hunk`, `prev_hunk`, `hunk_at`), `Review::hunk_patch`, `store_name`. Pure; no I/O.
- `crates/vix-git/src/lib.rs`: `review_diff`, `apply_patch`/`ApplyTarget`.
- Stash chooser: `r` closes it and reviews `stash@{N}` (`tui/src/app/git_stash_worktree.rs`). Both git calls go through the workspace authority like every other `vix-git` runner.
- `tui/src/app/git_review.rs`: the five actions, the `PromptKind::GitReview`
  and `GitReviewComment` handlers, and the `ReviewState` kept in `App::review`.
- Tests: the unit tests in `vix-review`, and `tui/tests/integration/git.rs`
  (comments saved, exported and reloaded; hunk navigation; real git ranges,
  ignored by default).
