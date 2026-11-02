# Git

Git integration: status/diff/staging via the git CLI.

Two layers:

- **Pure logic** (unit-tested, no I/O): [`parse_status`] turns
`git status --porcelain` output into [`FileStatus`] rows, and [`diff_marks`]
computes per-line editor-gutter [`LineMark`]s between a base text (HEAD by
default; the index, the saved file or a branch via [`GutterBase`] and the
`git_gutter_base` setting, T664 -- see
[git-integration](git-integration/index.md)) and the current buffer (via the
`similar` crate).
- **Runners** (shell out to the `git` CLI): [`is_repo`], [`branch`],
[`status`], [`head_blob`], and the staging/commit helpers. Using the user's
own `git` means credential helpers, SSH agents, and hooks all behave exactly
as on the command line. [`staged_diff`] (T125) is one of these: it feeds
`git diff --staged` to the host's AI commit-message generator (the Git
panel's `g` key) — see `docs/menus/index.md`'s AI menu section. With
`ai = "off"` (T730) `g` warns that AI is off and runs neither `git diff` nor
the assistant.
[`run`] (T637) is the generic one: any argument list, run through the same
`PATH`-resolved `git` binary, stdout on success or a [`GitError`]. Its one
caller outside this crate is the Rhai plugin installer
(`crates/vix-script/spec/index.md`, "Plugins"), which needs
`clone`/`fetch`/`checkout`/`rev-parse` and has no dedicated runner for them.
- **History** (T207): [`log`]/[`file_log`] (parsed by the pure, unit-tested
[`parse_log`]) list commits into a [`LogPanel`]; [`show_commit`]/
[`show_file_at`]/[`resolve_short_sha`] read a commit's diff or a file's
content at a revision. See [git-history](git-history/index.md).
- **Review** (T665): [`review_diff`] gives the unified diff of a revision
range for **Git → Review Range…**; the multi-file model, comments and export
are in the `vix-review` crate (see
[its spec](../../vix-review/spec/index.md)).
- **Stash and worktrees** (T634): [`parse_stash_list`]/
[`parse_worktree_list`] and their runners; see the section below.
- **Interactive rebase and cherry-pick** (T633): [`parse_todo`]/
[`serialize_todo`]/[`check_todo`]/[`plan`] and the [`RebaseTodo`] overlay
state (pure), with runners to start, continue and abort a rebase and to
cherry-pick a commit; see the section below.

## Stash list and worktrees (T634)

| Action                | Menu                         | Behavior |
| --------------------- | ---------------------------- | -------- |
| `git.stash_list`      | Git → Stash List…            | A chooser over `git stash list`: `Enter` shows the stash's diff (`git stash show -p`) in a read-only tab titled `stash@{N}`, the same as Browse Log; `a` applies it (`git stash apply`, kept); `p` pops it; `d`/`Delete` drops it after a confirmation |
| `git.worktree_add`    | Git → Worktree → Add…        | Prompt for a folder (relative to the workspace root, or absolute) and `git worktree add <path>`: a new branch named after the folder, from `HEAD` |
| `git.worktree_list`   | Git → Worktree → List…       | A chooser over `git worktree list --porcelain`, the current worktree highlighted and marked; `Enter` switches the workspace to it, `d` removes one after a confirmation, `a` adds one |
| `git.worktree_remove` | Git → Worktree → Remove…     | The same chooser, where `Enter` removes the highlighted worktree after a confirmation |

- **Pure parsers.** [`parse_stash_list`] reads the default `%gd: %gs` lines
  into [`StashEntry`] rows (index + message), skipping anything that isn't a
  `stash@{N}` line. [`parse_worktree_list`] reads the porcelain records into
  [`Worktree`]s: path, `HEAD`, a [`Checkout`] (branch without `refs/heads/`,
  detached, or bare), and the `locked`/`prunable` marks; unknown attribute
  lines are ignored so a newer git can't break it.
- **Runners.** [`stash_list`], [`stash_apply`], [`stash_pop_at`],
  [`stash_drop`], [`stash_show`] name a stash only as `stash@{N}` built from
  its index, so no typed text reaches git. [`worktree_list`],
  [`worktree_add`], [`worktree_remove`] refuse an empty, option-shaped
  (`-…`) or control-character path with `GitError::InvalidArgument` before
  starting git. Nothing passes `--force`: git itself refuses to remove a
  worktree with changes or untracked files, and that refusal is shown.
