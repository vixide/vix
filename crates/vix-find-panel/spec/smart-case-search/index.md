# Smart-Case Search

Find-box toggle (on by default); `Alt+S` or the Smart button.

When the explicit Case toggle is off, a query is matched case-insensitively only while it has no uppercase letter; an uppercase letter makes it case-sensitive.

`SearchBar::pattern` (the `Flags::SMART_CASE` bit). See `spec/find-and-replace/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
