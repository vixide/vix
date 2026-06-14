# Scrollbars

Vix draws one consistent scrollbar everywhere a view can scroll: the editor, the
file explorer, the bottom dock, and the scrollable overlay panels (the welcome
screen and the character/color pickers). Views that can overflow **horizontally**
— the editor (when soft wrap is off), the file explorer, the message drawer, and
the bottom dock — also get a matching horizontal scrollbar.

## Appearance

- A vertical, one-column scrollbar in a gutter on the right edge of the view, and
  — when content overflows sideways — a horizontal, one-row scrollbar along the
  bottom edge of the view.
- **The thumb is always exactly one character** — a `●` — never sized
  proportionally to the content length.
- The thumb's **position** along the track is proportional: it sits at the
  start cell at the start and the end cell at the end, interpolating in between.
  For cursor/selection views the vertical thumb tracks the cursor/selected item;
  for scroll-only views it tracks the scroll offset. The horizontal thumb tracks
  the horizontal scroll offset within `content_width − viewport_width`.
- The vertical track is a dim `│` spanning the gutter; the horizontal track is a
  dim `─` spanning the bottom row — neither has end-cap arrows.
- A scrollbar appears only when the content overflows that axis, and honors the
  **Show/Hide Scroll Bar** toggle (`show_scrollbar`).

## Interaction

- **Mouse wheel** and the keyboard scroll the view vertically as usual.
- **Click** a point on a track to jump there.
- **Press and drag** a scrollbar to scroll continuously (vertical or horizontal).

## As implemented in Vix

`src/ui.rs` owns the shared renderers — `draw_scrollbar` (vertical) and
`draw_hscrollbar` (horizontal) — and the click/drag position mapping
(`scrollbar_pos_from_row` / `scrollbar_pos_from_col`). Each scrollable view
records its scrollbar rectangle(s) for hit-testing. Horizontal rendering slices
rows to the visible window (`hslice` for plain lines, `hslice_spans` for styled
list rows); the editor uses its own `offset_x`, while the docks track an
`*_hscroll` offset in `App` and `*_hmax` for drag.
