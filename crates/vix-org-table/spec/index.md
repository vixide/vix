# Org Table

Org-mode pipe-table editing: parsing, alignment, structural edits, and TBLFM
formulas — a pragmatic subset of
[Org's built-in table editor](https://orgmode.org/manual/Tables.html)
for pipe (`|`) tables: recognition, alignment, field/row/column navigation and
structural edits, rectangle copy/cut/paste, sorting, and a `#+TBLFM:` formula
layer. The logic lives in the pure `vix-org-table` crate (unit-tested); it is
wired to the active buffer and cursor by `App::org_table_key` /
`App::org_table_action` in `src/app.rs`, and exposed as the **Org → Table**
menu (`vix-menu`'s `ORG_TABLE`/`ORG_TABLE_RECTANGLE`). See "## Actions" below
for the full key/action-id map, including what the manual documents but this
crate/app does not implement.

This is intentionally *not* a complete implementation of Org's table editor.
Out of scope: Calc-mode formulas and unit conversions, Lisp formulas
(`'(...)`), named field/column references (`$name`), duration-aware
arithmetic beyond the copy-down increment case below, `remote()` cross-table
references, recalculation-on-every-keystroke (recalculation only happens when
explicitly requested), and Unicode grapheme-cluster-aware column widths
(widths are counted in `char`s, so combining marks and wide CJK characters are
not accounted for). A literal `|` inside a field must be pre-escaped by the
user as `\vert{}` — cell splitting is naive on every unescaped `|`, matching
Org's own default behavior.

## Concepts

- **Table**: a maximal run of contiguous lines whose first non-whitespace
  character is `|`.
- **Row**: one line of a table — either a horizontal rule (**hline**, a line
  starting `|-` after trimming) or a **data row** of `|`-separated cells.
- **Field**: one cell of a data row. Structural functions address a field
  either by a **byte offset** into its line (the convention used by
  navigation/edit functions, matching cursor position) or by a **field
  index** (a 0-indexed column number, used only by the rectangle functions —
  documented per-function since the two conventions differ).
- **Header row**: when a table's first row is a data row immediately followed
  by an hline, that first row is treated as a header — excluded from the
  numeric-alignment heuristic (below) but still numbered like any other data
  row for `#+TBLFM:` purposes.
- **Column formula** vs. **field formula**: a `#+TBLFM:` directive targeting
  `$COL` applies to every data row's field in that column; one targeting
  `@ROW$COL` applies to a single field.

## Alignment

`align` re-renders a table: each column's width is the widest cell in that
column (header included), with a one-space margin on each side. A column
right-aligns ("numeric") when a majority of its non-empty data cells (the
header excluded) parse as a number; otherwise it left-aligns. This matches
the manual's own worked example exactly:

```
| Name | Phone | Age |     | Name  | Phone | Age |
|-             align   |-------+-------+-----|
| Peter | 1234 | 17 |  ──▶  | Peter |  1234 |  17 |
| Anna | 4321 | 25 |        | Anna  |  4321 |  25 |
```

`Name`/`Phone` are text columns (left-aligned); `Age`/`Phone`'s numbers make
those columns numeric (right-aligned) — `Name` stays text since names don't
parse as numbers.

A table with **no data rows at all** — one or more bare `|-` lines and nothing
else — has no columns to size. It renders as a minimum-width rule per line
(`|---|`), the way Emacs squares up a lone `|-`. It must not render as nothing:
every caller *replaces* the table's lines with what `render` returns, so an empty
string would delete the line the user was aligning. (It did; `cargo fuzz run
org_table` caught it with a render-then-parse round-trip assertion, and
`render_keeps_hline_only_tables` guards it now.)

## Field navigation

`next_field`/`previous_field` (Tab/Shift-Tab) realign, then step to the
adjacent field, wrapping across rows and skipping over hlines to the next/
previous data row. Stepping past the last field of the last row appends a
fresh empty data row (same column count) and lands on its first field;
stepping before the first field of the first row is a no-op. `next_row` (RET)
moves down one row in the same column, creating a new row if already on the
last one. `blank_field` clears the field under the cursor.

```
| a | b |      Tab (from a)     | a | b |
| c | d |     ─────────────▶    | c | d |
                                    ^ cursor now on b
```

## Row & column structural edits

`insert_row`/`kill_row` add or remove a whole row (data or hline);
`move_row_up`/`move_row_down` swap a row with its neighbor. `insert_hline`
inserts a horizontal rule; `hline_and_move` (`C-c RET`) inserts one below the
current row and drops a fresh empty row below that, landing there.
`insert_column`/`delete_column` add or remove a column at the field under the
cursor, shifting everything to its right; `move_column_left`/
`move_column_right` swap a column with its neighbor.

```
| a | b |   insert_column at b   | a |   | b |
| c | d |  ─────────────────────▶ | c |   | d |
```

## Cell swap-moves (Shift-arrow)

`move_cell_up`/`move_cell_down`/`move_cell_left`/`move_cell_right` swap the
field under the cursor with its neighbor, matching Emacs's shift-arrow
behavior: vertical moves skip over hlines to the nearest data row in that
direction; horizontal moves stay within the row (hlines have no cells, so a
horizontal move on an hline is a no-op).

## Sort

```rust
pub fn sort_rows(text, first_line, last_line, column, kind: SortKind, reverse, case_sensitive) -> Option<String>
```

Sorts the data rows in `[first_line, last_line]` by their `column`-th field.
`SortKind::Alphabetic` compares text; `Numeric` parses cells as numbers
(unparsable cells sort last); `Time` parses cells permissively as an
Org/ISO-ish timestamp (`<YYYY-MM-DD [Www] [HH:MM]>`, brackets optional) or a
bare `H(H):MM` duration, falling back to alphabetic comparison on parse
failure. Hlines in the range keep their line position; only data-row
*contents* are reordered into the data-row slots, in source order — the
simplest correct behavior for the common cases (a whole table, or a
caller-narrowed run between two hlines).

## Rectangle copy/cut/paste

`copy_rectangle`/`cut_rectangle` take corners as `(line, field index)` and
return the block of cell text, row-major, skipping any hline rows in the
range ("the process ignores horizontal separator lines", per the manual).
`paste_rectangle` takes a landing position as `(line, byte offset)` — the
same convention as the navigation functions — and pastes with its upper-left
at that field, overwriting existing fields and extending the table with new
rows/columns if the rectangle does not fit.

## Misc

- `transpose` swaps rows and columns, dropping hlines ("Transpose the table
  at point and eliminate hlines").
- `sum_column` sums the parseable numeric cells of a column (data rows only);
  the caller formats/displays the result.
- `copy_down` (`org-table-copy-increment`-ish): an empty field is filled from
  the nearest non-empty field above it in the same column; a non-empty field
  is copied into the next row's same column (creating a row if needed) and
  point follows. With `increment: true`, the copied value's trailing (or,
  failing that, leading) run of digits increments by one, preserving
  zero-padded width (`"07"` → `"08"`); a bare `H(H):MM` duration has its
  minutes incremented by one, rolling into hours (`"1:59"` → `"2:00"`). Any
  other value shape copies verbatim.
- `from_delimited`/`from_delimited_with` convert delimited text (CSV/TSV/
  whitespace-aligned columns) into a pipe table, auto-detecting (or, for the
  `_with` form, forcing) the separator: comma if every non-blank line
  contains one, else tab if every line contains one, else runs of 2+
  whitespace characters. `Separator::Regex` is a literal-substring
  stand-in — this crate has no regex-engine dependency.
- `to_tsv` exports a table's data rows (hlines skipped) as tab-separated
  text, for `org-table-export`.

## `#+TBLFM:` formulas

A `#+TBLFM:` line immediately following a table (no blank line in between)
holds `::`-separated formulas, each `<target>=<expr>[;<format>]`:

- `@ROW$COL=<expr>` — a **field formula**: write `<expr>`'s value into that
  one field.
- `$COL=<expr>` — a **column formula**: write `<expr>`'s value into every
  data row's field in that column.

`ROW`/`COL` are 1-indexed; **row numbering counts data rows only — hlines are
not counted** (a deliberate simplification of Org's fuller
hline-crossing-reference semantics). Inside `<expr>`:

- `$N` refers to column `N` of the row currently being evaluated (meaningful
  in a column formula, or in a field formula evaluated "as" its own row).
- `@R$C` refers to an absolute field.
- `@R$C..@R2$C2` is a rectangular range — expands to its numeric values,
  row-major, skipping non-numeric cells — but **only** as an argument to
  `vsum`/`vmean`/`vmin`/`vmax`/`vcount`. Everything else (`+ - * / ( )` and
  the substituted numbers) is handed to the [`evalexpr`](https://docs.rs/evalexpr)
  crate.
- An optional `;%.Nf`-style suffix formats the result to `N` decimal places
  exactly; without one, integers render with no decimal point and
  non-integers render to 2 decimals with trailing zeros trimmed.

Formulas run in the order written; a column formula loops rows top-to-bottom,
re-reading any already-updated cells from earlier in the same pass — so
`$2=$1*2` on row 2 sees row 1's freshly computed `$2` if referenced (forward
references within one column formula see prior rows' output; true circular or
backward dependency resolution is out of scope). Any reference or evaluation
error for a formula writes a literal `#ERROR` into its target field(s)
instead of panicking or being skipped — `apply_tblfm`/`recalc` only return
`None` when there is no table, or no `#+TBLFM:` line, to operate on at all.

```
| 1 | 2 |   |
#+TBLFM: @1$3=$1+$2

           recalc
          ───────▶

| 1 | 2 | 3 |
#+TBLFM: @1$3=$1+$2
```

`recalc` is the one-step convenience: realign the table, then apply its
`#+TBLFM:` line if one immediately follows. Recalculation never happens
implicitly on every keystroke — only when `recalc`/`apply_tblfm` is called.

## Actions

Grouped per the Org manual's own "Built-in Table Editor" summary. The "Emacs
key" column is what's *actually* wired in `src/app.rs` (`App::org_table_key`
for the plain-key gate, `EMACS_CTRL_C`/`EMACS_CTRL_C_X` for the `C-c`-prefixed
chords) — verified against the source, not copied from the manual — so it can
differ from Org's own default binding where this app made a different choice.
Items with no app-level action id (field motion, proper) are handled directly
by `App::org_table_key` and have no `Org → Table` menu entry; everything else
has both an `org.table.*` action id and a menu leaf.

### Creation and conversion

| Feature | Emacs key | Action id | Function |
| ------- | --------- | --------- | -------- |
| Convert selection/region to table | `C-c \|` | `org.table.create_from_region` | `from_delimited` / `from_delimited_with` |
| Export to TSV | menu only (no key) | `org.table.export_tsv` | `to_tsv` |

### Re-aligning and field motion

Handled directly by `App::org_table_key`, gated on the cursor being inside a
pipe table; not exposed as `Org → Table` menu items (they aren't discrete
"commands" so much as how typing in a table behaves).

| Feature | Emacs key | Function |
| ------- | --------- | -------- |
| Realign, then next field (wraps rows; appends a row past the last field) | `Tab` | `next_field` |
| Realign, then previous field | `S-Tab` / `<backtab>` | `previous_field` |
| Next row (create if last) | `RET` | `next_row` |
| Move cell up/down/left/right (swap with neighbor, skipping hlines vertically) | `S-<up>` / `S-<down>` / `S-<left>` / `S-<right>` | `move_cell_up` / `move_cell_down` / `move_cell_left` / `move_cell_right` |

`copy_down` is in this manual section too, but — unlike the rest of this
group — it has its own action id and menu leaf (`Org → Table → Copy Field
from Above`), since it's a discrete edit rather than pure navigation; see
"Miscellaneous" below.

### Column, row, and cell editing

| Feature | Emacs key | Action id | Function |
| ------- | --------- | --------- | -------- |
| Insert row above | `M-S-<down>` | `org.table.insert_row_above` | `insert_row` |
| Insert row below | menu only (no key — Org's own `C-S-<down>`-ish prefix-arg case isn't wired) | `org.table.insert_row_below` | `insert_row` |
| Delete row | `M-S-<up>` | `org.table.kill_row` | `kill_row` |
| Move row up/down | `M-<up>` / `M-<down>` | `org.table.move_row_up` / `org.table.move_row_down` | `move_row_up` / `move_row_down` |
| Insert hline below | `C-c -` | `org.table.insert_hline` | `insert_hline` |
| Insert hline below and move | `C-c RET` | `org.table.hline_and_move` | `hline_and_move` |
| Insert column | `M-S-<right>` | `org.table.insert_column` | `insert_column` |
| Delete column | `M-S-<left>` | `org.table.delete_column` | `delete_column` |
| Move column left/right | `M-<left>` / `M-<right>` | `org.table.move_column_left` / `org.table.move_column_right` | `move_column_left` / `move_column_right` |

### Regions

| Feature | Emacs key | Action id | Function |
| ------- | --------- | --------- | -------- |
| Copy rectangle | `C-c C-x M-w` | `org.table.copy_rectangle` | `copy_rectangle` |
| Cut rectangle | `C-c C-x C-w` | `org.table.cut_rectangle` | `cut_rectangle` |
| Paste rectangle | `C-c C-x C-y` | `org.table.paste_rectangle` | `paste_rectangle` |

`C-c C-x C-w`/`C-c C-x C-y` shadow their usual subtree cut/paste meanings
(`org.subtree.cut`/`org.subtree.paste`) only while the cursor is inside a
pipe table — see `App::emacs_c_x_chord_key`.

### Calculations

| Feature | Emacs key | Action id | Function |
| ------- | --------- | --------- | -------- |
| Align (realign the table at point) | `C-c C-c` | `org.table.align` | `align` |
| Recalculate (realign + apply `#+TBLFM:` formulas) | `C-u C-c C-c` | `org.table.recalc` | `recalc` |
| Sum column | `C-c +` | `org.table.sum_column` | `sum_column` |
| Sort rows | `C-c ^` (prompts for column/kind) | `org.table.sort` | `sort_rows` |

Plain `C-c C-c` (no universal argument) on a table line runs
`App::org_ctrl_c_ctrl_c`, which also calls the same realign-and-recalc code
path as `org.table.recalc` before falling through to its other `C-c C-c`
meanings (checkbox toggle, statistics refresh) — so in practice `C-c C-c`
recalculates a table too, not just aligning it.

### Miscellaneous

| Feature | Emacs key | Action id | Function |
| ------- | --------- | --------- | -------- |
| Transpose (drops hlines) | menu only (no key) | `org.table.transpose` | `transpose` |
| Copy field from above (increment-aware) | `S-RET` | `org.table.copy_down` | `copy_down` |

### Not implemented

Documented by the Org manual but not present in this crate or its app-level
wiring — no function, action id, or menu entry exists for these:

- **Blank field** (`C-c C-c` on an empty selection / `org-table-blank-field`)
  — the crate has a `blank_field` function (used only by its own unit tests)
  but it is never called from `src/app.rs`, has no `org.table.*` action id,
  and has no menu entry.
- **Edit field in a dedicated window** (`C-c \``, `org-table-edit-field`).
- **Field-bounds motion** (`M-a`/`M-e`, `org-table-beginning-of-field` /
  `org-table-end-of-field`).
- **Wrap-region into a field** (`M-RET` in a table,
  `org-table-wrap-region`) — also a physical-key collision, since `M-RET`
  already means "insert sibling headline" (`org.new_heading`-adjacent) in
  this app's Org bindings.
- **Formula debugger toggle** (`C-c {`, `org-table-toggle-formula-debugger`)
  and **column/row number toggle** (`C-c }`,
  `org-table-toggle-coordinate-overlays`) — this crate always recalculates
  on demand rather than live, so there is no debug overlay to toggle.
- **Import from file** (`org-table-import`) and **table.el
  conversion** (`C-c ~`, `org-table-create-with-table.el`) — `export_tsv`
  (`org-table-export`) covers the opposite direction (table → TSV in a new
  tab) but there is no matching file-to-table import.
- **`org-table-header-line-mode`** (sticky header row while scrolling).
- **Org Plot** (`orgtbl-ascii-plot` / Org Plot's Gnuplot integration) — out
  of scope for this crate; already excluded by the "Out of scope" list
  above (Calc-mode formulas, remote references, etc. are Org's spreadsheet
  layer, of which plotting is a downstream consumer).
