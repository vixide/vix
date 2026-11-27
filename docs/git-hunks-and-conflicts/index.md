# Hunk Staging and Merge Conflicts

This page goes deeper than the [Git Panel](../git-panel/index.md) overview
on two jobs you do from inside the editor, one hunk or one conflict at a
time:

- **Hunks.** Move between the changed regions of a file, then stage,
  unstage or revert one of them without touching the rest of the file.
- **Merge conflicts.** Find the conflict blocks git left in a file and keep
  ours, theirs or both, one block at a time or from a list of all of them.

Every command here is on the **Git** menu and in the command palette (type
`>` and part of the title). None of them has a default key in the shared
keymap; the [Zed keymap](../reference/keybindings-zed.md) binds
**Previous Change** to `Alt+,` and **Next Change** to `Alt+.`. Vix runs your
own `git` for all of it, so it works in any repository your `git` can read.

For a guided walk-through on a throwaway repository, see
[Tutorial 4: The Git Workflow](../tutorials/04-the-git-workflow/index.md).

<!-- Sources: crates/vix-menu/src/lib.rs (const GIT, GIT_RESOLVE);
docs/reference/actions.md; docs/reference/keybindings-zed.md. -->

## What a hunk is

A hunk is a run of adjacent changed lines in the active buffer compared with
the file's last commit (`HEAD`). Vix compares the **buffer**, not the file on
disk, so unsaved edits count. A deleted block has no lines left in the
buffer; Vix treats it as sitting on the line just after the deletion, so put
the cursor on that line to act on it.

The hunk commands always compare with `HEAD`. **Git → Change Gutter Base…**
changes only what the [change gutter](../gutter-base/index.md) colors (for
example `index`, so staged hunks stop being marked). It does not change which
hunks these commands find.

Hunks need a saved file inside a git repository: in an untitled buffer, or
with no changes, every hunk command reports *No changes against HEAD*.

<!-- Sources: tui/src/app/git.rs (active_hunks compares git_head_cache, the
HEAD blob, with Tab::text); crates/vix-git/src/lib.rs (Hunk, Hunk::contains:
a pure deletion matches its single anchor line; hunks groups maximal runs of
adjacent changed lines); crates/vix-i18n/locales/status.yml
(status.no_changes); docs/reference/settings.md (git_gutter_base). -->

## Moving between hunks

- **Git → Next Change** (`git.diff_next`) moves the cursor to the first line
  of the next hunk below it.
- **Git → Previous Change** (`git.diff_prev`) moves to the previous one.

Both wrap: past the last hunk, Next Change goes back to the first, and
Previous Change before the first goes to the last. The same two commands are
also on the **Go** menu.

<!-- Sources: tui/src/app/git.rs (diff_goto, the wrap fallback);
tui/src/app/git/tests.rs (diff_navigation_wraps_in_both_directions);
crates/vix-menu/src/lib.rs (menu.go.next_change / menu.go.prev_change). -->

## Stage Hunk

**Git → Stage Hunk** (`git.stage_hunk`) stages the hunk under the cursor and
nothing else:

1. Vix starts from what is staged for the file now (or from `HEAD` when
   nothing is staged).
2. It replaces that hunk's region with the hunk's lines from the buffer.
3. It writes the result as the file's index entry (`git hash-object`, then
   `git update-index --cacheinfo`).

The buffer and the file on disk do not change. Because the staged lines come
from the buffer, you can stage an edit you have not saved yet. The status
line says *Hunk staged*.

Vix refuses, and changes nothing, when:

| Status line | Why |
| --- | --- |
| *No change hunk at the cursor* | The cursor is not inside a hunk. |
| *Stage hunk failed: index diverged* | The index no longer holds the committed text for this region. This usually means you staged the hunk and then edited it again; unstage the old version first. |
| *Stage hunk failed: outside workspace* | The file is not under the workspace root. |

**Git → Diff with Staged** opens the file's diff against the index, so you can
check what you staged. Then commit from the Changes panel (`c`).

<!-- Sources: tui/src/app/git.rs (stage_hunk, active_hunk_context,
apply_hunk_index); crates/vix-git/src/lib.rs (index_blob, stage_content);
tui/src/app/git/tests.rs (stage_and_unstage_hunk_touch_only_the_index,
hunk_actions_refuse_a_file_outside_the_workspace);
crates/vix-i18n/locales/status.yml (status.hunk_staged, status.no_hunk,
status.stage_hunk_failed); crates/vix-i18n/locales/menu.yml
(menu.item.git.diff_index "Diff with Staged"). -->

