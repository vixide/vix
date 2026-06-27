# Stage Hunk

Git action `git.stage_hunk`.

Stage just the diff hunk under the cursor into the git index, leaving the rest of
the file's changes unstaged and the working tree untouched. Safe: it only stages
when the index still matches HEAD for the hunk's region (otherwise it reports
"index diverged" and does nothing).

Run it from **Git → Stage Hunk**, the command palette, or the action id
`git.stage_hunk`. Backed by `git::stage_content` (hash the new index blob and
point the index entry at it). The mirror operation is [unstage-hunk](../unstage-hunk/index.md).
