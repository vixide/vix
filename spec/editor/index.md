# Editor

The center pane of Vix is the **editor**: the text-editing surface where every
open file lives. It is a full code editor with Tree-sitter syntax highlighting,
unlimited undo/redo, multi-line selection, system-clipboard cut/copy/paste,
mouse support, and optional soft wrap. Each open file is a **tab**; the active
tab's buffer fills the editor area, and the status bar shows its 1-based
line/column.

This page documents the core editing surface and the commands that act on it,
grouped as they appear in the **Edit** menu, plus the **View → Editor** display
toggles. Search-and-replace, broader cursor navigation (position history, go to
definition/symbol), and spell-checking each have their own pages and are
cross-referenced rather than duplicated here.

## The editor widget

The editor is backed by the bundled `editor_core` crate. Each buffer is one
`editor_core` editor instance with its own undo/redo history, selection, scroll
offset, and highlight cache. The host (`src/editor.rs`) addresses the cursor as a
flat character offset internally and converts to/from 1-based line/column for the
status bar and go-to-line.

Capabilities:

- **Syntax highlighting** — Tree-sitter grammars, selected from the file name's
  extension. Unknown or extension-less files fall back to plain `text`. With a
  custom JSON theme active, tokens are colored per the theme's syntax palette;
  the default monochrome themes show foreground only (no token colors). See
  [Themes](../themes/).
- **Undo / redo** — a per-buffer history of edits, with no fixed depth limit.
- **Selection** — a single contiguous range with an anchor and an active end.
  Selections can span multiple lines and drive cut/copy, case transforms, and
  comment toggling.
- **System clipboard** — cut/copy/paste use the OS clipboard, with an in-process
  fallback when the system clipboard is unavailable.
- **Mouse** — click to place the cursor, drag to select, and wheel-scroll. The
  click/drag position maps screen cells to character offsets accounting for the
  line-number gutter and soft wrap.
- **Soft wrap** — long lines optionally wrap to the viewport width instead of
  scrolling horizontally (toggle below).
- **Marks** — the editor draws several independent underline/gutter channels:
  search hits, spell-check misspellings, LSP diagnostics, and Git diff gutter
  bars. These are owned by their respective features; the editor only renders
  them.
- **Bracket matching** — the partner of the bracket under the cursor is
  highlighted, and a command jumps to it (see below).
- **Bracket / quote auto-pairing** — typing an opener `( [ { " ' ``` inserts the
  matching closer and leaves the cursor between them; with a non-empty selection
  it wraps the selection instead; typing a closer when it already sits at the
  cursor steps over it. Quotes are not paired next to a word character (so
  apostrophes in prose/identifiers are untouched).
- **Auto-indent** — pressing Enter copies the current line's leading indentation
  (and the language's brace-depth indent) onto the new line.
- **Multiple cursors** — `Ctrl+D` adds the next occurrence of the word/selection
  as a caret, `Alt`+click adds one, and editing/movement then apply at every
  caret at once (see `spec/multiple-cursors/index.md`).
- **Smart indent** — pressing Tab inserts the buffer's configured indent string
  (spaces or a tab).

Image files (`.png`, `.jpg`, `.gif`, and similar) open in a view-only image tab
rather than the text editor; editing commands are no-ops on those tabs.

## Editing commands

The **Edit** menu groups the commands that act on the active buffer. Leaf
commands and their shortcuts:

| Command | Action | Shortcut |
|---|---|---|
| Undo | `edit.undo` | `Ctrl+Z` |
| Redo | `edit.redo` | `Ctrl+Shift+Z` |
| Cut | `edit.cut` | `Ctrl+X` |
| Copy | `edit.copy` | `Ctrl+C` |
| Paste | `edit.paste` | `Ctrl+V` |
| Toggle Comment | `edit.toggle_comment` | `Ctrl+/` |

Cut, paste, undo, and redo mark the buffer dirty and promote an ephemeral
preview tab to a permanent one. Copy does not modify the buffer. Cut and copy
operate on the current selection; paste inserts at the cursor (replacing any
selection).

The Edit menu also contains five submenus — **Select**, **Move**, **Go**,
**Find**, and **Case** — described in the following sections.

### Comment toggle

**Toggle Comment** (`Ctrl+/`) comments or uncomments the cursor's line, or every
line touched by the selection. The editor picks the correct comment token for the
buffer's language. The shortcut also responds to `Ctrl+7` and `Ctrl+_` (the same
physical key on many layouts).

