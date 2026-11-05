# Branch Review

Review a branch, a commit range, a commit or a stash as one multi-file diff,
and leave line comments that are saved with the repository and exported as
Markdown.

## Actions

All of these are in the **Git** menu and the command palette.

| Action id | Menu item | Does |
| --------- | --------- | ---- |
| `git.review` | Review Range… | Asks for a range and opens it in a read-only tab titled `review <range>`. |
| `git.review_comment` | Comment on Review Line… | Comments on the diff line under the cursor (pre-filled with the existing comment; empty text removes it). |
| `git.review_next` | Next Review Hunk | Next `@@` header, wrapping around. |
| `git.review_prev` | Previous Review Hunk | Previous `@@` header, wrapping around. |
| `git.review_export` | Export Review Comments | Writes `.vix/review/<range>.md` and `.json`. |
| `git.review_stage_hunk` | Stage Review Hunk | Stages the hunk under the cursor (`git apply --cached`). |
| `git.review_stage_all` | Stage All Review Hunks | Stages every text hunk of the open review at once; for a stash review, the whole stash. |
| `git.review_discard_hunk` | Discard Review Hunk… | After a yes/no, undoes the hunk under the cursor in the working tree (`git apply --reverse`). |
| `git.review_discard_all` | Discard All Review Hunks… | Undoes every text hunk of the open review in the working tree, after a confirmation that says how many hunks in how many files. |
| `git.review_stage_lines` | Stage Review Lines | Stages only the selected added and removed lines of one hunk. |
| `git.review_discard_lines` | Discard Review Lines… | Undoes only those lines in the working tree, after a confirmation. |
| `git.review_toggle_view` | Review Two-Pane View | Switches between the inline diff and a side-by-side view. `v` in the review does the same. |

The last nine work only in the review tab. Stage and discard hand the hunk's
patch to `git apply`, so they work when the hunk still fits the index or the
working tree (for example a review of the working-tree diff); otherwise git's
reason is shown and nothing changes. In **Git → Stash List…**, `r` opens the
highlighted stash in a review.

## Ranges

- `main..feature` and `main...feature` show a diff.
- A single revision (a commit, a tag, `stash@{0}`) shows what that revision
  changed.

Each changed file appears under a `=== path` heading with its hunks as git
printed them; each comment sits on its own `>> text` row under its line.

## Comments

A comment is tied to a file, a side (removed lines count on the old file) and
a line number, and keeps the line's text. One comment per line; commenting
again replaces it. They are stored in `.vix/review/<range>.json` under the
workspace root and reload when you review the same range again. If the diff
changes so a comment's line is gone, the comment stays in the file and the
export but is not shown.

The Markdown export groups comments by file, ordered by line, each followed
by its code line, ready to paste into a pull request or an agent prompt.

## Discard all

**Discard All Review Hunks…** asks first and names the size of what it will
undo ("all 7 hunks in 3 files"). It is all or nothing: git applies the whole
reverse patch or none of it, so if any hunk no longer fits (the file changed
since the review opened) nothing is discarded and git's reason is shown.
Binary files are not part of it. After a discard the review is diffed again.

## Staging or discarding single lines

Select some lines of one hunk (hold Shift and move the cursor; with no
selection the line under the cursor counts) and use **Git → Stage Review
Lines** or **Git → Discard Review Lines…**. Only the added and removed lines in
the selection are staged or undone; the rest of the hunk is left alone.
Context lines, headings and comments in the selection are ignored. A selection
that reaches into a second hunk is refused (select inside one hunk). Discard
asks first. If Git cannot apply the lines (the file changed since the review
opened) nothing changes and Git's reason is shown.

When a file's last line has no trailing newline, select both the removed and
the added copy of that line together; selecting only one is refused.

## Two-pane view

Press `v` in the review (or use **Git → Review Two-Pane View**) to see the old
text on the left and the new text on the right. A run of removed lines is
lined up with the added lines that replace it; a line with no counterpart
leaves the other side blank. Where a removed line and an added line are paired, the words that changed are
drawn reversed on both sides, so a one-word edit is easy to spot. Comments
appear under their group. The cursor,
hunk navigation, commenting and staging work as in the inline view, and both
views scroll together. With **View → Soft Wrap** on, long lines wrap within their own side and the
shorter side is padded, so the two sides stay on the same rows; with it off
they are cut at the edge. A click on a wrapped line's later row is a click on
that line. When the editor is narrower than 60 columns the inline
view shows instead. The `review_view` setting (`"inline"`, the default, or
`"side_by_side"`) chooses the view a review opens in.

In a split editor every pane that shows the review draws the two-pane view
(each pane is judged on its own width, so a narrow pane falls back to the inline
diff while a wide one keeps both sides). The view setting is shared by all panes
of the review.

The mouse works on the rows you see. A click on the left half puts the cursor
on that old line, a click on the right half on the new line (a click on a blank
half uses the other side's line). The wheel scrolls one row at a time and
carries the cursor along when it would otherwise leave the view.

See the specification at `crates/vix-review/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
