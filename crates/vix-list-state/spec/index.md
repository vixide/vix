# List state

Shared arithmetic for a scrollable, single-selection list — up/down (one row
or one page), select-by-index, and keeping a scroll offset following the
highlight within a fixed-height viewport.

**Status:** Shipped (T144). Six pure functions, no state of their own: `up`,
`down`, `page_up`, `page_down`, `select_index`, `ensure_visible`. Every
panel with a highlighted row in a scrolling list — the file browser, the
Nerd/ASCII/HTML/X11 character pickers, the outline panel, the contact/vcard
panels, the file/system information panels, the media-type picker, the
snippet library, the left-dock's file tree, the DB workbench's catalog/
results/statement-editor lists, the JSON/YAML value editor, the hex byte
editor, and the SQL statement editor — used to reimplement the same handful
of clamp expressions independently, with the inevitable copy-paste drift
(one panel's `ensure_visible` forgot to clamp scroll against the list's own
length at all; several duplicated the "page can't be zero" guard slightly
differently). T144's own count: `ensure_visible` in 17 crates, comparable
duplication across the other five functions wherever a panel exposed them.

## Why plain functions, not a `ListCursor` struct

The obvious shape — a `ListCursor { selected, scroll }` struct panels adopt
wholesale — was tried first and rejected: panels' fields aren't uniformly
named (`selected`/`sel`/`row`/`top`), several don't store a plain index at
all (`vix-edit-bytes` derives a row from a byte cursor via `cursor / COLS`;
`vix-edit-outline` derives a position from a computed list of currently-
visible nodes), and dozens of external call sites across `src/app.rs` and
`src/ui/*.rs` read a panel's `.selected`/`.scroll` fields directly for
rendering and hit-testing. Requiring every panel to rename its fields to
match a shared struct (or wrap them in one, turning `panel.selected` into
`panel.cursor.selected` everywhere it's read) would have meant hundreds of
external call-site edits for what should be a purely internal
deduplication.

Free functions over plain `usize`s sidestep all of that: a panel keeps its
own field names and public method signatures exactly as they were, and only
each method's **body** changes to call the shared function. `ensure_visible`
in particular takes `selected`/`scroll` as plain values (not `&mut self`),
so a panel whose "position" is derived rather than stored (`row = cursor /
COLS`, or an index into a freshly-computed visible list) can still use it —
it never needed a stored field in the first place.

## The functions

| Function | Signature | Behavior |
| -------- | --------- | -------- |
| `up` | `(selected) -> usize` | One row up, stopping at row 0. |
| `down` | `(selected, len) -> usize` | One row down, stopping at the last of `len` rows; `len == 0` is a no-op. |
| `page_up` | `(selected, page) -> usize` | `page` rows up (`page` clamped to at least 1), stopping at row 0. |
| `page_down` | `(selected, page, len) -> usize` | `page` rows down, stopping at the last of `len` rows; `len == 0` clamps to row 0. |
| `select_index` | `(idx, len) -> Option<usize>` | `idx` if it names a real row, else `None` — the caller's existing selection is left untouched on `None`, e.g. a click past the end of a shorter-than-expected list. |
| `ensure_visible` | `(selected, scroll, height, len) -> usize` | The new scroll offset: scrolls up/down just enough to bring `selected` into a `height`-row window, and never scrolls past where the last of `len` rows would leave blank space at the bottom. `height == 0` is treated as `height == 1`. |

`ensure_visible`'s "never scroll past the end" clamp is the one a few
panels had silently dropped before T144 (`vix-file-browser-panel` was the
one found missing it) — unifying onto the function that has it is a real,
if minor, behavior fix for those panels, not just deduplication.

## Adoption

A panel's public API — field names, method signatures — never changes; only
each method's body becomes a one-line delegation, e.g.:

```rust
pub fn up(&mut self) {
    self.selected = vix_list_state::up(self.selected);
}

pub fn down(&mut self) {
    self.selected = vix_list_state::down(self.selected, self.len());
}

pub fn ensure_visible(&mut self, height: usize) {
    self.scroll = vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
}
```

A panel whose "length" comes from something other than a plain
`self.len()` (a filtered/library-scoped count, a computed visible-node
list, a rows-in-a-byte-grid count) passes that instead — the function
doesn't care how `len` or `selected` were arrived at.
