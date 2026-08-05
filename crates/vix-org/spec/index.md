# Org

A pragmatic subset of [Org mode](https://orgmode.org/) for editing `.org`-style
outlines. The logic lives in the pure `crate::org` module (unit-tested); the
top-level **Org** menu wires it to the active buffer at the cursor line.

This is intentionally *not* a complete Org implementation — it covers the basics:
headline structure, TODO/checkbox toggling, folding, and lightweight export.

## Concepts

- **Headline**: a line of one or more leading `*` followed by a space
  (`* Top`, `** Child`). The star count is the level.
- **Subtree**: a headline plus all following lines up to the next headline of the
  same or higher level.
- **Drawer**: a `:NAME:` header line (e.g. `:PROPERTIES:`, `:LOGBOOK:`) through
  a matching `:END:` line. The lines between hold the drawer's contents (property
  lines such as `:foo: 123`).

## Drawer folding

With the cursor on a drawer header line — one that starts and ends with a colon,
like `:PROPERTIES:` — pressing **Tab** folds the drawer, hiding its body (through
`:END:`) the way code folding hides a block. The header stays visible with a
trailing `...` to signal the hidden content; pressing **Tab** again on the header
unfolds it. Folding is view-only — it never edits the buffer text. On any other
line Tab indents as usual. The foldable range is computed by `org::drawer_range`
(unit-tested); the editor's `toggle_manual_fold` performs the fold.

```
* Name              * Name
:properties:   Tab  :properties:...
:foo: 123     ───▶
:end:
```

## Menu

The **Org** menu (`Alt+O`):

