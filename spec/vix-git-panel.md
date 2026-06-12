# vix-git-panel

**Status:** Shipped — the Git Panel is the **Git → Changes…** panel (see
`git-integration.md`). Open it from the **Git** menu, the command palette
("Git: Changes"), or by **clicking the branch indicator in the status bar**.

The Git Panel shows the state of your working tree and Git's staging area.

In the panel you can see the state of your project at a glance: the active branch
(shown in the panel title and the status bar), which files have changed, and the
current staging state of each file (a `[✓]` checkbox plus the M/A/?/D/R/U change
letter). Stage/unstage with `Space`/`s`/`u`, commit with `c`, refresh with `r`.

Vix re-reads `git status` on open, after each save, and after every git action,
so changes you make on the command line are reflected (press `r` to refresh on
demand).

