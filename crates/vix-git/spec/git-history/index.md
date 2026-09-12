# git_history

**Status:** Shipped (improvement plan T207). Three history-browsing
features under **Git → Log**, alongside the pre-existing plain
streamed-to-the-dock log views (**All**, **Graph**, **Since…**): those show
raw `git log` text with no interactivity; these open an interactive commit
list you can select from.

## Browse Log…

**Git → Log → Browse Log…** (`git.browse_log`) lists the repository's most
recent commits (abbreviated hash, date, author, subject), most recent
first. `↑`/`↓`/`PageUp`/`PageDown` navigate; `Enter` (or a click) opens the
highlighted commit's own unified diff (`git show <sha>`) in a **read-only
tab** titled `<abbrev> log`; `Esc` closes the panel without opening anything.

## File History

**Git → Log → File History** (`git.file_history`) is the same panel, scoped
to the active file's own history (`git log --follow -- <path>`, so renames
are tracked). `Enter` opens the highlighted commit's diff for **just that
file** (`git show <sha> -- <path>`) in a read-only tab titled
`<abbrev> <filename>`. A no-op with a status message without an active file,
or one with no history yet.

## Open File at Revision…

**Git → Log → Open File at Revision…** (`git.open_at_revision`) prompts for
a revision — a branch, tag, or any commit-ish — and opens the active file's
content there (`git show <revision>:<path>`) in a read-only tab titled
`<filename> @ <abbrev>` (the revision resolved to its abbreviated hash via
`git rev-parse --short`, so `HEAD~3`, a branch name, or a tag all title the
same way a bare sha would). An unresolvable revision, or a path not present
in that tree, reports an error rather than opening anything; a no-op
without an active file.

## As implemented in Vix

- `crate::{log, file_log}` run `git log` with a custom `--format` (fields
  separated by `\x1f`, records by `\x1e`, parsed by `parse_log` — pure,
  unit-tested against real `git log` output captured from this repo, not
  guessed) into `LogEntry` rows; `LogPanel` (reusing `vix_list_state`, the
  same pattern every other list panel in the workspace follows) holds them
  plus the highlighted row.
- `crate::show_commit`/`show_file_at`/`resolve_short_sha` shell out to
  `git show`/`git rev-parse`, validating the revision argument with the
  same `valid_ref_name` + `--end-of-options` defense `checkout` already
  uses (rejects anything that could be misread as a flag before it ever
  reaches the `git` process).
- The host (`App`, `src/app/git.rs`) builds the panel, dispatches its keys/
  clicks, and opens the read-only result tabs via a small shared helper
  (`App::open_readonly_text_tab`) — a synthetic `path` (never a real file)
  gives the tab an arbitrary title through `Tab::title()`'s normal
  filename-based rendering, and `read_only: true` (checked by `App::save`
  before it ever tries to write) keeps `Ctrl+S` from doing anything to it.