## Unstage Hunk

**Git → Unstage Hunk** (`git.unstage_hunk`) is the reverse. It puts the
committed text back into the index in place of the hunk's lines, and leaves
the other staged changes, the buffer and the file on disk alone. The status
line says *Hunk unstaged*.

The index must still hold the hunk exactly as the buffer shows it. If you
edited the hunk after staging it, change it back first, or Vix refuses:

| Status line | Why |
| --- | --- |
| *Unstage hunk failed: nothing staged* | The file has no index entry to change. |
| *Unstage hunk failed: hunk not staged* | The index does not hold the buffer's lines for this hunk at that position. |

<!-- Sources: tui/src/app/git.rs (unstage_hunk); tui/src/app/git/tests.rs
(stage_and_unstage_hunk_touch_only_the_index: unstage after restoring the
staged text, then "hunk not staged" on a second try);
crates/vix-i18n/locales/status.yml (status.hunk_unstaged,
status.unstage_hunk_failed). -->

## Revert Hunk

**Git → Revert Hunk** (`git.revert_hunk`) replaces the hunk under the cursor
with its committed text, **in the buffer only**. Nothing is written to disk
or to the index until you save. The buffer is marked modified, the cursor
moves to the start of the hunk, and the status line says *Hunk reverted to
HEAD*. The revert is one undo step, so **Undo** brings the edit back.

<!-- Sources: tui/src/app/git.rs (revert_hunk, apply_rebuilt_buffer: sets
dirty, places the caret at the hunk start); crates/vix-editor-core/src/editor.rs
(set_content: "a single undo step"); tui/src/app/git/tests.rs
(revert_hunk_restores_only_the_hunk_under_the_cursor);
crates/vix-i18n/locales/status.yml (status.hunk_reverted). -->

## Getting into a merge conflict

A merge stops on a conflict when both branches changed the same lines. In Vix
that happens with:

- **Git → Branch → Merge…** (`git.merge_branch`): choose a local branch and
  Vix runs `git merge <branch>` into the current branch.
- **Git → Pull → Merge** (`git.pull_merge`), which runs
  `git pull --no-rebase`.

git's output, including the list of conflicting files, streams to the bottom
dock. Conflicted files then show the magenta `U` badge in the file explorer
and the `U` letter in the Changes panel.

<!-- Sources: tui/src/app/git.rs (open_branch_chooser_mode(true),
checkout_selected_branch: run_command("git merge {branch}"); "git.pull_merge"
=> git_remote_command("git pull --no-rebase")); crates/vix-menu/src/lib.rs
(GIT_BRANCH, GIT_PULL); docs/git-panel/index.md (badge table). -->

## What Vix reads as a conflict

A conflict block is the text between git's markers:

```text
<<<<<<< HEAD
our lines
=======
their lines
>>>>>>> other-branch
```

The lines above `=======` are **ours** (the branch you were on) and the lines
below are **theirs** (the branch coming in). A marker is any line that starts
with seven `<`, `=` or `>` characters; the labels after them are ignored.

Limits worth knowing before you resolve:

- **diff3 style is not understood.** With `merge.conflictStyle = diff3` (or
  `zdiff3`), git adds a `|||||||` line and the common ancestor's lines.
  Vix treats them as part of ours, so **Keep Ours** would keep them too. Use
  git's default style, or delete that section by hand.
- **Nested conflicts are not supported.**
- **An unfinished block ends the search.** If a block is missing its
  `=======` or `>>>>>>>` line, that block and every block after it are not
  found.