## Cursor navigation — the Go submenu

The **Edit → Go** submenu jumps the cursor without selecting. Commands that may
move off-screen scroll the target back into view.

| Command | Action | Behavior |
|---|---|---|
| Go to Line… | `nav.goto_line` | Opens the palette seeded with `:` to type a line (and optional column). |
| Line Start | `edit.line_start` | Column 0 of the current line. |
| Line End | `edit.line_end` | End of the current line. |
| Paragraph Start | `edit.para_start` | First line of the current paragraph. |
| Paragraph End | `edit.para_end` | End of the last line of the current paragraph. |
| Section Start | `edit.section_start` | First line of the current section. |
| Section End | `edit.section_end` | End of the last line of the current section. |
| Document Start | `edit.go_first` | Very start of the buffer. |
| Document End | `edit.go_last` | Very end of the buffer (past the last character). |

Definitions:

- A **paragraph** is delimited by blank (empty or whitespace-only) lines. From a
  blank line, Paragraph Start climbs to the paragraph above.
- A **section** is a larger unit, delimited by a run of two or more consecutive
  blank lines.

Additional cursor keys handled directly by the editor (not in the menu):

- **Home** is *smart Home*: it jumps to the first non-blank character of the
  line, or to column 0 if already there (or if the line is blank). Pressing Home
  again toggles between the two.
- **End** goes to the end of the current line.
- **Delete** forward-deletes the character to the right of the cursor.
- **Page Up / Page Down** move the cursor by roughly one viewport height.
- **Match Bracket** (`edit.match_bracket`, `Ctrl+]`) jumps to the partner of the
  bracket at or just before the cursor; a no-op when the cursor is not on a
  bracket.

For position history (`Alt+Left` / `Alt+Right`), go to definition (`F12`), go to
symbol, and the `path:line[:col]` open syntax, see
[Navigation](../navigation/).

## Selection commands — the Select submenu

The **Edit → Select** submenu changes what is selected.

| Command | Action | Shortcut |
|---|---|---|
| Select More | `edit.select_more` | `Ctrl+Shift+→` |
| Select Less | `edit.select_less` | `Ctrl+Shift+←` |
| Select Line | `edit.select_line` | |
| Select Paragraph | `edit.select_paragraph` | |
| Select Section | `edit.select_section` | |
| Select All | `edit.select_all` | `Ctrl+A` |

- **Select More / Select Less** grow or shrink the active end of the selection by
  one word boundary (right or left respectively). A word is a run of
  alphanumeric characters and underscores.
- **Select Line** selects the current line including its trailing newline, so it
  can be cut as a whole line.
- **Select Paragraph** and **Select Section** select the whole paragraph or
  section under the cursor (same blank-line / blank-run delimiters as the Go
  commands).
- **Select All** selects the entire buffer.

The mouse also selects: click-and-drag extends a selection, and `Ctrl+C` copies
it. (Single-line text fields used by the Vix menu's Website/Email dialogs are the
same widget, so their text is selectable and copyable too.)

## Line move — the Move submenu

The **Edit → Move** submenu reorders lines.

| Command | Action | Shortcut |
|---|---|---|
| Move Line Up | `edit.move_line_up` | `Alt+↑` |
| Move Line Down | `edit.move_line_down` | `Alt+↓` |

Each command swaps the cursor's line with the row above or below and scrolls it
back into view. **Duplicate Line** (`edit.duplicate_line`) copies the cursor line
(or the selection) is also available via the command palette.

## Case transforms — the Case submenu

