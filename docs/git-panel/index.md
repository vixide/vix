# Git

Vix™ is Git-aware. It shows the state of your repository while you edit and lets
you stage, commit, switch branches, and sync with remotes without leaving the
editor. Vix works by **shelling out to your own `git` CLI**, so your credential
helpers, SSH agents, and hooks behave exactly as they do on the command line.

Git state (whether the directory is a repo, the current branch, the list of
changed files, and the committed file contents) is cached and refreshed
automatically at startup, after each save, and after every git action. You can
also refresh it on demand from the Changes panel.

## Awareness

Vix surfaces the state of your working tree in three places, all of which update
as you work.

### Status bar

The status bar shows the current branch name. When the working tree has changes,
a dirty dot (`•`) appears next to the branch name. **Click the branch indicator
to open the Git Changes panel.**

### File explorer badges

Changed tracked files carry a colored one-letter badge in the file explorer:

| Badge | Meaning    | Color   |
| ----- | ---------- | ------- |
| `M`   | modified   | yellow  |
| `A`   | added      | green   |
| `?`   | untracked  | green   |
| `D`   | deleted    | red     |
| `R`   | renamed    | cyan    |
| `U`   | conflicted | magenta |

### Editor diff gutter

In the line-number gutter, the editor draws a colored bar for each line that
differs from its committed (HEAD) version:

- **green** — added line
- **yellow** — modified line
- **red** — deleted line

The gutter is computed by diffing the current buffer against its cached HEAD
contents, and is drawn in both the normal and the **soft-wrap** view (in soft-wrap,
a changed logical line shows its bar on its first visual row).

The [coverage gutter](../coverage/index.md) uses this same column to show
test coverage instead; the two share it, so only one shows per buffer at a
time.

## The Git menu

The **Git** menu gathers the git actions:

- **Changes…** — open the Changes panel (see below).
- **Switch Branch…** — choose a local branch to check out.
- **Pull** / **Push** / **Fetch** — sync with the remote.
- **Stash List…** and **Worktree ▸** — manage stashes and worktrees (see
  below).
- **Rebase ▸** — interactive rebase, continue/abort, and cherry-pick (see
  below).

Every Git menu item is also available from the command palette (for example,
"Git: Changes").

## Changes panel

Open the Changes panel from **Git → Changes…**, from the command palette, or by
**clicking the branch indicator in the status bar**.

The panel is a modal list of changed files. Each row shows a staged checkbox
(`[✓]`) and the file's change letter (`M`/`A`/`?`/`D`/`R`/`U`). The active branch
appears in the panel title (and in the status bar).

### Keybindings

| Key       | Action                                                     |
| --------- | ---------------------------------------------------------- |
| `↑` / `↓` | Move the selection                                         |
| `Space`   | Toggle the selected file staged / unstaged                 |
| `s`       | Stage the selected file                                    |
| `u`       | Unstage the selected file                                  |
| `Enter`   | Open the selected file's changes as a diff against HEAD    |
| `d`       | Discard the selected file's unstaged changes (asks first)  |
| `c`       | Commit (prompts for a one-line message; runs `git commit`) |
| `r`       | Refresh the status                                         |
| `Esc`     | Close the panel                                            |

Discard restores the file from the index (`git restore --worktree`), so staged
changes are kept. It refuses an untracked file rather than deleting it.

**Git → Diff with HEAD** and **Git → Diff with Staged** open the active
file's diff against its committed or staged version, including unsaved edits.

Commit is available only when something is staged. The commit prompt accepts a
**multi-line** message: press `Enter` to commit, or `Alt+Enter` to start a new
line (e.g. a subject line, a blank line, then a body).

### Mouse

A **left click on a row toggles its staged state.**

### Refreshing

Vix re-reads `git status` when the panel opens, after each save, and after every
git action, so changes you make on the command line are reflected. Press `r` to
refresh on demand.

## Switch Branch

**Git → Switch Branch…** lists your local branches in a chooser. Press `Enter`
to check out the highlighted branch (Vix runs `git switch`), then Vix refreshes
its cached state and the file explorer.

Open buffers are reloaded automatically: after the switch, every **clean**
(unmodified) open file is re-read from disk so it reflects the new branch.
Buffers with unsaved edits are left untouched, and a notification reports how many
files were reloaded.

## Pull, Push, and Fetch

**Pull**, **Push**, and **Fetch** run `git pull`, `git push`, and `git fetch`
through the async Run Command pipeline. Their output streams to the bottom dock,
and the cached git state refreshes when the command finishes.

## Examples

Stage a file and commit it:

1. Click the branch indicator in the status bar (or use **Git → Changes…**).
2. Use `↑` / `↓` to select the file, then press `s` to stage it.
3. Press `c`, type a one-line commit message, and confirm.

Switch to another branch:

1. Open **Git → Switch Branch…**.
2. Highlight the branch and press `Enter`.

## Per-hunk staging

Beyond whole-file staging in the panel, you can stage, unstage, and revert
individual diff hunks straight from the editor — put the cursor inside the hunk,
then:

- **Git → Stage Hunk** (`git.stage_hunk`) stages just that hunk into the index.
- **Git → Unstage Hunk** (`git.unstage_hunk`) removes just that hunk from the
  index.
