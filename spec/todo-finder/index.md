# TODO / FIXME Finder

Editor action `tools.todo_finder`.

Scan the project's files (honoring `.gitignore` via the file index) for comment tags -- TODO, FIXME, HACK, XXX, BUG, NOTE -- matched as whole words, and list them in the results panel; Enter jumps to a match. Capped at 2000 hits.

From **Tools -> Find TODOs...** or the command palette. `App::open_todo_finder`; whole-word match via `tag_column`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