The **Edit → Case** submenu rewrites the selected text into a chosen case. Each
transform applies to the current selection.

| Command | Action | Example |
|---|---|---|
| Upper | `edit.case_upper` | `FOO BAR` |
| Lower | `edit.case_lower` | `foo bar` |
| Title | `edit.case_title` | `Foo Bar` |
| Kebab | `edit.case_kebab` | `foo-bar` |
| Snake | `edit.case_snake` | `foo_bar` |
| Camel | `edit.case_camel` | `fooBar` |
| Pascal | `edit.case_pascal` | `FooBar` |

See also [Case](../case-change/).

## Find — the Find submenu

The **Edit → Find** submenu drives in-buffer search. It is summarized here and
documented fully in [Find and replace](../find-and-replace/).

| Command | Action | Shortcut |
|---|---|---|
| Find… | `edit.find` | `Ctrl+F` |
| Find Next | `edit.find_next` | `Ctrl+G` |
| Find Previous | `edit.find_prev` | `Ctrl+Shift+G` |
| Find Selection | `search.next_selection` | `Alt+N` |
| Search Workspace to Dock | `search.workspace_dock` | |

## View → Editor display toggles

The **View → Editor** submenu controls how the editor surface is drawn. Each
toggle applies to every open buffer at once.

| Toggle | Action | Effect |
|---|---|---|
| Line Numbers | `view.line_numbers` | Show/hide the line-number gutter. |
| Whitespace | `view.whitespace` | Show/hide visible-whitespace glyphs (spaces, tabs). |
| Scroll Bar | `view.scrollbar` | Show/hide the editor scrollbar. See [Scrollbars](../scrollbars/). |
| Soft Wrap | `view.soft_wrap` | Wrap long lines to the viewport instead of scrolling horizontally. |
| Spellcheck | `view.spellcheck` | Toggle the misspelling underline. See [Spellcheck](../spellcheck/). |

The same submenu also holds tab navigation: **Next Tab** (`Ctrl+Tab`) and
**Previous Tab** (`Ctrl+Shift+Tab`), which cycle through open buffers.

Line-number, whitespace, and soft-wrap settings are persisted in
[Settings](../) and re-applied to every buffer when changed.

## As implemented in Vix

`src/editor.rs` is the host wrapper. It owns the tab strip (`Editor`, a `Vec<Tab>`
plus the active index) and the global display settings (`line_numbers`,
`show_whitespace`, `soft_wrap`, `indent`). Each `Tab` wraps one
`editor_core::editor::Editor` (re-exported as `CodeEditor`), its file path, dirty
flag, ephemeral-preview flag, and optional image protocol. The wrapper implements
the cursor-jump and selection commands (`cursor_line_home`, `cursor_line_end`,
`cursor_paragraph_start/end`, `cursor_section_start/end`, `cursor_document_start/end`,
`select_line`, `select_paragraph`, `select_section`, `select_word`,
`jump_matching_bracket`, `move_line`, `duplicate_line`, `delete_forward`,
`page_up`, `page_down`) on top of the `editor_core` primitives, and converts the
flat character offset to 1-based line/column via `cursor_1based`.

`src/menu.rs` defines the **Edit** menu and its `Select`, `Move`, `Go`, `Find`,
and `Case` submenus, plus the **View → Editor** submenu, as static action tables.
Each item carries an `action` string that `App::run_action` in `src/app.rs`
dispatches; the command palette reuses the same action names.

The `editor_core` crate (`editor_core/src/editor.rs`) provides the underlying
buffer, Tree-sitter highlighting and highlight cache, undo/redo, selection
anchor/extend, the OS clipboard with in-process fallback, mouse hit-testing
(`cursor_from_mouse`, `handle_mouse_down/drag`), soft wrap, and the separate mark
channels (`marks`, `spell_marks`, `diagnostic_marks`, `gutter_marks`). Cut, copy,
paste, undo, redo, comment toggle, select-all, and duplicate are applied as
`editor_core` actions from `App::run_action`.
