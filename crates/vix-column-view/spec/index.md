# Column View

**Status:** Shipped. The interactive Column View overlay (Org `C-c C-x
C-c`): a live, write-through spreadsheet view onto the outline, driven by
the resolved `vix_org::ColumnsSpec` (`vix_org::resolve_columns_spec`).

Unlike `vix-edit-table`'s detached-grid-then-explicit-save model, column
view edits apply straight through to the real buffer text on every commit:
this matches Emacs's actual semantics (column view is a live overlay on the
*same* buffer, not a scratch copy — other Org commands see an edited
property value immediately) and this codebase's own dominant Org convention
("read whole buffer text → pure transform → splice back"). `ColumnView`
does not own the buffer text; the host (`App` in the root `vix` crate)
passes the active tab's current text into `ColumnView::handle_key` as `&mut
String` and splices any change back via `Tab::editor::set_content`
(persisting to *disk* still goes through the normal save/dirty flow,
unchanged).

## Deliberately unimplemented subset

A pragmatic subset of the Org manual's column-view keys, matching this
codebase's convention elsewhere:

- `SPC` (a transient cell "peek" distinct from `v`) is not modeled; `v`
  alone shows the full value, via `ColumnView::take_status`.
- `C-c C-o` (open the entry in another window) does not apply to a
  single-pane TUI overlay.
- Restricting `e`'s free-text edit to a column's allowed-value list is not
  enforced — `e` can type any value, matching this codebase's general
  "trust the user" convention for other free-text Org fields; `n`/`p`/
  digit-select are the constrained-cycle affordances instead.

## Contents (`vix_column_view`)

- `ColumnView` — the overlay state: the resolved columns/rows table (from
  `vix_org::build_column_table`), the cell cursor (`row`/`col`), row
  scroll, edit-mode buffer, and per-column display width. `open` resolves
  the spec for the buffer/line and builds the table; `handle_key` is the
  single entry point for input, splicing edits straight into the caller's
  `&mut String` buffer via `vix_org::apply_column_edit`/`set_property`;
  `take_status` drains a one-shot status line (e.g. a `v`-key full-value
  peek).
- `Outcome` — what the host does after a key: `Consumed` (nothing
  further), `Close` (`q`/Esc, or `C-c C-c` off a checkbox-shaped field), or
  `NeedsColumnPrompt` (`Shift+Alt+Right`: the host opens a text prompt for
  a property name, then calls `ColumnView::insert_column_before` with the
  answer). There's no `Save` variant — edits are already live in the
  buffer by the time a key returns.

## Roadmap

- Nothing open.
