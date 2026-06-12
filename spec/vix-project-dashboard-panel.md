# vix-project-dashboard-panel

Project dashboard panel displays helpful information about the current project, its files, its specifications, its history, etc.

**Status:** Shipped (first cut) — **Tools → Project Dashboard** opens a read-only
overlay. The folder name shows immediately; the other metrics are computed in
background threads and fill in live (the run loop ticks faster while they
compute), each showing `computing…` until ready. `Esc` (or `Enter`) closes.

- Top level folder name
- Disk usage (async, via a `du -sh` system call)
- Count files (async, recursive walk skipping `.git` and `target`)
- Count git commits (async, `git rev-list --count HEAD` via `vix-git`)

The crate (`vix-project-dashboard-panel`) holds the metric state; the host
(`src/app.rs`) spawns the threads, polls results each frame, and renders.

Roadmap: refresh on demand, specification/history summaries, and per-language
file breakdowns.
