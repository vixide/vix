# Tutorial 4: The Git Workflow

This tutorial walks through Vix's git integration end to end: watching your
working tree change live, reading a diff without leaving the editor, staging
whole files or single hunks, discarding a change you don't want, writing a
real commit message, browsing history, switching branches, and resolving a
merge conflict.

For the full reference (every action, every keybinding, every panel field),
see [`docs/git-panel/index.md`](../../git-panel/index.md). This tutorial is
example-driven — it walks you through doing each thing once, in order,
against real files.

Vix shells out to your own `git` CLI for every write, so your credential
helpers, SSH agents, and hooks behave exactly as they do on the command
line. Nothing below needs a remote — everything happens locally.

## Two ways to practice

You can do this tutorial two ways. Pick whichever you're more comfortable
with, or do both:

1. **In the vix repo you're reading this from.** It's a real git repo, so
   the status dock, diff gutter, and history views all have real data to
   show you immediately. Every edit suggested below is trivial and
   reversible (a blank line, a scratch comment) — **discard it before you
   move on, and don't commit throwaway changes to a repo you care about.**
2. **In a scratch repo.** If you'd rather not touch a real repo at all
   (useful for the staging/commit/conflict steps later in this tutorial,
   which do create real commits and branches):

   ```sh
   mkdir -p /tmp/vix-git-tutorial && cd /tmp/vix-git-tutorial
   git init
   echo "hello" > notes.txt
   git add notes.txt
   git commit -m "initial commit"
   vix .
   ```

   Everything from here on works the same way in either repo — swap in
   `notes.txt` wherever the text below says "the file you changed."

## Git awareness: three places that update live

Open the vix repo (`vix .` from its root, or `vix README.md`) and watch
three places:

- **The status bar** shows the current branch name. The moment your working
  tree has any uncommitted change, a dirty dot (`•`) appears next to it.
  **Click the branch indicator to jump straight to the Changes panel** —
  that's the fastest way in.
- **The file explorer** (`Ctrl+B` to toggle it) puts a colored one-letter
  badge on every changed tracked file: `M` modified, `A` added, `?`
  untracked, `D` deleted, `R` renamed, `U` conflicted.
- **The editor's line-number gutter** draws a colored bar next to every
  changed line: green for added, yellow for modified, red for deleted —
  computed by diffing the buffer against the file's committed (HEAD)
  content.

Try it now: open `README.md` and add a blank line at the end. Watch the
dirty dot appear in the status bar, the `M` badge appear on `README.md` in
the explorer, and a colored bar appear in the gutter next to the line you
added. Undo it (`Ctrl+Z`) before continuing, or press `Ctrl+Z` again if
you'd already saved — the gutter and badge clear the moment the file
matches HEAD again.

## Viewing a diff inline

You don't need a separate diff view to read a change — the gutter *is* the
diff, and you can jump between changed hunks without leaving the buffer.

1. Make a real (still trivial) edit: open any source file and change one
   word on one line, without saving.
2. Run **Git → Next Change** (action id `git.diff_next`; there's no default
   keybinding for it in any keymap, so reach it from the command palette
   — `Ctrl+P` then `> git next change` — or the Git menu). Your cursor jumps
   to the start of the next changed hunk below it.
3. **Git → Previous Change** (`git.diff_prev`) goes the other way. With only
   one hunk changed, both wrap around to the same spot — make a couple more
   edits in different parts of the file to see them step between hunks.

This is the same gutter the [coverage gutter](../../coverage/index.md)
uses when test coverage is displayed instead — only one shows per buffer at
a time.

If what you want is a diff against an arbitrary *other file* (not git HEAD)
— a backup, a sibling config — that's a different feature,
**Tools → Compare With File…**; see
[`docs/diff-view/index.md`](../../diff-view/index.md).

## The Changes panel

Open it with **Git → Changes…**, the command palette ("Git: Changes"), or
by clicking the branch indicator in the status bar. It lists every changed
file with a staged checkbox and its change letter:

| Key       | Action                                       |
| --------- | --------------------------------------------- |
| `↑` / `↓` | Move the selection                            |
| `Space`   | Toggle the selected file staged / unstaged    |
| `s`       | Stage the selected file                       |
| `u`       | Unstage the selected file                     |
| `c`       | Commit (prompts for a message, then runs `git commit`) |
| `r`       | Refresh the status                            |
| `Esc`     | Close the panel                               |

A left click on a row toggles its staged state too.

### Staging a whole file

In your scratch repo (or the vix repo with your reverted-later edit still
in place), change `notes.txt`, save it, then:

1. **Git → Changes…**.
2. Select the row for the file you changed and press `s` to stage it —
   its checkbox flips to `[✓]`.
3. Press `u` on the same row to unstage it again. `Space` does either,
   toggling from wherever it currently is.

### Staging (and unstaging) individual hunks

You don't have to stage a whole file — you can stage just the hunk your
cursor is sitting in and leave the rest of that file's changes untouched.
This works from inside the editor, not the Changes panel:

1. Make **two separate changes** in one file (two edits far enough apart
   that they land in different diff hunks).
2. Put the cursor inside the first change, then run **Git → Stage Hunk**
   (`git.stage_hunk`, from the command palette or Git menu). Only that
   hunk moves into the index; open **Git → Changes…** and the file shows
   as partially staged.
3. Run **Git → Unstage Hunk** (`git.unstage_hunk`) with the cursor back in
   that same hunk to remove just it from the index again.

Hunk staging is safe by design: it only acts when the index still matches
the expected text for that hunk's region, and reports "index diverged"
rather than guessing if something else already changed it underneath you.

## Discarding a change

Vix's discard tool works the same way staging does — per hunk, from the
cursor position, not as a single "discard this whole file" button:

1. Put the cursor inside a changed hunk you don't want.
2. Run **Git → Revert Hunk** (`git.revert_hunk`). It rewrites the buffer
   back to the committed text for just that hunk.

This edits the **in-memory buffer**, not the file on disk — the change
shows as reverted (dirty, ready to save) rather than being silently
written out. Press `Ctrl+S` to make it stick, or `Ctrl+Z` if you change
your mind before saving.

To discard every change in a file, use **Git → Next Change** /
**Git → Revert Hunk** together, hunk by hunk, until the gutter is clear —
or, since Vix is just driving your own `git`, run `git restore <file>` on
the command line and let Vix pick up the change (it re-reads git status
after every save and action, and on demand with `r` in the Changes panel).

## Committing

Stage something first (a commit is only offered once something is staged),
then from the Changes panel press `c`. The prompt is **multi-line**:

- Plain `Enter` submits and runs `git commit`.
- `Alt+Enter` inserts a newline — type a short subject line, `Alt+Enter`
  twice for a blank line, then a longer body, the same shape as a
  hand-written commit message on the command line.

Try it in your scratch repo:

```sh
# already staged notes.txt from the section above? if not:
echo "second line" >> notes.txt
```

1. **Git → Changes…**, select `notes.txt`, press `s` to stage it.
2. Press `c`.
3. Type `Add a second line`, then `Enter` to commit.

The panel refreshes and the file drops off the list — it matches HEAD
again.

## Viewing log and history

**Git → Log** has two kinds of view. The plain ones (**All**, **Summary**,
**Since 1 day/week/month ago**) stream raw `git log` text straight to the
bottom dock — good for a quick look, no interaction. The other three are
interactive, and all of them open their result as a **read-only tab**, not
the dock:

- **Git → Log → Browse Log…** (`git.browse_log`) lists recent commits
  (hash, date, author, subject). `↑`/`↓`/`PageUp`/`PageDown` navigate;
  `Enter` (or a click) opens that commit's full diff (`git show <sha>`)
  in a read-only tab titled `<abbrev> log`.
- **Git → Log → File History** (`git.file_history`) is the same list
  scoped to the file in your active tab (`git log --follow`, so renames
  are tracked). `Enter` opens that commit's diff for **just this file**,
  titled `<abbrev> <filename>`.
