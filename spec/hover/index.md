# Hover

1. Report hover events (src/main.rs) — after EnableMouseCapture, also request xterm any-motion tracking (\x1b[?1003h), disabled again on teardown (\x1b[?1003l). Without this, plain mouseover never reaches the app.

2. Drive the open menu from motion (src/app.rs) — when a menu is open, Moved and Drag(Left) events now call a new menu_hover: hovering a dropdown row sets menu.item; hovering a different top-level name switches the open menu. Clicks still commit as before. Extracted top_menu_index_at so the bar hit-test is shared with menu_click.

3. Guard other panes — enabling any-motion tracking means hover events now stream in everywhere. Since editor*mouse unconditionally grabs focus and forwards to the editor widget, I added a guard: with no menu open, a Moved event returns early, so hovering a pane never steals focus or moves the cursor. (The explorer/messages handlers already ignored motion via their * => {} arms.)
