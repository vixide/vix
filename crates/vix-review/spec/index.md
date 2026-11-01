# Branch review

Review a branch, a commit range, a commit or a stash as one multi-file diff,
with line comments that are saved per repository and exported as Markdown.

**Status:** Partly shipped (improvement plan T665). Shipped: the multi-file
buffer, hunk navigation, saved line comments and the Markdown/JSON export.
Not shipped: stage/discard from the review buffer (a hunk can still be staged
from the file itself with `git.stage_hunk`) and the two-pane view (T660's model
could be added later).

## Actions

All five are in **Git** and in the command palette.

| Action | Menu item | Does |
| --- | --- | --- |
| `git.review` | Review Range… | Prompts for a range and opens it in a read-only tab titled `review <range>`. |
| `git.review_comment` | Comment on Review Line… | Prompts for a comment on the diff line under the cursor, pre-filled with the existing one. Empty text removes it. On a comment row it edits that comment. Elsewhere it only says so in the status line. |
| `git.review_next` | Next Review Hunk | Moves to the next `@@` header, wrapping to the first. |
| `git.review_prev` | Previous Review Hunk | Moves to the previous `@@` header, wrapping to the last. |
| `git.review_export` | Export Review Comments | Writes the open review's comments to `.vix/review/<range>.md` and `.json`. |

The next, previous, comment and export actions work only in the review tab
(they say so in the status line elsewhere).

## Ranges

What the user types is checked like any ref name (no leading `-`, no
whitespace or control characters) and passed after `--end-of-options`.

- `a..b` and `a...b` run `git diff`.
- A single revision (a commit, a tag, `stash@{0}`) runs `git show` without the
  commit header, so it shows what that revision changed.

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
  (`next_hunk`, `prev_hunk`), `store_name`. Pure; no I/O.
- `crates/vix-git/src/lib.rs`: `review_diff`.
- `tui/src/app/git_review.rs`: the five actions, the `PromptKind::GitReview`
  and `GitReviewComment` handlers, and the `ReviewState` kept in `App::review`.
- Tests: the unit tests in `vix-review`, and `tui/tests/integration/git.rs`
  (comments saved, exported and reloaded; hunk navigation; real git ranges,
  ignored by default).
