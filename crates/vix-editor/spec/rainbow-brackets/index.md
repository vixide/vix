# Rainbow Brackets

Editor action `view.rainbow_brackets`; setting `rainbow_brackets`.

Color matching brackets by nesting depth (six colors cycling by depth). Off by default. Applies to every buffer and persists.

From **View -> Editor -> Rainbow Brackets**. `editor_core::render::draw_rainbow_brackets`; plumbed like `auto_pair` via `set_rainbow_brackets`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
