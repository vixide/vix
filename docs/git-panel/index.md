# Git

Vix is Git-aware. It shows the state of your repository while you edit and lets
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

## The Git menu

The **Git** menu gathers the git actions:

- **Changes…** — open the Changes panel (see below).
- **Switch Branch…** — choose a local branch to check out.
- **Pull** / **Push** / **Fetch** — sync with the remote.

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
| `c`       | Commit (prompts for a one-line message; runs `git commit`) |
| `r`       | Refresh the status                                         |
| `Esc`     | Close the panel                                            |

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
for the hunk's region. See `spec/stage-hunk/index.md` and
`spec/unstage-hunk/index.md`.

## Blame

**Git → Blame Line** shows the cursor line's commit (hash, author, date, summary)
in the status bar. **Git → Toggle Inline Blame** turns on a dimmed end-of-line
annotation for the cursor's line (`author, date · summary`) that follows the
cursor as it moves between lines; it persists in the `inline_blame` setting (off
by default) and is shown in the normal, non-wrapped view.

## Roadmap

The core Git features are in place. Remaining ideas:

- Inline blame in the soft-wrap view (currently the non-wrapped view only).
- Interactive rebase / cherry-pick helpers.
