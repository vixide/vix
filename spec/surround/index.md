# Surround

Editor actions `edit.surround.paren` / `bracket` / `brace` / `angle` / `double_quote` / `single_quote` / `backtick`.

Wrap the active selection in a bracket or quote pair; repeating the same action on the wrapped text removes the pair (a toggle). With no selection, the empty pair is inserted and the cursor placed between the halves.

From **Edit -> Surround** or the command palette. Dispatched by `App::surround`, which calls the shared `App::toggle_wrap` / `crate::affix` helper.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