- **Destructive actions confirm first.** Dropping a stash and removing a
  worktree go through a yes/no `Confirm` (`ConfirmKind::GitStashDrop`,
  `GitWorktreeRemove`); the chooser closes under the question and comes back
  after it, either answer. The main worktree (the first record) and the one
  holding the workspace can't be removed from the chooser.
- **Switching reuses the project switch.** `Enter` on another worktree calls
  the same `App::switch_workspace` as **Project → Switch…**: it saves the
  current workspace's session, re-roots the explorer, LSP and git state, and
  restores the worktree's own saved session, so each worktree keeps its own
  open tabs. A bare or missing (`prunable`) worktree can't be opened.
- **Remote workspaces.** Every runner goes through `git_command`, so under a
  remote authority the stash and worktree commands run on the remote host,
  and the worktree paths git prints are that host's paths, which is what the
  authority opens on a switch.

Code: `crates/vix-git/src/stash.rs`, `crates/vix-git/src/worktree.rs`; the
choosers in `tui/src/app/git_stash_worktree.rs` and
`tui/src/ui/choosers.rs` (`draw_stash_chooser`, `draw_worktree_chooser`).
Tests: the parsers' unit tests, `crates/vix-git/tests/stash_worktree.rs`
(the runners against a temporary repository), and
`tui/tests/integration/git.rs` (the choosers end to end).

## Interactive rebase and cherry-pick (T633)

| Action                   | Menu                                  | Behavior |
| ------------------------ | ------------------------------------- | -------- |
| `git.rebase_interactive` | Git → Rebase → Interactive Rebase…    | Prompt for a base (a branch, tag, or commit such as `HEAD~3`), then open the todo overlay over the commits after it, oldest first; `Enter` runs `git rebase -i` with the edited list |
| `git.rebase_continue`    | Git → Rebase → Continue Rebase        | `git rebase --continue` after an `edit` stop or a resolved conflict |
| `git.rebase_abort`       | Git → Rebase → Abort Rebase           | `git rebase --abort`: the branch goes back where it was |
| `git.cherry_pick`        | Git → Rebase → Cherry-Pick…           | Prompt for a branch (or any revision), then list its commits that `HEAD` lacks in the Browse Log panel; `c` cherry-picks the highlighted one, `Enter` shows its diff |

The todo overlay's keys:

| Key              | Effect                                                         |
| ---------------- | -------------------------------------------------------------- |
| `↑` / `↓`        | Move the highlight                                             |
| `Alt+↑` / `Alt+↓` | Move the highlighted commit one row earlier / later           |
| `p`              | `pick`: keep the commit                                        |
| `r`              | `reword`: prompt for a new message (one line, replacing the whole message; prefilled with the subject); an empty answer keeps the commit's own message |
| `e`              | `edit`: stop after the commit so it can be amended             |
| `s`              | `squash`: meld into the commit above, keeping both messages    |
| `f`              | `fixup`: meld into the commit above, dropping this message     |
| `d`              | `drop`: leave the commit out (shown dimmed)                    |
| `Enter`          | Run the rebase                                                 |
| `Esc`            | Close without rebasing                                         |

- **No editor ever opens.** [`rebase_interactive`] runs `git rebase -i
  <base>` with `GIT_SEQUENCE_EDITOR` set to `printf '%s' "$VIX_REBASE_TODO"
  >`, which writes the prepared list over git's own, and `GIT_EDITOR=true`,
  which accepts the combined message git proposes for a `squash`. The list
  travels in the `VIX_REBASE_TODO` environment variable rather than a
  temporary file, so a remote host gets it too. This is the same trick as
  **Branch → Edit Description…**'s throwaway `GIT_EDITOR`. Every variable
  is set on the spawned git process alone (through `ProcessSpec::env`),
  never on Vix's own environment, which `unsafe_code = "forbid"` rules out
  anyway.
- **A typed reword message is never parsed by a shell.** [`plan`] turns a
  `reword` that has a message into `pick <sha>` followed by
  `exec git commit --amend --allow-empty -m "$VIX_REWORD_<n>"`, the message
  in that environment variable. A `reword` without a message stays a
  `reword`, which `GIT_EDITOR=true` leaves unchanged.