- **Git → Revert Hunk** (`git.revert_hunk`) restores the committed text for it.
- **Git → Diff Next / Previous** jump between changed hunks.

Hunk staging is safe: it only acts when the index still matches the expected text
for the hunk's region. See `crates/vix-git/spec/stage-hunk/index.md` and
`crates/vix-git/spec/unstage-hunk/index.md`.

## Blame

**Git → Blame Line** shows the cursor line's commit (hash, author, date, summary)
in the status bar. **Git → Toggle Inline Blame** turns on a dimmed end-of-line
annotation for the cursor's line (`author, date · summary`) that follows the
cursor as it moves between lines; it persists in the `inline_blame` setting (off
by default) and is shown in the normal, non-wrapped view. Both blame the
buffer as you see it, so with unsaved edits the line numbers still match and
edited lines show as not committed yet.

## History

Beyond the plain streamed-to-the-dock **Log** views (**All**, **Graph**,
**Since…**), **Git → Log** has three interactive ones:

- **Browse Log…** lists recent commits (hash, date, author, subject);
  `↑`/`↓` navigate, `Enter` (or a click) opens the highlighted commit's
  diff in a read-only tab.
- **File History** is the same list, scoped to the active file
  (`git log --follow`, so renames are tracked) — `Enter` opens that
  commit's diff for just this file.
- **Open File at Revision…** prompts for a revision (a branch, tag, or any
  commit-ish) and opens the active file's content there, read-only, titled
  `file @ abbrev`.

None of these three write anything — they're all read-only views for
looking at history, not editing it.

## Stash list

**Git → Stash** and **Git → Stash Pop** stash the working tree and restore
the most recent stash. **Git → Stash List…** lists every stash
(`stash@{0}` is the newest) in a chooser:

| Key                | Action                                                     |
| ------------------ | ---------------------------------------------------------- |
| `↑` / `↓`          | Move the selection                                         |
| `Enter` (or click) | Open the stash's diff in a read-only tab titled `stash@{N}` |
| `a`                | Apply the stash and keep it in the list                    |
| `p`                | Pop the stash: apply it, then remove it from the list      |
| `d` / `Delete`     | Drop the stash, after a yes/no question                    |
| `Esc`              | Close the chooser                                          |

After an apply or pop, unmodified open files are reloaded from disk. A
conflict is reported in the message drawer and the stash is kept.

## Worktrees

A worktree is a second checkout of the same repository in its own folder,
usually on its own branch, so you can work on two branches at once. The
**Git → Worktree** submenu manages them:

- **Add…** asks for a folder (relative to the workspace, such as
  `../my-feature`) and runs `git worktree add`. git creates a new branch
  named after the folder, starting at the current commit.
- **List…** shows every worktree: its folder, its branch (or `detached` /
  `bare`), and whether it is the current one, locked, or missing. `Enter`
  switches the workspace to the highlighted worktree, `a` adds one, and `d`
  removes one.
- **Remove…** shows the same list; `Enter` removes the highlighted worktree.

Switching works like **Project → Switch…**: Vix saves the current
workspace's session and restores the worktree's own, so each worktree
remembers its own open files. Removing asks first, and git refuses to remove
a worktree with uncommitted changes or untracked files. You can't remove the
main worktree, or the one you have open, from here.

In a remote workspace, stash and worktree commands run on the remote host.

## Interactive rebase

**Git → Rebase → Interactive Rebase…** rewrites recent history. It asks for
a base: a branch, a tag, or a commit such as `HEAD~3`. Then it lists every
commit after that base, oldest first, each marked `pick`:

| Key               | Action                                                       |
| ----------------- | ------------------------------------------------------------ |
| `↑` / `↓`         | Move the selection                                           |
| `Alt+↑` / `Alt+↓` | Move the selected commit earlier / later                     |
| `p`               | **pick**: keep the commit as it is                           |
| `r`               | **reword**: type a new message for it (one line; it replaces the whole message) |
| `e`               | **edit**: stop after this commit so you can change it        |
| `s`               | **squash**: meld into the commit above, keeping both messages |
| `f`               | **fixup**: meld into the commit above, dropping this message |
| `d`               | **drop**: leave the commit out                               |
| `Enter`           | Run the rebase                                               |
| `Esc`             | Close without changing anything                              |

No editor opens: Vix hands git the list you made. The first commit can't be
a squash or fixup, because there is nothing above it to meld into. The tree
must be clean, as for any `git rebase`.

The rebase **stops** at an `edit`, or when a commit doesn't apply cleanly.
Change the files, stage them (or resolve the conflict), then choose
**Git → Rebase → Continue Rebase**. **Abort Rebase** gives up and puts the
branch back where it was.

## Cherry-pick

**Git → Rebase → Cherry-Pick…** asks for a branch (or any revision), then
lists that branch's commits that the current branch doesn't have, in the
Browse Log panel. `Enter` shows a commit's diff; `c` applies it on top of
the current branch (`git cherry-pick`). If it conflicts, git's message
appears in the message drawer; resolve the conflict and commit as usual.

## Roadmap

The core Git features are in place. Remaining ideas:

- Inline blame in the soft-wrap view (currently the non-wrapped view only).

---

Vix™ and Vix IDE™ are trademarks.
