# Scrollbars

Vix draws one consistent scrollbar everywhere a view can scroll: the editor, the
file explorer, the bottom dock, and the scrollable overlay panels (the welcome
screen and the character/color pickers).

## Appearance

- A vertical, one-column scrollbar in a gutter on the right edge of the view.
- **The thumb is always exactly one character** — a `●` — never sized
  proportionally to the content length.
- The thumb's **position** along the track is proportional to the scroll
  position: at the very top it sits in the top track cell, at the very bottom in
  the bottom track cell, interpolating in between.
- The track is a dim `│`, capped with `↑` (top) and `↓` (bottom) arrows.
- The scrollbar appears only when the content overflows the viewport, and (for
  the editor, explorer, and bottom dock) honors the **Show/Hide Scroll Bar**
  toggle (`show_scrollbar`).

## Interaction

- **Mouse wheel** and the keyboard scroll the view as usual.
- **Click** a point on the track to jump there; clicking the `↑`/`↓` arrow caps
  jumps to the top/bottom.
- **Press and drag** the scrollbar to scroll continuously.

## As implemented in Vix

`src/ui.rs` owns the shared renderer (`draw_scrollbar`) and the click/drag
position mapping (`scrollbar_pos_from_row`); each scrollable view passes its
`total`, `viewport`, and current position and records the gutter rectangle for
hit-testing.
