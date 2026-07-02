# Project Switcher Frecency

Ranking for **File -> Switch Project...** (`file.switch_project`).

Recent projects are ordered by frecency (frequency times a recency weight: within a day counts most, then a week, then a month) instead of plain most-recently-used. Each session records a visit count and last-open time.

`Session::frecency_ordered(now)`; `WorkspaceSession.visits` / `last_visit` (serde-default so old sessions load).

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