- **The list is Vix's, not git's.** [`rebase_commits`] resolves the base
  (`rev-parse --verify <base>^{commit}`, after refusing an empty or
  option-shaped one) and lists `<base>..HEAD` oldest first without merges,
  each a `pick`; that list replaces whatever git would have written, so
  `rebase.autoSquash` or `rebase.updateRefs` don't apply here.
- **Checked before git runs.** [`check_todo`] refuses a list whose first
  kept commit is a `squash` or `fixup` (git would stop with "cannot squash
  without a previous commit"); the overlay stays open with a status message.
- **Pure todo format.** [`parse_todo`] reads git's format: long or one-letter
  command words, `#` comments and blank lines skipped, and any other command
  (`exec`, `break`, `label`, `update-ref`, `fixup -C`, …) kept verbatim as
  [`TodoItem::Other`]; [`serialize_todo`] writes it back, so the two round-trip.
- **Stops.** An `edit`, or a conflict, leaves the rebase stopped
  ([`rebase_in_progress`]: git's `rebase-merge` or `rebase-apply` directory
  exists). The status bar says to use **Continue Rebase** or **Abort
  Rebase**; while one is stopped **Interactive Rebase…** won't start another.
  [`rebase_continue`] also runs with `GIT_EDITOR=true`.
- **Cherry-pick.** [`cherry_pick_candidates`] lists `HEAD..<rev>` (at most
  200), shown in the Browse Log panel with [`LogScope::Pick`];
  [`cherry_pick`] runs `git cherry-pick --end-of-options <sha>`. A conflict
  is git's error, shown in the message drawer.
- **Signing.** Rebasing, continuing and cherry-picking all write commits, so
  each first makes the same terminal-pinentry check as a commit (T570, below).
- After every run the git state and unmodified open buffers are refreshed.

Code: `crates/vix-git/src/rebase.rs`; the overlay and actions in
`tui/src/app/git_rebase.rs`, drawn by `draw_git_rebase` in
`tui/src/ui/choosers.rs`. Tests: the unit tests in `rebase.rs` (the todo
round trip, the plan, reordering), `crates/vix-git/tests/rebase.rs` (squash,
reorder/fixup/reword/drop, edit with continue and abort, and cherry-pick
against a temporary repository), and `tui/tests/integration/git.rs` (the
overlay and the cherry-pick list end to end).

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
so the crate resolves `git rev-parse --show-cdup --show-prefix` once per
directory (cached; only successes are cached), runs each path-taking command
with the top level as its working directory, and joins the prefix onto the
path (resolving `.`/`..` lexically). [`status`] converts its rows back, so a
[`FileStatus`] path is relative to `dir` too, `../`-prefixed for a changed file
outside it; those rows round-trip through [`stage`]/[`unstage`]. A path that
can't be mapped (absolute, or climbing above the top level) is passed to git
unchanged from `dir`, so git reports its own error.

The top level is `dir` joined with `--show-cdup` (`sub/../`), not git's
absolute `--show-toplevel` (T715): that is the path as the machine running
git spells it, which under a remote or fake authority need not be one of the
authority's paths. Changing into `dir/sub/../` resolves `..` physically (a
local `chdir`, the SSH authority's `cd -P`), so it is the same directory,
symlinks included.

## Where git runs (T710, T715)

Every runner builds its `git` with `git_command`, through
`vix_authority::current().command(…)`: under the SSH authority git runs on
the remote host, in the remote working directory, and every path in its
output is a remote path relative to `dir`. Nothing here reads the
repository's files directly (the one local file read, `gpg-agent.conf`, is
the user's own signing setup). `tui/tests/authority_search.rs` runs
`status` and `head_blob` under a `FakeAuthority` and asserts the `git`
`command` call; `tui/tests/authority_ssh.rs` runs status over a fake `ssh`.

## Literal pathspecs (T651)

Every runner sets `GIT_LITERAL_PATHSPECS=1`, so a path is always a file name
and never a glob: staging `app/[id]/p` (a Next.js-style route folder) no longer
also stages `app/i/p`. No caller relies on pathspec magic; one that ever needs
it opts back in with `:(glob)`.
