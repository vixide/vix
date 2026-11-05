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

## Two-pane view

Press `v` in the review (or use **Git → Review Two-Pane View**) to see the old
text on the left and the new text on the right. A run of removed lines is
lined up with the added lines that replace it; a line with no counterpart
leaves the other side blank. Comments appear under their group. The cursor,
hunk navigation, commenting and staging work as in the inline view, and both
views scroll together. When the editor is narrower than 60 columns the inline
view shows instead. The `review_view` setting (`"inline"`, the default, or
`"side_by_side"`) chooses the view a review opens in.

See the specification at `crates/vix-review/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
