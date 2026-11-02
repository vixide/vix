# Change Gutter Base

The editor's change gutter marks lines that differ from a **base**. By
default the base is the last commit; you can choose another.

**Git → Change Gutter Base…** (also in the command palette, action
`git.gutter_base`) opens a list of `head`, `index`, `disk` and one
`branch:<name>` row per local branch, with the current base preselected. The
choice applies for the session; to make it permanent set `git_gutter_base` in
your settings file.

| Value | Compares against | Use |
| ----- | ---------------- | --- |
| `head` (default) | the last commit | everything not yet committed |
| `index` | the staged version | staged hunks stop being marked |
| `disk` | the file as saved | exactly the unsaved edits |
| `branch:<name>` | the tip of that branch | a feature's or agent's whole change as you scroll |

An unrecognized value, or `branch:` with no name, means `head`. A file that
is absent from the base (new, or not on that branch) compares against empty,
so every line shows as added. Changing the base recomputes the marks at once.

Stage Hunk, Unstage Hunk and hunk navigation always work against HEAD and
the index, whatever the gutter base is. While the
[coverage gutter](../coverage/index.md) is visible it takes the gutter column
instead.

See the specification at `crates/vix-git/spec/git-integration/index.md`
(section "Gutter base").

---

Vix™ and Vix IDE™ are trademarks.
