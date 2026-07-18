# File Browser

A comprehensive **file browser** overlay behind **File → Open…** (`file.open`,
`Ctrl O`): a recursive listing of the current root directory that supports
live search (including fuzzy search), sorting, filtering, and directory
navigation. Walking uses the `walkdir` crate.

## Listing

Opening the browser walks the workspace root recursively (`walkdir`, walk
order sorted by file name) and records, per entry: absolute path,
root-relative path, name, directory flag, size in bytes, and the created /
modified times as Unix seconds (`None` where the filesystem lacks them).

- **Hidden entries** (dot-prefixed) are excluded by default; **Alt H** toggles
  them (hidden directories are pruned, not just hidden files).
- The walk stops at **10,000 entries** (`DEFAULT_MAX_ENTRIES`); the panel then
  shows a truncation marker. This keeps enormous trees responsive.

## Search and filter

Typing edits the query: a whitespace-separated list of tokens that an entry
must **all** match, checked against its root-relative path:

- `ext:rs` / `ext:rs,toml` / bare `.rs` — keep files with one of the listed
  extensions (case-insensitive). Directories are exempt so their contents stay
  reachable.
- A token containing `*` or `?` — a **glob** over the relative path,
  case-insensitive; `*` crosses `/` (so `*.rs` finds nested files), `?` is any
  one character.
- Any other token — **fuzzy** (in-order subsequence, case-insensitive) via
  `vix_palette::fuzzy_score`, which rewards contiguous runs, word boundaries,
  and early/prefix matches.

**Backspace** edits the query; an empty query lists everything.

A token may end with a **`:line[:col]` jump target** (`main.rs:120`,
`main.rs:120:8`), exactly as the classic Open prompt accepts: the suffix is
ignored while matching, and opening the file jumps to that position.

## Sort

**Ctrl S** cycles the sort column — **name → size → date created → date
modified** — and **Ctrl R** flips ascending/descending. Directories always
group before files; name (case-insensitive) breaks ties. Missing dates sort
oldest.

**Relevance ranking:** while the sort is untouched (name, ascending) and the
query has at least one fuzzy token, files are ranked by fuzzy score, best
first. Choosing any explicit sort (or direction) overrides relevance.

## Navigation

- **↑/↓**, **PgUp/PgDn**, **Home/End** move the highlight; the list scrolls to
  keep it visible.
- **Enter** (or a click) on a **file** opens it in the editor and closes the
  panel; on a **directory** it re-roots the browser there (clearing the query).
- **←** re-roots at the parent directory; **→** enters a highlighted directory.
- **Ctrl O** falls back to the classic path prompt (`file.open_path`) for
  typing an arbitrary path — including one that does not exist yet.
- **Esc** closes the panel.

The status line shows the sort column and direction, the hidden-files state,
and the match count. The host renders rows as name, size (humanized via
`size_label`), and modified date; the crate stays terminal-free.

## Module (`crate::file_browser_panel`)

- `Panel` — the browser state: `root`, walked `entries`, `query`, `sort` +
  `ascending`, `show_hidden`, `selected`/`scroll` over the *filtered* rows,
  `truncated`, `max_entries`. `open(root)` walks immediately; `refresh()`
  re-walks; `matches()` returns filtered indices in display order;
  `activate()` opens a file (returns its path) or enters a directory;
  `parent()` walks up.
- `SortKey` — `Name | Size | Created | Modified`, with `next()` for the cycle
  and `label_key()` returning the i18n key the host translates.
- `glob_match(pattern, text)` — the `*`/`?` matcher.
- `size_label(bytes)` — humanized size (`973 B`, `4.1 KB`, `12 MB`).

## Actions

- `file.open` — open the file browser (File → Open…, `Ctrl O`).
- `file.open_path` — the classic type-a-path prompt (also reachable from
  inside the browser with `Ctrl O`).
