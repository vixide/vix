# Git

Git integration: status/diff/staging via the git CLI.

Two layers:

- **Pure logic** (unit-tested, no I/O): [`parse_status`] turns
`git status --porcelain` output into [`FileStatus`] rows, and [`diff_marks`]
computes per-line editor-gutter [`LineMark`]s between the committed text and
the current buffer (via the `similar` crate).
- **Runners** (shell out to the `git` CLI): [`is_repo`], [`branch`],
[`status`], [`head_blob`], and the staging/commit helpers. Using the user's
own `git` means credential helpers, SSH agents, and hooks all behave exactly
as on the command line.
- **History** (T207): [`log`]/[`file_log`] (parsed by the pure, unit-tested
[`parse_log`]) list commits into a [`LogPanel`]; [`show_commit`]/
[`show_file_at`]/[`resolve_short_sha`] read a commit's diff or a file's
content at a revision. See [git-history](git-history/index.md).

## Sub-specs

- [diff-next](diff-next/index.md)
- [diff-previous](diff-previous/index.md)
- [git-history](git-history/index.md)
- [git-integration](git-integration/index.md)
- [git-panel](git-panel/index.md)
- [stage-hunk](stage-hunk/index.md)
- [toggle-diff-gutter](toggle-diff-gutter/index.md)
- [unstage-hunk](unstage-hunk/index.md)
- [word-diff](word-diff/index.md)
