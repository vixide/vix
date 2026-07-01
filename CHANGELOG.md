# Changelog

All notable changes to Vix are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this workspace aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Media types use clean `text/<lang>` forms (no `x-` prefix)** for source code —
  `text/rust`, `text/python`, `text/typescript`, `text/java`, `text/cpp`,
  `text/csharp`, `text/c`, `text/ruby`, `text/go`, …, `application/sql`. `.ts` now
  maps to `text/typescript` and `.cs` to `text/csharp` (C# added).
- **Snippet directories load every `*.json` file** (not just `snippets.json`), and
  media-type snippets also load from the project at
  `config/media-types/<type>/snippets/`.
- **Macro commands moved to Edit → Macro** and namespaced consistently under
  `macro.*`: `toggle_macro` → `macro.record`, `play_macro` → `macro.play` (joining
  `macro.save` / `macro.play_saved`). Menu items relabeled Record / Play / Save… /
  Play Saved….
- **Edit → Lines → "Sort Lines" renamed to "Sort".**
- **Docs/spec/AGENTS harmonized** with the current feature set (keymaps list,
  Org/Debug menus, edit surfaces, snippets, media types, module map, Rust 1.95
  toolchain floor).
- **Keymap renamed "Vim" → "Vi"** — display label, id (`vim` → `vi`), and action
  (`view.keymap:vim` → `view.keymap:vi`). Older configs using `vim` still load.
- **Edit surfaces moved to Edit → Mode.** The Edit Table/Outline/JSON/YAML/Bytes
  items moved from the Tools menu into a new **Edit → Mode** submenu (labeled
  Table/Outline/JSON/YAML/Bytes). The action ids and command-palette entries are
  unchanged.
- **Case submenu moved to Tools → Convert.** The Edit → Case transforms
  (Upper/Lower/Title/Kebab/Snake/Camel/Pascal) now live under Tools → Convert.

### Fixed

- **Menu navigation.** In a dropdown, pressing `Right` on a submenu now opens it
  **and highlights its first item** (was: nothing highlighted). Pressing `Up` on
  the first dropdown item now moves to the **menu title** (nothing highlighted)
  instead of wrapping to the last item.

### Added

- **Frecency ranking for the project switcher.** Switch Project now orders recent
  projects by frequency × recency (recent, frequently-opened projects first)
  instead of plain most-recently-used. Sessions track a visit count + last-open.
- **Offline structural selection expand.** Expand Selection now falls back to the
  Tree-sitter parse tree (smallest enclosing node, growing on repeat) when the
  file has no language server — previously it required LSP.
- **Scratch buffer** (File → New Scratch Buffer). Opens a throwaway, unsaved
  buffer with a header line for quick notes/calculations.
- **Highlight word occurrences** (View → Highlight Word Occurrences, off by
  default). Passively marks every whole-word occurrence of the identifier under the
  cursor, on its own render channel so it never clobbers (sticky) search
  highlights.
- **Align on delimiter** (Edit → Align). Pads selected lines so their first `=`,
  `:`, `,`, or `|` lands in a common column. Pure logic in `crate::align`.
- **Increment / Decrement number** (Edit menu). Bumps the integer at or after the
  cursor by one (Vim's Ctrl-A/Ctrl-X), handling a leading `-`.
- **TODO/FIXME finder** (Tools → Find TODOs…). Scans the project's files (honoring
  `.gitignore`) for comment tags — TODO, FIXME, HACK, XXX, BUG, NOTE — as whole
  words, listing them in the results panel; Enter jumps to the match.
- **Smart-case search** (find box, on by default; toggle with the Smart button or
  Alt+S). Matches case-insensitively when the query is all-lowercase, but becomes
  case-sensitive as soon as it contains an uppercase letter. The explicit Case
  toggle still forces sensitivity.
- **Surround** (Edit → Surround). Wrap the selection in a bracket/quote pair —
  parentheses, brackets, braces, angles, double/single quotes, backticks — and
  remove it by repeating the same action.
- **Minimap** (View → Minimap, off by default). A code-overview column at the
  right of the editor: each row is a band of lines drawn as a bar sized to the
  band's longest line, with the current viewport highlighted; click to jump.
  Single-pane view.
- **Call hierarchy (callers)** (Tools → Language Server → Call Hierarchy). Lists
  the incoming calls to the symbol under the cursor via LSP
  (`prepareCallHierarchy` → `incomingCalls`), shown in the references jump list.
- **Emmet expansion** (Edit → Emmet Expand). Expands the abbreviation before the
  cursor into HTML — child `>`, sibling `+`, multiply `*N`, `#id`, `.class`,
  `{text}`, and `$` numbering (e.g. `ul>li.item$*3`). Pure logic in `crate::emmet`.
- **Persistent undo.** The undo tree is saved per file on save and restored on
  reopen (under `<config>/undo/`), guarded by a content hash so it's only replayed
  when the file still matches. Setting `persistent_undo` (on by default).
- **Live Backlinks** (Org → Roam → Live Backlinks). A toggle that fills the bottom
  dock with the active node's linked + unlinked references and refreshes as you move
  between nodes.
- **Dailies calendar** (Org → Roam → Dailies → Calendar…). Opens the month grid;
  Enter on a day opens/creates that day's Org-roam daily note instead of inserting
  the date.
- **`[[` wiki-link autocomplete** for Org-roam/Node. Typing `[[` in an `.org` file
  opens node-title completion (also Org → Node → Complete Link…); accepting inserts
  the title and closes the link, without double-closing when auto-pair already
  added `]]`.
- **Rainbow brackets** (View → Rainbow Brackets, off by default). Colors `()[]{}`
  by nesting depth (6-color cycle) over the syntax colors; skipped for very large
  files. (Idea from the zee/editor feature.)
- **Sticky scroll** (View → Sticky Scroll, on by default). Pins the enclosing
  scope's header line (e.g. the `fn`/`class`/heading) at the top of the editor
  while you scroll through its body. Single-pane view. (Idea from the zee/VS Code
  feature.)
- **Indentation guides** — faint vertical bars at each indent level on the
  (non-wrapped) editor view, for space-indented buffers. (Idea from the zee/VS Code
  visual.)
- **Auto-save** (View → Auto Save, off by default). Periodically writes the active
  dirty file-backed buffer (every ~5s; plain save, no format-on-save churn).
- **Format on save** (View → Format on Save, off by default). On save it runs the
  language server's formatter and re-saves once the edits land; the plain save
  happens first, so nothing is lost if formatting is slow or unsupported.
- **Auto-reload on external change.** Files changed on disk by another process (a
  formatter, `git checkout`, a second editor) are detected (mtime poll, throttled
  to 1s): clean buffers reload automatically; a buffer with unsaved edits gets a
  one-time warning and keeps your edits.
- **Keymap renames + a new VSCode Windows keymap.** Display names dropped the `+`
  ("IntelliJ macOS"/"IntelliJ Windows") and reordered VS Code ("VSCode macOS");
  added a **VSCode Windows** keymap. Canonical ids are now full platform names
  (`vscode-macos`, `vscode-windows`, `intellij-macos`, `intellij-windows`); the
  old `vscode`/`intellij-mac`/`intellij-win` ids still load. Dropped the
  `jetbrains-*` aliases.
- **One syntax-highlight query per viewport.** The non-wrapped renderer now runs a
  single Tree-sitter highlight query over the whole visible region instead of one
  per line — cheaper while typing — and the highlight cache memoizes a single
  entry instead of one per visible line. (Idea from the zee editor.)
- **Undo tree (branch-preserving undo).** Editing after an undo no longer discards
  the redo history — it starts a new branch, so no state is ever lost. Undo/redo
  behave like a linear history by default; **Edit → Switch Undo Branch** cycles
  which branch redo follows, making every past state reachable. (Idea from the zee
  editor's edit-tree.)
- **Background (async) syntax parsing for large files.** Buffers ≥ 50 KB now
  reparse on a background thread after each edit instead of blocking the keystroke,
  with cancel-on-new-edit and generation-based stale-result rejection (the edited
  tree keeps rendering until the fresh one lands). Smaller files stay fully
  synchronous — identical behavior. (Idea from the zee editor's async parser.)
- **Org → Contacts ([org-contacts](https://github.com/doomelpa/org-contacts)).**
  Manage contacts stored as Org headlines: New Contact (skeleton with a property
  drawer), Find Contacts (name/email/phone table), Insert Field (Email/Phone/
  Address/Birthday/Nickname/Note), Birthdays, and Export to vCard 3.0. Pure logic
  in `crate::org_contacts`.
- **Help menu additions**: License (crate metadata from `Cargo.toml`), Report an
  Issue… (links the issue tracker), and Privacy Statement — each shown in a
  scrollable overlay.
- **Renamed the Debug menu to Run** (action ids `debug.*` → `run.*`) and moved it
  to just after the new Go menu (Alt+R).
- **Top-level Go menu** (promoted from Edit → Go, placed after View, Alt+N). Adds
  Go to Symbol / Declaration / Implementations / References, Next/Previous Issue
  (diagnostics) and Next/Previous Change (git hunks), and granularity submenus —
  Word, Sentence, Line, Paragraph, Section (each with Start/End/Next/Previous and
  go-to-Nth by number), plus File (Start/End). New LSP `textDocument/declaration`
  request and in-file diagnostic navigation.
- **Workspaces (multi-folder, saved to a file).** File → Open Workspace from
  File… / Save Workspace into File… / Add Folder to Workspace…. A workspace is a
  portable `.vix-workspace` (TOML) file listing project folders plus the files to
  reopen; the fuzzy finder and project index span every workspace folder.
- **base16 themes.** Ten well-known [base16](https://github.com/chriskempson/base16)
  color schemes (Tomorrow Night, Solarized Dark/Light, Gruvbox, Nord, Ocean,
  Monokai, Material, Dracula, …) generated from a compact palette table and
  merged into View → Theme. (Idea from the zee editor's base16 theme generation.)
- **`scripts/check` (and `make check`)**: a one-command local CI-parity gate —
  build + Clippy (pedantic, `-D warnings`) + full test suite. Plus Unicode/emoji
  stress fixtures in `test-data/` with a grapheme-width regression test. (Ideas
  from the zee editor.)
- **`.gitignore`-aware file finder.** The project file index now walks with the
  `ignore` crate (the engine ripgrep uses), so the fuzzy file finder honors
  `.gitignore`, `.ignore`, and git's global/excludes (even outside a git repo) and
  no longer surfaces build artifacts; `target`/`node_modules` are always pruned.
  (Idea from the zee editor.)
- **Sticky vertical "goal column."** Moving the caret up/down now remembers the
  column you started from, so passing through a short line no longer snaps the
  cursor inward (it returns to the original column on the next long line). Any
  horizontal move resets it. (Idea from the zee editor.)
- **Org checkbox & statistics cookies.** Parent checkboxes now reflect their
  children (all → `[X]`, none → `[ ]`, some → `[-]`), and `[/]`/`[%]` statistics
  cookies in headlines and list items are recomputed automatically after Toggle
  Checkbox / Cycle TODO (or on demand via **Org → Update Statistics**). Headline
  cookies count child checkboxes or child TODO headlines, honoring the
  `:COOKIE_DATA:` property (`checkbox`/`todo`, plus `recursive`). Pure logic in
  `org::update_statistics`.
- **Org → Node ([org-node](https://github.com/meedstrom/org-node) functionality).**
  A new submenu for fast, ID-based nodes where a node is a file *or* a subtree:
  nodeify the entry at the cursor (give it an `:ID:`), extract a subtree into its
  own file node (leaving an `[[id:…]]` link), insert a `#+transclude:` directive,
  list dead ID links, rename a file by its `#+title:`, and rebuild the node cache —
  alongside shared find / insert-link / random / backlinks. Pure helpers in
  `crate::roam`; see `spec/org/index.md`.
- **Org → Roam ([Org-roam](https://www.orgroam.com/) note-taking).** A new submenu
  for networked, Zettelkasten-style notes over a directory of `.org` files: find /
  capture / random node, insert `[[id:…]]` node links, a backlinks buffer (linked
  + unlinked references), dailies (today / capture / go-to-date), node metadata
  (tags / aliases / refs), a Mermaid node graph, and a stateless database sync that
  lists every node. Pure logic in `crate::roam`; see `spec/org/index.md`.
- **Tools → Draw (ditaa ASCII art).** A new submenu inserts ASCII-art shapes for
  [ditaa](https://ditaa.sourceforge.net/) diagrams: rectangles, rounded
  rectangles, document/storage shapes, horizontal/vertical (and dashed) lines,
  arrows (right/left/up/down), a point, a two-box flow, and a colored box. See
  `spec/tools/draw/index.md`.
- **PlantUML media type** (`text/plantuml`, `.plantuml`/`.puml`) plus a bundled
  snippet library (diagram skeleton, sequence/class/use-case/activity/state/
  component constructs, notes, skinparam) **and a 41-diagram example gallery**
  imported from `joelparkerhenderson/plantuml-examples` (activity, archimate,
  C4, class, deployment, ERD, gantt, mind-map, sequence, state, timing, …).
- **Graphviz media type** (`text/graphviz`, `.graphviz`/`.gv`/`.dot`) plus a DOT
  snippet library (directed/undirected graphs, nodes, edges, clusters, ranks,
  attribute defaults, record nodes).
- **Mermaid media type** (`text/mermaid`, `.mmd`/`.mermaid`) plus a snippet
  library covering flowcharts, sequence/class/state/ER diagrams, gantt, pie,
  mindmap, git graph, user journey, timeline, and quadrant charts.
- **Bundled example snippet libraries** under `config/media-types/<type>/snippets/examples.json`
  for ~38 media types — the originals (Python, Plain, TypeScript, Rust, Java, C++,
  C#, JavaScript, C, Ruby, JSON, YAML, SQL) plus HTML, CSS, Markdown, Go, TOML,
  Shell, PHP, Kotlin, Swift, Lua, Scala, Haskell, Elixir, Erlang, Clojure, Dart,
  R, Julia, Groovy, PowerShell, Objective-C, F#, OCaml, Perl, and XML — many
  common snippets each, modeled on TextMate/VS Code.
- **More programming-language media types** added to the table (Scala, Haskell,
  Elixir, Erlang, Clojure, Dart, R, Julia, Groovy, PowerShell, Objective-C, F#,
  OCaml), all using the clean `text/<lang>` form.
- **`Base` column in the media-type table** — each media type is classified
  `text` or `binary` (`MediaType::is_text()`); shown in the Media Types picker.
- **JSON snippet files (global / media-type / project).** Snippets can now be
  defined in JSON files (the VS Code shape: `name → {prefix, body, description}`)
  loaded from `~/.config/vix/global/snippets/snippets.json`, per-media-type
  (`…/media-types/<type>/snippets/snippets.json`), and a project file
  (`project_snippets`, default `config/snippets/snippets.json`). The Snippets
  picker (Tools → Snippets…) is now **searchable** and shows every in-scope
  snippet; typing a snippet **prefix** then **Tab** expands it. Bodies use the
  TextMate/VS Code tabstop syntax; interpolated shell code and `\u` are omitted.
  See `spec/snippets/index.md`.
- **Edit → Lines → Shuffle.** Randomly reorder the selected lines (or the whole
  buffer) with a Fisher–Yates shuffle (`edit.shuffle`).
- **Edit SQL mode.** A new **Edit → Mode → SQL** surface parses a `.sql` buffer
  into its statements (ignoring semicolons in strings/comments) and lists them by
  kind; reorder, delete, uppercase-format keywords, undo/redo, and save back.
  Backed by the unit-tested `edit_sql` module. See `spec/edit-sql/index.md`.
- **Media types.** A new **Tools → Media Types** picker lists common media types
  (MIME) with descriptions and extensions; type to filter, Enter to insert at the
  cursor. Opens pre-selected to the current file's type. Backed by the curated
  `spec/media-types/media-types.tsv` and the `media_type` module (with
  `for_extension` lookup). See `spec/media-types/index.md`.
- **Org → Clock In / Clock Out.** Clock In inserts an open `CLOCK: [now]` entry;
  Clock Out closes the most recent one with the end time and `=> H:MM` duration
  (local time via `jiff`). Feeds the Time Tracker report.
- **Org capture, agenda & time tracking.** **Org → Capture…** logs an idea/task
  as a `* TODO` headline via a quick prompt; **Agenda Tracker** compiles
  deadlines/scheduled items/TODOs from the project's `.org` files into a dated
  agenda; **Time Tracker** totals `CLOCK:` durations per headline into a report.
- **Org mode (basics).** A new top-level **Org** menu (`Alt+O`) brings headline
  structure editing (promote/demote and move subtree across siblings), fold
  cycling, TODO cycling (none → `TODO` → `DONE`), checkbox toggling, and export to
  Markdown / standalone HTML. Backed by the pure, unit-tested `org` module. See
  `spec/org/index.md`.
- **`affix` helpers** — `add` / `drop` / `toggle` a `prefix`/`suffix` pair around
  text (a conventional wrap: `add("alfa","bravo","charlie") == "bravoalfacharlie"`),
  for wrapping and unwrapping selections.
- **Tools → Insert → Org** now also hosts the inline **markers** (folded in from
  the former Markers submenu), with a new **Tag `:`** marker and a **Properties**
  (`:PROPERTIES: … :END:`) snippet.
- **Tools → Insert → Org / Markers / Begin-End** submenus — Org inserts Org-mode
  snippets (title/author, headlines, links/images, lists, table, TODO/DONE,
  planning lines, timestamps, drawer); Markers toggle inline emphasis (`*`/`/`/`_`
  /`+`/`~`/`=`) around the selection; Begin-End toggle `#+BEGIN_…/#+END_…` blocks.
- **Bracket / quote auto-pairing.** Typing `(`, `[`, `{`, `"`, `'`, `` ` ``
  inserts the matching closer (wrapping a selection when one exists), typing a
  closer steps over an auto-inserted one, and Backspace inside an empty pair
  deletes both. New `auto_pair` setting (on by default) and a **View → Editor →
  Auto-Pair Brackets** toggle.
- **Snippet tabstops.** Snippets (Tools → Snippets…) now support `$1`/`$2`/`$0`
  and `${1:placeholder}` fields: inserting one selects the first field and **Tab**
  walks the rest (Esc exits). Several bundled snippets gained fields. See
  `spec/snippets/index.md`.
- **Persistent macros.** Recorded keyboard macros can be saved by name
  (**Edit → Save Macro…**) to `macros.toml` and replayed in later sessions
  (**Edit → Play Saved Macro…**). See `spec/macros/index.md`.
- **View → Zoom** (In / Out / Reset) — best-effort terminal font zoom. A TUI
  can't portably resize the font, so Vix emits the font-resize escape for
  terminals that support one (xterm/urxvt) and tells you to use the terminal's
  own zoom otherwise.
- **Switch Project** (**File → Switch Project…**, `file.switch_project`) —
  re-root the editor at a recent workspace without relaunching: saves the current
  session, rebuilds explorer/Git/LSP, and restores the chosen project's session.
  See `spec/switch-project/index.md`.
- **IntelliJ & Eclipse keymaps** (**View → Keymap**) — three new keymaps
  mirroring IntelliJ (macOS and Windows defaults) and Eclipse (Windows). See
  `spec/keymaps/index.md`.
- **Spacemacs keymap** (**View → Keymap → Spacemacs**) — Vim-style modal editing
  plus a `Space` leader for menu-like command sequences (`SPC f f` open, `SPC g s`
  git, `SPC w /` split, `SPC q q` quit, …). See `spec/keymaps/index.md`.
- **Inline git blame** (**Git → Toggle Inline Blame**, `inline_blame` setting) —
  shows the cursor line's blame (`author, date · summary`) dimmed at the end of
  the line, following the cursor. See `spec/git-integration/index.md`.
- **Outline sidebar** (**View → Layout → Outline Sidebar**, `show_outline_dock`) —
  a persistent symbol list beside the editor that follows the cursor; click a row
  to jump. See `spec/outline-sidebar/index.md`.

- **Test runner.** **Tools → Run Tests** runs the configured `test_command`,
  parses the output (cargo libtest, pytest `-v`-style) into a pass/fail panel with
  ✓/✗/○ icons, jump-to-failure on click, and a summary notification. New
  `test_command` / `test_width` settings. See `spec/test-runner/index.md`.
- **Integrated debugger (DAP).** A new **Debug** menu drives an external debug
  adapter (configured via `debug_adapters`): breakpoints (gutter `●`), start/stop,
  continue, step over/into/out, pause, a call-stack + variables + watch panel, an
  evaluate REPL, and program output in the bottom dock. Built on a DAP client that
  reuses the LSP framing. See `spec/debugger/index.md`.

### Changed

- **Nested split panes (up to a 2x2 grid).** The editor split is now a binary
  tree: **View → Split → Vertical/Horizontal** split the *focused* pane, so
  repeated splits nest into a grid (up to four panes). Drag any divider to resize
  that split; Other Pane (`F6`) cycles focus; Unsplit collapses the focused pane.
  The layout persists in the session. See `spec/split-panes/index.md`.

## [0.4.0] - 2026-06-27

### Added

- **AI chat panel** (**AI → Chat…**, action `ai.chat`) — a persistent
  conversation surface for the configured assistant CLI. Each reply is remembered
  and fed back as context, so follow-up questions work; an editor selection seeds
  the input. `Alt+T` opens the last reply in a tab, `Alt+C` copies it. See
  `spec/vix-agent-panel/index.md` and `docs/agent-panel/index.md`.
- **AI edit review.** Annotate and Improve now open an accept/reject **diff
  review** (hunk by hunk) instead of overwriting the buffer immediately: `↑↓` move
  between hunks, `Space` toggles one, `a`/`r` accept/reject all, `Enter` applies,
  `Esc` discards. New `ai_diff_review` setting (on by default) controls it. See
  `spec/ai/index.md`.
- **Background results in the notification panel.** Run Command (and Git
  Pull/Push/Fetch) completions and AI menu outcomes now post to the right-dock
  notification feed (Info on success, Error on failure), giving a durable record
  beyond the transient status bar. See `spec/vix-notification-panel/index.md`.
- **Open buffers reload after a branch switch.** Switching branches now re-reads
  every clean open file from disk so it reflects the new branch; dirty buffers are
  left untouched and a notification reports the count.
- **Persistent spellcheck user dictionary.** Words added from the spell-suggest
  popup now persist across sessions in `user_dictionary.txt` (config directory)
  and are reloaded on launch, instead of being session-only.
- **Multi-line commit messages.** The Git commit prompt now accepts a multi-line
  message: `Alt+Enter` inserts a newline, `Enter` commits — so subject + body
  messages work.
- **Searchable keyboard-shortcut browser.** The Help → Shortcuts overlay (`F1`)
  now has a search field — type to filter shortcuts by key or description.
- **Tools → Insert → LaTeX** submenu — inserts Org/LaTeX markup snippets
  (headlines, link, bold/italic/underline, table, deadline/scheduled, timestamps,
  quote/verse/center blocks, drawer).
- **Tools → Insert → SQL** submenu — inserts ready-to-edit PostgreSQL snippets:
  Alter Role, Create Extension, Create Function, Create User, Grant Create, Grant
  Usage, and Create Table (with trigger + trigram index).
- **Diff gutter in soft-wrap mode.** The Git change bar in the line-number gutter
  now also renders when soft wrap is on (on a changed line's first visual row).
- **Richer session restore.** Reopening a workspace now also restores each file's
  **scroll position** and the **split-pane layout** (direction, ratio, focused
  side), not just open files, focus, and cursor. Older session files still load.
- **EditorConfig support.** Vix now reads `.editorconfig` files and applies their
  indent style/size and trim/final-newline rules per opened file, overriding the
  global settings. New `editorconfig` setting (on by default). See
  `spec/editorconfig/index.md`.
- **Task runner.** A workspace `tasks.toml` (or `.vix/tasks.toml`) defines named
  shell commands; **Tools → Tasks…** (action `tools.tasks`) lists and runs them
  through the async pipeline. See `spec/tasks/index.md`.
- **Compare With File.** **Tools → Compare With File…** (action `tools.diff`)
  shows a read-only unified diff between the active buffer and another file. See
  `spec/diff-view/index.md`.
- **Project-wide replace preview.** Workspace-wide replace now shows a
  preview/confirm overlay listing each affected file and its match count; nothing
  is written to disk until you confirm (`y`/`Enter`), with `n`/`Esc` to cancel.
- **Integrated terminal.** **Tools → Terminal** (action `tools.terminal`) opens a
  real interactive shell on a PTY (`portable-pty` + `vt100`) inside Vix —
  full-screen programs, colors, and the cursor all work. `Ctrl+]` closes it. See
  `spec/terminal/index.md`.

### Changed

- **AI menu is now CLI-agnostic.** The AI commands (Summarize, Explain, Define,
  Annotate, Improve) no longer hardcode `claude`; they run the new `ai_command`
  setting (default `claude -p "{prompt}"`). The template's `{prompt}` placeholder
  receives the instruction and the input text is piped on stdin (or substituted
  for `{file}`), so the menu can drive Codex, Mistral, a local `ollama` model, or
  any other assistant CLI. See `spec/ai/index.md` and the configuration docs.

## [0.2.0] - 2026-06-27

### Added

- **Edit surfaces (Tools menu)** — full-screen overlay editors for non-plain-text
  views of the active buffer: **Edit Table** (CSV/TSV spreadsheet, `edit_table`),
  **Edit Outline** (indented prose hierarchy with folding, `edit_outline`),
  **Edit JSON** / **Edit YAML** (foldable structured-value tree, `edit_value`),
  and **Edit Bytes** (hex/ASCII byte editor, `edit_bytes`).
- **Tools → Insert submenu** (renamed from "Generate") — inserts generated content
  at the cursor: UUID, ZID, **Markdown** and **HTML** snippets, **Lorem ipsum**
  (`lorem`), and **Date/Time** presets (ISO 8601, RFC 3339, Unix epoch).
- **QR Code generator** (Tools → QR Code) — encodes the selection or line into a
  scannable Unicode QR overlay (`qr_tool`, via the `qrcode` crate).
- **Git per-hunk unstage** (`git.unstage_hunk`) — completes interactive per-hunk
  staging alongside stage/revert; plus diff next/prev hunk navigation.
- **Select all occurrences** (`edit.select_all_occurrences`) — multi-cursor on
  every match of the selection — and **column / rectangular selection**
  (Alt+Shift+↑/↓, `edit.column_select_*`) with block editing.
- **Zen (focus) mode** (`view.zen`), an optional **breadcrumb bar**
  (`view.breadcrumbs`, `file ▸ symbol`), and **trim / final-newline on-save**
  toggles.
- **Release packaging** — `cargo-dist` (macOS arm64/x64, Windows, Linux MUSL) and
  a `Makefile` that tests then cross-builds the three targets.
- **Single-crate architecture (edition 2024).** Every former `vix-*` subcrate is
  now a module under `src/`; the editor widget is the `editor_core` module
  (Tree-sitter highlight queries in `langs/`, gated behind `lang-*` features).
  The workspace has no members. See `AGENTS/share/crate-map.md`.
- **`#![warn(clippy::pedantic)]` in every module**, with no blanket
  `#![allow(clippy::pedantic)]`/`#![allow(missing_docs)]` — findings fixed in
  code; only four targeted `struct_excessive_bools` allows remain.
- **Full Language Server Protocol support — all 25 methods** in
  `spec/lsp/language-server-protocol.tsv`: lifecycle + document sync,
  `publishDiagnostics` (colored underlines + diagnostics panel), `hover`,
  `completion` + `completionItem/resolve`, `definition`/`implementation`/
  `typeDefinition`, `references`, `documentHighlight`, `documentSymbol`,
  `workspace/symbol`, `signatureHelp`, `formatting`/`rangeFormatting`, `rename`,
  `codeAction`, `codeLens`, `selectionRange`, `foldingRange`, `inlayHint`,
  `linkedEditingRange`. Configured per language via `lsp_enabled`/`lsp_servers`
  (no built-in server); protocol core in `src/lsp_core/`, process IO in
  `src/lsp.rs`. See `spec/lsp`.
- **Editor features:** code folding (▾/▸ gutter), inline inlay hints, bookmarks
  (toggle/next/prev/list), keyboard macros, buffer-word autocomplete, overwrite
  mode, column ruler, multi-cursor up/down, line transforms (join/sort/sort-
  unique/reverse/dedupe/trim), and a shortcuts/key-menu overlay.
- **Tools:** MD5/CRC32 checksums, JWT decode, base conversion (dec/hex/bin/oct),
  a live regex tester, markdown preview, snippets, and a text-information panel.
- **Git:** stash / stash pop / commit amend, a merge-conflict resolver
  (keep ours/theirs/both, next conflict), and per-hunk stage/revert.
- **X11 Colors picker** (Tools → X11 Colors) — swatch + hex + name; inserts the
  hex. New `x11_color_picker` module.
- **HTML Characters picker** (Tools → HTML Characters) — glyph / entity / code
  point; clicking a cell inserts that cell's text. New `html_character_picker`
  module.
- **macOS VSCode keymap** — Quick Open (`Ctrl+P`), Command Palette
  (`Ctrl+Shift+P`), Go to Symbol, Go to Line, and the familiar VS Code chords.
- **Edit → Go submenu** — File Start/End, Line Number, and Line / Paragraph /
  Section Start & End cursor jumps.
- **Edit → Select** gains **Select Paragraph** and **Select Section**.
- **Scrollbars** on the file explorer and the character/color pickers.

### Changed

- **Renamed the Tools "Generate" submenu to "Insert"** (actions `tools.generate.*`
  → `tools.insert.*`); format/acronym menu labels route through `menu.name.*`
  locale keys.
- **`Alt+V` now opens the Vix menu** (the first menu); **View** moved to `Alt+I`.
- **Renamed "keyway" → "keymap"** throughout (setting, chooser module, menus, docs).
- **Renamed "project" → "workspace"** throughout — **Find In Workspace…**,
  **Workspace Dashboard**, workspace-wide search/replace, the `workspace_dashboard_panel`
  module, and the `workspace_search` module.
- **Renamed the picker crates** from `-palette`/`-panel` to `-picker`:
  `ascii_character_picker`, `html_character_picker`, `nerd_font_picker`,
  `x11_color_picker` (the ASCII one was `ascii_panel`).
- **Menu dropdowns open with nothing highlighted** — the user arrows, hovers, or
  types to pick an item (no auto-selected first row).
- **Removed the standalone Find & Replace menu item** — replace now lives inside
  the Find panel (`Ctrl+R` or `Tab` to the Replace field).
- **Per-crate specifications** moved into each crate's `spec/index.md`; the
  top-level `spec/` keeps the app-level specs.
- **Shortcut labels use spaces** instead of `+` (e.g. `Ctrl Shift Z` rather than
  `Ctrl+Shift+Z`) in menus and the keyboard-help overlay.
- **Recent-files count is configurable** via the `recent_files_max` setting
  (default 15), controlling how many entries **File → Open Recent…** keeps.
- **Spellcheck autodetects Hunspell dictionaries** from the platform's standard
  locations (`/usr/share/hunspell`, `/Library/Spelling`,
  `/opt/homebrew/share/hunspell`, `$XDG_DATA_HOME/hunspell`, and `hunspell -D`).
  The `dictionaries_dir` setting is replaced by `dictionary_path` (an extra
  directory to search; empty = autodetect only); both the standard
  `<name>.{aff,dic}` and wooorm `<name>/index.{aff,dic}` layouts are accepted.
- **Bottom-dock scrollback is configurable** via the `scrollback` setting
  (default 1000 lines, down from a hard-coded 5000); the oldest lines are dropped
  past the limit.
- **Redo shortcut is now `Ctrl+Shift+Z`** (was `Ctrl+Y`), matching the common
  undo/redo pairing.

### Added

- **Right-click context menu** in the editor: Cut / Copy / Paste, Select All /
  Select More / Select Less, and Find / Find Next / Find Previous.
- **File-explorer path filters**: filter the tree by **Include regex** and
  **Exclude regex** (command palette "Explorer: Include/Exclude Regex Filter").
  Files whose workspace-relative path fails the filter are hidden; directories
  stay visible. The explorer title shows `(filtered)` when active.
- **Outline panel** (`Ctrl+Shift+B`, or the palette "Outline"): a list of the
  active buffer's symbols (kind prefix + name); Enter or a click jumps to the
  symbol, and the cursor's enclosing symbol is selected on open. New internal
  `outline_panel` module.
- **Workspace Dashboard** (Tools → Workspace Dashboard): a live overlay showing the
  workspace folder name, disk usage (`du`), file count, and git commit count, each
  computed asynchronously and filled in as it completes. New internal
  `workspace_dashboard_panel` module.
- **Select More / Select Less** (Edit menu; `Ctrl+Shift+→` / `Ctrl+Shift+←`):
  grow or shrink the selection by a word. **Move Up / Move Down** (Edit menu;
  `Alt+↑` / `Alt+↓`) are now also surfaced in the menu.
- **Case transforms** (Edit → Case): convert the selection to Upper, Lower,
  Title, Kebab (`foo-bar`), Snake (`foo_bar`), Camel (`fooBar`), or Pascal
  (`FooBar`).
- **Workspace search path filters**: the workspace-wide search/replace panel gains
  **Include path** and **Exclude path** regex fields that narrow the searched
  files by their workspace-relative path (`Tab` cycles to them).
- **Git integration** via the new `git` module, shelling out to the `git`
  CLI. The status bar shows the current branch and a dirty dot; the file explorer
  shows colored M/A/?/D/R/U badges on changed files; the editor draws a colored
  diff gutter (added/modified/deleted) against HEAD. The **Git** menu offers a
  **Changes…** panel to stage/unstage files (`Space`/`s`/`u`) and commit (`c`),
  **Switch Branch…**, and **Pull / Push / Fetch** (streamed to the bottom dock).
- **Spell checking** (View → Editor → Toggle Spellcheck): underlines misspelled
  words in comments and string literals in red, using Hunspell dictionaries from
  the `dictionaries/<locale>/` directory (`dictionaries_dir` setting) via the new
  pure-Rust `spellcheck` module. The language follows the UI locale; code-like
  tokens (acronyms, camelCase identifiers) are skipped. Off by default. With the
  cursor on a misspelled word, **`Ctrl+;`** opens a suggestions popup with
  replace, add-to-dictionary, and ignore actions.
- **System Information panel** (Tools → System Information): a scrolling,
  read-only snapshot of the host — OS, CPU, memory, swap, disks, uptime, and
  environment (via the `sysinfo` crate). Enter or a click inserts the highlighted
  value into the editor; Esc closes. Lives in the new internal
  `system_information_panel` module.
- **Unsaved-changes prompt.** Closing a tab or quitting with unsaved changes now
  raises a modal asking to **(s)ave**, **(d)on't save**, or **(c)ancel**. Quit
  walks every dirty tab in turn before exiting. Vim `:q!` still force-quits
  without prompting.
- **ASCII panel** (Tools → ASCII): a scrolling overlay of the 128 ASCII codes
  showing each code's decimal, hexadecimal, and character representation. Arrow
  keys / PageUp / PageDown / Home / End move the highlight; Enter or a click
  inserts the highlighted character into the active editor; Esc closes. Lives in
  the new internal `ascii_character_picker` module.
- **View → Layout submenu.** The dock and status-bar toggles (Show/Hide Left
  Dock, Right Dock, Bottom Dock, Bottom Status) now live under a **Layout**
  submenu, alongside the existing **Editor** submenu.
- **Menu type-ahead.** With a menu open, typing a letter jumps to the next item
  whose label starts with it, cycling — e.g. in File, `S` → Save, `S` → Save As.
  Works inside an open submenu too.
- **Search in Workspace → Dock** (Edit → Find submenu, or the palette): search
  every workspace file for a term and list the hits in the bottom dock as
  `path:line:col` lines — each one click-to-jumps to the match. In the prompt,
  `Alt+C` toggles case-sensitivity and `Alt+R` toggles regex.
- **Run Command** (Tools → Run Command…, or the palette): prompt for a shell
  command, run it in the workspace root in a **background thread**, and **stream**
  its merged stdout/stderr into the bottom dock (shown automatically) line by
  line, with a `$ command` header and an `[exit N]` footer. The UI stays
  responsive; **Cancel Command** (Tools menu / palette) kills a running command.
- **Resizable bottom dock.** The bottom dock is pinned directly above the status
  bar, and its top edge is draggable to grow or shrink it (persisted in the
  `bottom_dock_height` setting), matching the draggable left/right docks.
- **Bottom-dock scrolling, focus & click-to-jump.** Click the bottom dock to
  focus it (its border brightens); then `↑`/`↓`, `PgUp`/`PgDn`, and `Home`/`End`
  scroll its buffer. The mouse wheel scrolls it any time; `Esc` returns focus to
  the editor. Clicking a line that names a `path:line[:col]` location (a build
  error, grep hit, …) opens that file there, making Run Command output
  actionable.
- **Bottom dock** (View → Show/Hide Bottom Dock, or the palette;
  `show_bottom_dock` setting, default off): a full-width scrollable line buffer at
  the bottom of the body for log messages, command/terminal output, data views,
  etc. State lives in the new `bottom_dock` module (line buffer + scroll).
- **Calendar month-nav arrows.** The calendar box's month header shows
  `◀ Month Year ▶`; the arrows are clickable (and mirror the `←`/`→` keys), and a
  bottom help line shows `◀ ▶ month   Esc close`.
- **Many more UI languages.** Added Italian, Korean, Turkish, Dutch, Vietnamese,
  Indonesian, Thai, Persian, Ukrainian, and Greek — plus **Klingon** (`tlh`) and
  **Sindarin** (`sjn`) — for 27 selectable languages. The full menu bar is
  translated into the 15 primary locales; other keys fall back to English.
- **Calendar click-to-insert.** In the calendar box, clicking one of the
  date-time lines (local date-time, UTC ISO instant, ISO week date) inserts that
  string into the editor; clicking a day in the month grid inserts that date
  formatted per the active locale. The box stays open for repeated inserts; a
  click outside closes it.
- **Nested submenus** in the menu bar. **View → Editor** groups the editor
  display toggles (line numbers, visible whitespace, scroll bar); **Edit → Find**
  groups the find-related items (Find, Find Next, Find Previous, Find Selection,
  Find & Replace). Arrow keys / clicks open and navigate submenus (Right or a
  click opens, Left or Esc backs out).
- **Show/Hide Editor Scroll Bar** (View → Editor, or the palette): toggle the
  editor's right-side scroll bar; the text reclaims the column when hidden.
  Persists in the `show_scrollbar` setting (default on).
- **Reopen Closed Tab** (`Ctrl+Shift+T`, File menu, or the palette): reopen the
  most recently closed file (remembers a stack of recently closed paths).
- **Close All Tabs** (File menu, after Close, or the palette): close every open
  buffer, leaving a single empty untitled buffer.
- **Find Next / Find Previous / Find Selection** in the Edit menu (after Find).
  Find Next (`Ctrl+G`) and Find Previous (`Ctrl+Shift+G`) repeat the last search
  — and now keep working **after the find box is closed** (the last pattern is
  remembered; `F3` / `Shift+F3` repeat it too). Find Selection jumps to the next
  occurrence of the selection (`Alt+N`).
- **Toggle Bottom Status** (View menu / palette / `view.status_bar`): show or
  hide the bottom status bar; the editor body reclaims the row when it is hidden.
  Persists in the `show_status_bar` setting (default on).
- **Editing comforts.** **Select All** (`Ctrl+A`, Edit menu, or the palette),
  **Duplicate Line** (`Ctrl+D` or the palette), **Move Line Up/Down**
  (`Alt+↑`/`Alt+↓` or the palette), and **Jump to Matching Bracket** (`Ctrl+]` or
  the palette). Auto-indent on Enter (carry the previous line's leading
  whitespace) was already present and is now covered by tests.
- **Nerd Font Palette** (Tools → Nerd Font Palette…, crate
  `nerd_font_picker`): a character picker showing a grid of curated Nerd
  Font glyphs. Browse with the arrow keys or the mouse; Enter or a click inserts
  the highlighted glyph into the active editor and leaves the palette open so
  several can be picked in a row; Esc closes it.
- **Menu separators.** Dropdowns group related items with non-selectable
  divider lines: in File (before Open, Close, Quit), Edit (before Cut, Toggle
  Comment, Find), and View (before Toggle Left Dock, Toggle Editor Line Numbers).
  Keyboard navigation, hover, and clicks skip separators.
- **Bracket matching.** When the cursor is on (or just after) a bracket
  `()[]{}`, its matching partner is highlighted. No auto-insertion of pairs.
- **Richer status bar.** The status bar now shows the language, line ending
  (LF/CRLF), encoding (UTF-8), and — when text is selected — the selected
  character and line count, alongside the existing line:column.
- **Fully-custom editor widget (`editor_core`) with soft wrap.** The editor was
  migrated from the vendored `editor_core` fork to an in-house widget:
  the Tree-sitter highlighting + buffer + undo/redo engine is reused, while the
  editor state, input, mouse, and renderer are owned by Vix. The renderer now
  supports **soft wrap** — toggle with **View → Toggle Soft Wrap** (or the
  palette); the `soft_wrap` setting persists. Long lines wrap across screen rows
  with cursor, scroll, and mouse all wrap-aware. (Also fixed a latent panic when
  jumping to a line past the end of the buffer.)

- **Internationalization** via `rust-i18n`. The entire UI is translatable; 15
  languages are selectable (English, Spanish, French, German, Welsh fully
  translated; Irish, Scottish Gaelic, Polish, Portuguese, Russian, Arabic, Hindi,
  Bengali, Chinese, Japanese with menu/theme coverage and English fallback).
  Language is chosen with `--locale`, the `locale` setting, or **View → Locale…**
  (a live chooser). English is the fallback. See `docs/i18n.md`.
- **Themes.** All themes are **JSON themes** with per-region RGB colors (menu
  bar, status bar, left/right dock, editor) and optional editor cursor and syntax
  colors. Dark and Light ship bundled; more are bundled too, and users can add
  their own in `~/.config/vix/themes/*.json`. Chosen live in **View → Theme…**.
  See `docs/themes.md`.
- **Configuration** via `confy`, stored as TOML in the platform config directory.
  New `theme` and `locale` settings. See `docs/configuration.md`.
- **Command-line interface** via `clap`: positional files (with optional
  `path:line:col`) and a `--locale` flag; `--help` / `--version`.
- **Vix menu** (first in the bar) with **About Vix** (shows `Vix <version>`),
  **Website**, and **Email** — each opens a modal dialog with an **Ok** button.
  The Website/Email dialogs show the text in a selectable text field (drag or
  arrow-select, `Ctrl+C` to copy).
- **Keymaps** (**View → Keymap…**, module `keymap_model`): choose the
  keyboard navigation style, which changes how keys are dispatched. The choice
  persists (`keymap` setting); Apple is the default.
  - **Apple** — modifier shortcuts (e.g. `Ctrl+O` open, `Ctrl+Q` quit).
  - **Emacs** — `Ctrl` chords and the `Ctrl+X` prefix: `Ctrl+X Ctrl+F` open,
    `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit, `Ctrl+X k` close; cursor motion
    with `Ctrl+F/B/N/P/A/E/V`, `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel.
  - **Vim** — modal: a Normal mode (`h/j/k/l`, `0`, `$`, `x`, `i/a/o/O` to enter
    Insert, `Esc` back to Normal) and a `:` command line (`:w`, `:q`, `:q!`,
    `:wq`/`:x`, `:Ex`). The status bar shows the current mode.
- **View menu** with theme, locale, and keymap choosers and the drawer/line-number
  toggles.
- **Indentation settings** — `indent_style` (`"spaces"` / `"tabs"`) and
  `tab_width` control what the Tab key inserts (default: 4 spaces), overriding the
  editor widget's per-language default.
- **Live go-to-line preview** — in the palette's `:` mode the cursor now follows
  the line number as you type (scrolling it into view); `Enter` commits (recording
  the original position in the jump history) and `Esc` reverts. (Also fixes a
  latent panic when jumping to a line past the end of the buffer.)
- **Find occurrence of selection** (`Alt+N` / `Alt+P`, or the palette): jump to
  the next/previous occurrence of the current selection — or the word under the
  cursor when there is no selection — without opening the search bar.
- **Smart Home** — `Home` jumps to the first non-blank character of the line;
  pressing it again jumps to column 0 (toggling between the two).
- **On-save normalization** — two settings (`trim_trailing_whitespace`,
  `ensure_final_newline`, both default on) strip trailing spaces/tabs from each
  line and append a final newline when saving. (Making the previously
  always-on final-newline behavior configurable.)
- **Toggle Comment** (`Ctrl+/`, the Edit menu, or the palette): comment or
  uncomment the cursor line or every line in the selection, using the language's
  comment token (`//`, `#`, `--`), as a single undoable edit. The editor widget's
  comment-token map gained TOML/YAML (`#`) and SQL (`--`).
- **Go to Symbol in File** — a new command-palette mode (`@` prefix, or the
  "Go to Symbol in File" command) listing the current file's declarations
  (functions, types, classes, traits, modules, `#define`s, …) to fuzzy-filter
  and jump to. A fast, offline, language-agnostic heuristic — no language server.
- **Open Recent** (`File → Open Recent…`, `Ctrl+Shift+O`, or the palette): a
  chooser of recently opened files. The list (most-recent first, de-duplicated,
  capped at 15) persists in the `recent_files` setting.
- **Toggle Editor Visible Whitespace** (View menu / palette / `view.whitespace`):
  render dim glyphs for space (`·`), tab (`→`), carriage return (`␍`), and line
  ending (`¶`). Off by default; persists in the `show_whitespace` setting.
- **Dock toggle icons** in the menu bar (clickable explorer/messages toggles;
  bright when open, dim when closed).
- A visible **block cursor** in the editor, themeable via a custom theme's
  `cursor` color.
- Custom themes can set per-region **`font-style`** (`normal`/`italic`) and
  **`font-weight`** (`normal`/`bold`); the editor also applies a custom theme's
  syntax token colors.
- **Editor scrollbar drag**: press and drag the scrollbar thumb/track to scroll.
- **Resizable docks**: drag the explorer's right edge or the message drawer's left
  edge to resize them. The widths persist (`explorer_width` / `messages_width`).
- A collection of themes **bundled into the binary** (Darker, Darkest, Lighter,
  Lightest, Matrix, Turbo, Solarized Dark/Light, Dracula, Nord, Gruvbox Dark,
  Monokai, One Dark, Tokyo Night) that appear in **View → Theme…** with no
  installation. A same-named theme in `~/.config/vix/themes/` overrides a
  bundled one.
- New modules: `theme_model`, `locale_model`,
  `keymap_model`, `keyboard_shortcut_panel`,
  `calendar_panel`, `nerd_font_picker`, and `find_panel`
  (the find / find-and-replace box state).
- New docs: `docs/themes.md`, `docs/i18n.md`, `docs/configuration.md`,
  `index.md`, `AGENTS.md` (+ `AGENTS/`), and this changelog.

### Changed

- **Docks and status bar extracted to modules.** The left dock (file
  explorer) moved to `left_dock`, the right dock (message drawer) to
  `right_dock`, and the status-bar segment formatting to
  `status_bar_panel`. The app re-exports them; behavior is unchanged.
- The main panes use a lighter border frame: the left and right docks keep only
  their inner (top + side-facing-the-editor) borders, the center editor keeps
  only its top border, and the bottom status bar gains a full-width top border
  that separates it from the body.
- The editor widget module was renamed `ratatui-code-editor` →
  `editor_core` and made **theme-aware** (configurable text,
  line-number, selection, and cursor styles, and a settable syntax palette).
- The calendar logic moved into `calendar_panel` and gained
  month navigation (Left/Right while the calendar is open).
- **Every theme is now a JSON theme.** The hardcoded monochrome Dark/Light
  *modes* were removed; **Dark** and **Light** are now ordinary bundled themes
  (`themes/dark.json` / `themes/light.json`, soft `[215,215,215]` on `[40,40,40]`
  and its inverse) loaded like any other. The chooser lists every theme
  (including Dark and Light) sorted by name, and the persisted `theme` setting is
  the theme's name.
- Settings moved from hand-rolled JSON to `confy` TOML.
- All public items are documented (`#![deny(missing_docs)]`); the module forbids
  `unsafe`.

### Fixed

- Panel border lines now use each pane's own foreground color (via
  `region_title`) instead of the global editor foreground, so borders match the
  pane under themes whose regions use different colors.
- In the find / replace box, clicking the Find or Replace field now focuses it
  (previously the box swallowed all mouse input, so the Replace field was only
  reachable with `Tab`, which the hint never mentioned). The hint now states
  `Tab / click: switch field` in replace mode.
- In the file explorer, `←` (Left) no longer expands a collapsed folder. It now
  collapses an expanded folder, or jumps to the parent folder when the selection
  is already collapsed — it never opens a folder.
- Duplicating the last line of a buffer with no trailing newline (`Ctrl+D`) now
  produces a real second line instead of concatenating the copy onto the
  original. Line-boundary detection at end-of-buffer was off by one, which also
  affected `Ctrl+K` (delete line) and triple-click line selection on the last
  line.
- Menu dropdown items keep at least one space between the label and the
  right-aligned keyboard shortcut (the widest item used to let them touch).
- Keyboard-only modal overlays (the calendar box, find, query-replace, workspace
  search, confirm, paste-conflict) now swallow mouse clicks instead of letting
  them fall through to the editor underneath.
- Menu mouseover now moves the selection: with a dropdown open, hovering a row
  highlights it and hovering another top-level name switches menus (any-motion
  mouse tracking is enabled for this; other panes ignore button-less motion).
- The theme, locale, and keymap choosers now respond to the mouse: clicking a
  row highlights (and, for theme/locale, live-previews) that entry instead of
  being ignored.
- The active editor tab keeps the theme background (marked with an underline)
  instead of reversed video, which showed a light background under a dark theme.
- Search-hit marks render monochrome (underline) instead of a hard-coded color.
- Overlays paint the theme background so they read correctly in the light theme.
- The menu dropdown no longer shows its raw i18n key as a title.
- Clicking an item in an open menu dropdown now runs it (and clicking away
  closes the menu).
- The tab bar paints the editor's theme background instead of resetting to the
  terminal default (it no longer shows white under a dark theme).
- Removed the gray app-name label from the right of the menu bar.
