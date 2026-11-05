# Branch review

Review a branch, a commit range, a commit or a stash as one multi-file diff,
with line comments that are saved per repository and exported as Markdown.

**Status:** Shipped (improvement plan T665): the multi-file buffer, hunk
navigation, saved line comments, the Markdown/JSON export, stage/discard of
the hunk under the cursor, staging and discarding every hunk at once, `r` in
the stash chooser, re-diffing after a stage or discard, `\ No newline at end
of file` markers, and the two-pane (side-by-side) view, and line-level stage and discard. Not
shipped: a stash chooser as a range source other than `r`.

## Actions

All twelve are in **Git** and in the command palette.

| Action | Menu item | Does |
| --- | --- | --- |
| `git.review` | Review Range… | Prompts for a range and opens it in a read-only tab titled `review <range>`. |
| `git.review_comment` | Comment on Review Line… | Prompts for a comment on the diff line under the cursor, pre-filled with the existing one. Empty text removes it. On a comment row it edits that comment. Elsewhere it only says so in the status line. |
| `git.review_next` | Next Review Hunk | Moves to the next `@@` header, wrapping to the first. |
| `git.review_prev` | Previous Review Hunk | Moves to the previous `@@` header, wrapping to the last. |
| `git.review_export` | Export Review Comments | Writes the open review's comments to `.vix/review/<range>.md` and `.json`. |
| `git.review_stage_hunk` | Stage Review Hunk | Stages the hunk under the cursor: `Review::hunk_patch` rebuilds a one-hunk patch (file headers use `/dev/null` for a new or deleted file) and `vix_git::apply_patch(.., ApplyTarget::Index)` feeds it to `git apply --cached`. A binary file, or a cursor outside a hunk, only says so in the status line. |
| `git.review_stage_lines` | Stage Review Lines | Stages only the added and removed lines in the visual selection (or the line under the cursor) of one hunk. See *Line-level stage and discard* below. |
| `git.review_discard_lines` | Discard Review Lines… | The same lines, reversed in the working tree after a yes/no (`ConfirmKind::GitReviewDiscardLines`). |
| `git.review_stage_all` | Stage All Review Hunks | `Review::full_patch` joins every hunk of every text file (one header per file, binary files skipped) and `apply_patch(.., ApplyTarget::Index)` stages it in one call; for a stash review that is the whole stash. Says so in the status line when there is nothing to stage. |
| `git.review_discard_hunk` | Discard Review Hunk… | Same patch, after a yes/no (`ConfirmKind::GitReviewDiscard`), through `git apply --reverse` on the working tree. |
| `git.review_discard_all` | Discard All Review Hunks… | Reverses `Review::full_patch` on the working tree after a yes/no (`ConfirmKind::GitReviewDiscardAll`) whose text names `Review::patch_counts` (hunks, files). `git apply` is all or nothing, so a hunk that no longer fits leaves the tree untouched and git's reason is shown as an error; there is never a silent partial discard. Binary files are skipped (they are not in the patch). Succeeds, then re-diffs like the other discards. |
| `git.review_toggle_view` | Review Two-Pane View | Flips `ReviewState::side_by_side`; also bound to a plain `v` in the review tab (`App::review_view_key`, ahead of the read-only guard). Outside a review it only says so. |

The next, previous, comment, export, stage, stage-all, discard, discard-all and view actions work only in the review tab
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

## Line-level stage and discard

`Review::select_rows(rendered, first, last)` turns buffer rows (the visual
selection, else the cursor row) into a `LineSelection`: the indexes of the
added and removed lines in one hunk. Context lines, headings and comment rows
in the range are ignored. No changed line is `SelectError::NoLines`; lines of
two hunks or files are `SpansHunks` (select within one hunk).

`Review::lines_patch(sel, mode)` builds the reduced one-hunk patch the way
`git add -p` edits a hunk:

| | unselected `-` line | unselected `+` line |
| --- | --- | --- |
| `LineMode::Stage` (`git apply --cached`) | becomes context | dropped |
| `LineMode::Discard` (`git apply --reverse`) | dropped | becomes context |

The `@@` counts are recomputed from the kept lines. The side the patch is
matched against keeps its start (old to stage, new to discard) and the other
start follows git's convention for an empty side (the line before, or 0 for a
whole file); the function-context suffix is kept, and a hunk that now starts
or ends a whole file switches `/dev/null` headers on or off accordingly.
Selecting every changed line gives exactly `hunk_patch`.

A `\ No newline at end of file` marker travels with its line. If an
*unselected* line carrying the marker would have to become context, the patch
cannot say what the last line looks like, so `SelectError::NoNewline` refuses
with a status line asking for both the removed and the added copy; selecting
both stages them with the marker.

The host stages through `apply_patch(.., ApplyTarget::Index)` and discards
through `ApplyTarget::RevertWorktree` after the confirmation (the patch is
built before asking, so an inexpressible selection is refused first). When git
rejects the patch (the lines no longer match) its reason is shown as an error
and nothing changes; there is never a silent partial result. A success
re-diffs like the hunk actions.

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

## Two-pane view

