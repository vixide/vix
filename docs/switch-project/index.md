# Switch Project

Jump between the projects you've recently worked on without quitting Vix™. **File →
Switch Project…** (or the command palette) lists your recent workspaces; pick one
and the editor re-roots there.

## Using it

- Open **File → Switch Project…**. The list shows every workspace Vix has opened
  before (most recent first), except the current one.
- `↑` / `↓` to choose, `Enter` to switch, `Esc` to cancel.

When you switch, Vix:

- saves your current session (open files, cursors, scroll, split layout),
- reopens the chosen project exactly where you left it last time,
- rebuilds the file explorer and Git state, and restarts language servers for the
  new root.

Recent projects come from the same per-workspace session store described under
[session restore](../../spec/session-restore/index.md); each workspace you open is
remembered automatically.

See the specification at `spec/switch-project/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
