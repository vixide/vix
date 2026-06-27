# Unstage Hunk

Git action `git.unstage_hunk`.

Unstage just the diff hunk under the cursor from the git index, leaving the rest
of the file's staged changes and the working tree untouched. The mirror of
[stage-hunk](../stage-hunk/index.md): it replaces the hunk's working-tree lines in
the index blob with the committed text. Safe — it only unstages when the hunk's
lines are present in the index at the expected position.

Run it from **Git → Unstage Hunk**, the command palette, or the action id
`git.unstage_hunk`. Backed by `git::stage_content`.
