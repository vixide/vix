# Org

Vix™ includes a working subset of [Org mode](https://orgmode.org/) for
outline notes, task lists, tables and agendas. Every command is on the **Org**
menu (`Alt+O`) and in the command palette. Under the **Emacs** keymap the
familiar `C-c` chords run the same commands (see [Emacs chords](#emacs-chords)).

This page is the reference. For a guided walk through the same features, see
[Tutorial 6: Org Mode & Roam](../tutorials/06-org-mode-and-roam/index.md). For
networked notes (nodes, backlinks, dailies), see [Org-Roam](../roam/index.md).

It is a pragmatic subset, not a full Org implementation: there is no Emacs
Lisp, no Calc, and source blocks run only in a build with the `jupyter`
feature (see [Source blocks](#links-footnotes-source-blocks-and-archiving)).

<!-- Sources: crates/vix-menu/src/lib.rs (ORG, menu "menu.org");
crates/vix-menu/spec/index.md (Org Alt+O); crates/vix-org/spec/index.md
(intro, Menu, Emacs chords). -->

## Where Org commands work

- Org commands act on the active buffer at the cursor line. Most work in any
  buffer, but **Tab** folding, the table keys and the `[[mailto:` /
  `[[contact:` completion only work in a file whose extension is `.org`.
- When a command does not apply, the status line says why, for example
  *"Not on an Org headline"*, *"No sibling subtree to swap with"* or *"No
  heading that way"*. The buffer is left unchanged.
- The Org menu shows each command's Emacs chord in its shortcut column. Those
  chords are bound only in the Emacs keymap; in other keymaps use the menu or
  the palette. The table keys (Tab, Enter, Alt+arrows, Shift+arrows) are the
  exception: they work in every keymap while the cursor is inside a table.

<!-- Sources: tui/src/app.rs (active_is_org: extension "org");
tui/src/app/org_table.rs (org_table_key: editor focus, not read-only,
active_is_org, cursor inside a table); tui/src/app/keymap.rs (org_table_key
runs before every keymap's dispatch); crates/vix-i18n/locales/status.yml
(status.org_not_headline, org_no_sibling, org_no_heading);
crates/vix-org/spec/index.md (Emacs chords: "These chords live only in the
Emacs keymap"). -->

## Headlines and structure

Org outlines are built from headlines: lines starting with one or more `*`
and a space. The number of stars is the level. A **subtree** is a headline
plus every following line up to the next headline of the same or a higher
level.

```
* Project
** Task one
** Task two
```

### Folding

- **Tab** on a headline folds or unfolds its subtree. A headline with no body
  has nothing to fold, and Tab does not indent it.
- **Tab** on a drawer header such as `:PROPERTIES:` or `:LOGBOOK:` folds the
  drawer through its `:END:` line; the header shows a trailing `...`.
- **Shift+Tab** folds every headline and drawer (an overview), or unfolds
  everything when something is folded.
- **Org → Show/Hide** has **Cycle Visibility (Fold)** (what Tab does),
  **Overview (Fold All)** and **Show All**.
- **Org → Show/Hide → Sparse TODO Tree** folds every subtree that has no
  `TODO` headline; **Sparse Tree Match…** asks *"Sparse tree: show subtrees
  containing"* and folds every subtree without that text (case-insensitive).
  The status line reports *"Sparse tree: N subtree(s) folded"*.

Folding never edits the text, and it needs no language server: the ranges come
from the headlines themselves.

### Moving around

**Org → Navigate Headings** has **Up to Parent**, **Previous Heading** /
**Next Heading** (any level) and **Previous Same Level** / **Next Same Level**
(siblings only, stopping at the parent). **Org → New Heading** inserts a
sibling headline below the cursor line.

### Editing structure

**Org → Edit Structure**:

| Item | Effect |
| --- | --- |
| **Promote** / **Demote** | Remove or add one `*` on every headline of the subtree. Promote is refused at level 1. |
| **Move Subtree Up** / **Move Subtree Down** | Swap the subtree with its previous or next sibling. The cursor follows it. |
| **Copy Subtree** / **Cut Subtree** | Copy the subtree to the system clipboard (*"Subtree copied"*); cut also removes it. |
| **Paste Subtree** | Paste the clipboard as a sibling, releveled to fit. Refused with *"Clipboard does not start with an Org headline"* otherwise. |
| **Sort Children** | Sort the subtree's direct children alphabetically. |
| **Refile Subtree…** | Open a chooser of every headline outside the subtree; Enter or a click moves the subtree under the chosen one, one level deeper (*"Subtree refiled"*). |

Headlines, `TODO` and `DONE` keywords and links are highlighted. A link's
description is underlined and its brackets and target are dimmed; the full
`[[…]]` text stays editable.

<!-- Sources: crates/vix-org/spec/index.md (Concepts, Headline folding and
highlighting, Drawer folding, Menu); crates/vix-menu/src/lib.rs (the Show/Hide,
Navigate Headings and Edit Structure submenus); crates/vix-i18n/locales/status.yml
(status.org_sparse, org_subtree_copied, org_no_subtree_clip, org_refiled);
crates/vix-i18n/locales/prompt.yml (prompt.org_sparse_match);
tui/tests/integration/org.rs (tab_folds_an_org_headline_without_a_language_server,
org_tab_folds_drawer_under_cursor, org_subtree_copy_cut_and_paste,
org_menu_edits_headlines_and_exports); tui/src/app/org/tests.rs
(tab_folds_drawers_and_shift_tab_cycles_globally, subtrees_move_among_their_siblings,
refile_chooser_lists_targets_and_moves_the_subtree, sparse_trees_fold_and_prompt,
sorting_children_reorders_the_subtree_under_the_cursor). -->

## Tasks, priorities and checkboxes

- **Org → Cycle TODO** cycles a headline: none, `TODO`, `DONE`, none.
- **Org → Mark Done with Note…** asks for a *"Closing note (Alt+Enter =
  newline)"*, then marks the headline `DONE`, adds a `CLOSED: [now]` line
  and logs the note in the headline's `:LOGBOOK:` drawer.
- **Org → Priority Up** / **Priority Down** step the `[#X]` cookie after the
  keyword. Vix's range is `0` (highest) to `9` (lowest), unlike Emacs's
  `A`..`C`; a headline without a cookie gets `[#0]`. The steps stop at the
  ends of the range. The `org_priority_highest`, `org_priority_lowest` and
  `org_priority_default` settings change this.
- **Org → Toggle Checkbox** flips a list item between `- [ ]` and `- [x]`.

### Statistics cookies

A parent checkbox reflects its children: all checked gives `[X]`, none
`[ ]`, some `[-]`. A cookie `[/]` or `[%]` in a headline or a parent list
item counts its children:

```
* Organize Party [33%]
** TODO Call people [1/2]
*** TODO Peter
*** DONE Sarah
** TODO Buy food
** DONE Talk to neighbor
```

A headline cookie counts its top-level checkboxes if it has any, otherwise
its direct child TODO headlines. A `:COOKIE_DATA:` property of `checkbox` or
`todo` decides, and adding `recursive` counts the whole subtree. Cookies are
recomputed after Toggle Checkbox and Cycle TODO, and on demand with **Org →
Update Statistics**.

**Org → Refresh Context (C-c C-c)** does the right thing for the line: inside
a table it realigns and recalculates the table; on a checkbox it toggles it;
then it recomputes every statistics cookie. In a build with the `jupyter`
feature, inside a source block it evaluates the block instead.

<!-- Sources: crates/vix-org/spec/index.md (Menu, Priority, Checkbox &
statistics cookies); crates/vix-org/spec/priority/index.md;
tui/src/app/org.rs (org_ctrl_c_ctrl_c: jupyter_org_ctrl_c, then
org_table_recalc, then toggle_checkbox, then org_refresh_statistics);
crates/vix-i18n/locales/prompt.yml (prompt.org_close_note);
tui/src/app/org/tests.rs (promote_demote_cycle_and_priority_rewrite_the_headline,
checkboxes_update_their_parents_and_ctrl_c_ctrl_c_picks_the_right_job,
closing_with_a_note_needs_a_headline); tui/tests/integration/org.rs
(org_checkbox_toggle_updates_parents_and_cookies,
emacs_org_ctrl_c_chords_cycle_todo_and_close_with_note). -->

## Tags and properties

**Org → Tags & Properties**:

- **Set Tags…** asks *"Tags (colon separated; empty clears)"*, seeded with
  the headline's current tags, and replaces its trailing `:tag:tag:` group.
- **Set Property…** asks *"Property (NAME VALUE)"* and creates or updates the
  `:NAME:` line in the headline's `:PROPERTIES:` drawer, creating the drawer
  when it is missing.
- **Column View ▸** opens the property table described in
  [Column view](#column-view).

<!-- Sources: crates/vix-org/spec/index.md (Menu: Set Tags, Set Property);
crates/vix-menu/src/lib.rs (ORG_TAGS_PROPS); crates/vix-i18n/locales/prompt.yml
(prompt.org_set_tags, org_set_property); tui/src/app/org/tests.rs
(archive_tag_tags_and_properties). -->

## Dates, scheduling and time tracking

**Org → Dates & Scheduling**:

| Item | Effect |
| --- | --- |
| **Insert Timestamp** / **Insert Inactive Timestamp** | Insert today as `<2026-11-27 Fri>` (active) or `[2026-11-27 Fri]` (inactive). |
| **Schedule Item…** / **Deadline…** | Ask for a `YYYY-MM-DD` date (seeded with today) and set the headline's `SCHEDULED:` or `DEADLINE:`, on the existing planning line or a new one under the headline. A bad date says *"Invalid date (expected YYYY-MM-DD)"*. |
| **Date 1 Day Later** / **Date 1 Day Earlier** | Shift the date under the cursor by a day and rewrite its weekday. Off a date: *"No date at the cursor"*. |

Clocking works on `CLOCK:` lines, as Org writes them:

- **Org → Clock In** inserts an open `CLOCK: [timestamp]` entry at the
  cursor, in local time.
- **Org → Clock Out** closes the most recent open entry with the end time and
  the `=> H:MM` duration (*"Clocked out"*; *"No open clock entry to close"*
  when there is none).
- **Org → Time Tracker** totals each headline's `CLOCK:` durations in the
  buffer into a report table in a new tab.

<!-- Sources: crates/vix-org/spec/index.md (Menu: Dates & Scheduling, Clock
In/Out, Time Tracker); crates/vix-menu/src/lib.rs (ORG_DATES);
crates/vix-i18n/locales/status.yml (status.org_bad_date, org_no_timestamp,
org_clocked_out, org_no_clock, org_time_report); crates/vix-i18n/locales/prompt.yml
(prompt.org_schedule, org_deadline); tui/src/app/org/tests.rs
(planning_and_timestamps, clocking_in_and_out_and_the_time_report);
tui/tests/integration/org.rs (org_capture_inserts_todo_and_time_report_tabulates). -->

## Agenda

**Org → Agenda** builds a view from your `.org` files into a read-only buffer.

| Item | Shows |
| --- | --- |
| **Weekly/Daily Agenda** | `DEADLINE:` and `SCHEDULED:` items grouped by date, plus unscheduled `TODO`s. |
| **Global TODO List** | Every `TODO` headline that is not `DONE`. |
| **Match Tags/Property…** | Headlines whose tags match a query such as `work-boss`: `+tag` requires, `-tag` excludes, a bare `tag` requires (case-insensitive). |
| **Search…** | Headlines whose entry contains **all** the keywords you type. |
| **Stuck Projects** | Headlines that are not `DONE`, have children, and have no child that is not `DONE` (no next action). |

The status line reports *"Agenda view: N item(s)"*. Every view is
interactive: press **t** on a task line to cycle that task's TODO state in its
source `.org` file on disk. Vix reloads an open, unmodified buffer of that
file and rebuilds the same view in place (*"Task state cycled in its source
file"*).

### Which files the agenda reads

1. **A restriction lock**, when set: only the locked file. **Org → Agenda →
   Set Restriction Lock (This File)** sets it (*"Agenda locked to …"*; the
   file must be saved), **Remove Restriction Lock** clears it. The lock lasts
   for the session.
2. **The agenda file list**, when not empty: the `org_agenda_files` setting,
   paths relative to the workspace, saved with your settings. **Org → Agenda
   → File List** has **Add Current File**, **Remove Current File** and
   **Clear List (All Project Files)**. Listed files that do not exist are
   skipped.
3. **Otherwise every `.org` file** in the project.

**Org → Agenda → File List → Show Scope** reports which applies, for example
*"Agenda scope: every .org file in the project"*.

<!-- Sources: crates/vix-org/spec/index.md (Agenda scope, Agenda views);
crates/vix-menu/src/lib.rs (ORG_AGENDA, ORG_AGENDA_FILES);
crates/vix-i18n/locales/status.yml (status.org_agenda_view, org_agenda_toggled,
org_agenda_locked, org_agenda_files_all); crates/vix-i18n/locales/prompt.yml
(prompt.org_agenda_match, org_agenda_search); tui/tests/integration/org.rs
(org_agenda_t_cycles_task_in_its_source_file,
org_agenda_views_list_todos_matches_searches_and_stuck_projects,
agenda_file_list_add_remove_clear_and_lock_report_their_state);
tui/src/app/org/tests.rs (the_agenda_scope_follows_lock_file_list_and_default). -->

## Capture

**Org → Capture** files a new entry from a template, in the style of Emacs
`org-capture`. Five templates are built in:

| Item | Key | Inserts at the cursor |
| --- | --- | --- |
| **Anything…** | `a` | `* TODO <task>` after one prompt. |
| **Task…** | `t` | A review buffer prefilled with `* TODO `. |
| **Babel…** | `b` | A `#+begin_src <language>` / `#+end_src` block, after a language prompt. |
| **Note…** | `n` | `* <note>` and a creation timestamp. |
| **Contact…** | `c` | An org-contacts entry: name, then Email, Phone, Address, Birthday, Nickname and Note properties. |

**Choose Template…** opens every configured template in a chooser.

Each `%^{…}` field is asked in turn, and the prompt shows a live preview of
the whole entry: answered fields filled in, the current one marked `‹Label›`,
later ones as `[Label]`. Unless the template finishes immediately, the
expanded text then opens in a review buffer (*"Review capture: … (Alt+Enter =
newline)"*); Enter files it and the status line says *"Captured: …"*.

### Your own templates

Templates live in the `org_capture_templates` setting, as TOML
array-of-tables in `config.toml`:

```toml
[[org_capture_templates]]
key = "j"
description = "Journal"
target = "file+datetree:journal.org"
template = "* %U %^{Entry}"
```

Setting the list **replaces** the five built-ins, so copy any of them you
still want (their exact text is in the
[settings reference](../reference/settings.md)). **Anything…** and the other
fixed items run their template by key; with that key gone they say *"No
org_capture_templates entry with key …"*.

| Field | Meaning | Default |
| --- | --- | --- |
| `key`, `description` | How the template is chosen and labeled. | (required) |
| `template` | The text, with the placeholders below. | (required) |
| `target` | Where it is filed (below). | `"cursor"` |
| `entry_type` | `entry` (adds `* ` unless the text starts with a headline), `plain`, `item` (`- `), `check-item` (`- [ ] `) or `table-line` (`\| … \|`). | `"entry"` |
| `prepend` | File at the top of the target instead of the bottom. | `false` |
| `empty_lines` | Blank lines around the entry. | `0` |
| `immediate_finish` | Skip the review buffer. | `false` |
| `clock_in` | Add a `CLOCK:` line after the entry's first line. | `false` |

Placeholders: `%^{Label}` (a prompt; `%^{Label|a|b}` prefills the first
choice), `%t`/`%T` and `%u`/`%U` (active and inactive date or date-time),
`%<strftime>`, `%a` (a link back to where you started), `%i` (the
selection), `%f`/`%F` (the file's name or path), `%c` (the clipboard),
`%^g`/`%^G` (a tags prompt), `%?` (where the cursor lands) and `%%`.

Targets: `cursor`; `file:<path>` (appended, created if missing);
`file+headline:<path>#<Headline>` (under that headline, created if
missing); `file+datetree:<path>` (under today in a Year, Month, Day tree);
`id:<ID>` (under the headline or node with that `:ID:`, searched across the
project; *"No node with :ID: … found in the project"* if none). File targets
open the file after filing.

<!-- Sources: crates/vix-org/spec/index.md (Capture);
crates/vix-org-capture/spec/index.md (Placeholder syntax, Targets, Entry
types, Properties, Settings shape); crates/vix-org-capture/src/lib.rs
(wrap_entry, wrap_entry_adds_marker_only_when_missing);
crates/vix-settings/src/model.rs (EntryType, kebab-case);
crates/vix-settings/src/lib.rs (a user org_capture_templates list replaces the
default: the round-trip test's first template is "j");
crates/vix-menu/src/lib.rs (ORG_CAPTURE); crates/vix-i18n/locales/status.yml
(status.org_captured, org_capture_missing_key, org_capture_id_not_found);
crates/vix-i18n/locales/prompt.yml (prompt.org_capture_review);
tui/src/app/org/tests.rs (capture_by_key_runs_the_wizard_into_the_cursor,
capture_with_a_missing_key_or_no_templates_reports,
the capture test that files into files, headlines, date trees and ids,
clocked_captures_get_a_clock_line); tui/tests/integration/org.rs
(org_capture_field_prompts_preview_the_template_in_progress). -->

## Tables

Org's pipe tables are edited in place. A table is a run of lines starting with
`|`; a line starting `|-` is a horizontal rule. While the cursor is inside a
table in an `.org` file, these keys work in every keymap:

| Key | Action |
| --- | --- |
| `Tab` | Realign, then the next field (adds a row past the last field) |
| `Shift+Tab` | Realign, then the previous field |
| `Enter` | The next row (adds one past the last) |
| `Shift+Enter` | Copy the field from above into an empty field, or this field into the next row |
| `Shift+arrows` | Swap the cell with its neighbor |
| `Alt+Up` / `Alt+Down` | Move the row |
| `Alt+Left` / `Alt+Right` | Move the column |
| `Alt+Shift+Down` / `Alt+Shift+Up` | Insert a row above / delete the row |
| `Alt+Shift+Right` / `Alt+Shift+Left` | Insert / delete a column |

**Org → Table** has every structural edit as a menu item, plus:

- **Create Table from Selection** turns delimited text into a table
  (*"Table created"*); **Export to TSV** opens the table as TSV in a new tab.
- **Insert Horizontal Line** and **Insert Line Below and Move**.
- **Rectangle ▸ Copy / Cut / Paste Rectangle** for a block of cells.
- **Align Table**, **Recalculate (Apply Formulas)**, **Sum Column** (*"Sum: …
  (copied to the clipboard)"*), and **Sort Table…**, which asks *"Sort table
  (COLUMN [a|n|t] [r])"*: a column number, then alphabetic, numeric or time
  order, and `r` to reverse. It sorts the rows between the horizontal rules
  around the cursor, so a header row stays put.
- **Transpose Table** (horizontal rules are dropped).

Columns where most data cells are numbers are right-aligned. Outside a table
every table action says *"Not inside an Org table"*.

### Formulas

A `#+TBLFM:` line right after the table holds `::`-separated formulas.
`$3=$1+$2` fills column 3 of every data row; `@1$3=$1+$2` fills one field.
Row numbers count data rows only, not horizontal rules. Inside a formula,
`$N` is column N of the current row, `@R$C` is one field, and a range
`@1$1..@3$1` works as the argument of `vsum`, `vmean`, `vmin`, `vmax` or
`vcount`. A `;%.2f` suffix sets the decimals. A formula that fails writes
`#ERROR` into its field.

```
| 1 | 2 |   |
#+TBLFM: @1$3=$1+$2
```

Formulas run only when you ask: **Recalculate**, or **Refresh Context (C-c
C-c)** on a table line. There are no Calc or Lisp formulas, named references
or `remote()` references.

<!-- Sources: crates/vix-org-table/spec/index.md (Concepts, Alignment,
#+TBLFM formulas, Actions, Not implemented); tui/src/app/org_table.rs
(org_table_key key map); tui/src/app/keymap.rs (before every keymap);
crates/vix-menu/src/lib.rs (ORG_TABLE, ORG_TABLE_RECTANGLE);
crates/vix-i18n/locales/status.yml (status.org_table_no_table,
org_table_created, org_table_sum); crates/vix-i18n/locales/prompt.yml
(prompt.org_table_sort); tui/src/app/org_table.rs (Shift+Enter and
org.table.copy_down both call copy_down with increment false; sort uses
section_range); tui/tests/integration/org.rs
(org_table_modifier_chords_edit_and_move_within_the_table,
org_table_actions_outside_a_table_report_no_table); tui/src/app/org_table/tests.rs. -->

## Column view

**Org → Tags & Properties → Column View → Turn On** shows the outline at the
cursor as a table: one row per headline, one column per property. It edits the
buffer directly: every change is written into the Org text at once (saving to
disk is still up to you). Without an open Org buffer it says *"Open an Org file
to use Column View"*.

The columns come from the nearest `:COLUMNS:` property above the cursor, else
the file's `#+COLUMNS:` line, else `%ITEM %TODO %PRIORITY %TAGS`. A column is
`%[WIDTH]PROPERTY[(TITLE)][{SUMMARY}]`; a `PROPERTY_ALL` property lists the
values a cell can cycle through:

```
:COLUMNS:  %25ITEM %9Approved(Approved?){X} %Owner %11Status %10Time_Estimate{:}
:Owner_ALL:    Tammy Mark Karl Lisa Don
```

A summary rolls parent rows up from their children: `+` (sum), `$`, `min`,
`max`, `mean`, `X`, `X/`, `X%` (checkboxes), `:` and `:min`/`:max`/`:mean`
(times), `@min`/`@max`/`@mean` (ages) and `est+` (estimate ranges).

| Key | Action |
| --- | --- |
| arrows | Move between fields |
| `n` / `Shift+Right`, `p` / `Shift+Left` | Next / previous allowed value |
| `1`–`9`, `0` | The first to ninth, or tenth, allowed value |
| `e` | Edit the value (Enter commits, Esc cancels) |
| `a` | Edit the column's allowed values |
| `v` | Show the full value on the status line |
| `C-c C-c` | Toggle a checkbox field; on any other field, close |
| `<` / `>` | Narrow / widen the column |
| `Alt+Left` / `Alt+Right` | Move the column |
| `Alt+Shift+Right` / `Alt+Shift+Left` | Insert a column (asks for the property name) / delete it |
| `Alt+Up` / `Alt+Down` | Move the row's headline in the outline |
| `r` / `g` | Recompute |
| `q` / `Esc` | Close |

**Quick Column Export** writes the same table once, as a plain Org table in a
new tab. **Dynamic Block ▸ Insert…** asks for a scope (*"Dynamic block scope
:id (empty = local)"*) and inserts a `#+BEGIN: columnview` … `#+END:` block;
**Update at Point** (cursor on the `#+BEGIN:` line) and **Update All**
refresh such blocks.

<!-- Sources: crates/vix-org/spec/index.md (Column view, Concepts, COLUMNS
syntax, Summary types, Capturing column view); crates/vix-column-view/spec/index.md;
crates/vix-column-view/src/lib.rs (normal_key, ctrl_c_ctrl_c);
crates/vix-menu/src/lib.rs (ORG_COLUMN_VIEW); crates/vix-i18n/locales/msg.yml
(msg.column_view_no_buffer); crates/vix-i18n/locales/prompt.yml
(prompt.org_columns_insert_dblock, org_columns_insert_column);
crates/vix-i18n/locales/status.yml (status.org_columns_dblock_not_found). -->

## Links, footnotes, source blocks and archiving

**Org → Hyperlinks**:

- **Store Link to Here** remembers a `[[file:path::line][path:line]]` link to
  the cursor (*"Stored …"*); it seeds the next **Insert Link…**.
- **Insert Link…** asks for a *"Link target"*, then a *"Link description
  (empty for a bare link)"*.
- **Follow Link** opens `file:` links (honoring `::line`), resolves `id:`
  links by searching the project's `.org` files for that `:ID:`, jumps to a
  `*Headline` target, and copies `http(s):` and `mailto:` links to the
  clipboard (*"Copied … to the clipboard"*).
- **Next Link** / **Previous Link** move between `[[…]]` links.

**Org → Editing**:

- **Emphasis ▸** wraps the selection in `*bold*`, `/italic/`, `_underline_`,
  `~code~`, `=verbatim=` or `+strike+`; **Insert Block ▸** inserts an empty
  `#+begin_src`, `example`, `quote`, `center`, `verse` or `comment` block.
- **Edit Source Block** opens the `#+begin_src` block at the cursor in its own
  tab (*"Editing source block — run Edit Source Block (C-c ') again to
  apply"*); running it again there writes the body back (*"Source block
  updated"*). Outside a block: *"Not inside a source block"*.
- **Footnote New/Jump** jumps from a `[fn:x]` reference to its definition and
  back, or inserts the next numbered footnote under a `* Footnotes` headline.

Source blocks are text unless the build has the `jupyter` feature. Then
**Run → Jupyter → Run**, or `C-c C-c` in the block, evaluates it in a kernel
for its language and writes a `#+RESULTS:` block; nothing runs on open, save
or export. See [Jupyter](../jupyter/index.md).

**Org → Archive → Archive Subtree to File** moves the subtree to the sibling
`<file>_archive` file, as a level 1 headline with an `:ARCHIVE_TIME:`
property (*"Archived to …"*; an unsaved buffer says *"Save the file first"*).
**Toggle ARCHIVE Tag** adds or removes the `:ARCHIVE:` tag.

For inserting Org snippets, headlines, lists and drawers from a menu, see
**Tools → Insert → Org / Markers / Begin-End** ([Insert](../insert/index.md)).

<!-- Sources: crates/vix-org/spec/index.md (Menu: Hyperlinks, Editing,
Archive); crates/vix-org/spec/babel/index.md (Evaluation, T734e);
crates/vix-menu/src/lib.rs (ORG_LINKS, ORG_EDITING, ORG_EMPHASIS, ORG_BLOCK,
ORG_ARCHIVE, RUN_JUPYTER); crates/vix-i18n/locales/status.yml
(status.org_link_stored, org_link_copied, org_src_editing, org_src_applied,
org_no_src_block, org_archived, org_archive_unsaved);
crates/vix-i18n/locales/prompt.yml (prompt.org_link_target, org_link_desc);
tui/src/app/org/tests.rs (storing_and_inserting_a_link,
link_navigation_and_following, following_file_and_id_links_opens_the_target,
editing_a_source_block_in_its_own_tab_and_writing_it_back,
footnote_creates_and_navigates,
archiving_moves_a_subtree_to_the_sibling_archive_file);
tui/tests/integration/org.rs (org_edit_src_round_trips_a_block_body,
org_store_and_follow_links). -->

## Export

**Org → Export** converts the buffer and opens the result in a new tab
(*"Exported to a new … buffer"*):

| Item | Output |
| --- | --- |
| **Markdown** | Headings as `#`, emphasis, links and bullet lists. |
| **HTML** | A standalone HTML document. |
| **LaTeX** | A standalone LaTeX article: sections, itemize lists, verbatim blocks, `\href` links. |
| **iCalendar** | The buffer's `SCHEDULED:` and `DEADLINE:` entries as all-day events. |

`#+title:` becomes the top heading. Block delimiters are dropped and their
text kept. Emphasis follows Org's rules, so `and/or`, `snake_case_name` and
the slashes in a URL stay plain text. It is a quick conversion, not a
typesetter.

<!-- Sources: crates/vix-org/spec/index.md (Menu: Export, Export mapping);
tui/src/app/org.rs (org_export, org_export_ics); crates/vix-menu/src/lib.rs
(ORG_EXPORT); crates/vix-i18n/locales/status.yml (status.org_exported);
tui/src/app/org/tests.rs (exports_open_a_new_tab_with_the_converted_text). -->

## Roam, Node and Dailies

**Org → Roam** and **Org → Node** turn a folder of `.org` files into linked
notes: a node is a file (or, with Node, a headline) carrying an `:ID:`, and
nodes link with `[[id:…][Title]]`. Find, insert and capture nodes, see
backlinks, build a Mermaid graph, and keep one daily note per day under
`daily/` with **Org → Roam → Dailies** (**Today**, **Capture Today…**, **Go to
Date…**, **Calendar…**). The full reference is [Org-Roam](../roam/index.md).

<!-- Sources: crates/vix-org/spec/index.md (Roam, Node);
crates/vix-menu/src/lib.rs (ORG_ROAM, ORG_ROAM_DAILIES, ORG_NODE);
docs/roam/index.md. -->

## Contacts

A contact is a headline whose `:PROPERTIES:` drawer holds `EMAIL`, `PHONE`,
`ADDRESS`, `BIRTHDAY`, `NICKNAME` or `NOTE`. Add one with **Org → Capture →
Contact…**. **Org → Contacts**:

| Item | Effect |
| --- | --- |
| **Find Contacts** | A name, email and phone table of every contact in the project's `.org` files. |
| **Complete Link…** | Complete a `[[mailto:` or `[[contact:` link at the cursor. |
| **Insert Field ▸** | Insert an Email, Phone, Address, Birthday, Nickname or Note property line into the current entry. |
| **Birthdays** | Contacts with a `BIRTHDAY`, sorted by date. |
| **Export to vCard** | Every contact as vCard 3.0 in a new tab. |

In an `.org` file, type `[[mailto:` or `[[contact:` and press **Tab** right
away: a popup offers `Name <email>` (completing to `[[mailto:email][Name]]`)
or names (completing to `[[contact:Name]]`). Typing anything first, or using
Complete Link… elsewhere, says *"Type [[mailto: or [[contact: first, then
complete the link"*.

<!-- Sources: crates/vix-org/spec/index.md (Contacts, Link completion);
crates/vix-org-contacts/spec/index.md; crates/vix-menu/src/lib.rs
(ORG_CONTACTS, ORG_CONTACTS_FIELD); crates/vix-i18n/locales/status.yml
(status.org_contacts_link_no_context, org_contacts_no_matches);
tui/src/app.rs (org_contacts_new_field_and_views,
org_contact_link_completes_email_and_name_on_tab); tui/src/app/org/tests.rs
(contact_link_completion_only_triggers_after_the_marker_in_org_files). -->

## Emacs chords

Under the Emacs keymap, `C-c` starts an Org chord and a which-key popup lists
the next keys; `C-c C-x` is a second prefix with its own popup. `C-u` before
`C-c` is the universal argument.

| Chord | Command |
| --- | --- |
| `C-c C-t` / `C-u C-c C-t` | Cycle TODO / Mark Done with Note… |
| `C-c C-c` / `C-u C-c C-c` | Refresh Context / Recalculate (in a table) |
| `C-c C-s` / `C-c C-d` | Schedule Item… / Deadline… |
| `C-c .` / `C-c !` | Insert Timestamp / Insert Inactive Timestamp |
| `C-c C-q` | Set Tags… |
| `C-c C-w` | Refile Subtree… |
| `C-c /` | Sparse Tree Match… |
| `C-c a` | Weekly/Daily Agenda |
| `C-c l` / `C-c C-l` / `C-c C-o` | Store Link to Here / Insert Link… / Follow Link |
| `C-c '` | Edit Source Block |
| `C-c \|` / `C-c -` / `C-c RET` | Create Table from Selection / Insert Horizontal Line / Insert Line Below and Move |
| `C-c +` / `C-c ^` | Sum Column / Sort Table… |
| `C-c C-x f` | Footnote New/Jump |
| `C-c C-x a` / `C-c C-x C-s` | Toggle ARCHIVE Tag / Archive Subtree to File |
| `C-c C-x <` / `C-c C-x >` | Set / Remove Restriction Lock |
| `C-c C-x C-i` / `C-c C-x C-o` | Clock In / Clock Out |
| `C-c C-x C-w` / `C-c C-x C-y` | Cut / Paste Subtree (in a table: Cut / Paste Rectangle) |
| `C-c C-x M-w` | Copy Rectangle (in a table) |
| `C-c C-x C-c` | Column View |
| `C-c C-x C-u` | Update Dynamic Block at Point |

Some terminals send `C-i` as Tab; use the menu for Clock In there.

<!-- Sources: docs/reference/keybindings-emacs.md (C-c and C-c C-x tables);
crates/vix-menu/src/lib.rs (shortcut columns of ORG, ORG_TABLE,
ORG_TABLE_RECTANGLE, ORG_DATES, ORG_LINKS, ORG_AGENDA, ORG_ARCHIVE);
crates/vix-org/spec/index.md (Emacs chords); crates/vix-org-table/spec/index.md
(Regions: C-c C-x C-w/C-y shadow subtree cut/paste inside a table). -->

## Specifications

- [`crates/vix-org/spec/index.md`](../../crates/vix-org/spec/index.md):
  headlines, agenda, column view, export, Roam, Node, contacts.
- [`crates/vix-org-table/spec/index.md`](../../crates/vix-org-table/spec/index.md):
  tables and formulas, including what Org has that Vix does not.
- [`crates/vix-org-capture/spec/index.md`](../../crates/vix-org-capture/spec/index.md):
  capture templates.
- [`crates/vix-column-view/spec/index.md`](../../crates/vix-column-view/spec/index.md):
  the column view overlay.

---

Vix™ and Vix IDE™ are trademarks.