- **Git → Log → Open File at Revision…** (`git.open_at_revision`)
  prompts for a revision — a branch, tag, or any commit-ish like `HEAD~3`
  — and opens the active file's content *at that revision*, read-only,
  titled `<filename> @ <abbrev>`.

None of the three write anything; they're read-only windows onto history.
Try it: open `README.md` in the vix repo, then **Git → Log → File
History**, and press `Enter` on any commit to see that file as it was.
`Ctrl+S` does nothing on that tab — it's marked read-only, so there's
nothing to accidentally overwrite.

## Switching branches

**Git → Branch → Switch…** lists your local branches in a chooser;
`↑`/`↓` to highlight one, `Enter` checks it out (`git switch`). Vix then:

- Refreshes the cached branch/status state and the file explorer's badges.
- Reloads every **clean** (no unsaved edits) open tab from disk so it
  reflects the new branch, and reports how many files it reloaded.
- Leaves any **dirty** tab untouched — it does not silently discard your
  unsaved edits just because you switched branches.

**Git → Branch → New…** creates a new branch (prompts for a name);
**Git → Branch → Merge…** merges a named branch into the current one —
which is how the conflict in the next section gets created.

## Resolving a merge conflict

This is easiest to see with a real conflict, so do this part in the
scratch repo (it makes a diverging branch and a merge on purpose):

```sh
cd /tmp/vix-git-tutorial
git checkout -b feature
echo "feature line" >> notes.txt
git commit -am "feature: add a line"
git checkout main
echo "main line" >> notes.txt
git commit -am "main: add a different line"
git merge feature   # conflicts on notes.txt
vix notes.txt
```

`notes.txt` now has real conflict markers in it:

```text
<<<<<<< HEAD
main line
=======
feature line
>>>>>>> feature
```

The file explorer shows notes.txt with the `U` (conflicted) badge. In the
editor:

1. **Git → Next Conflict** (`git.conflict_next`) moves the cursor to the
   start of the conflict block (repeat to cycle through more than one).
2. With the cursor inside the block, pick a resolution from
   **Git → Resolve**:
   - **Keep Ours** (`git.conflict_ours`) — keeps the `HEAD` side, drops
     the markers and the other side.
   - **Keep Theirs** (`git.conflict_theirs`) — keeps the incoming side.
   - **Keep Both** (`git.conflict_both`) — keeps both sides, markers
     removed, one after the other.
3. Save (`Ctrl+S`). The buffer no longer has conflict markers.
4. Stage and commit the resolution the normal way — **Git → Changes…**,
   `s` on `notes.txt`, `c`, and a message like `Merge feature`.

The conflict tool only parses and rewrites the block under the cursor; it
doesn't touch git's merge state itself, so you still stage and commit the
result exactly as you would after resolving by hand.

## Jujutsu (jj), briefly

If a workspace has a `.jj` directory, the **JJ** menu mirrors the Git
menu's shape for [Jujutsu](https://github.com/jj-vcs/jj) instead — log
views, `New Change`, `Describe…`, `Commit…`, bookmarks (jj's equivalent of
branches), and `jj git push`/`fetch` for repos colocated with git. Every
JJ action streams its command output to the bottom dock the same way
Git's Pull/Push/Fetch do. **JJ → Init** creates a jj repo, colocated with
an existing `.git` if one is present. This tutorial doesn't cover jj
further — the menu structure above is the current extent of the
documentation for it.

## Where to go next

- [`docs/git-panel/index.md`](../../git-panel/index.md) — the full
  reference: blame and inline blame, stash, amend, pull strategies
  (fast-forward / rebase / merge / squash), and everything above in one
  place.
- [`docs/command-palette/index.md`](../../command-palette/index.md) — every
  action in this tutorial is also reachable by name from `Ctrl+P` if you'd
  rather not use the menu.

---

**Previous:** [Tutorial 3 — Find, Replace, and Multi-Cursor](../03-find-replace-and-multi-cursor/index.md)
**Next:** [Tutorial 5 — Setting Up LSP](../05-setting-up-lsp/index.md)

---

Vix™ and Vix IDE™ are trademarks.
