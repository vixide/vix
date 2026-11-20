# Project Switcher Frecency

Ranking for **File -> Switch Project...** (`file.switch_project`).

Recent projects are ordered by frecency (frequency times a recency weight: within a day counts most, then a week, then a month) instead of plain most-recently-used. Each session records a visit count and last-open time.

`Session::frecency_ordered(now)`; `WorkspaceSession.visits` / `last_visit` (serde-default so old sessions load).

Each row shows the folder's own name first, then where it is, with the home
directory as `~` (`vix   ~/git/vix`), in a 72-column chooser: a full
absolute path in a narrow box was cut off before the part that tells
projects apart. Selecting a row opens the full path as before.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
