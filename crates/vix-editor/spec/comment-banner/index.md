# Comment Banner

Editor action `edit.comment_banner`.

Turn the current line's text into a three-line boxed section header: the title bordered above and below by a rule of `=`, each line prefixed with the language's line-comment token and the original indentation. An empty line uses a `Section` placeholder.

From **Edit -> Comment Banner** or the command palette. Host method `App::comment_banner`; comment token from `Editor::comment_prefix`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
