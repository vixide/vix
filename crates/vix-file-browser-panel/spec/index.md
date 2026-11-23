# File Browser

File browser panel (File → Open): one folder at a time, with typed paths,
fuzzy filtering and sorting. This overlay behind **File → Open…**
(`file.open`, `Ctrl O`) works like any open dialog: it lists one folder,
you type to filter it or type a path to go somewhere else, and Enter opens.
Finding a file anywhere in the project is the command palette's file finder
(**Ctrl+P** in the default keymap), which honors `.gitignore`. Listing goes
through the authority slot (`vix-authority`, T710).

## Listing

The browser opens in the **active file's folder** when that file is inside
the workspace (else at the workspace root) and lists that folder's own entries, not what the folders
inside it hold: name-sorted, links not followed, per entry the absolute path, name
(as `rel`), directory flag, size in bytes, and created / modified times as
Unix seconds (`None` where the filesystem lacks them). Folders sort first.

Before 2026-11-18 it walked the whole tree recursively, which in a large
checkout (a Rust project's `target/`, say) filled its 10,000-entry cap before
reaching the user's files, and gave no visible way to type a path.

- **Hidden entries** (dot-prefixed) are excluded by default; **Alt H**
  toggles them.
- A folder with more than **10,000 entries** (`DEFAULT_MAX_ENTRIES`) is cut
  off there and the panel shows a truncation marker.

## Typing a path

The filter line doubles as a path line:

- Typing a path separator after a folder's path moves into that folder and
  clears the filter: `docs/`, `../`, `~/` (the home folder) and `/` (the
  root). A separator after a name that is not a folder just stays in the
  filter.
- When the filter is a path (it contains a separator, or starts with `~`),
  **Enter** opens the file it names, relative to the folder shown, or a new
  file there if it does not exist yet, honoring a `:line[:col]` suffix; a
  path to a folder moves into it. `Panel::path_target` gives that path.
- **Backspace** on an empty filter goes up to the parent folder.
- **Enter** on a plain name that matches nothing opens that name in the
  folder shown, as a new file if it does not exist (nothing is written until
  it is saved), instead of doing nothing.

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
- **Ctrl P** closes the browser and opens the command palette's file finder,
  which searches the whole project.
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

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The glob matcher walks with `get` and a checked step; entry lookups use `get`; a pre-epoch timestamp that cannot be negated is `None`. **Declared (BROWSER-F1):** a fuzzy relevance past the top of the `i32` scale is the top of the scale. The display-only size conversion and the sort's indexes from `enumerate` carry `RCA-EXC-2` proofs.
