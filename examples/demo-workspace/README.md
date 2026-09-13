# Demo Workspace

A small, realistic project for touring Vix -- open this directory as the
editor's root and work through it. It's fixture content: a project a
tutorial opens *inside* Vix, not something the `vix` workspace itself
builds (see the `exclude` entry in the repo-root `Cargo.toml`).

| Path | What it's for |
| --- | --- |
| `rust-app/` | A tiny Rust binary with a deliberate TODO and a real bug -- try LSP navigation, multi-cursor, and structural replace here. |
| `scripts/hello.py` | The Python half, reading `data/sample.csv`; has its own TODO and an unvalidated-input bug. |
| `notes.md` | A scratch Markdown file with its own TODO/FIXME, for Markdown Preview and workspace-wide search. |
| `data/sample.csv`, `data/sample.tsv` | The same rows in two delimited formats, for the CSV/TSV table editor. |
| `api.http` | An httpbin.org request for Tools -> Send HTTP Request. |
| `org/roam/` | Three linked Org-roam nodes (`project-overview.org` is the entry point) for backlinks and the node graph. |
| `org/dailies/2026-09-13.org` | A daily note linking back into the roam nodes above. |
| `db/demo.sqlite` | A seeded SQLite database for the DB Workbench; regenerate it from `db/seed.sql` with `db/regenerate.sh`. |
| `tasks.toml` | Named tasks (`build`, `test`, `run`, `report`) for Tools -> Tasks... |

## Suggested first pass

1. Open `org/roam/project-overview.org` and follow its links (Org -> Roam ->
   Node -> Open at Point) to `getting-started.org` and
   `meeting-2026-09-01.org`.
2. Work through the checklist in `getting-started.org` -- it points back at
   every file in this table.
3. Run the `test` task from Tools -> Tasks... to see `rust-app`'s existing
   test pass, then fix the FIXME comment in `rust-app/src/main.rs` and
   re-run it.

Written tutorials that use this workspace live under `docs/tutorials/`
(see `tasks.md`'s T404/T405); the demo tapes in `docs/demos/` (T406) run
against it too.
