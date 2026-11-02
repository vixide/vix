# Interactive Rebase and Cherry-Pick

Rewrite recent history from inside Vix, without a text editor opening for
git's todo list. The full key tables are in the
[Git panel](../git-panel/index.md#interactive-rebase) guide; this page is the
short version.

## Actions (Git → Rebase)

| Action id | Menu item | Does |
| --------- | --------- | ---- |
| `git.rebase_interactive` | Interactive Rebase… | Asks for a base (branch, tag, or `HEAD~3`), then shows the commits after it, oldest first. `Enter` runs the rebase. |
| `git.rebase_continue` | Continue Rebase | Continues after an `edit` stop or a resolved conflict. |
| `git.rebase_abort` | Abort Rebase | Gives up; the branch goes back where it was. |
| `git.cherry_pick` | Cherry-Pick… | Asks for a branch or revision, lists its commits that `HEAD` lacks; `c` cherry-picks the highlighted one, `Enter` shows its diff. |

In the todo list: `p` pick, `r` reword, `e` edit, `s` squash, `f` fixup,
`d` drop, `Alt+↑`/`Alt+↓` reorder, `Enter` run, `Esc` cancel.

## How it avoids an editor

Vix replaces git's sequence editor with a command that writes your prepared
list, and sets the message editor to accept git's proposal. A reworded
message is passed through an environment variable, never through a shell. A
list that starts with a squash or fixup is refused before git runs. While a
rebase is stopped, **Interactive Rebase…** will not start another; use
Continue or Abort.

Because the list is built by Vix, git settings such as `rebase.autoSquash`
and `rebase.updateRefs` do not apply to it.

See the specification at `crates/vix-git/spec/index.md` (section
"Interactive rebase and cherry-pick").

---

Vix™ and Vix IDE™ are trademarks.