| Item | Action | Effect |
| ---- | ------ | ------ |
| Capture → Anything… | `org.capture` | Run the built-in `"a"` template: prompt for a task, insert `* TODO <task>` at the cursor. |
| Capture → Babel… | `org.capture.babel` | Run the built-in `"b"` template: prompt for a language, open a multiline review buffer around a `#+begin_src <language>` / `#+end_src` block (Alt+Enter = newline). |
| Capture → Contact… | `org.capture.contact` | Run the built-in `"c"` template: prompt for Name/Email/Phone/Address/Birthday, insert an org-contacts entry (moved here from Org → Contacts). |
| Capture → Note… | `org.capture.note` | Run the built-in `"n"` template: prompt for note text, insert `* <note>` plus a `%U` creation timestamp at the cursor. |
| Capture → Task… | `org.capture.task` | Run the built-in `"t"` template: open a multiline review buffer pre-filled with `* TODO ` (Alt+Enter = newline), insert its (possibly edited) text verbatim at the cursor. |
| Capture → Choose Template… | `org.capture.select` | Open a chooser listing every configured template; Enter runs the highlighted one (see below). |
| Show/Hide → Cycle Visibility (Fold) | `org.cycle_visibility` | Fold/unfold at the cursor (reuses the editor fold toggle). |
| Show/Hide → Overview (Fold All) | `editor.fold_all` | Fold every foldable range (the editor's fold-all). |
| Show/Hide → Show All | `editor.unfold_all` | Unfold everything (also clears a sparse tree). |
| Show/Hide → Sparse TODO Tree | `org.sparse.todo` | Fold every subtree containing no `TODO` headline down to its headline (Org `C-c / t`). Single-line subtrees keep their headline visible — the fold engine hides bodies, not headers. |
| Show/Hide → Sparse Tree Match… | `org.sparse.match` | Prompt for text; fold every subtree not containing it, case-insensitive (Org's occur view). |
| New Heading | `org.new_heading` | Insert a sibling headline below the cursor line (same level as the governing headline; level 1 outside any subtree), cursor after the stars. |
| Navigate Headings → Up to Parent | `org.nav.up` | From a body, the governing headline; from a headline, its parent. |
| Navigate Headings → Previous/Next Heading | `org.nav.previous` / `org.nav.next` | The adjacent headline at any level. |
| Navigate Headings → Previous/Next Same Level | `org.nav.backward` / `org.nav.forward` | The adjacent sibling headline, stopping at the parent boundary. |
| Edit Structure → Promote | `org.promote` | Remove one `*` from every headline in the subtree (refused at level 1). |
| Edit Structure → Demote | `org.demote` | Add one `*` to every headline in the subtree. |
| Edit Structure → Move Subtree Up | `org.move_up` | Swap the subtree with the previous sibling. |
| Edit Structure → Move Subtree Down | `org.move_down` | Swap the subtree with the next sibling. |
| Edit Structure → Copy/Cut Subtree | `org.subtree.copy` / `org.subtree.cut` | Copy the subtree governing the cursor to the system clipboard (cut also removes it). |
| Edit Structure → Paste Subtree | `org.subtree.paste` | Paste the clipboard as a sibling of the subtree at the cursor, releveled to match; refused when the clipboard does not start with a headline. |
| Edit Structure → Sort Children | `org.sort_children` | Sort the subtree's direct children alphabetically (top-level trees when outside any subtree). |
| Edit Structure → Refile Subtree… | `org.refile` | Open a chooser listing every headline outside the subtree being moved (indented by level; ↑↓/click + Enter); the subtree moves under the chosen target, releveled one deeper. |
| Editing → Emphasis → * | `org.emphasis.*` | Wrap the selection in the Org marker pair: bold `*`, italic `/`, underline `_`, code `~`, verbatim `=`, strikethrough `+` (reuses the surround toggle). |
| Editing → Insert Block → * | `org.block.*` | Insert an empty `#+begin_<kind>`/`#+end_<kind>` block: src, example, quote, center, verse, comment. |
| Editing → Edit Source Block | `org.edit_src` | Org `C-c '`: with the cursor in a `#+begin_src` block, open its body in a dedicated tab; the same action there writes the edited body back into the block (re-validating the fence line) and closes the tab. Switching to a file-backed tab first abandons the pending edit. |
| Editing → Footnote New/Jump | `org.footnote` | Org `C-c C-x f`, all three behaviors: on a `[fn:x]` reference jump to its definition; on a definition line jump back to the first reference; elsewhere insert the next numbered reference and append its definition under a `* Footnotes` headline (created when missing). |
| Archive → Archive Subtree to File | `org.archive.subtree` | Move the subtree into the sibling `<file>_archive` file (created/appended), promoted to level 1 and stamped with an `:ARCHIVE_TIME:` property. Needs a saved file. |
| Archive → Toggle ARCHIVE Tag | `org.archive.tag` | Add/remove the `:ARCHIVE:` tag on the governing headline. |
| Hyperlinks → Store Link to Here | `org.link.store` | Remember a `[[file:rel::line][rel:line]]` link to the cursor; it seeds the next Insert Link… prompt. |
| Hyperlinks → Insert Link… | `org.link.insert` | Prompt for target then description, insert `[[target][description]]` (bare `[[target]]` when the description is empty). |
| Hyperlinks → Follow Link | `org.link.follow` | Open `file:` links in the editor (honoring a `::line` suffix), copy `http(s):`/`mailto:` URLs to the clipboard (a TUI has no browser), resolve `id:` links by scanning project `.org` files for the matching `:ID:` property (opens that file at its headline), and jump to the matching headline for internal `*Headline` targets. |
| Hyperlinks → Next/Previous Link | `org.link.next` / `org.link.prev` | Move the cursor to the adjacent `[[…]]` link in the buffer. |
| Cycle TODO | `org.cycle_todo` | Cycle the headline keyword: none → `TODO` → `DONE` → none. |
| Priority Up | `org.priority.up` | Move the headline's `[#X]` priority cookie one step toward `org_priority_highest` (setting no cookie yet → `org_priority_default`; clamped at `org_priority_highest`, no wraparound). |
| Priority Down | `org.priority.down` | Same, toward `org_priority_lowest`. |
| Mark Done with Note… | `org.close_note` | Prompt for a closing note (multiline; Alt+Enter = newline), then mark the headline `DONE`, stamp `CLOSED: [now]` under it, and log the note into its `:LOGBOOK:` drawer. |
| Toggle Checkbox | `org.toggle_checkbox` | Toggle a list item's `[ ]` ⇄ `[x]`. |
| Update Statistics | `org.update_statistics` | Recompute every checkbox parent state and `[/]`/`[%]` cookie in the buffer. |
| Tags & Properties → Set Tags… | `org.set_tags` | Prompt (seeded with the current tags) and replace the governing headline's trailing `:tag:tag:` group; empty input clears them. |
| Tags & Properties → Set Property… | `org.set_property` | Prompt for `NAME VALUE`; create or update the `:NAME:` line in the headline's `:PROPERTIES:` drawer (created after any planning line when missing). |
| Tags & Properties → Column View | `org.column_view` | Org `C-c C-x C-c`, read-only flavor: tabulate every headline (ITEM indented by level, TODO, PRIORITY, TAGS) as an Org table in a new tab. |
| Dates & Scheduling → Insert Timestamp | `org.timestamp` / `org.timestamp_inactive` | Insert `<today Dow>` (active) or `[today Dow]` (inactive) at the cursor. |
| Dates & Scheduling → Schedule Item… / Deadline… | `org.schedule` / `org.deadline` | Prompt for a `YYYY-MM-DD` date (seeded with today) and set the headline's `SCHEDULED:`/`DEADLINE:` entry — replacing it on an existing planning line, appending there, or inserting a planning line after the headline. |
| Dates & Scheduling → Date 1 Day Later/Earlier | `org.date_up` / `org.date_down` | Shift the date under the cursor by ±1 day, rewriting its weekday. |
| Clock In | `org.clock_in` | Insert an open `CLOCK: [now]` entry at the cursor (local time). |
| Clock Out | `org.clock_out` | Close the most recent open `CLOCK:` entry with the end time and `=> H:MM` duration. |
| Agenda → * | (submenu) | The built-in agenda views (see below). |
| Time Tracker | `org.time_report` | Sum each headline's `CLOCK:` durations in the active buffer into a time-report table. |
| Export → Markdown | `org.export_markdown` | Convert the buffer to Markdown in a new tab. |
| Export → HTML | `org.export_html` | Convert the buffer to a standalone HTML document in a new tab. |
| Export → LaTeX | `org.export_latex` | Convert the buffer to a standalone LaTeX article in a new tab (sections, itemize lists, verbatim blocks, `\href` links, escaping). |
| Export → iCalendar | `org.export_ics` | Export the buffer's `SCHEDULED:`/`DEADLINE:` entries as RFC 5545 all-day events in a new tab. |
| Refresh Context (C-c C-c) | `org.ctrl_c_ctrl_c` | The context action: toggle the checkbox on a list item, else recompute statistics cookies (also bound to the Emacs-keymap `C-c C-c` chord). |

### Agenda scope

Every agenda view scans a file set decided by, in priority order:

1. **Restriction lock** (Org → Agenda → Set Restriction Lock, Emacs
   `C-c C-x <`): when set, only the locked file. Session-only state; Remove
   Restriction Lock (`C-c C-x >` in Emacs) clears it. Locking also covers
   Emacs's "Special views current file": lock, then open any agenda view.
2. **File list** (Org → Agenda → File List ▸): the `org_agenda_files`
   setting (workspace-relative paths, persisted; `vix-settings` spec). Add
   Current File / Remove Current File edit it; Clear List empties it.
3. **Default**: every `.org` file in the project index.

**Show Scope** reports the active scope in the status bar. Files in the list
that do not exist are skipped when the agenda is built.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Agenda → Set Restriction Lock (This File) | `org.agenda.lock` | Lock agenda views to the active file (needs a saved file). |
| Agenda → Remove Restriction Lock | `org.agenda.unlock` | Clear the lock. |
| Agenda → File List → Add/Remove Current File | `org.agenda.file_add` / `org.agenda.file_remove` | Edit the persisted `org_agenda_files` list. |
| Agenda → File List → Clear List | `org.agenda.file_clear` | Empty the list (back to all project files). |
| Agenda → File List → Show Scope | `org.agenda.file_list` | Status-bar report of lock / list / default scope. |

Agenda and Time Tracker output open in a new buffer. The pure builders
(`org::agenda`, `org::time_report`) are unit tested; `CLOCK:` durations are read
from the `=> H:MM` totals Org writes.

Structure commands operate on the headline/line under the cursor; the cursor
follows a moved subtree. When a command does not apply (e.g. the cursor is not on
a headline, or there is no sibling to swap with), the status bar says so.

### Capture

The **Org → Capture** submenu is a template-driven system, in the shape of
Emacs `org-capture`: named templates (the `org_capture_templates` setting,
`vix-settings` spec) with `%^{Prompt}`-style placeholders, wrapped as a
headline/item/checkbox/table-row, and filed at a target — the cursor, a node
by `:ID:`, a file, a headline within a file, or a date tree. The pure logic
(placeholder extraction/expansion, entry wrapping, target parsing, and all
five insertion shapes) lives in the unit-tested `vix-org-capture` crate;
`App` drives the interactive parts (prompting, the clipboard, the active
buffer) and file I/O. Full design: [`spec/org/capture/index.md`](../../../spec/org/capture/index.md).

Five built-in templates are seeded by default — `"a"` Anything, `"t"` Task,
`"b"` Babel, `"n"` Note, `"c"` Contact — the fixed menu items above run them
by key (`org.capture`/`org.capture.task`/`org.capture.babel`/
`org.capture.note`/`org.capture.contact`), so nothing regresses if you never
touch `org_capture_templates`. **Choose Template…** opens every configured
template (built-in and custom) in a chooser.

Placeholders: `%^{Label}` (and `%^{Label|choices}`, prompted in order,
wizard-style — the choice list's first entry pre-fills the prompt),
`%t`/`%T`/`%u`/`%U` (active/inactive timestamp, date/date-time),
`%<strftime>`, `%a` (a link back to where capture was invoked), `%i` (the
active selection), `%f`/`%F` (active file name/path), `%c` (clipboard), `%^g`/
`%^G` (a trailing tag prompt), `%?` (where the cursor lands after filing),
`%%` (literal `%`).

Targets: `cursor` (default — today's only behavior for the three built-ins),
`id:<ID>` (files under the node/headline carrying that `:ID:`, reusing
Roam/Node's id system), `file:<path>`, `file+headline:<path>#<Headline>`
(creating the headline if missing), `file+datetree:<path>` (a growing
`Year > Month > Day` outline tree, distinct from Roam Dailies'
one-file-per-day). A template with no unanswered prompts (or one that skips
review via `immediate_finish`) files immediately; otherwise the expanded text
opens in a final multiline review buffer (Alt+Enter = newline) before filing.

Each `%^{}` field prompt shows a **live preview** of the whole template above
the input box, not just the isolated field — the same way Emacs's capture
buffer stays visible behind the minibuffer prompt. Already-answered fields
are substituted for real; the field about to be answered is marked
`‹Label›`; later fields still show `[Label]`; every other placeholder (`%t`,
`%a`, …) is expanded immediately since it needs no input. The preview scrolls
to keep the current field's line in view for templates with many fields (the
org-contacts template's 17 is the extreme case). Built by
`vix_org_capture::preview` (pure, unit-tested) and `App::capture_preview`;
rendered by `draw_prompt_preview` (`src/ui.rs`).

### Priority

A priority cookie `[#X]` sits right after a headline's TODO/DONE keyword (or
right after the stars, if it has none), e.g. `* TODO [#0] Call a friend`.
Three settings control the range and default (`vix-settings` spec):
`org_priority_highest`/`org_priority_lowest` (which character sorts as most/
least important — Vix defaults to numeric `'0'`..`'9'`, unlike Emacs's
default `'A'`..`'C'`) and `org_priority_default` (given to a headline that
had no cookie yet, defaulting to `'0'`). **Priority Up**/**Down** step the
cursor's headline one character toward `org_priority_highest`/`_lowest`,
clamping at the bound rather than wrapping around or removing the cookie.
The pure logic (`org::priority`, `org::set_priority`, `org::priority_up`,
`org::priority_down`) is unit-tested for both the numeric scheme and the
classic letter scheme.

A capture template can prompt for a priority using the multi-choice
placeholder form (`spec/org/capture/index.md`):
`[#%^{Priority|0|0|1|2|3|4|5|6|7|8|9}]` — the prompt pre-fills with `0` (the
first choice); the full candidate list is parsed but not yet offered as a
select popup.

### Emacs chords

Under the **Emacs** keymap, the familiar Org `C-c` chords are wired to these
commands (discoverable via the which-key popup after `C-c`):

| Chord | Action | Effect |
| ----- | ------ | ------ |
| `C-c C-t` | `org.cycle_todo` | Cycle the headline's TODO keyword. |
| `C-u C-c C-t` | `org.close_note` | Mark the headline `DONE` and record a closing note + `CLOSED:` timestamp (the universal-argument variant). |
| `C-c C-c` | `org.ctrl_c_ctrl_c` | Context action: toggle the checkbox on the cursor line, else recompute statistics cookies. |
| `C-c C-s` / `C-c C-d` | `org.schedule` / `org.deadline` | Schedule / deadline prompt. |
| `C-c C-w` | `org.refile` | Refile-target chooser. |
| `C-c C-q` | `org.set_tags` | Set Tags prompt. |
| `C-c C-l` / `C-c C-o` / `C-c l` | `org.link.insert` / `.follow` / `.store` | Link commands. |
| `C-c .` / `C-c !` | `org.timestamp` / `org.timestamp_inactive` | Insert a timestamp. |
| `C-c '` | `org.edit_src` | Edit source block in a dedicated tab (and apply back). |
| `C-c /` | `org.sparse.match` | Sparse-tree match prompt. |
| `C-c a` | `org.agenda` | Weekly/daily agenda. |
| `C-c C-x f` | `org.footnote` | Footnote new/jump. |
| `C-c C-x a` / `C-c C-x C-s` | `org.archive.tag` / `org.archive.subtree` | Archive commands. |
| `C-c C-x <` / `C-c C-x >` | `org.agenda.lock` / `org.agenda.unlock` | Agenda restriction lock. |
| `C-c C-x C-i` / `C-c C-x C-o` | `org.clock_in` / `org.clock_out` | Clocking (`C-i` arrives as Tab in some terminals; use the menu there). |
| `C-c C-x C-w` / `C-c C-x C-y` | `org.subtree.cut` / `org.subtree.paste` | Subtree kill/yank. |
| `C-c C-x C-c` | `org.column_view` | Column view. |

`C-u` is the Emacs universal argument; it applies to the next command and is
cancelled by any key other than the `C-c` prefix. `C-c C-x` is a third-key
prefix with its own which-key popup. These chords live only in the Emacs
keymap; the Org menu displays them in its shortcut column as the reference.

### Agenda views

The **Org → Agenda** submenu offers the built-in views from the Org manual's
[Agenda Views](https://orgmode.org/manual/Agenda-Views.html), each compiled from
every project `.org` file (reindexed first) into a **read-only, interactive**
buffer:

| Item | Action | Org key | Builder | Shows |
| ---- | ------ | ------- | ------- | ----- |
| Weekly/Daily Agenda | `org.agenda` | `a` | `org::agenda_items` | `DEADLINE:`/`SCHEDULED:` items grouped by date, plus unscheduled `TODO`s. |
| Global TODO List | `org.agenda.todo` | `t` | `org::todo_list` | Every not-`DONE` `TODO` headline. |
| Match Tags/Property… | `org.agenda.match` | `m` | `org::tags_match` | Headlines whose trailing `:tags:` satisfy a query (`+tag`, `-tag`, bare `tag`; case-insensitive — a pragmatic subset of Org's match syntax). |
| Search… | `org.agenda.search` | `s` | `org::search` | Headlines whose entry body contains **all** the given keywords. |
| Stuck Projects | `org.agenda.stuck` | `#` | `org::stuck_projects` | Not-`DONE` headlines that have children but no not-`DONE` child (no next action). |

All views are **interactive**: pressing `t` on a task line cycles that task's
TODO state (`org::cycle_todo`) directly in its **source `.org` file on disk**,
reloads any open, clean buffer for that file, and rebuilds the *same* view in
place (keeping the cursor line) — mirroring Emacs `org-agenda-todo`. The pure
builders return `Vec<AgendaItem>` (each carrying its source line); `render_agenda`
/ `render_list` turn those into the buffer text plus a line→item map so the host
can act on the line under the cursor.

Marking a task `DONE` with a note (`C-u C-c C-t`) uses `org::close_headline`,
which forces the keyword to `DONE`, inserts (or refreshes) a `CLOSED: [now]`
planning line, and logs the note into a `:LOGBOOK:` drawer as
`- Note taken on [now] \\` + the indented body.

### Checkbox & statistics cookies

A checkbox list item with sub-items reflects their state: all children checked →
`[X]`, none → `[ ]`, otherwise → `[-]` (partial). A *statistics cookie* —
`[/]`/`[n/m]` (fraction) or `[%]`/`[n%]` (percent) — written anywhere in a
headline or parent list item counts its children:

```
* Organize Party [33%]
** TODO Call people [1/2]
*** TODO Peter
*** DONE Sarah
** TODO Buy food
** DONE Talk to neighbor
```

A headline cookie counts child checkboxes if its body has top-level checkboxes,
otherwise direct child TODO headlines. The `:COOKIE_DATA:` property resolves the
ambiguity (`checkbox` or `todo`); adding `recursive` counts TODO entries in the
whole subtree, not just direct children. Cookies and parent checkboxes are
recomputed automatically after **Toggle Checkbox** / **Cycle TODO**, and on
demand via **Update Statistics**. The pure builder is `org::update_statistics`.

## Roam

The **Org → Roam** submenu brings [Org-roam](https://www.orgroam.com/)-style
networked, Zettelkasten note-taking to a directory of `.org` files. A **node** is
an `.org` file with an `:ID:` property and a `#+title:`; nodes link to one another
with `[[id:<id>][Title]]` links, forming a graph. The pure logic lives in the
unit-tested `crate::roam` module; the host wires it to prompts and the filesystem.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Find Node… | `roam.node_find` | Prompt for a title; open the matching node, or create `<slug>.org` (with a fresh `:ID:`) and open it. |
| Insert Node Link… | `roam.node_insert` | Prompt for a title; insert an `[[id:…][Title]]` link at the cursor, creating the node file (without leaving the current buffer) if new. |
| Random Node | `roam.node_random` | Jump to a randomly chosen node. |
| Capture Node… | `roam.capture` | Prompt for a title and create/open a new node. |
| Backlinks | `roam.backlinks` | Compile a buffer of *linked* references (files linking to the active node's `:ID:`) and *unlinked* references (files mentioning its title). |
| Dailies → Today | `roam.dailies_today` | Open (creating if needed) today's daily note `daily/YYYY-MM-DD.org`. |
| Dailies → Capture Today… | `roam.dailies_capture` | Append a `* HH:MM …` entry to today's daily note. |
| Dailies → Go to Date… | `roam.dailies_date` | Prompt for a `YYYY-MM-DD` date and open that daily note. |
| Metadata → Add Tag… | `roam.tag_add` | Add a tag to the node's `#+filetags:` line. |
| Metadata → Add Alias… | `roam.alias_add` | Append a quoted alias to the `:ROAM_ALIASES:` property. |
| Metadata → Add Ref… | `roam.ref_add` | Append a URL / cite key to the `:ROAM_REFS:` property. |
| Graph | `roam.graph` | Build a Mermaid `flowchart` of all nodes and `[[id:…]]` links into a new buffer. |
| Sync Database | `roam.db_sync` | Re-index the project and open a sortable table of every node (title, file, tags). |

Nodes live in the project root; daily notes live in `daily/`. There is no
persistent database — *Sync Database* simply re-scans the project's `.org` files,
matching org-roam's `org-roam-db-sync` semantics in a stateless way.

## Node

The **Org → Node** submenu brings [org-node](https://github.com/meedstrom/org-node)
functionality — a fast, ID-based take on networked notes where a **node** is
either a whole file *or* any subtree carrying an `:ID:`. It shares the on-disk
format with Roam (`:ID:`, `:ROAM_ALIASES:`, `:ROAM_REFS:`, `[[id:…]]` links), so
the two coexist. Find / Insert Link / Random / Backlinks reuse the shared node
machinery; the rest are org-node's distinctive operations.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Find Node… | `roam.node_find` | Open or create a node by title. |
| Insert Link… | `roam.node_insert` | Insert an `[[id:…]]` link to a node. |
| Insert Transclusion… | `node.insert_transclusion` | Insert a `#+transclude: [[id:…]]` directive for a node (created if new). |
| Random Node | `roam.node_random` | Jump to a random node. |
| Nodeify Entry | `node.nodeify` | Give the headline at the cursor an `:ID:`, making it a (subtree) node. |
| Extract Subtree to Node | `node.extract_subtree` | Cut the subtree at the cursor into its own file node, leaving an `[[id:…]]` link behind. |
| Backlinks | `roam.backlinks` | Show linked + unlinked references to the active node. |
| List Dead Links | `node.dead_links` | Report `[[id:…]]` links whose target ID is not declared by any node. |
| Rename File by Title | `node.rename_by_title` | Rename the active file to the slug of its `#+title:`. |
| Rebuild Cache | `node.reset` | Re-scan the project's nodes (the stateless `org-mem-reset` equivalent). |

*Nodeify* and *Extract Subtree* embody org-node's headline-as-node model;
extraction promotes the subtree's nested headlines so they sit at the top level
of the new file. Pure helpers (`roam::nodeify`, `roam::dead_links`,
`roam::transclusion`, `roam::all_ids`) are unit-tested.

## Contacts

The **Org → Contacts** submenu brings
[org-contacts](https://github.com/doomelpa/org-contacts)-style contact management
to Org files. A contact is a headline (its text is the name) whose `:PROPERTIES:`
drawer holds `EMAIL` / `PHONE` / `ADDRESS` / `BIRTHDAY` / `NICKNAME` / `NOTE`.

New Contact… moved to **Org → Capture → Contact…** (`org.capture.contact`,
see above) once capture became template-driven.

| Item | Action | Effect |
| ---- | ------ | ------ |
| Find Contacts | `org.contacts.find` | Compile a name/email/phone table of every contact in the project's `.org` files. |
| Complete Link… | `org.contacts.link_complete` | Autocomplete a `[[mailto:`/`[[contact:` link at the cursor (see below). |
| Insert Field → Email/Phone/Address/Birthday/Nickname/Note | `org.contacts.field.*` | Insert a `:KEY:` property line into the current entry's drawer. |
| Birthdays | `org.contacts.birthdays` | List contacts that have a `BIRTHDAY`, sorted by date. |
| Export to vCard | `org.contacts.vcard` | Convert all contacts to a vCard 3.0 buffer. |

Pure logic (parse, directory, birthdays, vCard) lives in the unit-tested
`crate::org_contacts` module.

### Link completion

Typing `[[mailto:` or `[[contact:` anywhere (a TODO item, a note, …) and then
pressing **Tab** or **Alt+Tab** opens a completion popup sourced from every
contact in the project's `.org` files:

- `[[mailto:` → email addresses, shown as `Name <email>`; accepting one
  completes the link to `[[mailto:email][Name]]`. Contacts without an
  `EMAIL` field are not offered.
- `[[contact:` → contact names; accepting one completes the link to
  `[[contact:Name]]`.

The trigger only fires immediately after typing the marker (no chars typed
in between), mirroring the Org-roam `[[` wiki-link completion — start typing
past the marker and the popup no longer applies (any other key dismisses
it). The same lookup is reachable from the command palette / **Complete
Link…** menu item, which requires the cursor to already sit right after one
of the two markers. `crate::org_contacts` has no notion of `contact:` as an
Org link *type* (Vix does not register a link handler for it — it is purely
this completion's insertion target); `mailto:` is the standard Org link
scheme.

## Insertion

Org *content* insertion (snippets, inline markers, blocks) lives under
**Tools → Insert → Org / Markers / Begin-End** — see
[`crates/vix-org/spec/insert-org.md`](../tools/insert/org.md).

## Export mapping (pragmatic)

- Headlines → `#`×level (Markdown) / `<h1..6>` (HTML); `#+title:` → top heading.
- Inline: `*bold*`, `/italic/`, `_underline_`, `+strike+`, `~code~`, `=verbatim=`,
  and `[[url][desc]]` / `[[url]]` links.
- Bullet lists → Markdown `-` / HTML `<ul><li>`. Block delimiters (`#+BEGIN_…`)
  are dropped, their inner text kept.
