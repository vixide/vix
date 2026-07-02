# Jump to Line (Labels)

Navigation action `nav.jump`.

Leap/EasyMotion-style motion: each visible line is given a short label (`a`, `b`, `c`, ..., then `aa`, `ab`, ...) drawn at its left edge; typing a label moves the cursor to that line's start. Esc (or any non-label key) cancels.

From **Go -> Jump to Line** or the command palette. State `JumpMode`; `App::open_jump` / `jump_key` (intercepted at the top of `on_key`); overlay `draw_jump_labels`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
