# Minimap

Editor action `view.minimap`; setting `show_minimap`.

A code-overview column at the right of the single-pane editor: each row is a band of source lines drawn as a bar sized to the band's longest line, with the current viewport highlighted. Clicking the minimap jumps to the proportional source line. Off by default.

From **View -> Editor -> Minimap**. `ui::draw_minimap`; `App::minimap_click`; `layout.minimap`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