<!-- Sources: crates/vix-conflict-tool/src/lib.rs (module docs "Marker grammar
and limits", Resolution, is_marker, next_block);
crates/vix-conflict-tool/spec/index.md. -->

## Resolving one conflict at a time

- **Git → Next Conflict** (`git.conflict_next`) moves the cursor to the
  `<<<<<<<` line of the next conflict. It skips the block the cursor is in and
  wraps to the first block after the last one.
- **Git → Resolve → Keep Ours** (`git.conflict_ours`) replaces the whole
  block, markers included, with our lines.
- **Git → Resolve → Keep Theirs** (`git.conflict_theirs`) keeps their lines.
- **Git → Resolve → Keep Both** (`git.conflict_both`) keeps ours followed by
  theirs.

The Resolve commands act on the block the cursor is in, or on the next block
below it when the cursor is between blocks. Each one edits the buffer as a
single undo step, marks it modified, puts the cursor where the block began,
and says *Conflict resolved*. With no block at or after the cursor, they say
*No merge conflict here*.

For example, with the cursor in the first block of

```text
a
<<<<<<< HEAD
ours1
=======
theirs1
>>>>>>> x
mid
```

**Keep Both** leaves `a`, `ours1`, `theirs1`, `mid`. Later blocks stay as
they were.

<!-- Sources: tui/src/app/git.rs (conflict_next, resolve_conflict,
rebuild_with_resolution, apply_rebuilt_buffer);
crates/vix-conflict-tool/src/lib.rs (find: "the one under, or next after,
the cursor"; Conflict::resolved, Both = ours then theirs);
tui/src/app/git/tests.rs (conflict_resolution_keeps_the_chosen_side,
conflict_actions_report_when_there_is_no_conflict,
conflict_next_advances_and_wraps); crates/vix-i18n/locales/status.yml
(status.conflict_resolved, status.no_conflict); crates/vix-i18n/locales/menu.yml
(menu.item.git.resolve "Resolve", conflict_ours/theirs/both "Keep Ours/Theirs/Both"). -->

## The Conflicts list

**Git → List Conflicts…** (`git.conflict_list`) opens the **Conflicts**
overlay for the active buffer. Each row is a number and the first line of
each side, ours on the left:

```text
  1: ours1 → theirs1
  2: ours2 → theirs2
↑↓ select   Enter go to   o/t/b resolve   Esc close
```

| Key | Action |
| --- | --- |
| `↑` / `↓`, `PageUp` / `PageDown`, `Home` / `End` | Move the highlight |
| `Enter` or a click on a row | Jump to that conflict and close the list |
| `o` / `t` / `b` | Keep ours / theirs / both for the highlighted conflict, and stay in the list |
| `Esc` | Close the list |

After each `o`, `t` or `b` the list is rebuilt from the buffer, so it shrinks
by one. It closes by itself when the last conflict is resolved. If the buffer
has no conflicts, the list does not open and the status line says *No merge
conflict here*.

The list covers the active buffer only. To find the other conflicted files,
look for the `U` rows in **Git → Changes…**.

<!-- Sources: tui/src/app/git.rs (open_conflict_list, conflict_list_key,
conflict_list_mouse, resolve_conflict_list_selection);
tui/src/ui/edit_surfaces.rs (draw_conflict_list: row format "{:>3}: {} → {}",
title ui.conflict_list, hint ui.conflict_list_hint);
crates/vix-i18n/locales/ui.yml (ui.conflict_list "Conflicts",
ui.conflict_list_hint); tui/tests/snapshots/snapshots__merge_conflict_list_overlay.snap;
tui/src/app/git/tests.rs
(conflict_overlay_navigates_jumps_and_resolves,
clicking_a_conflict_row_selects_and_jumps);
crates/vix-conflict-tool/spec/index.md (workspace-wide is a non-goal). -->

## Finishing the merge

Resolving a block only edits the buffer. To finish:

1. Save the file.
2. In **Git → Changes…**, select the file and press `s` (or `Space`) to stage
   it. Staging is how git learns the conflict is resolved.
3. When every conflicted file is staged, press `c` and commit. git records
   the merge commit.

A conflict during an interactive rebase is finished the same way, except that
step 3 is **Git → Rebase → Continue Rebase** (or **Abort Rebase** to give
up). Vix has no command to abort a merge; run `git merge --abort` in the
[terminal](../terminal/index.md).

<!-- Sources: docs/git-panel/index.md (Changes panel keys s / Space / c);
tui/src/app/git_rebase.rs and crates/vix-menu/src/lib.rs (GIT_REBASE:
git.rebase_continue, git.rebase_abort); docs/reference/actions.md (no merge
abort action). -->

## See also

- [Git Panel](../git-panel/index.md): whole-file staging, commits, branches.
- [Change Gutter Base](../gutter-base/index.md): what the gutter compares with.
- [Branch Review](../branch-review/index.md): stage or discard hunks while
  reviewing a whole branch.
- [Interactive Rebase](../interactive-rebase/index.md).
- Specs: `crates/vix-git/spec/stage-hunk/index.md`,
  `crates/vix-git/spec/unstage-hunk/index.md`,
  `crates/vix-conflict-tool/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
