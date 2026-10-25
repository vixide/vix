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
as on the command line. [`staged_diff`] (T125) is one of these: it feeds
`git diff --staged` to the host's AI commit-message generator (the Git
panel's `g` key) — see `docs/menus/index.md`'s AI menu section.
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

## Gutter diff budget (T574)

`diff_marks` runs on the UI thread. It returns no marks when either side
exceeds 50,000 lines, and gives Myers a 40 ms deadline, after which the
diff degrades to a coarser (still valid) result rather than stalling a frame.

## Cargo features

- `process` (default on): the runners that start `git`. Off, the crate is only its data
  types and pure parsers (`parse_status`, `diff_marks`, `hunks`, `LogPanel`, …) and never
  starts a process. The root `git` feature enables it (`spec/cargo-features`, T595).

## Signing under a terminal pinentry (T570)

`pinentry_wants_tty(gpg_agent_conf)` reports whether the last `pinentry-program` line names
a curses or tty pinentry; `commit_signing_needs_tty(dir)` combines that with
`commit.gpgsign=true` (and a non-SSH `gpg.format`) for the user's `gpg-agent.conf`. Such a
pinentry opens `/dev/tty` itself, so under Vix's raw-mode screen it draws over the UI and
steals keystrokes. The host calls it before `git commit` and `git commit --amend`, and shows
`msg.git_signing_needs_tty` instead of running the commit.

## Gutter diff budget (T574)

`diff_marks` skips buffers over 50,000 lines and puts a 40 ms deadline on Myers. The
`tui/benches/gutter_diff.rs` Criterion bench keeps this honest: its `rewritten` rows (a few dozen
repeated lines in scrambled order, Myers' worst case) should stay near the deadline as the line
count grows, and `over_the_line_cap` should cost only the line count.


## Paths when Vix starts in a repository subdirectory (T650)

Every runner that takes a file path takes it **relative to `dir`** (the
workspace root), which may be a subdirectory of the working tree. Git reads
`HEAD:<path>` / `:<path>` blob names and `update-index --cacheinfo` paths from
the repository top level, and `git status --porcelain` prints top-level paths,
so the crate resolves `git rev-parse --show-toplevel --show-prefix` once per
directory (cached; only successes are cached), runs each path-taking command
with the top level as its working directory, and joins the prefix onto the
path (resolving `.`/`..` lexically). [`status`] converts its rows back, so a
[`FileStatus`] path is relative to `dir` too, `../`-prefixed for a changed file
outside it; those rows round-trip through [`stage`]/[`unstage`]. A path that
can't be mapped (absolute, or climbing above the top level) is passed to git
unchanged from `dir`, so git reports its own error.

## Literal pathspecs (T651)

Every runner sets `GIT_LITERAL_PATHSPECS=1`, so a path is always a file name
and never a glob: staging `app/[id]/p` (a Next.js-style route folder) no longer
also stages `app/i/p`. No caller relies on pathspec magic; one that ever needs
it opts back in with `:(glob)`.