`Review::side_by_side()` lays the review out as `SideBySide`, a list of
`PaneRow`s: `Heading` rows (title, file heading, `(binary file)`, hunk header)
and `Comment` rows span the width; `Pair` rows hold an optional old `Cell`
(left) and new `Cell` (right), each with its line number and text. Inside a
hunk a run of removed lines is zipped with the run of added lines that follows
it; the longer run leaves `None` on the other side. Context lines are on both
sides. Comments follow the group they belong to. Every row records the first
and last row of the inline buffer it stands for (`first`/`last`), computed by
counting rows exactly as `render` does (a test checks that every row indexes a
matching `Target`).

The inline buffer stays the source of the cursor, hunk navigation, comments
and stage/discard, because those all work on its rows. The host (rendering
only, `tui/src/ui/review_panes.rs`) shows the pairs instead of the buffer text
when `ReviewState::side_by_side` is on and the text area is at least
`MIN_SIDE_BY_SIDE_WIDTH` (60) columns; narrower, the inline buffer is drawn
and the choice is kept. The view starts at `SideBySide::row_for_inline` of the
buffer's top line and moves just far enough to keep the cursor's row visible,
so both views scroll together; the cursor's row is bold and underlined.
`SideBySide::view_start` is that computation (pure, shared by drawing and the
mouse), `halves(width)` the pane widths, `inline_at(row, Half)` the inline row
a click on a half stands for (each `Cell` records its own `inline` row) and
`top_for_row(row)` the inline top line that makes a row the first shown.

**Split panes.** Every editor pane that shows the review's tab draws it, not
only the focused one: `App::review_panes_for(tab, width)` gives each pane the
layout together with that tab's own wrap flag, cursor row and top row
(`PaneView`), and the width test is per pane, so a pane under
`MIN_SIDE_BY_SIDE_WIDTH` columns shows the inline buffer while a wider one
shows the pairs. The view choice itself (`ReviewState::side_by_side`, toggled
by `v`) belongs to the review, not to a pane: toggling changes every pane that
shows it. Clicking another pane focuses it and, before the click is mapped,
`layout.editor` is pointed at that pane's text area (it was only refreshed by
the next draw, which mapped the first click in a newly focused pane against the
previous pane's rectangle).

**Wrapping.** With the editor's soft wrap on (View → Soft Wrap, the review
tab's `soft_wrap_enabled`; there is no separate setting) a pair cell wraps
within its pane: `wrap_text(text, width - GUTTER)` splits it into char ranges
by display width (`GUTTER` is the 5 columns of line number), the first piece
carries the number and the rest a blank gutter. `PaneRow::height(left, right)`
is the longer side's piece count, the shorter side is padded with blank
cells, and so both panes stay row-aligned. Headings and comment rows are not
wrapped (they are cut). With wrap off every row is one line and long text is
cut. The pieces keep the word marks (a mark is a range of the whole text).
Everything that maps rows to screen lines goes through a `row_height`
function (`SideBySide::row_height(index, width, wrap)`): `view_start` keeps
the cursor's whole row in the view, `row_at(first, y, ..)` finds the row (and
which of its lines) under a screen line, which makes a click on a continuation
line mean its row, and `last_first(height, ..)` bounds the wheel.

**Word marks.** Where a removed line is zipped with an added line, `Cell::marks`
holds the char ranges that differ word by word, taken from
`vix_diff_view::build` run on the two lines (the same word-level emphasis
Compare uses). A side that differs from end to end gets none, and so do
context cells and unpaired lines. The host (`tui/src/ui/review_panes.rs`)
draws marked characters reversed and bold on top of the line's red or green;
the inline view is the ordinary editor and has no word marks.

**Mouse.** While the two-pane view is drawn, `App::review_mouse` takes the
editor's mouse events (the inline buffer and other tabs keep the ordinary
handling): a left click moves the cursor to `inline_at(row, half)` without
scrolling the hidden inline buffer; the wheel moves the first row by one, sets
the inline top line with `top_for_row`, and moves the cursor to the nearest
visible row when it would leave the view (so the keep-the-cursor-in-view rule
does not snap the scroll back); drags and releases are swallowed rather than
selecting text in the hidden buffer. Both panes share the rows, so a click or
the wheel means the same thing on either side. The
`review_view` setting (`"inline"` or `"side_by_side"`, anything else is
inline) picks the view a review opens in.

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
  (`next_hunk`, `prev_hunk`, `hunk_at`), `Review::hunk_patch`/`full_patch`/`patch_counts`, `Review::select_rows`/`lines_patch` (`LineSelection`, `LineMode`, `SelectError`), `Review::side_by_side` (`SideBySide`, `PaneRow`, `PaneKind`, `Cell`), `store_name`. Pure; no I/O.
- `Cargo.toml` depends on `vix-diff-view` for the word marks.
- `crates/vix-git/src/lib.rs`: `review_diff`, `apply_patch`/`ApplyTarget`.
- Stash chooser: `r` closes it and reviews `stash@{N}` (`tui/src/app/git_stash_worktree.rs`). Both git calls go through the workspace authority like every other `vix-git` runner.
- `tui/src/ui/review_panes.rs`: paints `SideBySide` (old left, new right).
- `tui/src/app/git_review.rs`: the review actions, the `PromptKind::GitReview`
  and `GitReviewComment` handlers, and the `ReviewState` kept in `App::review`.
- Tests: the unit tests in `vix-review`, and `tui/tests/integration/git.rs`
  (comments saved, exported and reloaded; hunk navigation; real git ranges,
  ignored by default).
