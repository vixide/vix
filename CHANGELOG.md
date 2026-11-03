# Changelog

All notable changes to Vix are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this workspace aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

This is the 2.0 release: it makes breaking changes to the library API (the `vix` command and settings files are unaffected) so later additions can be non-breaking.

### Fixed

- **musl release build (T670).** The `x86_64-unknown-linux-musl` build is now configured to link statically and without PIE (`.cargo/config.toml`), and CI checks the result with the new `scripts/check-static-binary`. The previous build was dynamically linked against `/lib/ld-musl-x86_64.so.1`, so it did not start on glibc systems and crashed on Alpine. Not yet verified on a real release build.

### Added

- **2.0 launch kit (T745).** A `/whats-new/` page on the website, user-facing release notes in [`docs/whats-new-2-0`](docs/whats-new-2-0/index.md), and announcement drafts in [`docs/launch/`](docs/launch/index.md) (never posted). All are labelled as not released until 2.0 ships.
- **Install-channel tooling and CI wiring (T742).** New `scripts/packaging` (`check`, part of `scripts/check`, keeps the winget, Scoop and AUR templates, the `cargo binstall` metadata and the Nix flake consistent; `render <version>` rewrites the templates from a release's `.sha256` files, adding Windows ARM when the asset exists) and `.github/workflows/packaging.yml`, which renders and verifies the templates after a release (`nix build`, `winget validate`, `makepkg`) and has publish jobs for winget-pkgs, a Scoop bucket and the AUR that do nothing unless `PACKAGING_PUBLISH` is `true` and a placeholder secret exists. Spec: [`spec/packaging`](spec/packaging/index.md). **Nothing is published and the workflow has not run**; the channels stay listed as "not yet published" until each is verified on a real machine.
- **Review follow-ups (T665).** In a branch review, **Git → Stage Review Hunk** stages the hunk under the cursor and **Discard Review Hunk…** undoes it in the working tree after a confirmation (both via `git apply`; git's reason is shown when the hunk no longer fits). `r` in **Git → Stash List…** opens the highlighted stash in a review.
- **User-defined environment detectors (T667).** A project can add its own environment sources in `.vix/env.toml` (`[[detector]]` with `name`, `detect` files and a `command` that prints the same JSON as `direnv export json`); the status bar shows `custom`. Detectors run project-controlled code, so they are gated by the workspace-trust decision exactly like `direnv`/`mise`. The trust prompt is now also raised for a workspace that has only an `.envrc`, a mise config or a `.vix/env.toml` detector (new `ScriptTrustPrompt::env`; new `Source::Custom`). Spec: [`crates/vix-project-env/spec`](crates/vix-project-env/spec/index.md).
- **Debugger leftovers (T617).** Run → Function Breakpoint… (`run.function_breakpoint`) toggles a function breakpoint by name, sent as `setFunctionBreakpoints` to adapters that support it; and the Debug panel now keeps expanded variables expanded across steps. Data and instruction breakpoints and breakpoint persistence are still not done.
- **ACP follow-ups (T635).** **AI → Start Agent** now asks which agent when `ai_agents` lists several (a number; empty is the first). An agent's `fs/write_text_file` may create a new file (parent directory must exist): the content arrives as a pending diff in an in-memory tab and reaches the disk only after you accept and save. **AI → Agent Plan** (`ai.agent_plan`) opens the plan and tool calls in a live read-only tab. **AI → Resume Agent** (`ai.agent_resume`) restarts the last agent and loads its previous session over `session/load` when the agent supports it, falling back to a fresh session otherwise. Trust gating is unchanged. Spec: [`crates/vix-acp/spec`](crates/vix-acp/spec/index.md).
- **On-type formatting (T624).** With the new `format_on_type = true` setting (default off; **View → Format on Type**, action `view.format_on_type`), typing a character that the language server lists in `documentOnTypeFormattingProvider` (for example `;`, `}` or Enter) sends `textDocument/onTypeFormatting` and applies the returned edits, dropping them if you typed on before the reply. New: `Lsp::request_on_type_formatting`, `Lsp::is_on_type_trigger`, `vix_lsp_core::message::on_type_formatting_params`. See `crates/vix-lsp/spec/index.md` § On-type formatting.
- **Document colors and links (T621; [`crates/vix-lsp/spec`](crates/vix-lsp/spec/index.md#document-colors-and-links-t621)).** A language server that offers `colorProvider` has its colors drawn as `■` swatches in the color itself (end-of-line virtual text, so the swatch sits after the line, not beside the color), switched by the `document_colors` setting; one that offers `documentLinkProvider` has its links opened by **Tools → Language Server → Open Link** (`lsp.open_link`, the link under the cursor or the only one on its line) or Ctrl/Cmd+click. `file:` targets open in the editor (`#L12` jumps to the line); `http(s):` and `mailto:` go to the system opener; other schemes are refused. Library: `vix_lsp_core::document_extras`, `Lsp::refresh_document_extras`, `LspEvent::{DocumentColors, DocumentLinks}` and `VirtualTextLayer::ColorSwatches`. `colorPresentation` (writing a picked color back) is not done.
- **File-rename and watched-file notifications (T623).** Renaming a file with **File → Rename** now first asks language servers that want it (`workspace/willRenameFiles`) for edits such as import updates, applies them, then renames the file (it waits at most 3 seconds, and a server error never blocks the rename) and sends `workspace/didRenameFiles`. Servers that register file watchers (`client/registerCapability` for `workspace/didChangeWatchedFiles`, as rust-analyzer and typescript-language-server do) are told about outside edits: open files that changed on disk plus the workspace-root `Cargo.toml`, `Cargo.lock`, `package.json`, `tsconfig.json`, `pyproject.toml`, `go.mod`, `go.sum` and `deno.json`, through the existing once-a-second change poll. New: `vix_lsp_core::file_ops`, `Lsp::{will_rename_files, did_rename_files, did_change_watched_files, watches_files}`, `LspEvent::WillRenameFiles`. Not done: explorer and directory renames, file create/delete notifications, recursive watching.
- **Agent Client Protocol client (T635, first slice).** New `vix-acp` crate speaks ACP (the protocol Zed defined for coding agents such as Gemini CLI) over the agent's stdio, and **AI → Start Agent / Prompt Agent… / Stop Agent** (`ai.agent_start`, `ai.agent_prompt`, `ai.agent_stop`) drive the first command in the new `ai_agents` setting. The agent is started **only in a trusted workspace** and never through a shell. Its replies, tool calls and plan appear in the AI chat panel. A permission request opens a prompt that denies unless you answer `y`; an edit the agent writes opens as a pending accept/reject diff and nothing reaches the disk until you save; file reads come from open (even unsaved) buffers; paths outside the workspace are refused. Not done yet: choosing between several agents, creating new files, a dedicated plan/tool-call pane, session resume. The framing is newline-delimited JSON, not LSP's `Content-Length`, so `vix-lsp-core` is not reused.
- **Inline ghost-text completion (T618).** A suggestion drawn dimmed after the cursor, from a language server (`textDocument/inlineCompletion`, so copilot-language-server works) or from an AI fill-in-the-middle prompt, chosen by the new `inline_completion = "off" | "lsp" | "ai"` setting (default `"off"`). It is requested after an idle pause once you have typed, only when nothing but blanks follows the cursor, and dropped when the buffer or cursor changes or a server reply is for an older document. Tab accepts it, Ctrl+Right accepts one word, Esc dismisses; the actions `inline_completion.trigger`, `.accept`, `.accept_word` and `.dismiss` do the same from the palette. The `"ai"` source sends the text around the cursor to your configured AI provider, so it is opt-in and obeys `ai = "off"`. New: `vix_lsp_core::inline_completion`, `vix_lsp::ghost`, `vix_ai_core::fim`, `LspEvent::InlineCompletion` and `Lsp::request_inline_completion`. Only the first line is drawn (with `…` when more follow), and soft-wrapped views draw none.
- **Project environment activation** (T667): new `vix-project-env` crate. Vix detects a project's `.venv`/`venv` (`pyvenv.cfg`), direnv (`.envrc`) or mise (`mise.toml`, `.mise.toml`, `.tool-versions`) and applies its variables to language servers, tasks/commands and the integrated terminal, with the source shown in the status bar. `direnv export json` and `mise env --json` run project-controlled code, so they run only in a workspace you trusted (the existing T132 trust decision); an untrusted workspace falls back to the file-only `.venv` source and says so. New action `project.reload_env` (Project → Reload Environment). Already-running servers keep their old environment until restarted. Custom detectors are not implemented.
- **Safe saving (T663).** Saving a file with several hard links, or one owned by another user, now writes in place instead of renaming a temp file over it, so the links and the owner survive. Every in-place save first copies the original to the recovery directory (`~/.local/state/vix/recovery`) and removes the copy once the write is flushed; a copy left behind by an interrupted save is reported at the next start. (The Restore / Diff / Discard prompt is not built yet.)

### Changed

- **Code documentation pass (group 0).** Module, item and inline documentation (plus doctests) for `vix-editor-core`, `vix-im`, `vix-mcp-server`, `vix-oci`, `vix-project-env`, `vix-roam`, `vix-encoding`, `vix-input` and 14 smaller crates; no behavior change. Fixed several stale docs (for example `vix_roam::slugify`, `vix-column-view`'s `Outcome::Close`).

### Breaking

- **Typed Slint window errors** (T610, seventh slice): `vix-slint`'s `window::compile` and `window::create` return the `#[non_exhaustive]` `WindowError` (`Compile`, `MissingComponent`, `Create`, `Property`) instead of `String`. `Display` text is unchanged.
- **Typed MCP server errors** (T610, sixth slice): `vix_mcp_server::workspace::{resolve, read_text}` return the `#[non_exhaustive]` `WorkspaceError`, `tools::call` returns `ToolError`, and `Host::sqlite_schema` reports a `HostError` (a boxed error) instead of `String`. `Display` text is unchanged.
- **Typed database session errors** (T610, fifth slice): `vix_db::session::Session::{connect, send, restart, run}` and `vix_db::catalog::read_sqlite_schema` return the `#[non_exhaustive]` `SessionError` (`Spawn`, `Connect`, `Setup`, `NotConnected`, `Query`) instead of `String`. `Display` text is unchanged.
- **Typed REST body-file errors** (T610, fourth slice): `vix_http_client::rest::load_body_file` returns the `#[non_exhaustive]` `BodyFileError` (`Outside`, `Read`) instead of `String`. `Display` text is unchanged.
- **Typed tool errors** (T610, third slice): the small tool crates return `#[non_exhaustive]` error enums instead of `String` — `vix_base_tool::BaseError`, `vix_base64_tool::Base64Error`, `vix_url_tool::UrlError`, `vix_calculator_tool::CalcError` (also carried by `Outcome::Err`), `vix_format_tool::FormatError`, `vix_jwt_tool::JwtError`, `vix_edit_value::ParseError` (`Tree::from_text`) and `vix_script::CatalogError` (`Catalog::insert_toml`). `Display` text is unchanged.
- **Typed convert errors** (T610, second slice after T673): `vix_convert_tabular::json_to_rows` and the `convert` function of every `vix-convert-from-*-tool` crate return the new `#[non_exhaustive]` `vix_convert_tabular::ConvertError` instead of `String` (`Parse`, `NotArrayOfObjects`, `ElementNotObject`, `Render`). Its `Display` text is unchanged. Other `Result<_, String>` library functions are still to convert.
- **Multiple carets** (T622): `vix_editor_core::multicursor::CaretMove` is `#[non_exhaustive]` and gains `WordLeft`, `WordRight`, `LineStart`, `LineEnd`, `TextStart`, `Home`, `BufferStart` and `BufferEnd`. Carets now keep their add order — the primary is always the newest caret — so `add_caret_at`, `add_caret_above`, `add_caret_below` and `column_select` make the new caret primary instead of keeping the old one, and edits no longer make the lowest caret primary.
- **`EditorBehaviorSettings::auto_save`** (T628) is a `vix_settings::AutoSave` (`Off`, `Interval`, `FocusLost`) instead of a `bool`. Config files are unaffected: `auto_save = true`/`false` still load.
- **`#[non_exhaustive]`** on the public types that keep growing: `PromptKind`, `App`, `Focus`, `Capability`, `Tab`, `vix_editor::Editor`, `LspEvent`, `Settings` and its section structs, and the theme structs (`CustomTheme`, `RegionColors`, `EditorColors`, `SyntaxColors`). Outside their crate they can no longer be built with a struct literal (start from `Default::default()` and assign fields) or matched exhaustively (add a `_` arm). Adding a field or variant to them is no longer a breaking change.
- **`PromptKind::PipeSelection` and `PromptKind::InsertCommandOutput`** replace the private flag the Pipe Through Command prompts used.
- **Removed:** the deprecated empty module `vix_editor_core::editor_crossterm` (use `editor_input`), the deprecated `vix_editor_core::Editor::set_clipboard` (use `copy_to_clipboard`), and `LspEvent::Edits` / `LspEvent::LinkedRanges`, which nothing has emitted since T652 (use `Lsp::take_edit_replies`).
- **`vix_fileops::read_text_guarded`** returns a `#[non_exhaustive]` `TextFile { text, lossy }` instead of `(String, bool)`.
- **`App`'s public surface** (T674): 95 of its 114 public fields are now `pub(crate)`. The 19 that stay public are the ones the GUIs, examples and benchmarks use (`root`, `editor`, `settings`, `menu`, `palette`, `prompt`, `dialog`, `picker`, `focus`, `flags`, `layout`, `status`, `messages`, `bottom_dock`, `db`, `lsp`, `hover`, `edit_table`, `welcome`). The integration tests now compile inside the library.
- **Typed errors** (T673) in place of `Result<_, String>`: `vix_git::GitError` (`NotInstalled`, `Io`, `Failed { code, stderr }`, `InvalidArgument`) for the staging, commit, branch and stash functions; `vix_http_client::HttpError` (`UnsupportedScheme`, `InvalidRequest`, `Transport`) for `send`; and `vix_ai_core::AiError` (`Transport`, `NotJson`, `Api`, `NoContent`) for `complete` and each provider's `parse_response`. Each implements `Display` with the old message text. In Vix, a git action run without git installed now shows how to install it instead of an OS error.
- **Typed tunnel errors** (T610): `vix_db::tunnel::open` and `ssh_args` return `TunnelError` (`InvalidField`, `NoPort`, `NotInstalled`, `Spawn`, `Timeout`) instead of `String`. A missing `ssh` binary now shows a translated "ssh is not installed" message.
- **CodeLens events** (T726): `vix_lsp::LspEvent` gains `CodeLensUpdate`, `CodeLensResolved` and `CodeLensRefresh`; `Lsp` gains `refresh_code_lens`, `resolve_code_lens`, `resolves_code_lens` and `executes_command`; `vix-lsp-core` gains `parse_code_lens_list`, `parse_code_lens`, `code_lens_line`, `parse_code_lens_provider` and `parse_execute_commands`. `vix-settings` has a new flattened `language_features` group (`code_lens`). The TUI's `CodeLensMenu` chooser is gone: the lens list uses the shared `vix-choice-list` overlay.
- **LSP completion types** (T612): `vix_lsp_core::CompletionItem` is `#[non_exhaustive]` (build one with `CompletionItem::new(label, insert_text)`) and gains `range`, `additional_edits`, `filter_text`, `sort_text`, `kind` and `commit_chars`; `message::parse_completion` and `LspEvent::Completion` carry a new `CompletionList { items, is_incomplete }` instead of a `Vec<CompletionItem>`, and `LspEvent::Completion` is now sent even for an empty reply.
- **Snippet completion items** (T613): `vix_lsp_core::CompletionItem` gains `insert_text_format` (and `is_snippet()`), and `vix_snippet_tool::parse` follows the full LSP/VS Code snippet grammar — nested placeholders, `${1|a,b|}` choices, `\}`/`\\` escapes, a repeated tabstop number visited once — with a new `parse_with(body, Dialect)`/`parse_lsp` that also resolves `$NAME` variables. A snippet body's `\` and `}` must now be escaped to be literal.
- **`vix_script::DiscoveredScript`** is `#[non_exhaustive]` and gains `locales_dir` (T735); build one with `DiscoveredScript::new(stem, path)`.
- **`vix_terminal::Terminal::open` and `open_command`** take a fifth/sixth argument, `scrollback` (lines kept, clamped to `MAX_SCROLLBACK`; pass `0` for the old behavior), and `vix-terminal` now uses `vt100` 0.16 (T631). In Vix, **Tools → Terminal** run again and `Ctrl+]` now *hide* the terminal, which keeps running; **Tools → Terminal → Close Terminal** (`terminal.close`) stops it.
- **`vix_git::blame_line`** takes a fourth argument, `contents: Option<&str>`: the buffer's unsaved text to blame instead of the file on disk (T657). Pass `None` for the old behavior. `vix_git::discard` is new.
- **`vix_tasks::Task` and `vix_tasks::task::NamedTask`** gain a `matcher: Option<String>` field (T632), the task's problem matcher; struct literals must set it (`None` means the built-in matchers).
- **Debugger API** (T617): `vix_dap::Dap::start` takes `HashMap<String, Vec<SourceBreakpoint>>` instead of line numbers, plus a `StartMode` (`Launch`/`Attach`); `Dap::set_breakpoints` takes `&[SourceBreakpoint]`. `DapEvent` is `#[non_exhaustive]`, `DapEvent::Variables` is now `{ reference, variables }`, and it gains `Capabilities`, `Threads`, `Scopes` and `VariableSet`. `Frame`, `Variable` (which gains `variables_reference`) and `vix_settings::DebugAdapter` (which gains `attach`) are `#[non_exhaustive]`; build them with `Frame::new`, `Variable::new` and `DebugAdapter::new`. `App`'s `Focus` gains `Debug`, and `PromptKind` gains the Edit Breakpoint and set-variable prompts.
- **LSP hierarchy and pull diagnostics** (T619, T620): `vix_lsp::LspEvent::CallHierarchyPrepared` carries a `HierarchyItem { server, item }` (the item tagged with the language id of the server that prepared it) instead of a bare `serde_json::Value`, and `Lsp::request_incoming_calls` takes that `&HierarchyItem` instead of `(path, item)`; incoming calls now arrive as the new `LspEvent::Hierarchy { direction, locations }` instead of `LspEvent::References`. `vix_lsp_core::message::first_call_hierarchy_item` is renamed `first_hierarchy_item`. `vix_workspace_search::WorkspaceSearch` gains a `title: Option<String>` field.

### Added

- **Edit a directory as text** (T636; [`crates/vix-fileops/spec`](crates/vix-fileops/spec/index.md)): **File → Edit Directory…** (`explorer.edit_directory`, also in the palette) opens a directory as an editable buffer, one `/<id> <name>` line per entry, in the style of Emacs's wdired and Neovim's Oil. Change a name to rename it (swapping two names works), add a line to create a file (or a directory, ending in `/`), and delete a line to delete the entry. Saving shows what will change and applies it only after `y`/Enter; deletes follow `explorer_delete` (the OS trash by default), nothing is overwritten, and open buffers follow renamed files. Library: `vix_fileops::dir_edit` (`render`, `diff`, `schedule`).
- **Debugger depth** (T617; [`crates/vix-dap/spec`](crates/vix-dap/spec/index.md), [`docs/debugger`](docs/debugger/index.md)): **Run → Edit Breakpoint…** (`run.edit_breakpoint`) sets the cursor line's breakpoint condition, hit count and log message (a logpoint prints instead of stopping). **Run → Exception Breakpoints…** (`run.exception_filters`) is a checklist of the exception filters the adapter offers in `initialize`, with its defaults ticked, sent as `setExceptionBreakpoints`. **Run → Attach** (`run.attach`) sends `attach` with a new `debug_adapters.attach` table instead of `launch` (values that read as JSON numbers, booleans, objects or arrays are sent typed). The Debug panel lists the program's threads (`threads`), and **Run → Focus Debug Panel** (`run.focus_panel`) gives it the keyboard: Enter selects a thread or a frame (the editor follows the frame), expands a structured variable to any depth with lazy `variables` requests, Left collapses, and `e` sets a value with `setVariable` when the adapter supports it. Watches and Evaluate run in the selected frame, and while stopped the stopped line shows the values of the variables it mentions, on a new end-of-line virtual-text layer. Watches are now re-evaluated once per stop instead of on every event-loop tick while stopped. Library: `vix_dap::{SourceBreakpoint, Capabilities, ExceptionFilter, StartMode, Thread, Scope, auto_expand_scope}`, `Dap::{capabilities, exception_filters, set_exception_filters, select_thread, select_frame, thread_id, frame_id, variables, set_variable}`, the `vix_dap::view` tree model (`DebugView`, `Row`, `RowKind`, `Section`, `Activation`), and `VirtualTextLayer::DebugValues`.

- **Interactive rebase and cherry-pick** (T633; [`crates/vix-git/spec`](crates/vix-git/spec/index.md#interactive-rebase-and-cherry-pick-t633), [`docs/git-panel`](docs/git-panel/index.md#interactive-rebase)): **Git → Rebase → Interactive Rebase…** asks for a base and lists the commits after it, oldest first, in an overlay where `p`/`r`/`e`/`s`/`f`/`d` pick, reword (a prompt for the new message), edit, squash, fix up or drop each one and `Alt+↑`/`Alt+↓` reorder them; `Enter` runs `git rebase -i` with that list, and a leading squash or fixup is refused first. No editor opens: git gets the list through `GIT_SEQUENCE_EDITOR`, and a reworded message through an environment variable, both set on that one git process only. **Continue Rebase** and **Abort Rebase** finish or abandon a rebase stopped at an `edit` or a conflict. **Cherry-Pick…** asks for a branch and lists its commits the current branch lacks; `c` cherry-picks one. Library: `vix_git::{RebaseAction, TodoCommit, TodoItem, TodoProblem, RebasePlan, RebaseTodo, RebaseState, parse_todo, serialize_todo, check_todo, plan, rebase_commits, rebase_interactive, rebase_continue, rebase_abort, rebase_in_progress, cherry_pick_candidates, cherry_pick}`; `LogScope` gains `Pick` and is now `#[non_exhaustive]`.
- **Per-document pull diagnostics** (T619; [`crates/vix-lsp/spec`](crates/vix-lsp/spec/index.md#pull-diagnostics-t619)): a language server that offers `diagnosticProvider` is asked for each document's diagnostics (`textDocument/diagnostic`) once it is ready, after each open, and 250 ms after the last change, so pull-only servers such as Roslyn now underline problems too. Each pull sends the last report's `resultId` as `previousResultId`; an `unchanged` reply keeps the diagnostics, reports for `relatedDocuments` are merged too, and a server's `workspace/diagnostic/refresh` request re-pulls every open document (the client now declares `refreshSupport`). Library: `vix_lsp_core::message::{document_diagnostic_params, parse_diagnostic_provider, parse_document_diagnostic_report, DiagnosticReport}`.
- **Outgoing calls and type hierarchy** (T620; [`crates/vix-lsp/spec`](crates/vix-lsp/spec/index.md#call-and-type-hierarchy-t620)): **Tools → Language Server** gains Call Hierarchy (Callees), Type Hierarchy (Supertypes) and Type Hierarchy (Subtypes) (`lsp.call_hierarchy_outgoing`, `lsp.type_hierarchy_supertypes`, `lsp.type_hierarchy_subtypes`) beside Call Hierarchy (Callers). Results open in the result list titled by direction, and `Alt+D` there flips callers ⇄ callees or supertypes ⇄ subtypes without preparing again. Each prepared item remembers the server that produced it and its follow-up requests go to that server only, which lifts T123b's first-server-only limit for call hierarchy when two servers handle one file. Library: `vix_lsp::{HierarchyItem, HierarchyDirection}`, `LspEvent::{TypeHierarchyPrepared, Hierarchy}`, `Lsp::{request_prepare_type_hierarchy, request_hierarchy, request_outgoing_calls, request_supertypes, request_subtypes}` and `vix_lsp_core::message::{parse_outgoing_calls, parse_type_hierarchy_items, hierarchy_item_params}`.

- **Tree-sitter textobjects and function/class motions** (T616; [`crates/vix-editor-core/spec/textobjects`](crates/vix-editor-core/spec/textobjects/index.md)): **Edit → Select** gains Function, Class and Parameter (Inside / Around), and **Go → Function / Class** jump to the next or previous one, wrapping at the file's ends; all ten are in the palette. They work offline from bundled `textobjects.scm` queries for Rust, Python, JavaScript, TypeScript, Go, Java, C#, C, C++, shell, HTML, CSS, TOML, JSON, YAML and Markdown. Repeating a selection grows to the enclosing object, and **Shrink Selection** now works without a language server, stepping back through offline expansions and textobject selections. In the Vi and Spacemacs modal engine: `if`/`af`, `ic`/`ac`, `ia`/`aa` after an operator or in Visual mode (which also gains `i`/`a` for the existing objects, e.g. `viw`), and `]f`/`[f` (`]m`/`[m`), `]c`/`[c`, `]a`/`[a` motions, all with counts. Lookups read only a small window of the existing parse tree, so they stay fast in large buffers and never parse synchronously. Library: `vix_editor_core::textobject` (`TextObjectKind`, `TextObjectDir`, `TextObject`, `Code::{enclosing_textobjects, textobject_range, next_textobject, has_textobjects}`).

- **Multi-cursor depth** (T622; [`crates/vix-editor-core/spec`](crates/vix-editor-core/spec/index.md#multiple-carets-t622)): word, line start/end, smart Home/End and buffer start/end motions move every caret (keys, Ctrl+←/→ and the named motions, with or without Shift). New **Edit → Select → Split Selection into Lines** (`edit.split_selection_into_lines`, Helix/Kakoune `Alt-s`) and **Select Regex Matches in Selection** (`edit.select_regex_in_selection`, Kakoune/Helix `s`). The catalog actions now do what their names say: `skip_multi_cursor`/`skip_multi_cursor_back` move the newest match on to the next/previous occurrence, `remove_multi_cursor` removes only the caret added last, and `spawn_multi_cursor_select` splits the selection into lines. Copy with several carets copies every selection one per line, and a paste with one line per caret puts one line at each. Undoing a multi-caret edit is one step and returns to a single caret. Completion and snippet expansion are explicitly single-caret. Library: `Editor::{caret_count, caret_selection_texts, multi_paste, multi_cut, skip_occurrence, remove_last_caret, split_selection_into_lines, select_matches_in_selection}` and `PromptKind::SelectRegexInSelection`.
- **Problem matchers and next-error over task output** (T632; [`crates/vix-tasks/spec`](crates/vix-tasks/spec/index.md#problem-matchers-and-next-error-t632), [`docs/tasks`](docs/tasks/index.md#problems-in-task-output)): the output of a task, a Project menu command, Tools → Test or Run Command is read for compiler errors, lint findings and failing tests (rustc/cargo, gcc/clang, tsc, eslint, Python tracebacks and pytest, Go), including multi-line forms such as rustc's header-then-` --> ` block. **Go → Next Error** / **Previous Error** (`go.next_error`, `go.prev_error`) open each problem in turn, wrapping around; the Problems list (Tools → Language Server → Diagnostics…) shows them after the language servers' diagnostics; the open file shows them inline on a new end-of-line layer under the `inline_diagnostics` setting; and a click on a rustc arrow or Python frame line in the bottom dock jumps to it. A `tasks.toml` task can choose a matcher, turn matching off, or give its own regular expression with `matcher = "…"`. Paths resolve against the folder the command ran in, and a file is opened only when you go to it. Library: new `vix_tasks::matcher` (`MatcherSpec`, `Builtin`, `Scanner`, `Problem`, `ProblemList`, `scan`, `locate`, `resolve_path`) and `VirtualTextLayer::TaskProblems`.

- **Terminal-to-editor bridge** (T666; [`crates/vix-terminal/spec`](crates/vix-terminal/spec/index.md)): Ctrl+click a `path:line[:col]` in terminal output (or scrollback) to open it, resolved against the shell's working directory, which Vix follows through OSC 7; rustc, Python, gcc, tsc, eslint and Go locations use the task dock's problem-matcher parser (T632). New **Tools → Terminal** items: Send Selection to Terminal (`terminal.send_selection`, the selection or current line, opening a terminal if none), Restart Terminal (`terminal.restart`, in place, keeping the transcript) and Toggle Terminal Key Capture (`terminal.toggle_capture`, also `Ctrl+\`; with capture off Ctrl/Alt chords reach the editor and the title says `editor keys`). A shell that exits no longer closes its tab: it is marked `exited`, Enter restarts it and any other key closes it. Library: `vix_terminal::{Osc7Scanner, paste_bytes, word_in}` and `Terminal::{cwd, restart, send_text, row_text, word_at, capture, set_capture, take_exit_notice}`, `Tabs::iter_mut`.
- **Terminal scrollback and multiple terminals** (T631; [`crates/vix-terminal/spec`](crates/vix-terminal/spec/index.md)): **Tools → Terminal** is a submenu — Show/Hide Terminal, New Terminal, Next/Previous Terminal, Copy Terminal Text and Close Terminal (`tools.terminal`, `terminal.new`, `terminal.next`, `terminal.prev`, `terminal.copy`, `terminal.close`). Several shells open as numbered tabs in the terminal overlay; `Ctrl+PgUp`/`Ctrl+PgDn` switch, and hidden terminals keep running. Each keeps a bounded scrollback (new `terminal_scrollback` setting, default 2000 lines, at most 50 000): `Shift+PgUp`/`Shift+PgDn` page it, `Shift+Home`/`Shift+End` jump to its top or back to the live screen, the mouse wheel scrolls it, and typing returns to the live screen. A left drag selects text and copies it on release; programs that ask for the mouse (`htop`, `tmux`, `vim` with `mouse=a`) get clicks, drags and the wheel in the encoding they asked for (SGR, UTF-8 or legacy; Shift selects in Vix instead). In a remote workspace every tab is its own remote shell (T716). A Cowork share (T697) stays on the tab that was active when the host confirmed it, marked in the tab strip; switching tabs never changes what guests see, and the host's scrolling is not shared. Library: `vix_terminal::{Tabs, Selection, encode_mouse, scroll_parser, clamp_scrollback, DEFAULT_SCROLLBACK, MAX_SCROLLBACK}` and `Terminal::{scroll_by, scroll_to_bottom, scroll_offset, live_screen, selection, set_selection, copy_text, wants_mouse, send_mouse}`.

- **Terminal protocol polish** (T628): each frame is drawn inside a synchronized update (DEC mode 2026), so a large redraw never shows half-painted. Vix asks for focus reports: returning to the terminal reloads files changed on disk at once, and the new `auto_save = "focus_lost"` mode saves every dirty buffer when the terminal loses focus (`auto_save` is now `"off"`, `"interval"` or `"focus_lost"`; an old `true`/`false` still loads). Colors now fall back on terminals without truecolor: the depth is decided from `COLORTERM`, `TERM` and `TERM_PROGRAM` (Terminal.app gets 256 colors, the Linux console 16), with no startup terminal query, every frame is downsampled to it, and `vix --doctor` reports it. URLs in the markdown preview and in notifications are OSC 8 hyperlinks. Library: `vix_theme::color_depth` (`ColorDepth`, `TermEnv`, `detect`, `downsample`, `downsample_buffer`), `vix_settings::AutoSave`, `vix::term_caps`, and `App::{terminal_caps, on_focus_gained, on_focus_lost}`.
- **Picker preview pane, query history and Resume Last Picker** (T630; [`crates/vix-palette/spec`](crates/vix-palette/spec/index.md#preview-pane-t630)): on a screen at least 100 columns wide, the file finder, `@`/`@@` symbols and Find In Workspace show the highlighted result's file beside the list, a numbered window around its line with that line highlighted. The preview reads at most 256 KiB of a file (an open buffer previews its unsaved text from the rope), skips binary files, and is skipped under a remote authority. **Up**/**Down** on an empty palette input recall accepted queries. **Tools → Resume Last Picker** (`palette.resume`) reopens the palette or workspace search that closed last with its mode, query and selection. Library: `vix_palette::{Preview, preview_from_bytes, preview_from_lines, QueryHistory, PaletteMemory, Mode::prefix}` and the `MIN_PREVIEW_WIDTH`/`PREVIEW_BYTES`/`PREVIEW_RADIUS`/`HISTORY_CAP` constants.
- **Git stash list and worktrees** (T634; [`crates/vix-git/spec`](crates/vix-git/spec/index.md#stash-list-and-worktrees-t634), [`docs/git-panel`](docs/git-panel/index.md#stash-list)): **Git → Stash List…** lists every stash to show its diff in a read-only tab (`Enter`), apply (`a`), pop (`p`) or drop (`d`, after a confirmation). The new **Git → Worktree** submenu adds a worktree on a new branch (**Add…**), lists them and switches the workspace to one through the project-switch path, so each worktree restores its own session (**List…**), and removes one after a confirmation (**Remove…**). In a remote workspace they run on the remote host. Library: `vix_git::{StashEntry, parse_stash_list, stash_ref, stash_list, stash_apply, stash_pop_at, stash_drop, stash_show, Worktree, Checkout, parse_worktree_list, worktree_list, worktree_add, worktree_remove}`.
- **Undo tree** (T638; [`docs/undo-tree`](docs/undo-tree/index.md)): **Edit → Undo Tree…** (`edit.undo_tree`, also in the command palette) draws the active buffer's undo tree as an ASCII graph, one row per state with its number and time, each branch in its own column. Moving the selection shows that state in the buffer live; `Enter` keeps it, `Esc` puts back the starting text, cursor, selection and modified flag. It refuses a Cowork shared buffer, where local undo is off. While it is open, auto-save, hot-exit backups and reload-on-change wait, and quitting restores the starting state first. Library: new crate `vix-undo-tree-panel` (`TreeNode`, `Row`, `layout`, `Panel`); `vix_editor_core::history::History` is `Clone` and gains `nodes()` (a new `#[non_exhaustive]` `UndoNode` with `seq`, `parent`, `children`, `timestamp`), `current()`, `path_to()` and `select_redo_branch()`; `Code::select_redo_branch` and `Editor::goto_undo_state` are new. Undo states now record their time; undo files written before this load with no times.
- **Incremental LSP sync and ranged semantic tokens** (T615; [`crates/vix-lsp/spec`](crates/vix-lsp/spec/index.md#document-sync)): a language server that asks for incremental sync (`textDocumentSync.change` 2) now gets one ranged `didChange` per edit instead of the whole buffer, in its negotiated position encoding; other servers still get the full text. Semantic tokens are requested for the lines around the view with `semanticTokens/range` when the server offers it (again when the view scrolls away), else as a `full/delta` against the last result, and are debounced (150 ms) while typing; a reply for an older version of the document is dropped. Library: new `vix_lsp_core::sync` (`SyncKind`, `ContentChange`, `diff`, `apply`, `position_at`, `offset_at`) and `message::{did_change_incremental_params, semantic_tokens_range_params, semantic_tokens_delta_params, parse_semantic_tokens_support, parse_semantic_tokens_reply, apply_semantic_tokens_edits, decode_semantic_tokens}`; `Lsp::request_semantic_tokens` takes the visible range (`Option<Range>`), and `Lsp::semantic_tokens_ranged` is new.
- **Hot exit** (T629): unsaved buffers, untitled ones included, are backed up to `<config>/backup/` while Vix is idle and on quit, so a crash or a closed terminal no longer loses typed text. Opening a file that has a backup asks whether to recover it (`r`), show the difference from the file (`d`), discard it (`x`) or decide later; untitled buffers are offered back when the workspace next starts. Nothing is applied without asking, a clean save deletes the backup, and only buffers that changed since their last backup are written (large ones at most once a minute). Remote-workspace buffers are backed up locally, keyed by their remote name. Turn it off with `hot_exit = false`.
- **Jupyter: Org babel evaluation, images and trust settings** (T734e–T734h; [`docs/jupyter`](docs/jupyter/index.md), [`spec/jupyter`](spec/jupyter/index.md)), in a build with the opt-in `jupyter` feature. `C-c C-c` (or **Run → Jupyter → Run**) in an Org source block runs it in its language's kernel and writes a `#+RESULTS:` block under it in Org's format, as one undo step, honoring `:results value`/`output` and `replace`/`append`/`silent`, `:file` and `:eval no`/`never`/`query`; **Clear Results** in the block removes it. Kernel images (`image/png`, `image/jpeg`) open in an image tab, or for an Org block are saved to `:file` (or `<org-file>-images/<hash>.png`) and linked; an image over 16 MB, 16,384 pixels on a side or 16 megapixels, or not the type it claims, is refused before decoding, and a `:file` outside the Org file's folder is refused. New `jupyter_confirm_evaluate` setting (`"untrusted"` by default, `"always"`, `"never"`); the Org-block confirmation now offers `a`, "always in this workspace", remembered in the session file (new `WorkspaceSession::jupyter_trusted`); a block in a file outside the workspace always asks; a kernelspec inside the workspace (such as its `.venv`) is used only once the workspace is trusted. New `vix_jupyter_core::{babel, image}`, `org::results_append_edit`, `kernelspec::gate_project_dirs` and `vix_jupyter::PrivateDir`. `jupyter` adds 0.88 MiB (+3.5%) to a stripped release `vix`; it stays out of `full` for now (the recommendation is recorded in `spec/jupyter`).
- **Jupyter kernels in the editor, opt-in** (T734c, T734d; [`docs/jupyter`](docs/jupyter/index.md), [`spec/jupyter`](spec/jupyter/index.md)): in a build with the `jupyter` feature (`cargo install vix-tui --features jupyter`; not in `default` or `full`), the new **Run → Jupyter** submenu and `>jupyter` palette entries start a Jupyter kernel from an installed kernelspec (a picker shows each one's name, language and exact command) and run code in it: **Run** (`Ctrl+Shift+Enter`) sends the selection, else the Org source block at the cursor, else the `# %%` cell (the language's comment followed by `%%`), else the current line; **Run and Advance** then moves past it. Output streams into the bottom dock as plain text without ANSI colors, errors with their tracebacks, and the last result's first line shows at the end of the line that ran (**Clear Results** removes it); images are covered by the entry above. **Interrupt**, **Restart** and **Shut Down Kernel**, and a status-bar segment with the kernel's state and execution count. One kernel per language per workspace; the new `[jupyter_kernels]` setting names the kernelspec for a language. Kernels start only on an explicit action, never for a remote workspace, and the first run of each Org block asks first in an untrusted workspace. New `Capability::Jupyter`, `vix_jupyter_core::cell` and `ConfirmKind::JupyterRunBlock`. Org `#+RESULTS:`, images and the trust settings are in the entry above (T734e–T734g).
- **Measured startup and memory vs other editors** (T744; [`docs/performance`](docs/performance/index.md#startup-and-memory-vs-other-editors-t744)): new `scripts/bench-editors` (Python standard library, installs nothing) opens a 10,000-line Rust file and a 100 MB log in Vix and every other terminal editor it finds (Vim, Neovim, Helix, Emacs, micro, nano, mg, Kakoune), each with its configuration disabled and a throwaway `HOME`, in a pseudo-terminal that answers terminal queries, and reports time to first screen, memory of the whole process tree and idle CPU as median and min–max. Measured on an Apple M4 Max: Vix shows the 10k-line file in 41 ms using 34 MiB (behind nano, Vim and Neovim; level with micro; ahead of Emacs and Helix), and the 100 MB file in 99 ms but holding 1.1 GiB, the most of the eight editors. All results are published, including where Vix is behind; the README FAQ quotes them.
- **Jupyter kernel protocol and transport, not yet in the editor** (T734a, T734b; [`spec/jupyter`](spec/jupyter/index.md)): two new library crates for the coming Jupyter kernel REPL. `vix-jupyter-core` is pure: the Jupyter wire protocol (v5.3) with HMAC-SHA256 signing, size caps and constant-time signature checks before any JSON is parsed, typed messages, the connection file, `kernel.json` parsing and Jupyter's kernelspec search path, the output and MIME model, ANSI stripping for tracebacks, and the Org `#+RESULTS:` formatter. `vix-jupyter` finds installed kernelspecs and, behind its `process` feature, launches a kernel with a fresh random key kept only in a `0600` connection file in a private `0700` directory (Unix sockets on Unix, `127.0.0.1` TCP on Windows) and talks to it from a worker thread through the pure-Rust `zeromq` crate: `KernelHandle::{launch, connect, execute, interrupt, restart, shutdown, events}` and `KernelEvent`. Every received message's signature is checked, and failures are dropped. Tested against an in-process fake kernel and, opt-in (`VIX_JUPYTER_PYTHON`), a real ipykernel; a `jupyter_wire` fuzz target covers the codec. New build feature `jupyter`, not in `default` or `full`; no menu or action uses it yet (T734c). New dependencies: `zeromq` and `hmac`.
- **Large File Viewer** (T733; [`crates/vix-large-file/spec`](crates/vix-large-file/spec/index.md)): **File → Open in Large File Viewer…** (`file.open_large`, also in the palette) views a multi-gigabyte file read-only without loading it. It reads only the lines on screen with positioned reads (no memory map, no `unsafe`), builds a sparse line index (every 1,024th line) on a background thread, and goes to a line, a percentage or a byte offset, searches forward and backward through the file in the background with progress and Esc to cancel, copies selected lines (up to 16 MiB), and shows the byte offset, position, line number, and indexing progress. Invalid UTF-8, CRLF endings and very long lines (horizontal scroll) display safely. The T571 size-limit confirmation now offers **v** to view the file instead of loading it, and a file over the 1 GiB buffer ceiling, which used to be refused, now offers the viewer. Local files only. On an Apple M4 Max, a 4 GiB log shows its first screen in 0.2 ms, jumps to its end in 0.1 ms, is fully indexed in 0.5 s and searched end to end in 0.3 s, at under 5 MB peak memory (warm cache; see [`docs/performance`](docs/performance/index.md#large-file-viewer-t733), reproduced by `scripts/bench-large-file`). New crate `vix-large-file`. Editing a large file is out of scope.
- **Install-channel templates, not yet published** (T742, T743; [`packaging/`](packaging/index.md)): a Nix flake (`flake.nix`, for `nix run github:vixide/vix`), `cargo binstall` metadata in `tui/Cargo.toml` matching dist's real release asset names, a container image (`docker/Dockerfile`: a multi-stage build that runs `vix` as a non-root user on the demo workspace, plus a `container.yml` workflow that pushes `ghcr.io/vixide/vix` on a release tag only), winget manifests, a Scoop manifest, an AUR `vix-tui-bin` PKGBUILD and the mise commands. None of these channels is published yet; the getting-started page lists them under "Not yet published / unverified", and `packaging/index.md` gives the steps to verify and submit each one.
- **AI optional** (T730): the new `ai` setting (`"on"` by default) turns AI off with `ai = "off"`, no rebuild needed. The AI menu and every AI command-palette entry are hidden, and every AI request is refused with a translated *"AI is off"* warning: the AI menu's actions reached by a key binding or script, the chat panel, the Git panel's generated commit message and the DB workbench assistant. The shared spawn path refuses as well, so neither the `ai_command` CLI nor an HTTP provider is ever started. Any value other than `"on"` counts as off. Library: `Settings::ai`/`ai_enabled`, `vix_menu::{menus_without_ai, is_ai_action}`, `Menu::ai_hidden`/`Menu::defs`, `App::action_listed`; `vix::ui::{menu_title_x, menu_title_width, menu_dropdown_rect, menu_dropdown_rect_for}` now take the menu list (`app.menu.defs()`) as their first argument.
- **MCP server mode** (T731; [`spec/mcp-server`](spec/mcp-server/index.md)): `vix --mcp [DIR]` serves the Model Context Protocol (2025-06-18) over stdio so an AI agent can read a workspace as Vix sees it, without opening the editor. Six read-only tools — `list_open_buffers` (from the saved session), `read_buffer`, `document_symbols` and `outline` (Vix's offline symbol scan, Markdown headings, Org headlines), `diagnostics` (merge-conflict blocks; no language server in this mode) and `db_schema` (SQLite files in the workspace, opened read-only) — plus the open buffers as `file://` resources. Paths are confined to the workspace root after resolving symbolic links, secret files (`.env`, keys, `.git/`, …) are refused, and the mode writes nothing (it does not even load `config.toml`). New crate `vix-mcp-server`; `vix_session::Session::load_existing` and `vix_db::catalog::read_sqlite_schema`. See `docs/mcp-server/index.md`.
- **LSP status and progress** (T732): language-server `$/progress` tasks (indexing, building) now show in the status bar until the server reports them ended, tracked per server and token. **View → LSP Status…** (`lsp.status`) lists every configured server live — name, state (not started / starting / running / failed), command, root, extensions, open files, in-flight progress, advertised capabilities and last error — and `r` restarts the highlighted one. New `vix_lsp::{ServerStatus, ServerState, ServerError}`, `Lsp::statuses`, `Lsp::progress_line`, `Lsp::restart_language`, and `vix_lsp_core::message::{parse_progress_update, parse_capability_names}`.
- **Worldwide spec** (`spec/worldwide`): Vix targets the whole world, not just the US or UK; standards, protocols, conventions and locales must give equitable capabilities to every country, region and federation.
- **IDE-parity features** (T720–T727; [`spec/ide-parity`](spec/ide-parity/index.md)): **Local History** (a snapshot on every save, labels, diff, restore a file or a range; `local_history_keep` setting; in `vix-undo-store`), **Search Everywhere** (one palette box over files, commands and symbols), a **Refactor…** menu and **Organize Imports** over LSP `codeAction`, **Code Cleanup**, named **run configurations** in `.vix/run.toml` (before-launch steps, compounds; in `vix-tasks`), **Peek Definition**, **CodeLens** (T726: lenses drawn as dimmed virtual rows above their lines in `vix-editor-core`, with soft wrap and folds; refreshed on open, 500 ms after edits, on save and on `workspace/codeLens/refresh`; unresolved lenses resolved when they scroll into view; **Run Code Lens on Line** (`lsp.code_lens_run`) runs the cursor line's lens or offers a choice, sending advertised commands as `workspace/executeCommand` and mapping rust-analyzer's `runSingle`/`showReferences` to Vix's command runner and references list; local **Run test** lenses from `vix-tasks` test-at-point where the server has none; `code_lens` setting and **View → Editor → Code Lens**), and **postfix completion** (`expr.if` + Tab; in `vix-snippets`). Each is on a menu and in the command palette. New crate `vix-choice-list` is the shared pick-one overlay.
- **Todo Tree** (T682): **Tools → Todo Tree** groups the project's `TODO`/`FIXME`/`HACK`/`XXX`/`BUG`/`NOTE` comments under foldable headers, by tag or by file (`g`), and jumps to the one you pick. The pure model is the new `vix-todo-tree` crate; the flat **TODO Finder** shares its scan.
- **Format with your own Prettier** (T683): list languages in the new `prettier_languages` setting (`typescript`, `css`, `markdown`, …) and **Format Document** and format-on-save run `prettier --stdin-filepath` for them, honoring `.prettierrc*` and `.prettierignore`. It uses the project's `node_modules/.bin/prettier` (trusted workspaces only) or one on `PATH`, times out after 10 seconds, and shows a translated hint when Prettier is missing. Vix never bundles Node. See `docs/prettier/index.md`.
- **FHIR client** (T684): **Tools → FHIR Request** sends a FHIR read, search, create, update, delete, history or capabilities request (`Accept: application/fhir+json`) to the server named in `.vix/fhir.json`, with no auth, a bearer token, or a SMART-on-FHIR backend-services client-credentials token, all taken from `FHIR_*` environment variables rather than the file. Responses open as pretty-printed JSON; an `OperationOutcome` is shown as readable issue lines first. **Tools → FHIR Bundle** browses a Bundle one row per entry (Enter opens the resource, `n` follows the `next` link). New crate `vix-fhir`; `vix_editor::Editor::new_tab_with_content_as`. Nothing is logged or written to disk.
- **REST Client** (T680): a `.http` buffer can hold many requests separated by `###`, and **Send HTTP Request** sends the one at the cursor. Adds `@name = value` file variables, `{{$guid}}`/`{{$timestamp}}`/`{{$datetime}}`/`{{$randomInt}}`/`{{$processEnv}}`/`{{$dotenv}}`, named-response chaining (`{{login.response.body.$.token}}`), `?`/`&` query continuation, `< file` bodies (confined to the file's folder), Basic-auth shorthand, and **Tools → Copy HTTP Request as curl**. `vix_http_client` gains `rest` and `send_response`/`Response`.
- **REST Client environments** (T680): `.vix/http-env.json` (a `$shared` section plus named environments; **Tools → HTTP Environment…** picks the active one, remembered per workspace). Variables resolve environment, then the file's `@name`, then `$shared`; put secrets in with `{{$processEnv NAME}}` and keep the file out of git. Also `{{$localDatetime}}`, `{{$aadToken}}` (from the Azure CLI, never cached or logged) and XML chaining with `{{name.response.body.//xpath}}`. New dependency `roxmltree`.
- **Advisory coverage CI job (T607)**: GitHub CI runs `cargo llvm-cov --workspace --cobertura` and uploads `cobertura.xml`, which the coverage gutter can load (see `docs/coverage`).
- **Legacy text encodings** (T661): Vix now opens UTF-16, UTF-8 with a BOM, windows-125x, ISO-8859-2/15, KOI8-R, GBK/gb18030, Big5, Shift_JIS, EUC-JP and EUC-KR files as editable text, shows the encoding in the status bar, and saves each file back in the encoding and BOM it was read in. A guessed encoding is only used when it round-trips the file byte for byte; otherwise the file still opens read-only, as before. **File → Reopen with Encoding…** and **Save with Encoding…** override the guess or convert a file. A character the file's encoding can't hold fails the save with a message naming it, instead of writing a substitute. Replace across files keeps each file's encoding too. New crate `vix-encoding`.
- **Dependencies open read-only** (T662): files under `read_only_paths` (by default the Cargo registry, `~/.rustup`, `/nix/store`, Homebrew's Cellar, `node_modules` and `site-packages`) open locked with a 🔒, so Go to Definition can't lead to an accidental edit; **View → Editor → Read-Only** unlocks one. Files under `ephemeral_paths` (default `**/.git/**`, e.g. `COMMIT_EDITMSG`) are left out of the saved session.
- **HL7 v2 view and MLLP send** (T685): new `vix-hl7` crate and **Tools → HL7 View** / **HL7 Send…**. The view lists the buffer's HL7 v2 (pipe-and-hat) message as numbered segments and fields (`PID-5.1`) using the delimiters `MSH` declares, decodes escape sequences, filters as you type and jumps to the field. Send asks for an explicit `host:port`, sends over plaintext MLLP on a background thread with timeouts, and opens the acknowledgement in a read-only tab. The parser is lenient and never panics; nothing is logged or saved, and sending needs the `http` capability. See `docs/hl7/index.md`.
- **IM menu** (T752, T754, T755): code-aware chat between **Git** and **Org**. A dock panel (**IM → Chat Panel**) lists conversations, shows each message as one line (`time author: text`) and has an input line with slash commands (`/me`, `/join`, `/part`, `/msg`, `/nick`, `/topic`, `/status`, `/clear`; an unknown command is an error, never sent). A local **Notes** conversation works in every build with no network. **Connect…** joins IRC (IRCv3 over TLS) through `[[im.accounts]]` in `config.toml`, with the SASL password read only from `VIX_IM_PASSWORD_<NAME>`. Enter on a `path:line` reference opens it, only inside the workspace. **Share Selection** shows exactly what will be sent and to whom first; **Share Location** sends `path:line:col`. The status bar shows `IM n unread` and `@` for a mention; **Status ▸ Busy** and **Notifications ▸** (`im.notify`, default mentions) decide when a status-line note appears; IM never opens or focuses anything by itself. `history = "disk"` keeps JSONL per conversation under the platform data directory, pruned to `history_days`; **Clear History** deletes it after a confirm. Received text is sanitized (controls, escape sequences, bidi overrides). New `[im]` settings and a new `im` capability feature (in `net` and `full`). Line threads came with T753 (next entry). See `docs/im/index.md`.
- **Threads on lines** (T753): **IM → Comment on Line…** starts a thread on the cursor's line (or the selected lines), or replies to the one there; the line shows `◆` in the gutter (`◇` once resolved). **IM → Threads…** lists every thread in the workspace with the selected one's messages (Enter opens it, `r` resolves or reopens); **Next Thread**, **Previous Thread** and **Resolve or Reopen Thread** are new IM items with palette entries. A thread follows its text as you edit (exact text, then its surrounding lines, else "outdated") and its new line is stored when the file is saved. Threads live one JSON file each in `.vix/im/threads/`, so they can be shared through Git; nothing pops up when one arrives. They need no network and work in every build. The editor's gutter-sign column now picks one sign per line in a fixed order — debugger line, breakpoint, thread, diff/coverage bar — and soft wrap now shows breakpoints and the stopped line too (new `vix_editor_core::editor::CommentMark` and `Editor::set_comment_marks`).
- **OC menu** (T700-T708): manage containers, images, volumes, networks and Podman pods on Docker, Podman or nerdctl through the engine CLI. A panel lists each kind with per-row actions (start, stop, restart, pause, logs to the dock, shell, inspect JSON, run, tag, pull, push, remove, prune), plus Compose Up/Down/Logs and Build Image. Every removing, running or network command shows its exact command first; commands are argument lists, never a shell; `oc_mode = "read-only"` limits OC to viewing. New settings `oc_engine` and `oc_mode`, a new `oc` capability feature, and the new `vix-oci` and `vix-oci-panel` crates. Registry login, Dev Containers and an App-level fake-engine test are not done. See `docs/oc/index.md`.
- **Git graph** (T681): **Git → Log → Graph…** (`git.graph`) opens an interactive commit graph with branch lanes drawn from the parent links, branch and tag decorations, a branch/tag filter (`Tab`), search (just type), and per-commit actions: `Enter` opens the commit as a read-only `git show` tab, `Ctrl+O` checks it out, `Ctrl+B` creates a branch at it, `Ctrl+P` cherry-picks it (checkout and cherry-pick ask first; a failed pick is undone) and `Ctrl+Y` copies its hash. History loads 200 commits at a time. The lane algorithm is the new pure `vix-git-graph` crate, property-tested over random commit DAGs. See `docs/git-graph/index.md`.
- **Cowork: edit together in real time** (T690-T696, `spec/cowork`, opt-in `cowork` build feature): **Tools → Cowork** hosts a session (a single-use, 10-minute invite; the host approves each guest by name and admits it view-only unless it says otherwise), joins one, shares buffers one by one, and shows everyone's cursors and selections in their own contrast-checked color, with their names in the status bar. Edits from any side merge without conflicts (a Loro CRDT, chosen in a measured spike; decision record in `spec/cowork`). Only the host writes to disk: a guest's Save asks the host. Undo and Redo in a shared buffer undo only your own typing. **Follow…** pins your view to another participant until you press a key. **Share File Tree** lets guests browse the project (`.gitignore` honored) and open files themselves, never `.env` files, `.git/`, `.vix/`, keys or credentials, and never a path or symlink that leaves the project. Everything travels Noise-encrypted and authenticated with no unencrypted mode and no server in between. New crates `vix-cowork` (the pure core: document, peers, permissions, paths, wire messages, presence, sessions) and `vix-cowork-net` (the encrypted transport). The wire format reserves room for session chat (T756). Not in the default or `full` build; build with `--features cowork`. Not yet: reconnecting after a dropped connection, content search across the shared tree. See `docs/cowork/index.md`.
- **Cowork: share your terminal, read-only by default** (T697, `spec/cowork`): **Tools → Cowork → Share Terminal** shows the host's integrated terminal to everyone in the session, after a confirmation that warns that everything it shows, including any password, token or key printed in it, will be visible. Guests open it with **Shared Terminal** (`Ctrl+]` hides it) and can only watch. **Allow Terminal Input** (confirmed again) lets guests with the `terminal` level type in it; each new share starts read-only again. The shell never leaves the host: guests receive its rendered screen (then only what changed), carried as five new `Channel::Terminal` messages in the same encrypted Cowork connection. The confirmation dialog now wraps a long message instead of cutting it off, and a terminal opened before the first frame starts at 24 x 80. Not yet: more than one shared terminal, scrollback, or input for one guest only.
- **Cowork session chat, roster and session notes** (T756, T759; `spec/cowork`, `spec/instant-message`; `cowork` build): every Cowork session now has a chat with nothing to set up. A **Session** conversation appears in the IM panel when a session starts and is dropped when it ends (never written to disk history, never opened on its own: a new line raises the unread count). Lines travel inside the session's encrypted connection as a new `Channel::Chat` (`Chat`, `ChatMuted`, tags 48–49; 50–63 stay reserved and are skipped by older and newer peers alike), carry text, `/me` actions, shared locations and snippets, and are signed by the host with the sender's verified identity (`ana #2`). While the session conversation is selected the panel lists every participant and the shared file their cursor is in; `/follow N` follows one. **Tools → Cowork → Session Notes** opens one Markdown buffer shared by the session through the same CRDT sync as a shared file (never saved to disk). The host can **Mute in Chat…** or **Remove Guest…** by number. Library: `vix_cowork::{ChatLine, ChatKind, Channel::Chat, GuestEvent::{Chat, Muted, PeerJoined, PeerLeft}, Handled::chat, session::check_chat, SharedPath::session_notes}`, `HostSession::{chat, set_muted, is_muted, kick}`, `GuestSession::{chat, is_muted}`, `wire::{MAX_CHAT_BYTES, MAX_NAME_BYTES}`, `vix_im::Command::Follow`, `PromptKind::{CoworkMute, CoworkRemove}`; `HostSession::new` now holds the empty notes document and `shared()` leaves it out. Not yet: private messages between two participants, chat history beyond the session, anchored threads over Cowork.
- **Project and per-language settings** (T627): a workspace's `.vix/settings.toml` is merged over your `config.toml` (tables key by key), and a `[languages.<id>]` table in either file sets `tab_width`, `indent_style`, `soft_wrap`, `format_on_save`, `trim_trailing_whitespace` and other per-buffer settings for one language. Later layers win: built-in, user, project, language. The project file applies only once the workspace is trusted, at the same prompt as `.vix/scripts/` (which now also asks for a settings file, in every build), and is never read for a remote workspace. Unknown keys are reported; in-app changes are saved to `config.toml` without copying the project's values into it; saving `.vix/settings.toml` applies it live. `vix-settings` gains `Settings::languages`, `Settings::for_language`, `merge_layer`, `unmerge`, `unknown_keys`, `language_key_problems` and `project_settings_path`; `ScriptTrustPrompt` is `#[non_exhaustive]` with a new `settings` field.
- **Live settings reload** (T626): saving `config.toml` (opened with **Vix → Settings…**) applies it at once, with no restart: the theme, keymap, language and time zone switch and the menus are rebuilt. A file that doesn't parse keeps the current settings and reports the error instead of falling back to defaults. **Vix → Reload Settings** (`vix.reload_settings`) re-reads the file on demand.
- **Copy over SSH and tmux with OSC 52** (T625): a new `clipboard_provider` setting — `"auto"` (default), `"system"` or `"osc52"`. With `auto`, copying over SSH (`SSH_TTY`/`SSH_CONNECTION` set), or when the system clipboard fails, writes the terminal's OSC 52 escape so the clipboard of the machine you are sitting at gets the text; under tmux it is wrapped in DCS passthrough (tmux needs `allow-passthrough on`). Paste under OSC 52 reads Vix's own last copy. `vix-clipboard` gains `Provider`, `Environment`, `select_backend`, `encode_osc52` and `configure`.
- **Completion that keeps up with typing** (T612): the LSP completion popup now narrows as you type (fuzzy-matched on each item's `filterText`, ordered by `sortText`) instead of closing on the next key, and Backspace widens it again; a server's incomplete list is re-requested as you type. Accepting an item replaces the server's `textEdit` range and applies its `additionalTextEdits`, so auto-import works, as one undo step; an item's commit characters accept it. Completion now opens on its own after you type a word character or one of the server's trigger characters (like `.`), after a 100 ms pause; turn that off with the new `lsp_auto_complete = false` setting to keep `Ctrl+Space` only. See `crates/vix-lsp/spec/index.md` § Completion.
- **Snippet completions** (T613): Vix now tells language servers it accepts snippets, so accepting a completion like a function call inserts its parameters as fields — the first is selected, Tab moves to the next and then to the end, Esc stops. The import and range handling from T612 still apply. Snippet files and the Snippets picker gain nested placeholders (`${1:outer ${2:inner}}`) and choices (`${1|one,two|}`).
- **Inline diagnostics** (T614): a language server's diagnostic now shows as dimmed, severity-colored text at the end of its line, truncated at the window edge. The new `inline_diagnostics` setting picks the lines: `"cursor-line"` (default), `"all"` or `"off"`. When the cursor reaches a line with a diagnostic, the status bar shows it (`Error: …`), and **Go → Next/Previous Issue** always shows the message it lands on. For library users, `vix_editor_core::virtual_text` adds end-of-line virtual text that several producers can share.
- **Translations in Rhai plugins** (T735; [`docs/scripting`](docs/scripting/index.md#translating-a-plugin)): a plugin can ship `locales/<code>.toml` string tables, and its scripts read them with the new `tr(key)` / `tr(key, #{ name: value })` functions (`%{name}` placeholders) and `locale()`, so a plugin's command labels and messages appear in the user's language. Lookup follows Vix's own chain: the current locale, then without its region, then English, then the key itself. Changing the language reloads scripts. The `locales/` folder is data only (a `.rhai` file there never runs), it is covered by the plugin's trust digest, and a broken file is reported and skipped. No new capability: `tr` reads only the plugin's own strings. New `vix_script::Catalog`, `Runtime::set_locale`/`locale`/`load_with_catalog`, `DiscoveredScript::locales_dir` and `plugins::LOCALES_DIR`.
- **Rhai plugin installer** (T637): `vix --plugin install <git-url>[@rev]`, `list`, `update [name]` and `remove <name>` install git repositories of `.rhai` scripts into `<config dir>/scripts/<name>/`, pinned by commit in `<config dir>/plugins.lock`. A new plugin is disabled until you enable it in **Vix → Plugins…**; an update that changes its code, or a local edit, disables it again; removing or disabling forgets the trust; and a plugin with a symlink pointing outside its directory is refused. Plugin scripts run in the same sandbox as any other script.
- **Review a branch or commit range** (T665, partly): **Git → Review Range…** (`git.review`) opens `a..b`, a commit or a stash as one read-only multi-file diff, with Next/Previous Review Hunk. **Comment on Review Line…** saves a line comment in `.vix/review/<range>.json`, and **Export Review Comments** writes Markdown to paste into a pull request or an agent prompt. New `vix-review` crate and `vix_git::review_diff`. Stage/discard from the review buffer is not done yet.
- **Choose what the change gutter compares against** (T664): the new `git_gutter_base` setting (`head`, `index`, `disk` or `branch:<name>`) and **Git → Change Gutter Base…** choose the base. `index` stops marking staged hunks (T657), `disk` marks exactly the unsaved edits, and a branch marks the whole difference from it. New public `vix_git::GutterBase` and `vix_git::rev_blob`.
- **Filler rows in side-by-side Compare Files** (T671): the shorter side of each change now gets blank hatched rows, so both panes stay row-aligned below a change. Fillers are not text: the cursor skips them, a click on one lands on the next real line, and scrolling counts them. `vix_editor_core::Editor` gains `set_filler_rows`, `clear_filler_rows`, `filler_rows`, `filler_before` and `display_row`; `vix_diff_view::compare::fillers` computes them. They are drawn in unwrapped views only.
- **Windows on ARM release** (T658): `vix` now ships an `aarch64-pc-windows-msvc` `.zip` and `.msi`, built natively on GitHub's Windows ARM runner. The desktop GUIs stay x86_64-only on Windows.
- **Shebang language detection** (T658): a file with no extension, like `deploy` starting `#!/usr/bin/env bash`, now gets its syntax highlighting and comment prefix from the `#!` line (shell, Python, JavaScript, TypeScript).
- **View → Fold / Unfold** (T658): `editor.fold` folds the innermost range around the cursor, and `editor.unfold` opens it, without toggling (also in the command palette). Repeat Fold to fold outward a level at a time.
- **Open and discard changes from the Git panel** (T657): in **Git → Changes…**, `Enter` opens the selected file's diff against HEAD and `d` discards its unstaged changes after a confirm (staged changes stay; untracked files are refused, never deleted). New **Git → Diff with HEAD** / **Diff with Staged** actions (`git.diff_head`, `git.diff_index`) diff the active file, unsaved edits included.
- **Phosphor White Monochrome** theme: warm P4 white (`[230, 230, 215]`) on black, all one color like the other single-color themes.
- **Editable side-by-side Compare Files** (T660): **Tools → Compare → Compare With File…** and `vix --diff OLD NEW` (so also `git difftool`) now open both files as real, editable tabs side by side. The panes scroll together, the gutter marks each side's changes, **Next/Previous Difference** step through them, and **Copy Change to Left/Right** copy the change at the cursor across as one undo step (like vimdiff `do`/`dp`). Saving a side saves that file. **Show as Unified Diff** keeps the read-only unified view, which an untitled buffer still gets. The shorter side of a change gets filler rows (T671). See `docs/diff-view/index.md`.
- **Pipe the selection through a shell command** (T659): **Edit → Lines → Pipe Through Command…** replaces the selection (or the whole buffer) with a command's output, as one undo step, and **Insert Command Output…** inserts the output at the cursor. Both are in the command palette, and the Emacs keymap binds `M-|`. A command that fails or runs past 10 seconds leaves the buffer unchanged and shows its stderr in the bottom dock. Builds without the `tasks` feature leave both out. See `crates/vix-editor/spec/pipe-selection/index.md`.
- **Linux packages** (T668): every release now carries native `.deb`, `.rpm` and Arch `.pkg.tar.zst` packages of `vix-tui` and the four released desktop GUIs, for x86_64 and aarch64. Install with `sudo apt install ./vix-tui_*_amd64.deb`, `sudo dnf install ./vix-tui-*.x86_64.rpm` or `sudo pacman -U ./vix-tui-*.pkg.tar.zst`. They need glibc 2.35+ (Ubuntu 22.04+, Debian 12+, Fedora 36+). Each package is install-tested on Debian, Ubuntu, Fedora and Arch before upload. See `spec/linux-packages/index.md`. (Landed on 1.8.0's release day: the 1.8.0 GitHub Release's assets already include these packages.)
- **File → Save All** (T654): saves every modified file tab (also in the command palette), reports how many were saved, and stops at the first failure. The `save_all` action used to save only the active tab.
- **Website social cards and theme gallery** (T740, T741): every page of the website now carries Open Graph and Twitter card tags (title, description, URL and image) with a new 1200×630 PNG social card built around a real screenshot of Vix, and the features page has a theme gallery: one real screenshot per bundled theme (all 21) in a carousel with previous/next buttons and a button per theme. The screenshots (`docs/demos/themes/*.png`) and the card (`docs/demos/social-card.png`) come from the new `scripts/render-theme-gallery.py`, which runs the real `vix` release binary in a pseudo-terminal on a copy of the demo workspace, feeds its output to a terminal emulator (pyte) and paints the emulated screen to PNG; it needs no headless browser.
- **`vix-lsp`: `Lsp::take_edit_replies()`** (T652) returns formatting and linked-editing replies as `EditReply`, tied to the document they were requested for and version-checked; `LspEvent::Edits` and `LspEvent::LinkedRanges` are no longer emitted. `vix-editor` gains `Editor::reload_clean_from_disk_checked`, which reports refused files.

### Fixed

- **Large files: ~9× less memory and no idle CPU** (T749; [`docs/performance`](docs/performance/index.md#startup-and-memory-vs-other-editors-t744)): a 100 MB log now holds 126 MiB instead of 1.1 GiB (Vim 117 MiB, Neovim 130 MiB), and with no input Vix uses no CPU instead of ~7.5% of a core. Every frame used to copy the whole buffer to size the horizontal scrollbar; the longest line is now cached until the next edit and measured in place. Opening streams plain UTF-8 straight into the buffer instead of holding a second file-sized copy, and no longer copies the text for persistent undo (unless a saved history exists) or for test code lenses in files with no test syntax. Whole-buffer copies allocate once instead of growing by doubling.
- **No terminal queries at startup** (T749): the image-support probe (kitty graphics query, `ESC [ c`, `ESC [ 16t`, `ESC [ 5 n`) now runs the first time an image is opened, so a terminal that never answers it no longer delays the first screen by ~250 ms. Images given on the command line (`vix photo.png`) now open too; before, the probe ran after them and they were refused.
- **Reference VS Code keymaps** ([`keymaps/`](keymaps/index.md)): the three `keymaps/visual-studio-code-for-{linux,macos,windows}/keybindings.json` files were saved GitHub HTML pages, not keybindings. They now hold VS Code 1.144.0's real default keybindings, downloaded from the raw files of `codebling/vs-code-default-keybindings`, with the source, commit and date recorded in `keymaps/index.md`. These files are reference material only (Vix's VS Code keymap is the hand-written table in `vix-keybindings`), so the editor's behavior is unchanged. A new test, `tui/tests/keymaps_reference.rs`, parses every `keymaps/**/*.json` as JSONC and fails if one is not a keybindings array.
- **Go → Next/Previous Issue lands on the diagnostic** (T614): it used to put the cursor one line above it, so Next Issue could get stuck.
- **Blame with unsaved edits** (T657): Blame Line and inline blame now blame the buffer as shown (`git blame --contents -`), so after inserting or deleting lines they no longer report another line's commit.
- **The single-color themes are all one color**: they no longer highlight code in another theme's blue, orange and green, draw a blue cursor, or show gutter marks, spell-check underlines, diagnostics or the terminal pane in other colors. Each now draws the whole screen in its own color on black, with a reverse-video cursor. See "Single-color themes" in `crates/vix-theme/spec/index.md`.
- **Keybinding reference tables** (T659): a key containing `|` (the Emacs Org `C-c |` binding) no longer breaks its row in `docs/reference/keybindings-emacs.md`.
- **Git from a repository subdirectory** (T650): when Vix is started in a subdirectory of a repository, the diff gutter, hunk list, Stage/Unstage Hunk, the Git panel's Stage Selected, file history and Open File at Revision now use the right file. They could read a same-named file at the repository root, and Stage Hunk could overwrite that file's index entry. JJ actions are offered from a subdirectory of a jj repository too.
- **Git paths are literal** (T651): staging a path such as `app/[id]/page.tsx` no longer also stages `app/i/page.tsx`.
- **LSP formatting edits go to the right file** (T652): formatting and format-on-save edits apply to the file they were requested for, not whichever tab is active when the server replies. Edits computed against text that has since changed are dropped with a status message instead of corrupting the file. Linked-editing ranges get the same check.
- **Editable search results and Replace in Files** (T653): a line that changed after the search is skipped and reported instead of being overwritten. Writes keep CRLF line endings, the final-newline state, indentation and text past the 120-character preview, go through the atomic writer, refuse files open in read-only tabs, and change a file open with unsaved edits in its buffer only, instead of silently saving those edits.
- **Save As asks before overwriting** (T654) an existing file, and refuses a target that is a dangling symlink.
- **Reloading after a branch switch or external change** (T656) applies the same guards as opening a file: a path replaced by a FIFO or device, a file past the size limit, or a binary file is not read; the buffer keeps its text and a warning says why. Non-UTF-8 files reload read-only.
- **The `vix-db` tests no longer touch the OS keyring** (T639). The keyring is now opt-in (`vix_db::secret::use_system_keyring`), which `vix` and the desktop GUIs turn on at startup, so a slow macOS Keychain lookup can't time out a test, and a test can't read a real saved password.

### Security

- **A session save no longer forgets the script-trust answer** (found in T714): saving a workspace's session on exit replaced its whole entry, resetting `scripts_trusted` (T132) to "not asked". `Session::set_workspace` now carries both trust decisions forward.
- **The workspace script-trust prompt needs an explicit `y`** (T655): it no longer accepts Enter, and ignores keys pressed within 250 ms of opening, so an Enter typed ahead of the startup or reload prompt can't approve a cloned repository's `.vix/scripts`.
- **Dependabot alerts cleared.** The website moves from a SvelteKit 3 prerelease to stable 3.1.0 (and adapter-static 4.0.0), fixing two SvelteKit advisories (enhanced-form open redirect; cache-control XSS in prerendered pages). `vix-egui` and `vix-iced` turn off muda's default `gtk3` feature: muda is macOS-only here, but the feature still put GTK 3 and the unsound glib 0.18 (GHSA-wrw7-89jp-8q8g) into `gui/Cargo.lock`.

### Changed

- **Dependency refresh.** Both lockfiles (`Cargo.lock`, `gui/Cargo.lock`) updated to the latest releases compatible with Rust 1.97 (`crc32fast`, `encoding_rs`, `log`, `pulldown-cmark`, `sysinfo`, `unicode-properties`, plus `unicode-script` in the GUI workspace), and `vix-egui` moves to `fontique` 0.12. `evalexpr` (AGPL from 12.0), `vt100` 0.18 and `keyring` 4 (both need call-site migrations) stay where they are.
- **One overlay table orders drawing and keys** (T611, step 1; [`spec/overlay-priority`](spec/overlay-priority/index.md)): every modal layer has one row in `OVERLAYS` (`tui/src/app/overlays.rs`); the frame draws the open rows in table order and a key goes to them in reverse, so the layer drawn on top always takes the key. Where the old key chain disagreed with what was drawn (only with two layers open, for example the calendar or clock under a panel, or a confirmation over the workspace search), the key now follows the picture.
- **The list choosers are one `Chooser` enum** (T611, step 2, first slice; [`spec/overlay-priority`](spec/overlay-priority/index.md)): fifteen `App::*_chooser` `Option` fields (branch, stash, worktree, task, script, plugin, location, capture, refile, recent, encoding, macro, clipboard, workspace, HTTP environment) became one `App::chooser: Option<Chooser>`, so two of them can no longer be open at once; opening one replaces the other. Drawing, keys and mouse behave as before.
- **Open Remote, the remote status and a connection-lost state** (T713; [`docs/remote-editing`](docs/remote-editing/index.md)): **File → Open Remote…** lists recent remote names and takes a new `ssh://…` or `/ssh:…` one, then connects on a background thread (the status bar shows *connecting*) and makes the folder the workspace. It connects with `ssh`'s `BatchMode`, so it never prompts under the UI: a new host key or a locked key fails with a message saying to run `ssh host` once (or `ssh-add`), or to start Vix with `vix ssh://…`, which connects before the UI and can prompt. The status bar shows the remote host. When an operation reports the connection gone, a persistent *connection lost* segment and a warning appear, buffers are kept, saves (and the once-a-second change poll) stop instead of failing one by one, and **File → Reconnect Remote** brings it back. In a remote workspace every save checks the file is still the version the buffer last read or wrote (length and modification time, captured on open, reload and save) and otherwise asks: overwrite, compare (a diff of the file on disk against the buffer), or cancel. Local files keep today's behavior (no check). Library: `App::{adopt_remote, open_remote, poll_remote, remote_host, remote_connection_lost, remote_busy, remote_untrusted, with_ssh_options}`, `vix_editor::editor::{DiskStamp, Flags::CHECK_CONFLICTS}`, `Tab::disk_stamp`, `Editor::save_active_overwriting`, `PromptKind::OpenRemote`, `ConfirmKind::{SaveConflict, TrustRemote}`, `vix_session::{Session::recent_remotes, push_recent_remote, set_remote_trusted, WorkspaceSession::remote_trusted}`; `vix::remote::connect_cli` returns the `SshTarget`.
- **Remote workspace trust** (T714): a remote workspace is untrusted until you say otherwise. Vix asks once per host and path (`y` trusts, `n` keeps restricted mode; keys typed ahead of the prompt are ignored) and remembers the answer in the local session file; **File → Trust Remote Workspace…** asks again. Until trusted, its tasks, project commands, run configurations, test lenses, the debugger, language servers, project scripts and a project-local Prettier don't run, and the status bar says *restricted*. Host keys are `ssh`'s business: Vix never passes `StrictHostKeyChecking` or a password, and a test checks that no secret from the settings reaches `ssh`'s argv.
- **Remote editing over SSH, experimental** (T712; [`docs/remote-editing`](docs/remote-editing/index.md), [`crates/vix-authority/spec/ssh`](crates/vix-authority/spec/ssh/index.md)): `vix ssh://[user@]host[:port]/path` (or TRAMP's `/ssh:host:/path`) works on a folder on another machine with nothing installed there. Vix connects once before the editor starts, so `ssh` can prompt as usual, then shares that connection (`ControlMaster`, with sockets in a private 0700 directory) for every file operation and process; your `ssh_config` applies. Remotely it needs only POSIX `sh` and coreutils (GNU, BusyBox, BSD or macOS `stat`), under any login shell, `fish` and `csh` included. Saves are atomic (temp file and `mv` in the same directory, mode kept, written through links). No dialog, status indicator or connection-lost state yet (T713). Library: `vix_authority::ssh::{SshAuthority, SshOptions, SshTarget, TargetError, quote}` (`ssh` feature for the authority), `Authority::write_atomic_if_unchanged` with `WriteConflict` and `same_version` (an mtime+size check before overwriting; in the conformance suite), and `vix_fileops::{decode_text, encode_like}`.
- **A real-`sshd` test for remote editing** (T719): `scripts/test-ssh-local` starts a throwaway, non-root `sshd` on `127.0.0.1` with generated keys and its own client configuration (host-key checking on, `~/.ssh` untouched) and runs the SSH authority's conformance suite over it; the new GitHub-only `.github/workflows/ssh.yml` runs it when `crates/vix-authority` changes or on demand. See "Testing against a real host" in [`docs/remote-editing`](docs/remote-editing/index.md).
- **Remote search and git** (T715; [`docs/remote-editing`](docs/remote-editing/index.md)): over SSH, Find in Workspace, Search in Workspace (to dock), the TODO finder and Todo Tree, and go-to-definition without a language server run one `rg --json` on the remote machine (or `grep -rnF` for a plain-text query when `rg` isn't there) through the authority, instead of reading every file one round trip at a time. Each reported line is re-checked with Vix's own regex and only indexed files count, so the results, order and counts match a local search; unsaved buffers are still searched as edited. A regular expression without `rg` falls back to reading each file. `vix-git` now finds the repository top level with `--show-cdup` (joined to the workspace path) instead of git's absolute `--show-toplevel`, so it stays a path of the machine the workspace is on, and the SSH authority changes directory with `cd -P`, as a local `chdir` does. Library: `Authority::is_local` (`true` only for `LocalAuthority`), `vix_workspace_search::tool::{ToolQuery, Tool, ToolLine, parse_rg_json, parse_grep}`.
- **Remote language servers, debugger, tasks and terminal** (T716; [`docs/remote-editing`](docs/remote-editing/index.md)): in an SSH workspace, once trusted, language servers and debug adapters run on the remote machine over the `ssh` channel with the remote paths as their `file://` URIs, root and workspace folders, and every reply (definition, references, rename and other workspace edits, call hierarchy, diagnostics, code actions, debugger stops) opens or writes the remote file. Each server or adapter is looked up on the remote `PATH` first, so one that isn't installed there fails to start (and shows as failed in **LSP Status**) instead of crash-looping; it starts in the workspace folder; `initialize` sends `processId: null` (this machine's process id means nothing there); and diagnostics are no longer canonicalized through a round trip on every frame. **Tools → Terminal** opens `ssh -t` over the same shared connection into your remote login shell in the workspace folder (in the remote home folder while the workspace is untrusted); a local workspace's terminal is unchanged. Tasks, project commands, run configurations and **Run Command…** use the shell of the machine they run on, so a Windows client runs `sh` on its POSIX remote. Library: `Authority::shell_is_posix`, `Authority::terminal`, `vix_authority::can_run`, `Dap::set_workdir`; `vix_terminal::Terminal::open_command` takes any `OsStr` program and arguments.
- **Container workspaces and dev containers** (T718; [`docs/remote-editing`](docs/remote-editing/index.md#containers), [`crates/vix-authority/spec/container`](crates/vix-authority/spec/container/index.md)): open a folder inside a running Docker or Podman container, a Kubernetes pod's container or a WSL distribution the way you open one over SSH: `vix docker://web/srv`, `podman://user@ctr/path`, `k8s://namespace/pod/container/path`, `wsl://distro/path`, on the command line or in **File → Open Remote…**. Every file operation, search, git, task, language server and the terminal run inside through the engine's `exec`; saves are atomic and conflict-checked, a stopped container shows as a lost connection, the workspace is untrusted until trusted, and the remote agent (`remote_agent`) works inside too. Docker and Podman receive environment variables by name, never on the command line. **OC → Open Workspace in Container…** (`w` on a running container) and **OC → Reopen in Dev Container** (asks first, then runs `devcontainer up` with the `devcontainer` CLI and opens the container it reports, trusted). Kubernetes and WSL are so far tested against a stand-in only. Library: `SshAuthority` is now `ShellAuthority<SshTransport>` (same API), with `vix_authority::ssh::{ShellAuthority, Transport, SshTransport}`, `ssh::quote::sh_argv`, `vix_authority::container::{ContainerTarget, ExecKind, ContainerAuthority, ExecTransport, ExecOptions}`, `vix_authority::remote::{RemoteTarget, ConnectOptions, Connection}`, `TargetError::BadName`, `vix_oci::devcontainer`, `vix_oci_panel::Request::OpenWorkspace`; `App::adopt_remote` takes any `Into<RemoteTarget>`, new `App::{with_exec_options, with_devcontainer_cli}`, and `vix::remote::connect_cli` returns a `RemoteTarget`.
- **Remote agent** (T717; [`docs/remote-editing`](docs/remote-editing/index.md#the-remote-agent), [`crates/vix-authority/spec/agent`](crates/vix-authority/spec/agent/index.md)): `vix --remote-agent` serves a length-prefixed binary protocol on stdin/stdout (versioned handshake requiring the exact same Vix version, request ids, batches, file watches, a 64 MiB cap on every frame and hostile-input-safe decoding), answering file operations with the local authority on its machine and starting no process. Over SSH, Vix now uses a `vix` of the same version already on the remote machine (its `PATH`, or `~/.cache/vix/agent/<version>/vix`) for every file operation instead of one shell command each, and falls back to the shell commands for anything it can't do. The new `remote_agent` setting: `"installed"` (default; never writes to the remote machine), `"upload"` (also copies this `vix` there when the remote OS and architecture match; the copy's SHA-256 is checked on the remote side before it is made executable, and it is run once to confirm its version, or nothing is installed) or `"off"`. The once-a-second check for files changed on disk is now one round trip for all open files, with or without the agent. Library: `vix_authority::agent::{wire, serve, serve_stdio, AgentAuthority, Watch, VERSION}`, `Authority::metadata_batch`, `vix_authority::ssh::{AgentMode, AgentStatus, same_platform}`, `SshOptions::{agent, agent_binary, with_agent, with_agent_binary}`, `SshAuthority::{start_agent, agent_status, agent_running, upload_agent, watch, options}`; `vix::remote::connect_cli` takes the `remote_agent` setting and also returns a note for the Messages drawer.
- **Background commands' no-prompt environment travels with the command** (T712): `GIT_TERMINAL_PROMPT`, `GCM_INTERACTIVE` and `GIT_SSH_COMMAND` are now part of the process description, so a remote authority sets them on the remote side; the AI `cli` provider always runs on this machine; `vix-git` resolves `git` on the remote `PATH` under a remote authority.
- **IM code references and review threads use the workspace authority** (T710 follow-up): opening a message's code reference and the anchored threads in `.vix/im/threads/` read and write through the authority, so they follow a remote workspace. Thread files are now written atomically.
- **Workspace file access and process spawning go through one authority** (T710, T711; [`crates/vix-authority`](crates/vix-authority/spec/index.md)), the groundwork for remote editing. New crate `vix-authority`: the `Authority` trait, `LocalAuthority` (installed by default, doing exactly what Vix did before), a temp-dir-backed `FakeAuthority` and a shared `conformance` suite. Opening, saving, the explorer, File → Open, the file finder, tasks, Org and Org-roam, workspace files, `.http` environment and body files, and every git, jj, shell, language-server, debug-adapter and Prettier process now use it; configuration, themes, session, undo history, IM, credentials and temp files deliberately stay on the local machine (the spec lists each one and why). No behavior change.
- **Single-color themes renamed**: `Phosphor Green` → `Phosphor Green Monochrome`, `Phosphor Amber` → `Phosphor Amber Monochrome`, `Safelight Red` → `Safelight Red Monochrome`. A saved setting that names an old theme still finds it.
- **Docs are spell-checked** in `scripts/check-docs` (CSpell; required in GitHub and Codeberg CI), and the existing spelling findings are fixed.
- **README and website rewritten for new visitors**, and the website now uses the current Lily design system (PickerBar 0.5 with docs search, theme, language, text size and share pickers; 57 themes from `@lilydesignsystem/themes`).
- **Dependencies updated (2026-10-30).** The MSRV moves to **Rust 1.97** (stable is 1.99; `rust-version`, the GitHub `msrv` job, GitLab's `rust:1.97` image and the Codeberg toolchain move together). Manifest bumps: `getrandom` 0.2 → 0.4 (`getrandom::fill`), `sysinfo` 0.39 → 0.40 (System Information now leaves out facts the platform can't report instead of failing), `etcetera` 0.10 → 0.11, and the compatible `arboard` 3.7, `rhai` 1.27, `rust-embed` 8.13, `tokio` 1.54.1, `tree-sitter-md` 0.5.4; `cargo update` refreshes the root, `gui/` and `fuzz/` lockfiles (among them `ureq` 3.5, `uuid` 1.29, `icu_*` 2.4, `pest` 2.9.5, `cc` 1.10, `wasm-bindgen` 0.2.130). Not taken: `keyring` 4 (a rewrite around separate credential-store crates), `vt100` 0.18 (`NonZeroU16` sizes and `String` screen dumps through the Cowork terminal mirror), `fontique` 0.12 (egui, parley and Slint are all on 0.11) and `evalexpr` 12+ (AGPL). Dependabot no longer ignores `getrandom` or `ureq`. GitHub Actions: `actions/checkout` v7, `actions/upload-artifact` v7, Docker's setup, login and build-and-push actions (v4, v4, v7); dist 0.33.1 (`release.yml` regenerated). Website: Svelte 5.57.3, Vite 8.3.5, svelte-check 4.7.7, `@sveltejs/vite-plugin-svelte` 7.3.1, `@lilydesignsystem/svelte-picker-bar` 0.5.1, pnpm 11.28.9, and its deploy workflow's Pages/pnpm/Node actions; TypeScript stays on 6 (SvelteKit 3.1's tsconfig check rejects 7).

## [1.8.0] - 2026-10-25

### Added

- **Desktop GUIs, experimental** (T641, T642, T646): `gui/` builds two front ends that run the whole of Vix — every panel, overlay and dock — in a native window on macOS, Windows and Linux, so the toolkits can be compared: `vix-gpui` (GPUI Kit) and `vix-egui` (egui/eframe, with IME input), each a complete implementation with no common GUI library. Both have a native menu bar (egui: macOS) with a system open-file dialog, and closing the window runs Vix's own quit flow. Run with `cargo run --manifest-path gui/Cargo.toml --release -p vix-gpui` or `-p vix-egui`. See `gui/spec/index.md`.
- **More desktop GUIs** (T647): `vix-iced` (iced), `vix-gtk4` (GTK 4: Pango text, a native GIO menu bar and file chooser on every platform) and `vix-slint` (Slint, used under the GPLv3, so `vix-slint` is GPL-3.0-only), each a complete, self-contained front end like the first two. Run with `cargo run --manifest-path gui/Cargo.toml --release -p vix-iced` (or `vix-gtk4`, `vix-slint`); see `gui/spec/index.md` for how they compare.
- **Desktop GUI distributions** (T645, T647): releases now ship `vix-gpui`, `vix-egui`, `vix-iced` and `vix-slint` alongside the terminal app — `brew install vixide/tap/vix-gpui` (or `vix-egui`, `vix-iced`, `vix-slint`), shell and PowerShell installers, archives for macOS and Linux (x86_64 and aarch64), and a Windows `.zip` and `.msi`.

### Changed

- **Binary licenses follow each framework and its dependencies** (T648): Vix's source stays under all five licenses, but each distributed binary now declares and ships (`LICENSE` in its package) only those its linked code allows. `vix`, `vix-gpui`, `vix-egui`, `vix-iced` and `vix-gtk4` are `Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only`: GPUI is Apache-2.0 only, and every binary links Apache-2.0-only crates, which the FSF holds incompatible with GPL-2.0-only. `vix-slint` is `GPL-3.0-only` (Slint). `scripts/check-binary-licenses` recomputes this on every CI run. The root `LICENSE` now points at the license texts' real directory, `docs/licenses/`.
- **The terminal UI is now the `vix-tui` package in `tui/`** (T643), a sibling of the desktop GUIs in `gui/`; the repository root is a virtual Cargo workspace. The command is still `vix` and the npm package is still `vix`; the Homebrew formula becomes `vix-tui` (`brew install vixide/tap/vix-tui`, T645), but the crate published to crates.io is now `vix-tui` (the `vix` crate stays at 1.7.1), and release archives and installer scripts are named `vix-tui-*` — the install one-liners become `…/latest/download/vix-tui-installer.sh` (and `.ps1`) from the next release on.
- **Backend-neutral input** (T640): key and mouse handling throughout Vix now uses the new `vix-input` crate's event types instead of crossterm's; only the terminal event loop converts crossterm events. No behavior change in the terminal.
- **No unchecked `unwrap`/`expect` in shipped code** (T603): draw functions now bail out with `let … else` instead of unwrapping an `Option` they just checked, and `clippy::unwrap_used`/`expect_used` are enforced workspace-wide (tests exempt via `clippy.toml`; the remaining provably-safe sites carry `#[expect(.., reason)]`).
- **One lint policy** (T599): `unsafe_code`, `missing_docs` and `clippy::pedantic` now live in `[workspace.lints]`, and every crate and target (tests, benches, examples, build scripts) inherits them; `allow` attributes now need a `reason`.
- **`Editor::set_clipboard` is deprecated** (T649) in `vix-editor-core`: use `Editor::copy_to_clipboard`, which cannot fail and returns nothing. `set_clipboard` keeps its 1.x signature and will be removed at 2.0.
- **Smaller crate**: the published `vix` package no longer includes the project website
  (`vixide.github.io/`), CI definitions or agent settings; none are read by the build or tests.

## [1.7.1] - 2026-10-21

### Changed

- **HTTP client library** (security): `ureq` 2 → 3 for the HTTP client and the AI providers.
  ureq 2 pinned `webpki-roots` 0.26, whose bundled roots trust the distrusted emSign
  intermediates (RUSTSEC-2026-0381); ureq 3 uses the fixed 1.x. Response header names
  in the HTTP client's output are now lower-case, and JSON bodies are sent compact.
- **TLS certificates**: `rustls-webpki` 0.103.16 and `webpki-roots` 1.0.10 (RUSTSEC-2026-0382/0381).
- **tree-sitter 0.27**: updated the syntax-highlighting query code for the `QueryMatch::captures()` accessor.

## [1.7.0] - 2026-10-20

### Added

- **Chaos monkey `--rtl 1`** (T564): runs the random-input driver in Arabic with
  bidi reordering and the mirrored layout forced on; the nightly chaos workflow
  runs it as a second pass.

- **Spawn-site guard** (security): `tui/tests/spawn_sites.rs` fails the build when a
  source file outside a documented allowlist can start a child process, so the
  minimal build's "starts no other program" property cannot regress unnoticed.

- **Right-to-left text order** (T564): after each frame is drawn, stretches of
  Arabic (and other RTL) text are reordered into visual order per UAX #9, with
  numbers and Latin runs kept left-to-right and borders and column gaps held in
  place. New `bidi` setting: `auto` (default; on for a right-to-left UI language),
  `on`, `off` (for terminals that reorder text themselves). New `mirror_layout`
  setting (same values) swaps the docks and the editor's scroll bar to the
  opposite sides, including dock-resize dragging, and mirrors the menu bar (first
  menu rightmost, dropdowns and submenus open leftwards, dock toggles at the left),
  and right-aligns the explorer, menu dropdown, messages and bottom-dock text (indentation
  grows from the right), swaps the status bar's halves, right-aligns the tab strip and the dialogs (the Find box and Calculator move their click
  rectangles with the drawing).

- **Large files ask, binary files open in the byte editor** (T571): a file over
  `large_file_mb` (default 50, `0` never asks) now asks "open it anyway?" instead of
  failing, up to a 1 GiB ceiling; a binary file asks to open in the byte editor, which
  saves the bytes straight back to the file (no lossy text round trip). The same FIFO,
  size and binary guard now covers Insert File, Revert, Compare with File, `--diff` and
  the TODO finder.
- **A terminal pinentry no longer corrupts the screen on commit** (T570): with
  `commit.gpgsign=true` and a curses/tty `pinentry-program`, Git → Commit and Amend show
  a translated hint instead of running a command that would draw over Vix.
- **A gutter-diff benchmark** (T574): `tui/benches/gutter_diff.rs` covers unchanged, lightly
  edited and heavily rewritten buffers up to 40,000 lines, plus the line cap.
- **Scroll bar markers** (T578): the editor's scroll bar track shows colored `┃`
  marks where git changes, diagnostics and search matches are (diagnostics win a
  shared row, then git changes, then search), with the `●` thumb drawn on top.
  Turn them off with `show_scrollbar_markers = false`.
- **A chaos monkey for the real `App`** (T575): `tui/examples/chaos_monkey.rs` feeds
  seeded random key, mouse, paste and resize streams into the app on a
  `TestBackend`, each iteration under a watchdog, and checks that the cursor and
  selections stay inside the text. A panic or hang prints the seed, the Vix frames
  and the last events, and `--seed N --iterations 1` replays it. It runs in a
  sandbox (empty `PATH`, throwaway `HOME`), nightly via
  `.github/workflows/chaos.yml`, with a tiny smoke in `scripts/check`.
- **`theme = "auto"` follows the terminal's light/dark background** (T579): at
  startup Vix asks the terminal for its background (OSC 11, 200 ms limit), then
  `COLORFGBG`, then assumes dark, and applies the new `theme_light` /
  `theme_dark` setting (defaults `light` / `dark`). Asked once per launch, and
  only when `theme` is `"auto"`.
- **The System/File/Text Information panels (Tools menu) are now
  localized** (self-audit, T563): all three panel crates called `t!()`
  zero times — every row label (`Operating System`, `Total RAM`,
  `Characters`, `Size`, `Last modified`, …) was a hardcoded English
  string regardless of the active locale. New `locales/info.yml`
  translates all 45 labels across all 15 locales.
- **Three more small i18n gaps closed** (self-audit, T566/T567/T568):
  the calendar panel's month/year heading (`Tools → Calendar…`) now
  translates the month name instead of always showing English
  (`locales/calendar.yml`); byte sizes (`16.0 KiB`) now use a `,`
  decimal separator for the locales that conventionally write it that
  way (`de`/`es`/`fr`/`pl`/`pt`/`ru`); and Find/Replace's toggle hint
  ("Alt C case / Alt R regex") is translated, matching the sibling
  DB-workbench hint next to it.
- **DB workbench and AI-provider "remember this" credentials now persist
  on Windows too** (found by a codebase self-audit, T548): the OS-keyring
  backend was already implemented for macOS/Linux, but the `keyring`
  crate's Windows Credential Manager support was never turned on — a
  Windows user opting into "remember password"/a saved API key silently
  got prompted every time regardless. Fixed by enabling the feature; the
  same code path macOS already used handles Windows too.
- **8 demo GIFs** (improvement plan T406) under `docs/demos/`, embedded
  (the overview tour) in the README's `## Demos` section: an overview
  tour, the command palette, multi-cursor editing, git hunk staging, the
  DB workbench, Org-roam, the interactive table editor, and syntax
  highlighting — each a real recorded terminal session against
  `tui/examples/demo-workspace/`. Regenerate with `scripts/render-demos.sh`
  (real VHS, needs a headless-capable browser) or the new
  `scripts/render-demos-lite.sh` (no browser at all — a real pty, a
  hand-written asciicast recorder, and `agg` for the GIF encode; see its
  own header and `tasks.md`'s T406 entry for why it exists and what it
  found along the way).
- **`git.stage_hunk`/`git.unstage_hunk`/`org.link.follow`/
  `roam.backlinks`/`roam.graph`/`view.theme_edit` in the Command
  Palette** (found while scripting the demo GIFs above, T406): all six
  had a menu item but no Command Palette entry and (all but the last)
  no keybinding in any keymap — `org.link.follow` in particular carried
  only an Emacs-keymap `C-c C-o` chord, unreachable under every other
  keymap including the default. `>Stage Hunk`, `>Follow Link`, etc. now
  work.
- **Multi-root LSP workspaces** (improvement plan T123f): `initialize` now
  sends every open workspace folder (`workspaceFolders`), not just one
  `rootUri` — a server sees the whole workspace, not only its first
  folder. Adding a folder to the workspace after startup now notifies any
  already-running server that asked to hear about it
  (`workspace/didChangeWorkspaceFolders`). Closes the last open item from
  the T123 LSP audit. See `crates/vix-lsp/spec/index.md`.
- **Multiple LSP servers per file** (improvement plan T123b): a file can
  now be handled by more than one configured server at once — e.g. a
  type-checker plus a separate linter both watching `.rs`. Document sync
  and most per-document requests (hover, go to definition, references,
  completion, code actions, formatting, rename, and more) fan out to
  every matching server; diagnostics from different servers for the same
  file now coexist instead of one clobbering the other. A handful of
  requests that continue one specific server's earlier response
  (completion-item resolve, executing a code action's command,
  call-hierarchy) still target only the first matching server — a
  documented limitation, not a regression, when more than one server
  handles the same file and that specific feature is in use. See
  `crates/vix-lsp/spec/index.md`.
- **LSP semantic tokens** (improvement plan T123a): a server's
  `textDocument/semanticTokens/full` response now feeds the syntax
  highlighting layer alongside (and on top of, where they overlap)
  Tree-sitter highlighting — including for grammars with no bundled
  Tree-sitter query at all, where semantic tokens are now the only
  source of highlighting. Mapped onto the 4 syntax color slots every
  theme already defines (`comment`/`keyword`/`number`/`string`); a token
  type with no mapping is silently skipped rather than shown unstyled or
  erroring. Requested once a server's `initialize` response advertises
  `semanticTokensProvider`, and re-requested whenever the active buffer's
  content changes.
- **LSP pull diagnostics and `$/progress`** (improvement plan T123d):
  opening the Problems panel now also pulls a fresh `workspace/diagnostic`
  report from every running server, so it reflects a server's whole-project
  analysis instead of only files that happened to be opened; a server's
  `$/progress` update (e.g. an initial index build) now shows as a status
  message instead of nothing.
- **LSP server crash recovery** (improvement plan T123c): a crashed
  language server for a file that was already open used to just go
  silently dead until you closed and reopened it. It now respawns
  automatically (up to 3 consecutive attempts since it last stayed up for
  30 seconds — a real crash loop gets 3 tries, then a message instead of
  respawning forever) and replays `didOpen` for every file that was open
  on it, with each file's current content.
- **LSP `prepareRename` and `relatedInformation`** (improvement plan
  T123e): renaming now confirms with the server first — a position it
  says can't be renamed shows a status message instead of opening a
  prompt that would only fail, and a server-supplied placeholder seeds
  the prompt when one is given (falls back to the previous word-under-
  cursor guess for servers with no `prepareRename` support). A
  diagnostic's secondary locations (`relatedInformation`, e.g. "previous
  definition here") now show as their own navigable rows in the Problems
  panel, right after the diagnostic they belong to, instead of being
  silently dropped.
- **Three new AI features** (improvement plan T125), all explicit-invoke
  only: **Edit Selection with Instruction…** (AI menu) prompts for
  free-text and applies it to the selection, always as a reviewable diff
  regardless of `ai_diff_review`; **Generate Doc Comment** (AI menu)
  writes a doc comment for the symbol under the cursor and inserts it just
  above; **Generate Commit Message** (Git panel, `g`) runs the staged diff
  through the AI and pre-fills the commit prompt with the reply — it only
  fills the message box, it never commits on its own. All work on either
  AI path (the CLI default or T124's HTTP providers).
- **Direct HTTP AI providers** (improvement plan T124): a new `vix-ai-core`
  crate adds Anthropic, OpenAI-compatible, and Ollama clients as an
  opt-in alternative to the CLI shell-out every AI feature (the chat
  panel, the AI menu's Summarize/Explain/Define/Annotate/Improve, and the
  DB workbench's assistant) has always used — set `ai_provider` to
  `"anthropic"`/`"openai"`/`"ollama"` (new settings, alongside `ai_endpoint`/
  `ai_model`/`ai_api_key_command`); `"cli"` (unchanged) stays the default,
  so nobody's existing setup changes. The API key resolves through the
  same command-then-keyring waterfall the DB workbench already uses for
  database passwords. See `crates/vix-ai-core/spec/index.md`.
- **TUI snapshot test harness** (`tui/tests/snapshots.rs`, improvement plan T004):
  boots the real `App` against a ratatui `TestBackend`, drives it with
  scripted key events, and compares the rendered frame to a golden text file
  with `insta::assert_snapshot!`. New "Snapshot" test layer documented in
  `spec/test/index.md` (writing + reviewing snapshots) and
  `agents/conventions.md`.
- **9 more TUI snapshot scenarios** (improvement plan T005): welcome screen,
  an opened Rust file, File menu open, find bar with matches, the git
  changes panel, the table editor, the F1 keyboard-shortcut overlay, zen
  mode, and a non-default theme — 12 golden screens total.
- **Benchmark baselines** (improvement plan T006): widened `tui/benches/` to the
  plan's spec'd scenarios — opening/highlighting a ~100 MB and ~5 MB file, a
  10k-operation random insert/delete burst, and workspace search over a
  10k-file tree — and published the numbers in a new
  `docs/performance/index.md`. New `[profile.bench]` so `cargo bench` no
  longer silently inherits the size-optimized, slow-to-link release profile.
- **4 more fuzz targets** (improvement plan T007): `query_replace`
  (`vix-find-panel` search/replace), `tabular_convert` (the shared CSV/TSV/JSON
  core), `structured_convert` (JSON⇄YAML/TOML), `macro_tokens` (`vix-macros`
  token decoding) — 10 targets total. Each ran a full 10 minutes clean, up to
  75M executions; no bugs found in Vix's own code.
- **CI binary-size tracking** (improvement plan T008): GitHub gets a
  `binary-size` job that measures the stripped release binary on every push,
  caches `main`'s size for comparison, and posts a sticky PR comment with the
  size and delta; GitLab logs the same measurement to the job output.
- **`AI_STATEMENT.md`**: how AI-authored work on this project is disclosed
  (commit trailers, session links), what bar it has to clear (the same
  spec-driven workflow and gate as anything else), and what an AI agent is —
  and isn't — pre-authorized to do without asking first: `cargo publish`,
  and (added the same day) judging on its own that a specific release is
  ready to do that for — neither extends to cutting a version-tag release
  or creating a GitHub/GitLab/Codeberg Release. Linked from `README.md`,
  `AGENTS.md`, `docs/index.md`, and `spec/ci/index.md`.
- **Scripting design spec** (improvement plan T101, `crates/vix-script/spec/index.md`):
  Rhai as the engine, script discovery (`~/.config/vix/scripts/*.rhai` and
  project `.vix/scripts/`), the API v1 surface (register command, bind key,
  buffer/selection get/set, prompt, message), error handling, and
  sandboxing. Design-only — the `vix-script` crate itself is a documented
  no-op behind a new default-on `scripting` feature; the engine and host
  wiring land in T102+.
- **Modal-editing design spec** (improvement plan T111, `crates/vix-modal/spec/index.md`):
  audits the current Vi/Spacemacs `vim_normal_key` binding table (only
  `gg`/`dd`/`yy` compose; no counts, registers, Visual mode, dot-repeat, or
  text objects) and designs the v1 replacement — a `Mode` enum, pure
  motion functions, `d`/`c`/`y` composing with any motion/text
  object/Visual selection, counts, unnamed + named a–z registers,
  keystroke-replay dot-repeat, and delimiter-based text objects — with an
  explicit cut line (no ex commands, no macro-via-`q`, no Visual Block, no
  WORD motions). Design-only — the `vix-modal` crate itself is a
  documented no-op, added as a plain workspace dependency; the mode
  engine, motions, operators, and text objects land in T112–T115.
- **`vix-script` engine core** (improvement plan T102): the Rhai engine
  wrapper, `.rhai` script discovery, and the full API v1 surface
  (`register_command`, `bind_key`, buffer/selection/cursor get/set,
  `prompt`, `message`/`error`) — implemented, unit-tested against a mock
  host, and host-agnostic (no `App`, no terminal). Resource limits (10M
  operations, expression/statement depth 64, 1M-char strings, 100k-element
  arrays/maps) catch a runaway script deterministically.
- **`vix-script` host wiring** (improvement plan T103): scripts under
  `Settings::scripts_dir()` and `<workspace root>/.vix/scripts/` now load
  at startup and on a new `script.reload` action; registered commands are
  namespaced `script:<script-stem>:<id>` into the command palette
  (verbatim labels) and listed by a new Tools → Scripts → Run… chooser
  overlay (`script.run`); running one applies its effects to the active
  buffer through the same `set_content`/`paste_text` paths a real edit or
  paste uses (one undo step, blocked on a read-only buffer); a script's
  `prompt()` opens a real single-line prompt and answering it re-invokes
  the handler; `message`/`error` reach the message drawer, and a load or
  runtime error is reported there too, never a crash.
- **Keybinding registry design spec** (improvement plan T104,
  `crates/vix-keybindings/spec/index.md`): T104 set out to wire `vix-
  script`'s already-recorded `bind_key` requests into "the existing
  keymap-model override layer" — an audit found no such layer exists, so
  this designs it instead: a `Binding{key_token, action_id}` schema
  shared by every built-in binding (across all 10 keymap ids) and every
  override (a new persisted `keybindings.toml`, plus scripts), one new
  `on_key` choke point, and conflict handling (two overrides on the same
  key are both rejected; an override shadowing a built-in is allowed but
  reported once). Design-only — the `vix-keybindings` crate itself is a
  documented no-op; the registry and every keymap's conversion land in
  T104a–T104j, one keymap per task.
- **Keybinding registry: Emacs converted** (improvement plan T104a):
  `emacs_key` and its five chord-continuation handlers now dispatch
  through the new `vix-keybindings` registry instead of hardcoded
  `match`es and five ad hoc `EMACS_CTRL_*` consts. Fixed a real,
  pre-existing bug found while converting: the old `EMACS_CTRL_X` const
  (used only for the which-key popup and F1 help, never actual dispatch)
  had drifted from the real `C-x` chord handler — it named a `"buffers"`
  action that didn't exist, and was missing bindings the real handler
  accepted. The Emacs keymap's top-level Ctrl and Meta bindings now also
  appear in the F1 help overlay for the first time (previously only the
  chord tables did). Zero other intended behavior change.
- **Keybinding registry: Vi and Spacemacs converted** (improvement plan
  T104b): `vim_normal_key` (shared by both keymaps) now dispatches
  through `vix-keybindings` instead of its own hardcoded `match`; 6 new
  `vim.*` action ids (`vim.insert`/`append`/`append_end`/
  `insert_line_start`/`open_below`/`open_above`) give the Insert-mode-
  entry keys (`i`/`a`/`A`/`I`/`o`/`O`) real names. Spacemacs's own
  `SPC`-leader is converted too, via a new `lookup_sequence` query for
  leader-style multi-character sequences (its matching algorithm — exact/
  prefix/neither over whole typed sequences — doesn't fit the chord-depth
  shape Emacs's `C-x`/`C-c` families used). Zero intended behavior change.
- **Keybinding registry: VS Code converted** (improvement plan T104c):
  `vscode_ctrl_key` now dispatches through `vix-keybindings` too — one
  shared table for both `vscode-macos` and `vscode-windows`. VS Code's
  bindings also appear in the F1 help overlay for the first time (a new
  `vscode_key_display` renders stacked modifiers, e.g. `Ctrl Shift P`).
  Zero intended behavior change, including for the one real subtlety
  found converting it: `Ctrl+Shift+<letter>` is disambiguated from plain
  `Ctrl+<letter>` via the Shift modifier bit (not letter case, since a
  terminal can report the former as a lowercase letter with the bit set)
  — a new `vscode_ctrl_token` encodes that explicitly rather than relying
  on the shared token encoder's usual "Shift implicit in an uppercase
  char" assumption.
- **Keybinding registry: `IntelliJ` converted** (improvement plan T104d):
  `intellij_key` now dispatches through `vix-keybindings` too — but,
  unlike VS Code, `intellij-macos` and `intellij-windows` are two
  genuinely different tables (the "go to" family alone uses different
  keys per platform). `IntelliJ`'s bindings also appear in the F1 help
  overlay for the first time (the VS Code display helper is now shared,
  renamed `modifier_token_display`). A real, pre-existing bug found and
  fixed along the way: an integration test used the wrong IntelliJ
  keymap ids and had silently been testing the Apple keymap's fallback
  instead. Zero other intended behavior change — including a genuine
  quirk in the original dispatch preserved on purpose: `Ctrl+Shift+N`
  (macOS) and `Ctrl+Shift+G` (Windows) were never actually distinct from
  their plain counterparts, and still aren't.
- **Keybinding registry: Eclipse converted** (improvement plan T104e):
  `eclipse_key` now dispatches through `vix-keybindings` too — one flat
  table, no schema change needed. Eclipse's bindings also appear in the
  F1 help overlay for the first time (folded into the shared
  `modifier_token_display` match arm). The one real subtlety: Eclipse's
  original dispatch has a binding that isn't a `Ctrl` chord at all
  (`Alt+/`, word completion), examined only when `Ctrl` is *not* held —
  so `Ctrl+Alt+/` falls through to plain `Ctrl+/`'s action (toggle
  comment), preserved exactly rather than "fixed." Zero intended behavior
  change.
- **Keybinding registry: Sublime Text converted** (improvement plan
  T104f): `sublime_key` now dispatches through `vix-keybindings` too —
  one flat table, no schema change needed, and no new subtlety beyond
  the by-now-familiar Shift-modifier-bit token encoding VS Code/
  `IntelliJ`/Eclipse already needed. Sublime's bindings also appear in
  the F1 help overlay for the first time. Zero intended behavior change.
- **Keybinding registry: Apple converted, all 10 keymap ids now
  exhaustive** (improvement plan T104g): `apple_ctrl_key` now dispatches
  through `vix-keybindings` too, mixing Shift-guarded and Shift-agnostic
  letters in one table for the first time (the Shift-agnostic ones get an
  explicit duplicate row, same technique `IntelliJ`'s conversion used).
  `global_shared_key` — called identically by every keymap, not itself
  keyed on one — is now backed by a new keymap-agnostic `SHARED` binding
  list for its unconditional bindings (`Ctrl+Space`, `Ctrl+Tab`/
  `BackTab`, `Alt+Left`/`Right`/`j`, the F-keys); the menu-mnemonic lookup
  and 6 focus-gated bindings (extend/shrink selection, move/column-select
  lines, next/prev search selection) stay host-side, since `App::focus`
  is runtime state a static table can't express. The F1 help overlay now
  also shows Apple's bindings and, for the first time, the shared
  bindings every keymap gets regardless of which one is active. Zero
  intended behavior change.
- **Persisted key binding overrides: file format and round trip**
  (improvement plan T104h): a new `Settings::keybindings_path()`
  (`<config dir>/keybindings.toml`, mirroring `macros_path()`) and
  `vix-keybindings::user_bindings` (`UserBinding`, `load`, `upsert`) —
  the `macros.toml` pattern copied verbatim, `upsert` keyed on
  `key_token`. Not wired into anything yet — no dispatch, no conflict
  checking, no `App` changes at all — just the format and the round
  trip; that wiring is T104i/T104j.
- **Key binding overrides now actually apply** (improvement plan T104i):
  `keybindings.toml` entries are resolved against the active keymap's
  built-ins and each other at startup (and via a new Tools → Reload
  Keybindings / `keybindings.reload` action), then take priority over
  every keymap's own dispatch through a single new `on_key` choke point.
  Two overrides claiming the same key are both rejected with an error
  message naming the token and every source; an override that also
  happens to claim a key a built-in binding already owns still wins, but
  is reported once so the built-in's silence isn't a surprise. Script
  `bind_key` requests don't feed into this yet (T104j).
- **Script key bindings now actually fire — the `vix-keybindings` epic
  is complete** (improvement plan T104j): a loaded script's `bind_key`
  requests are resolved into the very same choke point `keybindings.toml`
  overrides use, so a script can genuinely rebind a key, and two
  overrides claiming the same key are rejected together regardless of
  whether they came from a script, `keybindings.toml`, or both.
  `script.reload` now also refreshes key overrides, alongside the
  existing `keybindings.reload`. This is the scripting engine's original
  "wire key bindings into the real keymap" promise from `vix-script`'s
  own spec, finally kept.
- **6 sample scripts + a scripting guide** (improvement plan T105):
  `tui/examples/scripts/*.rhai` — wrap-selection-in-markdown-link,
  insert-file-header, title-case-line, dedupe-selection,
  timestamp-signature, open-scratch-with-template — each verified against
  the real `vix-script` runtime, plus `docs/scripting/index.md`
  documenting the full API. A new `now() -> String` script function
  (today's date) — v1 had no clock at all, and the timestamp sample
  needed one. `docs/keybindings/index.md` also gained an "Overrides"
  section documenting `keybindings.toml`, previously undocumented outside
  the specs.
- **Keybinding editor** (improvement plan T204): **Vix → Keybindings…**
  opens a searchable, sortable table of the active keymap's effective
  bindings, tagged `[user]`/`[script: name]` when overridden. Enter opens
  a prompt to type the replacement key as text and rebind; Delete resets
  a user override back to its default. New `vix-keybinding-editor-panel`
  crate; `vix_keybindings::user_bindings::remove` (the reset primitive,
  alongside the existing `load`/`upsert`).
- **`SECURITY.md`** (improvement plan T131): how to report a vulnerability
  privately, supported-version policy, and what's deliberately out of
  scope (the HTTP client's intentional no-SSRF-guard design, and a known
  `vix-db` streaming-session edge case) — previously only in internal
  notes, now public. Linked from `README.md`/`index.md` and `AGENTS.md`.
- **Script trust prompt** (improvement plan T132): a workspace's
  `.vix/scripts/` scripts no longer run automatically the first time it's
  opened — a one-time prompt asks to trust and run them, remembered per
  workspace. Global scripts (`Settings::scripts_dir()`) are unaffected. A
  declined workspace isn't locked out: `Tools → Scripts → Reload` always
  re-asks.
- **A real action catalog for F1 help and the palette** (improvement plan
  T147): 157 action ids titled nowhere at all (no menu item, no
  `palette::COMMANDS` entry — bare editor primitives like `cursor_down`,
  Org/roam/project/tools actions, …) previously showed their raw id in F1
  help and were entirely unreachable from the command palette's `>`
  Commands mode — now titled and searchable via a new `vix-action-catalog`
  crate, translated into every core locale.
  `vix-keyboard-shortcut-panel::ROWS` shrank from 17 hand-curated rows
  (several of which could show the wrong binding — several keymaps don't
  actually bind `Ctrl P` to the command palette, for instance) to 3
  purely-informational rows nothing else can supply (`F10`/menu mnemonics,
  `F1`, `Mouse`); every real binding now comes from the menu, `SHARED`, or
  the active keymap's own tables.
- **Full i18n coverage for the 14 core locales** (improvement plan
  T148): Spanish, French, German, Welsh, Irish, Scottish Gaelic, Polish,
  Portuguese, Russian, Arabic, Hindi, Bengali, Chinese, and Japanese now
  translate every one of the 2,418 entries in `locales/` (100%, up from
  ~70%) — 711 previously `en`-only menu items, status messages, prompts,
  and help text backfilled, plus two longstanding entries
  (`msg.workspace_unsafe_root`, `ui.db_field_sslmode`) that had been
  showing the *wrong* message's translations under their key, now
  showing their own. `tui/tests/i18n_keys.rs` gained a per-locale coverage
  report and a regression-proof floor for all 14. `locales/app.yml`
  also split into one file per key namespace (`menu.yml`, `status.yml`,
  `ui.yml`, `action.yml`, `cmd.yml`, `msg.yml`, `prompt.yml`, `help.yml`,
  `misc.yml`) so a translation change is a small, readable diff instead
  of a multi-thousand-line one.
- **2 new bundled themes** (improvement plan T203): Catppuccin Mocha, and
  High Contrast — a WCAG AA theme (every text color at least 11:1 against
  its pure-black background, well past the 4.5:1 minimum). Solarized Dark,
  Solarized Light, and Tokyo Night were already bundled; all five now have
  regression tests pinning their color values.
- **Trash on delete** (improvement plan T209): the file explorer's `Delete`
  now moves the selection to the OS trash / Recycle Bin by default,
  undoable from there, instead of removing it outright. New
  `explorer_delete` setting (`"trash"`, the default, or `"hard"` for the
  previous no-undo behavior; an unrecognized value falls back to
  `"trash"`). The confirmation prompt's wording says which will happen.
- **CLI surface** (improvement plan T208): `vix --diff OLD NEW` opens a
  read-only diff of two files directly — the shape a `git difftool`
  driver invokes with (`docs/cli/index.md` has the config snippet);
  `vix -` reads standard input into an unsaved scratch buffer; `vix
  --version --json` prints `{"name","version"}` for tooling. `git
  mergetool` needs no new flag — Vix's existing conflict tool already
  resolves conflict markers in a normally-opened file.
- **Theme editor** (improvement plan T202): **View → Edit Theme…** lists a
  theme's 15 color slots, reuses the existing X11 color picker to choose
  each one's new color, applies every change live, and Save As
  (`Ctrl+S`) writes the result to `~/.config/vix/themes/<name>.json` and
  makes it the active theme. `Esc` without saving reverts to the theme
  that was active before you opened the editor.
- **New Snippet from Selection** (improvement plan T205): **Tools → New
  Snippet from Selection…** captures the active selection and prompts for
  a prefix, saved to `~/.config/vix/global/snippets/snippets.json` as a
  snippet with that prefix as both its name and its expansion prefix (an
  existing prefix is overwritten). No-op with a status message when
  there's no selection. Snippet tabstops (`$1`/`${2:placeholder}`) were
  already fully implemented; T205's scope narrowed to this one
  create-from-selection flow rather than a full editor for existing
  snippets, whose fields still take a JSON-file edit.
- **Coverage gutter** (improvement plan T210): **Tools → Load Coverage
  File…** parses an LCOV or Cobertura XML report (new `vix-coverage`
  crate) and colors the gutter green/red/yellow for covered/uncovered/
  partially-covered lines, reusing the git diff gutter's mark mechanism —
  the two share that one gutter column, so only one shows per buffer at a
  time. **Tools → Toggle Coverage Gutter** shows/hides the loaded report
  without re-parsing. New `coverage_path` setting pre-fills the Load
  Coverage File… prompt. Vix visualizes an existing report; it doesn't
  generate one.
- **Markdown preview scroll-sync + table of contents** (improvement plan
  T206): **Tools → Markdown Preview** now opens scrolled to wherever the
  cursor was in the source buffer instead of always at the top; pressing
  `t` inside the preview opens a table of contents of the document's
  headings (`Enter` jumps the preview there, `Esc` closes just the TOC).
- **Editable workspace search results ("wgrep"-style)** (improvement plan
  T211): **Alt+E** in workspace search turns the hit list into a real,
  editable buffer (one line per hit, `path:line: text`) instead of a
  read-only list. Editing a line and saving (`Ctrl+S`) writes that edit
  back to its source file at the recorded position; deleting a line skips
  that hit. Saving previews a summary ("N edits in M files") before
  writing — the same confirm step search-and-replace already uses. Not
  available for a static results list (go-to-definition, diagnostics, …).
- **Git history browsing** (improvement plan T207): **Git → Log** gains
  three interactive views alongside the existing streamed-to-the-dock ones
  — **Browse Log…** (a commit list, `Enter` opens the highlighted commit's
  diff in a read-only tab), **File History** (the same list scoped to the
  active file via `git log --follow`), and **Open File at Revision…**
  (prompts for a branch/tag/commit and opens the active file's content
  there, read-only, titled `file @ abbrev`). All three are read-only —
  none of them write anything.
- **Structural search & replace** (improvement plan T201): **Edit →
  Structural Replace…** matches by structure instead of regex — a pattern
  like `if $COND { $$BODY }` matches any `if` statement regardless of
  formatting, with `$X` capturing one token (or a whole bracketed group)
  and `$$X` capturing a run of tokens (a whole argument list, a whole
  block's body). Step through matches exactly like interactive
  query-replace (`y`/`n`/`!`/`q`), scoped to the active selection when one
  exists. **Edit → Structural Replace in Workspace…** does the same search
  across every file and previews a summary before writing, reusing the
  same confirm step workspace search-and-replace already uses. New
  `vix-structural-replace` crate: a token/bracket-based matcher, not
  tree-sitter (see its spec for why).
- **Docs coverage audit** (improvement plan T302): `scripts/docs-coverage`
  lists crates with no detected `docs/` coverage into `docs/coverage.md`
  (currently 38), for the next docs passes (T303/T304) to work through.
  Dev tooling only — no user-facing change.
- **mdBook documentation site** (improvement plan T301): `book.toml` +
  `docs/SUMMARY.md` organize the existing `docs/` pages (no files moved)
  into Getting Started / Guides / Features / Reference / Contributing.
  New GitHub CI jobs build the book on every push and deploy it to
  GitHub Pages from `main` (enabling Pages itself is a one-time repo
  setting, done separately from this change).
- **Generated reference docs** (improvement plan T305): `cargo run
  --example list_commands -- --write` (re)generates
  `docs/reference/actions.md` (every dispatchable action id, resolved
  title, and source), `docs/reference/settings.md` (every `Settings`
  field: key, type, default, description), and one
  `docs/reference/keybindings-<keymap>.md` per keymap plus a shared-
  bindings page — all read straight from the real menu/palette/catalog/
  settings/keybindings data, so they can't drift. All three forges (and
  `scripts/check`) now regenerate and `git diff --exit-code` these pages
  as part of the gate.
- **Migration guides and comparison matrix** (improvement plan T308): new
  `docs/for-helix-users/index.md`, honest about what genuinely differs
  (Vix has no Helix-style selection-first modal). Corrected two real
  inaccuracies in the existing `docs/for-visual-studio-code-users/index.md`
  (a claimed `Alt+Up`/`Alt+Down` move-line binding that doesn't exist;
  `Ctrl+Shift+K` cuts a line rather than deleting it) and added the
  honest gaps it was missing (no `Ctrl+D` incremental multi-select, no
  `F5` one-key debug start). `docs/comparison/index.md` rewritten into a
  13-row feature-parity matrix against Vim, Helix, Micro, and Zed.
- **Man page** (improvement plan T307): `man vix` works wherever Vix was
  installed via a package manager. Generated from the real CLI flag
  definitions (`cargo run --example generate_man`, `clap_mangen`), so it
  can't drift from `--help`; bundled into every release archive and
  installer.
- **Getting-started guide** (improvement plan T306): new
  `docs/getting-started/index.md` — every real install path (Homebrew,
  the shell/PowerShell installer scripts, npm, direct GitHub Release
  downloads, and building from source; none of the first four had any
  end-user-facing documentation before this), first launch, and "the 10
  things to learn first." Linked from `index.md`'s top.
- **Docs coverage filled** (improvement plan T303/T304): all 38 crates
  `docs-coverage` (T302) found lacking a docs page now have one —
  `scripts/docs-coverage` reports zero gaps. New pages: Surround
  (`docs/affix`), Align, Clipboard, Clock, Convert (the whole
  Tools → Convert family), Emmet, HTML Character Picker, HTTP Client,
  Matching Tag (`docs/tags`), Persistent Undo (`docs/undo-store`), Roam,
  Tools (the small single-purpose Tools-menu utilities), Welcome Panel,
  and X11 Color Picker. `docs/calendar-panel/index.md` was also
  corrected — it had gone stale describing date/time strings that moved
  to the new Clock box.
- **Demo workspace** (improvement plan T501): `tui/examples/demo-workspace/`,
  a small realistic project (Rust + Python + Markdown sources with real
  TODO/FIXME bugs, `tasks.toml`, an `.http` file against httpbin.org,
  three linked Org-roam notes plus a dailies entry, `data/*.csv|tsv`, and
  a seeded `demo.sqlite` with its seed SQL and a regenerate script) for
  the written tutorials and VHS demo tapes (T404–T406) to open and run
  against. Excluded from the workspace build via a root `Cargo.toml`
  entry; ~76 KB total.
- **`vix-tutor` spec** (improvement plan T401): the design for an
  interactive, in-editor tutorial — `vimtutor`-style, but each chapter's
  lesson is a real, freely-editable Vix buffer, with cheap textual
  progress checks against what the learner typed rather than a scripted
  walkthrough. Launches via `vix --tutor` or **Help → Tutorial** (both
  land in T402). Design-only for now, matching `vix-modal`'s T111
  precedent: an empty crate plus this spec, no functional code yet.
- **`vix-tutor` engine + chapter 1** (improvement plan T402): `vix --tutor`
  and **Help → Tutorial** now really work. Every chapter's starter text is
  copied into a fresh `vix-tutor-<pid>` working directory and opened as an
  ordinary, freely-editable tab; **Tutorial: Next/Previous/Restart
  Chapter** navigate and reset it. Chapter 1, "Moving Around," has three
  text-only progress checks (deliberately not cursor-position checks — see
  the spec for why) that re-run after every keystroke and show live as
  `N/3` in the status bar. Chapters 2–6 are T403.
- **`vix-tutor` chapters 2–6** (improvement plan T403): Editing Basics,
  Find & Replace, Multi-Cursor & Selection, Files/Tabs/Palette, and Git
  Basics — all six chapters in `tasks.md`'s T402/T403 list are real now.
  Found and fixed a real class of bug along the way: a check written as a
  bare `!text.contains("word")` can never pass when that chapter's own
  instructions use the same word (they usually do, describing the task) —
  every check now matches an exact original line/block, or checks a
  specific line's start/end, instead (see the spec's new note on this).
- **Written tutorials, all ten** (improvement plan T404/T405): new
  `docs/tutorials/01`–`10`, a full guided series — Your First Session,
  Editing Power Techniques, Find & Replace and Multi-Cursor, The Git
  Workflow, Setting Up LSP, Org Mode & Roam, The DB Workbench, HTTP
  Client & Tools Suite, Make Vix Yours, and Debugging with DAP — each
  grounded in the real crate specs/menu source and worked against the new
  demo workspace's real files (T501). Written independently by ten
  parallel passes, each required to verify every command/menu
  path/keybinding against the real repo rather than assume; between them
  they found and fixed nine small pre-existing doc/spec drifts along the
  way (stale menu paths in `docs/menus/index.md`, `docs/git-panel/index.md`,
  `crates/vix-case/spec/index.md`, `crates/vix-theme-editor-panel/spec/
  index.md`, and this repo's own new `tui/examples/demo-workspace/README.md`;
  a wrong default in `docs/language-server-protocol/index.md`; a
  self-contradicting "not yet ported" paragraph in `crates/vix-db/spec/
  index.md`; a stale single-prompt description of Org Capture in
  `docs/org/index.md`; and a stale "Base16…" submenu claim in
  `docs/themes/index.md`). New `# Tutorials` section in `docs/SUMMARY.md`.
  Also fixes a real, reproducible test flake found in the process: every
  `vix --tutor` session shared one `vix-tutor-<pid>` temp directory, since
  `cargo test` runs every test as a thread within one process (identical
  `std::process::id()` across all of them) — now disambiguated with a
  per-session counter, `vix-tutor-<pid>-<n>`.
- **VHS demo tapes, partial** (improvement plan T406): new
  `docs/demos/*.tape` — 8 [VHS](https://github.com/charmbracelet/vhs)
  scripts (overview tour, command palette, multi-cursor, git hunk
  staging, the DB workbench, Org-roam, the table editor, themes), every
  command/menu path grounded against the real repo and confirmed with
  `vhs validate`, plus `scripts/render-demos.sh` to regenerate the GIFs
  and a new README `## Demos` section. The GIFs themselves are not
  rendered/committed yet — VHS's screenshot-based capture needs a working
  headless Chrome/Chromium, which is not available in every environment;
  see the tape files and `scripts/render-demos.sh` for what's needed to
  finish this once run somewhere that has one.
- **4 library examples, Cargo examples batch 1** (improvement plan T502):
  `render_frame` (a real frame rendered to a ratatui `TestBackend`,
  printed as plain text — the same technique `tui/tests/snapshots.rs` uses),
  `theme_roundtrip` (load a bundled theme, tweak a color, save, reload,
  and confirm the tweak survived), `textops_pipeline` (sort, dedupe, and
  uppercase a real file's lines from the CLI, chaining a real `Editor`
  method pipeline into a pure `vix_case` function), and `macro_replay`
  (parse a `macros.toml`, decode its tokens, and replay them through a
  real `App::on_key` — the exact path the real macro player uses, not a
  separate one). Each ≤ 60 lines; listed in the README `## Examples`
  section.
- **6 more library examples, Cargo examples batch 2** (improvement plan
  T503): `query_search` (searches a directory of files for a pattern via
  `App::search_workspace_to_dock`, the same engine the real UI uses),
  `org_export` (Org → Markdown/HTML via `vix_org::to_markdown`/`to_html`),
  `vcard_parse` (parses Org-contacts, exports to vCard 3.0),
  `lsp_headless` (spawns a language server — a tiny mock, reusing
  `tui/tests/lsp_smoke.rs`'s technique — opens a document, prints its
  diagnostics), `i18n_lookup` (one translation key resolved against all
  15 locales via `t!(key, locale = ...)`), and `calculator_eval`
  (evaluates expressions with the same engine Tools → Calculator uses).
  Found and corrected two real drifts from this task's own description
  in `tasks.md`: `vix-query` (folded into `tui/src/app.rs` by T151) was never
  a directory-search tool — it's the query-*replace* confirm-choice enum
  — so `query_search` targets the real search engine instead; and
  `vix-org-contacts` parses *Org* contacts and exports *to* vCard, with
  no reverse direction, so `vcard_parse` demonstrates that real direction
  rather than a "vCard parser" that doesn't exist. Listed in the README
  `## Examples` section alongside T502's four.
- **Config examples** (improvement plan T504): new `tui/examples/config/` —
  a fully-annotated `config.toml` covering all 63 settings (each with
  its real default value and its own doc comment, cross-checked against
  `docs/reference/settings.md`), a custom `theme.json`, a
  `snippets.json`, and a `macros.toml`. Every file is verified by
  actually round-tripping it through the real loader
  (`Settings::load_from`, `CustomTheme` via `serde_json`,
  `snippets::parse_json`, `vix_macros::load`/`decode`), not just
  hand-typed. Caught a real TOML gotcha while verifying `config.toml`:
  a `[[array-of-tables]]` section silently reparents any scalar `key =
  value` line that follows it into the array item instead of the root
  document, so all three `Vec<Struct>` settings (`lsp_servers`,
  `debug_adapters`, `org_capture_templates`) have to come after every
  plain key, not interleaved in `docs/reference/settings.md`'s own
  field order.
- **Examples run in CI** (improvement plan T505, closes Run F): all
  three forges (`.github/workflows/ci.yml`, `.gitlab-ci.yml`,
  `.forgejo/workflows/ci.yml`) and `scripts/check` now actually *run*
  five headless examples (`list_commands`, `headless_edit`,
  `render_frame`, `textops_pipeline`, `query_search`) after `cargo
  test`, not just compile them (`cargo build --workspace --all-targets`
  already did that). Found and fixed a real pre-existing drift while
  touching it: `spec/ci/index.md`'s gate list said "seven" steps but
  `scripts/check` already ran eight — T307's man-page-current? check
  had never been added to the spec. Now nine, with this task's own
  addition.
- **Modal-editing engine, first slice** (improvement plan T112): new
  `vix_modal::Mode` (Normal/Insert/Visual/Visual Line) and a new
  `Settings::modal_engine` flag (default off). With the flag on, the Vi and
  Spacemacs keymaps gain real `v`/`V` Visual/Visual Line entry, `h j k l`/
  arrow-key selection extending, and `Esc` to return to Normal, shown live
  in the status bar — reusing the editor's existing native shift-selection
  mechanism rather than hand-tracking an anchor. Everything else keeps
  running through the existing `vim_normal_key` table unchanged; T113–T115
  progressively move more of it onto the new engine. See
  `crates/vix-modal/spec/index.md` for the full v1 design.
- **Modal-editing engine, motions + counts** (improvement plan T113): with
  `Settings::modal_engine` on, Normal mode in the Vi/Spacemacs keymaps now
  has real `h j k l w b e 0 ^ $ gg G { } ( ) f t F T` motions and a numeric
  count prefix (`3w`, `5j`, `2gg`, …) — the old table had neither (no counts
  at all, `0`/`^` conflated into one toggle, `h`/`l` crossing line
  boundaries). `d`/`c`/`y`/`x`/`p` are unaffected, still the old table's
  (T114's job).
- **Modal-editing engine, operators + registers** (improvement plan T114):
  with `Settings::modal_engine` on, `d`/`c`/`y` now compose with any T113
  motion (`dw`, `de`, `d}`, `dgg`, `df.`, …), `x` is real sugar for `d` +
  one right motion, `dd`/`cc`/`yy` act on the whole current line, and
  `p`/`P` paste a register — inline or as a whole line depending on how it
  was written. `"{a-z}` selects a named register for the next operator or
  paste; otherwise it's the unnamed register, still the same
  `vix_clipboard` every other keymap's Cut/Copy/Paste already uses. `2d3w`
  deletes 6 words — a count before the operator and before the motion
  multiply.
- **Modal-editing engine, text objects + dot-repeat, on by default**
  (improvement plan T115, closing the T112–T115 arc): `i`/`a` + `w`, a
  bracket pair (`(`/`)`/`b`, `{`/`}`/`B`, `[`/`]`, `<`/`>`), or a quote
  (`"`/`'`/`` ` ``) now compose with `d`/`c`/`y` (`diw`, `daw`, `di(`,
  `da"`, …). `.` repeats the last real change — `d{motion}`, a text object,
  or `p`/`P` — with `{count}.` overriding its recorded count.
  **`Settings::modal_engine` is now on by default** for the Vi and
  Spacemacs keymaps (previously opt-in while the engine was still landing
  in slices); turn it off in Settings to fall back to the original binding
  table. See `docs/for-vim-users/index.md`'s updated "Where Vim still wins"
  for exactly what's still not implemented (Visual Block, WORD motions,
  `cw`'s "acts like `ce`" quirk, and a few others).

### Changed

- **Minimal release workflow** can be run by hand (`workflow_dispatch`) as a dry run: it
  builds and packages the archives but attaches nothing unless run for a tag.

- **YAML library** (security): the deprecated `serde_yaml` (and its unmaintained
  `unsafe-libyaml`) is replaced by the maintained drop-in `serde_norway`, so the
  `RUSTSEC-2024-0449` ignore in `deny.toml` is gone.


- **Bool fields on `Settings` and `App` are grouped internally**
  (improvement plan T149, now fully done — 7/7 structs): the 32 on/off
  preferences on the `vix-settings` `Settings` struct now live in 11
  small per-topic sub-structs (`settings.gutter.line_numbers`,
  `settings.save.format_on_save`, …) instead of directly on `Settings`.
  **Your `config.toml` is unaffected** — every setting is still the same
  flat top-level key with the same name and default, and a config
  written by any earlier version loads identically (now covered by
  tests). Separately, all of `App`'s own bools are now typed: 15
  UI-surface-visibility bools (`show_explorer`, `show_bottom_dock`,
  `pomodoro_open`, …) are one `App::visible: Visible` bitset; 6
  mutually-exclusive Emacs-keymap chord-prefix bools are one
  `App::emacs_chord: EmacsChord` enum; recording/replaying a keyboard
  macro is one `App::macro_state: MacroState` enum; a pending
  `gg`/register-select in the modal engine is one
  `App::modal_pending: ModalPending` enum; and the last 14
  single-purpose, mutually-independent toggles (`clip_cut`, `overwrite`,
  `suspend_requested`, `git_repo`, `spellcheck`, `calendar_dailies`,
  `should_quit`, …) are one `App::flags: AppFlags` bitset. Only Rust
  code that reads these fields directly (embedders, scripts built
  against the crate) sees the new shapes (`settings.<group>.<field>`,
  `app.visible.contains(Visible::…)`, `app.emacs_chord == EmacsChord::…`,
  `app.macro_state == MacroState::…`,
  `app.modal_pending == ModalPending::…`,
  `app.flags.contains(AppFlags::…)`). See
  `crates/vix-settings/spec/index.md`.
- **Opening a large file no longer blocks on a synchronous parse**
  (improvement plan T121): buffers at or above the existing 50 KB
  async-reparse threshold now route their *initial* Tree-sitter parse
  through the same background worker a post-edit reparse already used,
  instead of parsing synchronously inside `Editor::new`/`Code::new`.
  Opening a 100 MB file dropped from 5.05 s to 14.1 ms
  (`docs/performance/index.md`) — the file is simply plain, unhighlighted
  text for the one or two frames until the background parse lands, exactly
  like the brief window during a large-file edit already worked. Two real,
  previously-latent bugs an edit made *during* that window would have hit
  are fixed as part of this: the pending-parse flag couldn't distinguish
  "generation 0 requested" from "nothing requested yet", and `insert`/
  `remove` would have silently dropped such an edit from the tree
  machinery entirely (see `crates/vix-editor-core/spec/syntax-highlighting/
  index.md` for the detail).
- **Startup no longer waits on `git` before showing the editor**
  (improvement plan T122): measuring cold start (opening this repository
  itself) found `refresh_git` — three `git` subprocesses (repo?/branch/
  status) — was 75–82 ms of it, all spent before the terminal even took
  over the screen; every other startup step was already sub-millisecond.
  `main` now draws the first frame before calling `refresh_git` rather
  than after, so the editor appears immediately and the branch/status
  indicator fills in one frame later instead of blocking everything else.
  Also fixed in passing: `App::new` scanned the custom-themes directory
  twice; now once.

- **LSP depth audit against LSP 3.17** (improvement plan T123): a full
  method-by-method diff found `vix-lsp`/`vix-lsp-core` already implement
  ~28 request/notification methods end-to-end (the spec's own Features
  table had gone stale and listed only 4); document/range formatting, one
  of the task's own suspected gaps, turned out to already be fully wired.
  `crates/vix-lsp/spec/index.md` rewritten and fact-checked against real
  action-id strings, with a new "Known gaps against LSP 3.17" section
  covering eight real, larger gaps filed as follow-up tasks (T123a–T123f)
  rather than implemented now: semantic tokens, one server per
  `language_id` instead of per-buffer, no crash recovery for an
  already-open buffer, pull-based `workspace/diagnostic`, `$/progress`,
  `prepareRename`, `relatedInformation` parsing, and multi-root workspace
  propagation.
- `vix-script` is now a **plain, non-optional** dependency of the root
  `vix` package — T101's `scripting` Cargo feature (`dep:vix-script`,
  default-on) is removed now that T103 has wired it into the App shell's
  core (struct fields, startup, palette, menu, prompt system), the same
  call T111 made for `vix-modal`.
- `deny.toml`: added a dated advisory ignore for RUSTSEC-2026-0249
  (`smartstring`, an unmaintained but mandatory transitive dependency of
  the new `rhai` dependency), matching the existing `paste`/
  RUSTSEC-2024-0436 precedent.

### Fixed

- **Crash on a one-row terminal** (chaos monkey, seed 777215): the editor widget painted
  into a rect that overhung the screen buffer and indexed outside it. It now clips its
  area to the buffer first.

- **Counted messages are grammatical in Arabic, Russian and Polish** (T565): 76
  strings that put a number before a noun (`%{n} wierszy`, `%{n} عنصر`), correct for
  only some counts, now read `Label: N` in those three locales (Rows: 3). A new test
  keeps every counted key that way. Welsh, Irish and Scottish Gaelic still use the
  single form.
- **Crashes the chaos monkey found** (T575): the AI, diff, replace-confirm and
  chooser popups panicked in a terminal narrower than 30 columns or shorter than
  8 rows (`clamp` with `min > max`), as did LSP popups, the About/confirm dialog,
  a drag in a view narrower than its line-number gutter, and the editor's scroll
  step in a very small view. Replace All (and any whole-buffer replacement) to a
  shorter text left the selection and extra carets pointing past the end, which
  crashed the status bar or the next keystroke; multi-cursor typing with a caret
  inside another caret's selection applied an edit at a removed offset.
- **Stage/unstage failures in the Git panel showed a generic "Git
  command failed" with no reason** (found by a codebase self-audit,
  T540): now shows git's own message, matching how a stash/amend
  failure was already reported.
- **A genuine settings-file syntax error (not a missing file — that's
  fine) used to silently reset everything to defaults, with zero
  notification** (found by a codebase self-audit, T537): a real parse
  failure in an existing `config.toml` is now reported once the editor
  starts, instead of quietly losing the theme, keybindings, and every
  other setting.
- **A rejected debugger request (an invalid breakpoint, a step that
  isn't valid in the current state, …) used to just silently do
  nothing** (found by a codebase self-audit, T536): the status line
  now reports the adapter's own reason, mirroring how a failed language
  server request is already reported.
- **The JSON/YAML editor's "not valid JSON or YAML" message now says
  what's actually wrong and where** (found by a codebase self-audit,
  T543): it used to be one static message regardless of the real
  parse failure; now it includes the parser's own line/column detail.
- **Several settings/state saves that happen automatically after an
  action — adding/removing an agenda file, saving a theme selection,
  the command-palette recent-commands list, DB connection/query-history
  persistence, opening the settings file for editing — silently
  discarded a write failure**, unconditionally showing success (found
  by a codebase self-audit, T545): the in-memory change still takes
  effect (nothing is lost this session), but a failure to persist it is
  now reported instead of hidden.
- **Error messages for save/open/revert/rename/delete now name the
  specific file** (found by a codebase self-audit, T539): all five used
  to show only the raw underlying error, e.g. "Save failed: Permission
  denied" with no indication of which file — now "Save failed for
  ~/notes.md: Permission denied." Reverting a file also used to show
  "Open failed," the wrong verb for what actually happened.
- **Following a web/mail link from Org, or copying a table column's sum
  (`C-c +`), could claim "Copied to the clipboard" even when nothing was
  actually copied** (found by a codebase self-audit, T542): both now
  route through the same clipboard path (real clipboard, falling back to
  an in-memory one) the rest of the app already uses, so the claim is
  genuinely true.
- **Spell-check silently doing nothing gave no indication why** (found
  by a codebase self-audit, T541): a missing/corrupt dictionary, or a
  permissions problem reading one, all looked identical to the user —
  zero feedback. Pressing Ctrl+; (spell-suggest) now reports the
  specific reason.
- **A session-save failure on exit (or when switching workspaces) was
  silently discarded**, unlike the adjacent settings-save failure, which
  was already reported (found by a codebase self-audit, T544) — both
  are now reported, and (since exit-time messages can no longer render
  in the already-closed TUI) also printed to stderr so they survive in
  the terminal's scrollback.
- **Org-node: inserting a `#+transclude:` directive for a new node could
  silently fail to create the node's file** (found by a codebase
  self-audit, T518): `node_insert_transclusion` was missing the
  `create_dir_all` call its sibling `roam_insert_link` has, a copy-paste
  drift — it now creates the node's directory first, exactly like every
  other way of creating an Org-roam node.
- **A save that failed both its atomic write and its in-place fallback
  could report the wrong, stale error** (found by a codebase self-audit,
  T535): the reported error always named the *first* (atomic) attempt's
  failure even when the *second* (fallback) attempt failed for an
  unrelated reason — e.g. showing "permission denied creating temp file"
  when the real, final problem was "disk full." The fallback's own error
  is now what's reported (with the atomic one kept alongside, not lost).
- **Editing a git-tracked file, or scrolling with sticky-scroll/breadcrumbs/
  the minimap on, recomputed real work every single frame instead of only
  when the buffer actually changed** (found by a codebase self-audit,
  T510–T512): the diff gutter re-ran a full Myers diff against HEAD on
  every redraw, the sticky-scroll header and breadcrumb bar re-scanned the
  whole buffer for declarations (and recompiled the same fixed regex to do
  it) on every frame the file was scrolled, and the minimap cloned every
  line of the buffer into a fresh `String` on every frame it was visible.
  All three are now cached and skip the recompute when nothing relevant
  changed since the last frame. Most noticeable on a large file.
- **A handful of UI labels (the Theme Editor's title and all 15 color-slot
  names, Structural Replace's two menu entries) could render as their raw
  internal i18n key instead of translated text** (found scripting the
  T406 demo GIFs): a real LLVM fat-LTO codegen bug in `vix-i18n`'s
  translation-table initializer (one function doing ~2500 sequential
  `HashMap` inserts per locale — exactly the shape most likely to trip
  it), triggered only by `[profile.release]`'s `lto = true`, never by
  `cargo test` or a debug build. Switched to `lto = "thin"` (a lighter
  LTO mode, and what `[profile.dist]` — the profile actual releases
  build with — already used, so shipped releases were very likely never
  affected). See `Cargo.toml`'s own comment on `[profile.release]` and
  `tasks.md`'s T406 entry for the full investigation.
- **Switching keymaps (Vix → View → Keymap) could leave a pending Emacs
  chord armed** (found during improvement plan T149's `App` bool-to-enum
  pass): resetting per-keymap state on a keymap switch only ever cleared
  a pending `Ctrl+X` prefix, so a pending `C-c …` or `C-c p …` chord
  survived the switch and could fire unexpectedly on the very next key
  in the new keymap. Consolidating the six separate chord-prefix flags
  into one `EmacsChord` enum made the reset a single, unconditional
  assignment, closing the gap as a direct consequence.
- **Three real LSP bugs found by the T123 audit** (above): the
  `initialize` request's advertised `capabilities` didn't match reality —
  it claimed `"didSave": false` while `textDocument/didSave` is actually
  sent, and declared no support for most already-implemented features
  (rename, code actions, document/workspace symbols, signature help,
  references, code lens, inlay hints, folding/selection ranges, document
  highlight, linked editing, call hierarchy, `workspace/applyEdit`,
  `workspace/executeCommand`) — a spec-correct server could reasonably
  withhold behavior for capabilities a client never declared. A JSON-RPC
  `error` response was silently dropped instead of being surfaced — a
  failed rename/code-action/format/… just appeared to do nothing, with no
  feedback; now shown on the status line as `LspEvent::RequestFailed`
  (`status.lsp_request_failed`). Signature help was manual-invoke-only; it
  now also auto-triggers right after typing `(`/`,` inside a call, matching
  every other editor's convention.
- **`docs/dictionaries/` and `crates/vix-spellcheck/spec/dictionaries/`
  were silently uncommitted.** `.gitignore`'s `dictionaries/` pattern
  (meant only for the ~287 MB Hunspell cache fetched separately at the
  repo root) was unanchored, so it also matched these two real, small
  documentation directories and kept them out of every commit —
  invisible locally (the untracked files still sat on disk) but breaking
  `scripts/check-docs` and CI's `lychee` link check in a fresh clone
  (`docs/SUMMARY.md`'s link to `dictionaries/index.md` had nothing to
  resolve to). Root-anchored to `/dictionaries/`; both directories are
  now tracked. Also fixes a bad relative link in the new
  `tui/examples/demo-workspace/notes.md` (T501, above) caught by the same
  `lychee` run.
- **Two bundled themes shared one name.** `tui/themes/safelight-red.json`
  declared `"name": "Phosphor Amber"` — the same name as the real
  `phosphor-amber.json` — so **View → Theme…** only ever offered one of
  the two (a red-on-black theme, unreachable under its own name). Found
  while adding regression tests for T203; renamed to `Safelight Red`.
- **`cargo build`/`cargo test` stack overflow on `App::new()`.** The
  fuller `locales/` catalog above (T148) pushed `rust_i18n`'s generated
  translation-table initializer — one very large function, unoptimized
  in a debug build — past the default 8 MiB thread stack. A targeted
  `[profile.dev.package.vix-i18n] opt-level = 2` in the root
  `Cargo.toml` fixes it (a release build was never affected).
- **Flaky `sqlite_connect_browse_query_and_filter`** (improvement plan
  T009): the recurring `ubuntu-latest`-only CI flake was the test
  harness's own wait for an async query, bounded by a fixed spin-count
  budget with no relationship to actual wall-clock time, not a race in
  `vix-db` itself. Replaced with a 30-second wall-clock deadline
  (`tui/tests/db_smoke.rs`'s `drain_query`).
- **Command palette's Files mode is now ranked** (improvement plan T153):
  results are scored with the same fuzzy ranking Commands mode uses and
  sorted accordingly, instead of showing raw filesystem-walk order (not
  portable across filesystems, and not relevance-ranked).
- **An unrecognized persisted keymap id no longer fails silently**
  (improvement plan T146): a corrupted `settings.toml` naming an unknown
  keymap is now reported in the messages panel at startup and falls back
  to Apple, instead of silently mapping to Apple with no indication
  anything was wrong.
- **File Explorer and the DB workbench's SQL statement editor could
  scroll past the end of their own list** (improvement plan T144): the
  new shared `vix-list-state` crate's `ensure_visible` clamps scroll
  against the list length for every migrated panel, and these two were
  the only ones (of 17) that had never done so — found while
  unifying 17 panels' near-identical scroll-cursor logic into one crate,
  not chased for its own sake.

### Security

- **A trusted project script could silently import and run a second,
  unreviewed script** (improvement plan T134, a post-scripting/AI
  re-audit): `Engine::new()`'s default Rhai module resolver made
  `import "some_file";` actually load and execute a real `.rhai` file
  resolved against the process's current working directory — undermining
  the T132 workspace-trust prompt, which shows the user one script file
  and asks them to trust *it*. Verified empirically (a throwaway script
  really did load and print a value from an unrelated temp file) before
  fixing it with `engine.set_module_resolver(DummyModuleResolver::new())`;
  `import` now fails unconditionally (`crates/vix-script/tests/sandbox.rs`).
  The rest of the audit (AI provider keys go through the keyring, never a
  plaintext config value; every T125 feature is genuinely explicit-invoke
  only) found nothing else to fix.
- **`rustls` updated 0.23.43 → 0.23.45**, fixing RUSTSEC-2026-0285 (a
  TLS 1.3 handshake message could be accepted across an encryption-level
  boundary it shouldn't cross; the handshake transcript stays
  authenticated either way, so this couldn't be used to alter or
  complete a handshake — see the advisory for the full detail). Pulled
  in transitively via `sqlx`/`ureq`; `cargo update -p rustls`, no code
  changes needed.
- **Persisted files that can carry sensitive content are now owner-only**
  (improvement plan T133): undo history, recorded macros, `config.toml`,
  `session.toml`, and the DB workbench's query history/saved queries are
  narrowed to 0600 on Unix. Previously these were left at the process's
  default (umask) mode, same as any other new file.

## [1.6.0] - 2026-08-29

### Added

- **A Project menu** (`vix-tasks`): six lifecycle command slots — Configure,
  Compile, Test, Install, Package, Run (`C-c p c o/c/t/i/p/r`) — resolved from
  the detected project type (Cargo, npm/yarn/pnpm/bun, Make, just,
  Python/Poetry, Go, Deno), a shareable `.vix/project.toml` override, and a
  per-user resolved-command cache, in that order. Also: **Run Task…**
  (the merged `tasks.toml` + project-type + discovered task list — discovery
  covers npm/yarn/pnpm/bun scripts, Deno tasks, Composer scripts, justfile
  recipes, go-task Taskfiles, Rake tasks, and Make targets), **Test at
  Point** (`C-c p c .`, Rust/Python/Go/JS/TS/Ruby/Elixir), **Repeat Last
  Task**, and a **Subproject ▸** family (`C-c p c m …`) that repeats the six
  lifecycle commands plus Find File, scoped to the nearest enclosing
  subproject of a monorepo. See
  [`crates/vix-tasks/spec/index.md`](crates/vix-tasks/spec/index.md).
- **Org table editor + Column View.** A structural Org table editor
  (`vix-org-table`: cell/row/column edits that keep the table aligned, plus
  pragmatic `TBLFM` formula evaluation) and an interactive, write-through
  **Column View** overlay (Org `C-c C-x C-c`) on the outline at the cursor —
  edit a headline's properties as a table, with dynamic-block (`columnview`)
  capture and update. See
  [`crates/vix-org/spec/index.md`](crates/vix-org/spec/index.md#column-view)
  and [`crates/vix-org-table/spec/index.md`](crates/vix-org-table/spec/index.md).
- **macOS: the `Command` key now drives Vix's `Control` bindings.** `Cmd+S`
  saves, `Cmd+F` finds, `Cmd+Shift+Z` redoes — the modifier is folded into
  `Control` before dispatch, so it works in every keymap with no second binding
  table. Vix asks the terminal for the kitty keyboard protocol on startup, since
  that is the only way `Command` can be reported at all; terminals without it
  (Terminal.app) are unaffected, and terminals that keep a `Cmd` shortcut for
  their own menus still never forward it.
- **Edit → Delete ▸**: delete the text unit at the cursor — **Character**
  (Emacs `C-d`), **Word**, **Sentence**, **Paragraph** (blank-line delimited),
  or **Section** (two or more blank lines delimit). Each takes the separator
  after the unit with it — or the one before it for the last unit — so the text
  closes up; Character, Word, and Sentence stay inside the line, while Paragraph
  and Section swallow the blank lines between blocks.
- **Edit → Transpose ▸** groups the transpose commands and extends them past
  characters and words: **Lines** (swap the cursor's line with the one above,
  Emacs `C-x C-t`), **Sentences**, **Paragraphs** (blank-line delimited), and
  **Sections** (two or more blank lines delimit). Each swaps the unit before
  the cursor with the unit at it, keeping the separator between them.
  Transpose Characters / Transpose Words are now **Characters** / **Words**
  inside that submenu; their actions and `Ctrl+T` / `Alt+T` keys are unchanged.
- **Edit → Wrap** (`edit.wrap`): hard-wrap (fill) the selection, or the
  paragraph at the cursor when nothing is selected, at the new `wrap_column`
  setting (default `80`). Keeps indentation, a comment/quote marker shared by
  every line, and list bullets with a hanging indent; blank lines separate
  paragraphs, and no word is ever split.

- **One Find dialog instead of four commands.** The find box now carries the two
  things that used to be separate menu items: **Replace** (`Alt+H`, or the
  button) turns a find into a find-and-replace in place, and **In:** (`Alt+I`)
  chooses where to look — **Buffer**, **Files**, or **Workspace**. Widening
  carries the query, the replacement, and the case/regex toggles to the surface
  that shows that many results, so nothing is retyped. **Find in Files…**,
  **Replace in Files…**, and **Find In Workspace…** are gone from the menu; their
  action ids (and `Ctrl+Shift+F`) still work for key bindings and the palette.
  The workspace panel takes `Alt+H` too, and `Alt+I` there moves on to the dock.
- **Edit → Find sits above Edit → Select**, and **Column Select Up** above
  **Column Select Down**, so the pair reads in arrow order.
- **Fuzz testing** (`fuzz/`, `cargo +nightly fuzz run <target>`): six
  coverage-guided targets over the code that parses input Vix did not write —
  `textops`, `org_table`, `lsp_frame` (LSP `Content-Length` framing), `vcard`,
  `conflict` (merge markers), and `http_request`. Each asserts invariants, not
  just the absence of a panic. See [`fuzz/README.md`](fuzz/README.md).
- **Benchmarks** (`tui/benches/`, `cargo bench`): Criterion benchmarks for the
  per-keystroke and per-frame paths — text transforms, opening/typing/pasting/
  undo in the editor widget, find-in-buffer, and palette fuzzy filtering.
- **`scripts/check-docs`**, now part of `scripts/check`: fails on a broken
  documentation link, a crate with no spec, a crate missing from the crate map,
  or a `README.md` that has drifted from its `index.md` twin.

### Changed

- The first-run welcome dialog is now controlled by a `show_welcome_dialog`
  setting (default `true`), replacing the inverted `welcomed` flag. Vix turns it
  off and **saves the settings immediately** after showing the dialog, instead of
  waiting for a clean exit, so a crashed or killed first run no longer brings the
  dialog back. Set it to `true` again to see the welcome screen on the next
  launch. A config file that already had `welcomed = true` shows the dialog once
  more, then settles.

### Fixed

- **Pasting in the terminal is one edit again.** `Cmd+V` (and any other paste
  made with the terminal's own shortcut) arrived as one key event per character,
  so undo removed the paste one character at a time, auto-indent re-indented
  every pasted line — an indented block walked right, closing braces included —
  and auto-pairing doubled brackets and quotes. Vix now enables **bracketed
  paste**: the text arrives in one piece and is inserted at the cursor as a
  single edit, exactly like `Ctrl+V`. Pastes into a prompt, the palette, the
  find bar, or a panel still go to that input.
- **Pasting a whole line keeps its trailing newline.** The indentation-aware
  paste dropped it, so the next paste ran onto the same line.
- **Opening a file was ~26 ms slower than it needed to be.** Every buffer
  compiled its language's Tree-sitter highlight query from scratch — on every
  file opened, every preview tab the explorer scans past with an arrow key, and
  every split, plus once per injected language. Compiled queries are now cached
  per language for the life of the process: opening a 200-line Rust file went
  from 26 ms to 0.6 ms, and a 5,000-line file from 41 ms to 15 ms.
- **Aligning a lone `|-` line no longer deletes it.** An Org table with no data
  rows has no columns to size, and rendering it produced an empty string — which
  the caller then wrote over the table's lines. It renders as `|---|` now, the
  way Emacs squares up a bare rule. (Found by the new `org_table` fuzz target.)
- **Switching the UI language showed `Language: %{locale}`.** `t!` reserves
  `locale` for choosing the target locale, so the argument never reached the
  placeholder. The placeholder is `%{language}` now.
- **A failing git command lost its reason.** The message was handed the error
  text but had nowhere to put it; there is now a `msg.git_failed_reason` string
  that shows it, in fifteen languages.
- **Command-palette filtering is ~40% faster.** The fuzzy matcher allocated a
  `Vec<char>` and a second copy of every candidate string on every keystroke;
  it now allocates once per candidate. Filtering 20,000 paths went from 4.5 ms
  to 2.8 ms per keystroke. The previous implementation is kept as a test-only
  reference that the current one must agree with, over fixed cases and a
  property test.
- **A flaky regression test made the suite unreliable.** The run-command cancel
  test judged "did the cancel get through?" with a 20-second stopwatch, and its
  own reader held the mutex *across* its poll sleep — a lock discipline
  `run_command` does not have. It was therefore measuring lock starvation (8+
  seconds under load, sometimes never) rather than the deadlock it guards. It
  now mirrors the real code and asserts how the child died — killed by a signal,
  not exited on its own — and runs in 0.2 s instead of 20-30 s.
- **The Org menu had no hover tooltips.** All 66 of its items (and submenus) now
  carry help text, so no menu item anywhere is left blank with
  `show_menu_tooltips` on.
- **The file explorer's delete confirmation showed `confirm.delete`.** The
  message had no entry in `locales/app.yml`, and a missing key renders as the
  key itself. It is translated now, and `tui/tests/i18n_keys.rs` fails the build for
  any `t!` key the catalog does not define.
- **`Ctrl+D` is forward delete in the Apple keymap.** It now removes the
  character to the right of the cursor, like the `Delete` key and every macOS
  text field, instead of falling through to the editor widget's
  add-next-occurrence caret. `Ctrl+Shift+D` still duplicates the line or
  selection, and the keymaps modeled on VS Code, IntelliJ, and Sublime keep
  their own `Ctrl+D`; in the Apple keymap, add carets with `Alt`+click or
  **Edit → Select → Select All Occurrences**.
- **Forward delete at the end of the buffer no longer backspaces.** With the
  cursor past the last character, `Delete` (and now `Ctrl+D`) stepped right —
  a no-op there — and then deleted backwards, eating the character *behind* the
  cursor. It is a no-op now.
- **Running the tests no longer overwrites your clipboard.** The test suite
  copied and cut through the real platform clipboard, so `cargo test` left
  whatever a test had cut (usually the word `doomed`, from a keymap test) on the
  macOS pasteboard — every later paste, in any app, produced it. The platform
  clipboard is now opt-in: `vix-clipboard` keeps an in-memory clipboard until
  `use_system` is called, and only the `vix` binary calls it, at startup.
- **Naming the open menu again now closes its dropdown.** A second click on the
  same top-level name re-opened the menu instead of toggling it shut; clicking a
  different name still switches menus. `Alt+<letter>` mnemonics toggle the same
  way and now work from inside an open dropdown or submenu, where they were
  previously ignored.
- **File explorer crash when a directory changes while it is being read.** The
  explorer sorted entries with a key that re-stats the filesystem during the
  sort, so a file or directory created or removed mid-sort could make the same
  entry compare two different ways and panic Rust's sort ("comparison function
  does not correctly implement a total order"). The key is now computed once per
  entry. This also fixes the intermittent panic in the app's render tests.

## [1.5.0] - 2026-08-05

### Added

- **File → Insert File…** (`file.insert_file`): insert another file's
  contents at the cursor, resolved against the workspace root.
- **File → Revert** (`file.revert`): reload the active buffer from disk,
  discarding unsaved edits as a single (undoable) step.
- **Emacs Org-menu parity.** The Org menu is reorganized into the Emacs
  org-mode menu groups, closing the gap analysis against
  `spec/emacs-menus/index.md`:
  - **Dates & Scheduling ▸** — active/inactive timestamps for today,
    Schedule Item… / Deadline… date prompts (weekday computed, planning
    line created or updated), and ±1-day date shifting under the cursor.
  - **Structure** — New Heading, Navigate Headings ▸ (parent, previous/next,
    same-level both ways), and Edit Structure ▸ grows Copy/Cut/Paste Subtree
    (system clipboard, paste relevels), Sort Children, and Refile Subtree…
    via a headline-chooser overlay.
  - **Show/Hide ▸** — fold-all/show-all plus sparse trees: Sparse TODO Tree
    and Sparse Tree Match… fold every subtree without a match.
  - **Editing ▸** — Emphasis wrapping, Insert Block templates, Footnote
    New/Jump (`C-c C-x f` semantics), and Edit Source Block: a `#+begin_src`
    body opens in a dedicated tab and the same action writes it back
    (`C-c '`).
  - **Archive ▸** — Archive Subtree to the sibling `<file>_archive` file
    (promoted to level 1, stamped `:ARCHIVE_TIME:`) and the ARCHIVE tag
    toggle.
  - **Hyperlinks ▸** — store/insert/follow links (`file:` with `::line`,
    `id:` resolved across project files, internal `*Headline` targets;
    web/mail URLs copy to the clipboard), next/previous link.
  - **Tags & Properties ▸** — Set Tags…, Set Property…, and a read-only
    Column View table.
  - **Agenda scoping** — a session restriction lock (`C-c C-x <`/`>`) and a
    persisted agenda file list (new `org_agenda_files` setting) ahead of the
    all-project-files default.
  - **Export ▸** — LaTeX (standalone article) and iCalendar (SCHEDULED /
    DEADLINE as all-day events) join Markdown and HTML.
  - **Emacs keymap chords** — the full `C-c` Org family plus a `C-c C-x`
    third-key table, with which-key popups; the Org menu displays the chords
    in its shortcut column.

### Changed

- Patch-level dependency bumps: `ratatui-image` 11.0.6, `clap` 4.6.5,
  `rust-i18n` 4.2.1, `rust-embed` 8.12.

## [1.4.1] - 2026-07-26

### Fixed

- **`evalexpr` held at 11.x (licensing).** `evalexpr` 12.0.0 relicensed from
  MIT to AGPL-3.0-only, and Vix 1.4.0 shipped with `evalexpr = "13"`. Vix is
  offered under "Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-2.0-only OR
  GPL-3.0-only", so that dependency took the licensing choice away from anyone
  distributing a build. The Calculator now uses 11.3.1, the last MIT release;
  its behavior and tests are unchanged. **Anyone who built from 1.4.0 should
  upgrade.**
- **`portable-pty` 0.8 → 0.9**, which replaces the unmaintained `serial`
  crate (RUSTSEC-2017-0008) with `serial2`.
- **`anyhow`, `crossbeam-epoch`, and `spin` updated** off an unsoundness
  (RUSTSEC-2026-0190), a vulnerability (RUSTSEC-2026-0204), and a yanked
  release respectively.

### Added

- **Supply-chain gate on all three forges** (T003). `deny.toml` sets the
  policy — RUSTSEC advisories, a permissive license allow-list, wildcard bans,
  registry sources — and `cargo deny --workspace --all-features check`
  enforces it via `.github/workflows/security.yml`, the `deny` job in
  `.gitlab-ci.yml`, and `.forgejo/workflows/security.yml`, weekly as well as
  on every push and pull request. It deliberately sits outside the
  `scripts/check` gate: that one is reproducible offline from the lockfile,
  while the advisory database moves under a lockfile that has not changed.
  cargo-deny is pinned and checksummed rather than `cargo install`ed. See
  `spec/ci/index.md`.

## [1.4.0] - 2026-07-26

### Added

- **CI/CD on all three forges.** Vix is pushed to GitHub, GitLab, and Codeberg,
  and each now enforces the same gate as `scripts/check` — `cargo fmt --check`,
  build, Clippy pedantic with `-D warnings`, the test suite, and warnings-denied
  `cargo doc`. GitHub adds `.github/workflows/ci.yml` (lint job, Linux + macOS
  test matrix, MSRV check, `Swatinem/rust-cache`); GitLab adds `.gitlab-ci.yml`
  (check/test stages, plus a tag-triggered static musl build published to the
  package registry and a GitLab Release); Codeberg adds `.forgejo/workflows/`
  (single-job gate for Forgejo Actions, plus a tag-triggered release). Releases
  on GitHub continue to be produced by `dist`. New spec: `spec/ci/index.md`.

- **Org → Agenda submenu with the built-in agenda views.** The former single
  *Agenda Tracker* item is now an **Agenda** submenu offering the views from the
  Org manual's Agenda Views: **Weekly/Daily Agenda** (`org.agenda`), **Global
  TODO List** (`org.agenda.todo`), **Match Tags/Property…** (`org.agenda.match`,
  prompt-driven), **Search…** (`org.agenda.search`, prompt-driven), and **Stuck
  Projects** (`org.agenda.stuck`). Every view opens read-only and interactive —
  pressing `t` on a task cycles it in its source file and rebuilds the same view.
  New pure builders `org::todo_list` / `tags_match` / `search` / `stuck_projects`
  / `render_list`.
- **Org task workflow: Emacs chords, close-with-note, and an interactive
  agenda.**
  - The **Emacs** keymap now wires the Org `Ctrl+C` chord family: `C-c C-t`
    cycles a headline's TODO keyword, `C-c C-c` runs the context action (toggle
    the checkbox on the line, else recompute statistics), and `C-u C-c C-t`
    (universal argument) marks the headline `DONE` with a closing note. A new
    `C-c-` prefix indicator and which-key list make the chords discoverable.
  - **Mark Done with Note…** (`org.close_note`, in the Org menu and `C-u C-c
    C-t`): prompts for a note, then marks the headline `DONE`, stamps
    `CLOSED: [now]` under it, and logs the note into its `:LOGBOOK:` drawer —
    Emacs `org-log-done` behavior. New pure `org::close_headline`.
  - The **Agenda Tracker** is now interactive: it opens read-only, and pressing
    `t` on a task line cycles that task's TODO state directly in its source
    `.org` file, reloads any open buffer for it, and rebuilds the agenda in
    place. New pure `org::agenda_items` / `org::render_agenda` (structured items
    + a line→source map); the agenda now reindexes before compiling so newly
    added `.org` files are included.
- **Org drawer folding with Tab.** In an `.org` buffer, pressing **Tab** on a
  drawer header line (one that starts and ends with a colon, e.g.
  `:PROPERTIES:`) folds that drawer — hiding its body through `:END:` like code
  folding — and marks the collapsed header with a trailing `...`
  (`:PROPERTIES:...`). Tab again on the header unfolds it. Folding is view-only
  and never edits the buffer — so it works even in read-only buffers; on any
  other line Tab still indents. New pure helper `org::drawer_range` and editor
  method `toggle_manual_fold`; folded lines now render a trailing `...` in the
  editor generally.
- **File → Open… is now a comprehensive file browser** (new crate
  `vix-file-browser-panel`, built on `walkdir`): a recursive listing of the
  workspace root with live search — fuzzy text, `*.rs`-style globs, and
  `ext:rs` / `.rs` extension filters, all combinable, plus the classic
  `:line[:col]` jump suffix. Sort by name, size, date created, or date
  modified (`Ctrl S` cycles, `Ctrl R` flips), toggle hidden files (`Alt H`),
  navigate directories (Enter/→ enters, ← goes up), and fall back to the
  classic type-a-path prompt with `Ctrl O` (new action `file.open_path`).
  Fuzzy queries rank by relevance until an explicit sort is chosen.
- **Org → Capture submenu** (first in the Org menu), now a template-driven
  system in the shape of Emacs `org-capture` (new `vix-org-capture` crate; see
  `spec/org/capture/index.md`): named templates (the `org_capture_templates`
  setting) with `%^{Prompt}`-style placeholders (`%^{Label|choices}`,
  `%t`/`%T`/`%u`/`%U`, `%<strftime>`, `%a`, `%i`, `%f`/`%F`, `%c`, `%^g`/`%^G`,
  `%?`, `%%`), wrapped as a headline/item/checkbox/table-row, and filed at a
  target (`cursor`, `id:`, `file:`, `file+headline:`, `file+datetree:`). Five
  built-in templates ship by default — **Anything…** (`org.capture`),
  **Contact…** (`org.capture.contact`, moved from Org → Contacts → New
  Contact…), **Task…** (`org.capture.task`, a multiline review buffer,
  Alt+Enter = newline), **Babel…** (`org.capture.babel`, prompts for a
  language, opens a review buffer around a `#+begin_src`/`#+end_src` block),
  and **Note…** (`org.capture.note`, a plain headline with a `%U` creation
  timestamp) — plus **Choose Template…** (`org.capture.select`) for any
  custom templates added to `org_capture_templates`. Each `%^{}` field prompt
  now shows a live preview of the whole template above the input — answered
  fields substituted, the current one marked `‹Label›`, later ones `[Label]`,
  scrolled to keep the current field in view — instead of an isolated prompt
  box with no context.
- **Org → Priority Up/Down** (`org.priority.up`/`.down`): step a headline's
  `[#X]` priority cookie toward the `org_priority_highest`/`org_priority_lowest`
  settings (default numeric `'0'`..`'9'`, `'0'` = highest), clamped at the
  bound. New pure `org::priority`/`set_priority`/`priority_up`/`priority_down`.

### Changed

- **Help → Keyboard Shortcuts… now lists every active shortcut** in a
  two-column table — action name and key combo — assembled from the curated
  global rows, every menu-item accelerator, and the active keymap's chord
  tables (Spacemacs leader, Emacs `Ctrl X`). Click a column header to sort it
  ascending, click again for descending; type to filter; ↑↓/PgUp/PgDn and the
  wheel scroll.
- **File → Open Recent… is now a multi-column table**: file basename, path,
  size, created at, and modified at.
- **All three docks now show by default** — the left dock (explorer), right
  dock (messages), and bottom dock (`show_bottom_dock` default flipped to
  `true`). Also fixed a latent bug where, before the first render, a visible
  dock's unrecorded rectangle made clicks on row/column 0 start a dock
  resize.

- **Harmonize the modal keymaps.** Spacemacs Normal mode now shares the Vi
  keymap's Normal-mode handler, gaining the full vocabulary (`w`/`b`, `gg`/`G`,
  `dd`/`yy`/`p`, `u`, `/` + `n`/`N`, `%`, `I`/`A`); the two keymaps' separate
  Insert-mode flags were unified into one, and switching keymaps now also clears
  a pending Vi two-key operator.
- **Refactor:** the pure cursor-relative text helpers (increment/decrement
  number, smart toggle, transpose chars/words, TODO-tag matching) moved from
  `app` into `crate::textops` with their unit tests, and their three identical
  host wrappers collapsed into one `App::rewrite_at_cursor`. Behavior unchanged.
- **Media types use clean `text/<lang>` forms (no `x-` prefix)** for source code —
  `text/rust`, `text/python`, `text/typescript`, `text/java`, `text/cpp`,
  `text/csharp`, `text/c`, `text/ruby`, `text/go`, …, `application/sql`. `.ts` now
  maps to `text/typescript` and `.cs` to `text/csharp` (C# added).
- **Snippet directories load every `*.json` file** (not just `snippets.json`), and
  media-type snippets also load from the project at
  `config/media-types/<type>/snippets/`.
- **Macro commands moved to Edit → Macro** and namespaced consistently under
  `macro.*`: `toggle_macro` → `macro.record`, `play_macro` → `macro.play` (joining
  `macro.save` / `macro.play_saved`). Menu items relabeled Record / Play / Save… /
  Play Saved….
- **Edit → Lines → "Sort Lines" renamed to "Sort".**
- **Docs/spec/AGENTS harmonized** with the current feature set (keymaps list,
  Org/Debug menus, edit surfaces, snippets, media types, module map, Rust 1.95
  toolchain floor).
- **Keymap renamed "Vim" → "Vi"** — display label, id (`vim` → `vi`), and action
  (`view.keymap:vim` → `view.keymap:vi`). Older configs using `vim` still load.
- **Edit surfaces moved to Edit → Mode.** The Edit Table/Outline/JSON/YAML/Bytes
  items moved from the Tools menu into a new **Edit → Mode** submenu (labeled
  Table/Outline/JSON/YAML/Bytes). The action ids and command-palette entries are
  unchanged.
- **Case submenu moved to Tools → Convert.** The Edit → Case transforms
  (Upper/Lower/Title/Kebab/Snake/Camel/Pascal) now live under Tools → Convert.

### Fixed

- **Stale locale table after editing `locales/app.yml`.** `vix-i18n` embeds
  the translation table at macro-expansion time (`rust_i18n::i18n!`), which
  Cargo couldn't detect as a dependency — editing only `locales/app.yml`
  never triggered a rebuild, so several recently-added/renamed menu labels
  (`org.capture.task`/`.babel`/`.note`/`.select`, `org.priority.up`/`.down`)
  rendered as their raw i18n key instead of translated text until something
  else happened to recompile `vix-i18n`. Fixed with
  `crates/vix-i18n/build.rs` (`cargo:rerun-if-changed=../../locales/app.yml`);
  guarded against regressing by a new `vix-menu` test,
  `every_menu_label_translates`, that walks the whole menu tree.
- **Menu navigation.** In a dropdown, pressing `Right` on a submenu now opens it
  **and highlights its first item** (was: nothing highlighted). Pressing `Up` on
  the first dropdown item now moves to the **menu title** (nothing highlighted)
  instead of wrapping to the last item.

### Added

- **Database workbench** (new top-level **DB** menu, `Alt+D`, after **AI**;
  `spec/db`). A full-screen overlay that connects to **SQLite / PostgreSQL /
  MySQL** through embedded sqlx `Any` drivers — no external client tools — and
  presents a three-pane workbench: a schema tree, a syntax-highlighted SQL
  editor with autocomplete, and a filterable results grid. Passwords are held
  only in memory; saved connections persist without a password field. Includes:
  - **Saved connections & catalog browsing** — add/edit connections, expand
    schemas/tables/views, and view columns / indexes / foreign keys / triggers /
    constraints / **statistics** and the table's **`CREATE` statement**; `p`
    previews a table's rows, `/` filters the tree.
  - **Query editor** — statement-at-cursor execution (`F5`/`Ctrl+Enter`) and
    execute-all (`F9`), theme-colored syntax highlighting, autocomplete over
    keywords / tables / `table.column` / **`JOIN … ON` from the foreign-key
    graph**, beautify (`Alt+Shift+F`), `EXPLAIN`/`EXPLAIN ANALYZE` (`F6`/`F7`)
    with a full-scan insight, query **history** (`Ctrl+R`) and **saved queries**
    (`Ctrl+B`/`Ctrl+S`), and `:name` **bind parameters** that prompt before running.
  - **Results grid** — column select/sort/filter, cell/row yank, a scrollable
    cell viewer, **expanded row view** (`x`), **foreign-key follow** (`f`), an
    ASCII **result chart** (`c`), and **export** to CSV/TSV/JSON/NDJSON/Markdown/
    SQL. Editable table previews support **staged cell edits** (`i` to stage,
    `W` to commit as `UPDATE`s in one transaction, with conflict detection).
  - **Read-only by default** — connections open read-only (two-layer: a
    database session pragma plus a client-side write guard); `F8` toggles write
    mode, and the editor title badges the current access.
  - **Async execution** — statements and `EXPLAIN` run off the event loop with a
    live elapsed indicator and `Ctrl+C` cancellation; large results **stream** in
    batches (capped, with a truncation note).
  - **Transactions** — a client-side `TX` badge; the write-confirmation gate is
    relaxed inside an explicit transaction; **DB → Begin / Commit / Rollback**.
  - **AI SQL assistant** — `Ctrl+A` turns a natural-language question into SQL
    from the **schema only** (never row data), validated with `EXPLAIN`;
    `Ctrl+O` optimizes a query from its plan, `Ctrl+F` fixes the last error,
    `Ctrl+K` explains a query, and a leading `?` answers a schema question. Uses
    the configurable `ai_command` CLI, so it is provider-agnostic.
  - **Query log** (`Ctrl+L`) with per-statement duration/rows/origin, a Mermaid
    **ER diagram** (`Ctrl+E`), and **CSV/TSV import** (`Ctrl+U`) into a new table.
  - **Connectivity** — an optional **SSH tunnel** (`ssh -N -L`, lifetime tied to
    the connection) and a **credential waterfall** that resolves a password from
    a `password_command` or the OS keyring (macOS `security`, Linux
    `secret-tool`) before prompting, with an opt-in save-to-keyring.
- **Sublime Text keymap** (View → Keymap → Sublime Text). `Ctrl+P` Goto
  Anything, `Ctrl+Shift+P` Command Palette, `Ctrl+R` Goto Symbol, `Ctrl+G` Goto
  Line, `Ctrl+D` select occurrences, `Ctrl+L` select line, `Ctrl+J` join,
  `Ctrl+M` matching bracket, `Ctrl+Shift+D`/`Ctrl+Shift+K` duplicate/delete
  line, `Ctrl+H` replace, `Ctrl+B` build, ``Ctrl+` `` console. Plus a
  `docs/for-sublime-users/` migration guide.
- **Migration guides** for Vim, Emacs, VS Code, IntelliJ, Eclipse, Sublime
  Text, and Spacemacs users (`docs/for-*-users/`): an honest pros/cons pitch, a
  muscle-memory mapping table for each keymap, and a try-it-for-an-afternoon
  experiment.
- **Keymap improvements.** Vi: `w`/`b` words, `^`, `gg`/`G`, `dd`/`yy`/`p`, `u`,
  `/` + `n`/`N`, `%`, `I`/`A`, and `:N` / `:e [path]` commands. Emacs: `M-x`
  palette, word/page/buffer-end Meta motions, kill ring (`C-w`/`M-w`/`C-y`/`C-k`),
  transpose, `C-/` undo, and `C-x` window/buffer chords (`2`/`3`/`1`/`0`/`o`/`b`).
  VS Code: `Ctrl+\` split, `Ctrl+J` panel, ``Ctrl+` `` terminal, `Ctrl+Shift+K`
  delete line, `Ctrl+Shift+L` select all occurrences, `Ctrl+Shift+M` Problems.
- **CSpell config** (`cspell.json`) with an external custom dictionary
  (`project-words.txt`) of project/domain terms, so `cspell` can spell-check the
  repo's docs and text with the project vocabulary pre-seeded.
- **Word-level diff** in the Compare With File overlay. Changed lines now
  highlight the specific words that differ (bold/reversed) instead of coloring the
  whole line, via `similar`'s inline diff.
- **Matching tag** (Go → Matching Tag). Jump between an HTML/XML tag and its
  partner (open ↔ close), handling nested same-name tags. Pure logic in
  `crate::tags`.
- **Comment banner** (Edit → Comment Banner). Turns the current line into a boxed
  section header — the title bordered above and below by a `=` rule, each line
  prefixed with the language's line comment.
- **Go to Percent / Byte** (Go menu). Jump to a percentage through the file (by
  line) or to a byte offset, complementing Go to Line.
- **Read-only buffer toggle** (View → Read-Only). Locks the active buffer against
  edits — typing, deletes, and editing commands are blocked (with a status note),
  while navigation, selection, copy, and search still work.
- **Relative line numbers** (View → Relative Line Numbers, off by default). The
  gutter shows each line's distance from the cursor line (hybrid: the cursor line
  shows its absolute number), making count-based motions easy.
- **Line-ending conversion** (Edit → Lines → Line Endings → To LF / To CRLF),
  **Squeeze Blank Lines** (Edit → Lines), and **ROT13** (Tools → Convert). Pure
  transforms in `crate::textops`, applied to the selection or whole buffer.
- **Which-key popup.** While a chorded prefix is pending — Emacs `Ctrl+X` or the
  Spacemacs `Space` leader — a popup lists the candidate next keys and the action
  each runs, making the keymaps discoverable.
- **Clipboard history / kill-ring** (Edit → Paste from History…). Every copy/cut is
  recorded into a ring (most-recent first, de-duplicated, capped at 30); the picker
  pastes any past entry at the cursor.
- **Transpose** (Edit → Transpose Characters / Transpose Words). Swaps the two
  characters around the cursor (Emacs `C-t`; at line/buffer end swaps the last two)
  or the two neighboring words (Emacs `M-t`, preserving the separator).
- **Toggle value** (Edit → Toggle Value). Flips the token under the cursor to its
  opposite: word pairs (`true`/`false`, `yes`/`no`, `on`/`off`, `enable`/`disable`,
  `left`/`right`, `up`/`down`, `min`/`max`, `and`/`or`) whole-word with case
  preserved, and symbol pairs (`&&`/`||`, `==`/`!=`, `<=`/`>=`, `<`/`>`, `++`/`--`).
- **HTTP/REST client** (Tools → Send HTTP Request). Write a request in a
  `.http`-style buffer (`METHOD url`, `Header: value` lines, blank line, body) and
  send it with the pure-Rust `ureq` client on a background thread; the response
  (status, headers, body) opens in a new tab. Pure parser in `crate::http_client`.
- **Jump to line (labels)** (Go → Jump to Line). EasyMotion/leap-style: each
  visible line gets a short label (a, b, c, …); type it to jump the cursor there.
  Esc cancels.
- **Frecency ranking for the project switcher.** Switch Project now orders recent
  projects by frequency × recency (recent, frequently-opened projects first)
  instead of plain most-recently-used. Sessions track a visit count + last-open.
- **Offline structural selection expand.** Expand Selection now falls back to the
  Tree-sitter parse tree (smallest enclosing node, growing on repeat) when the
  file has no language server — previously it required LSP.
- **Scratch buffer** (File → New Scratch Buffer). Opens a throwaway, unsaved
  buffer with a header line for quick notes/calculations.
- **Highlight word occurrences** (View → Highlight Word Occurrences, off by
  default). Passively marks every whole-word occurrence of the identifier under the
  cursor, on its own render channel so it never clobbers (sticky) search
  highlights.
- **Align on delimiter** (Edit → Align). Pads selected lines so their first `=`,
  `:`, `,`, or `|` lands in a common column. Pure logic in `crate::align`.
- **Increment / Decrement number** (Edit menu). Bumps the integer at or after the
  cursor by one (Vim's Ctrl-A/Ctrl-X), handling a leading `-`.
- **TODO/FIXME finder** (Tools → Find TODOs…). Scans the project's files (honoring
  `.gitignore`) for comment tags — TODO, FIXME, HACK, XXX, BUG, NOTE — as whole
  words, listing them in the results panel; Enter jumps to the match.
- **Smart-case search** (find box, on by default; toggle with the Smart button or
  Alt+S). Matches case-insensitively when the query is all-lowercase, but becomes
  case-sensitive as soon as it contains an uppercase letter. The explicit Case
  toggle still forces sensitivity.
- **Surround** (Edit → Surround). Wrap the selection in a bracket/quote pair —
  parentheses, brackets, braces, angles, double/single quotes, backticks — and
  remove it by repeating the same action.
- **Minimap** (View → Minimap, off by default). A code-overview column at the
  right of the editor: each row is a band of lines drawn as a bar sized to the
  band's longest line, with the current viewport highlighted; click to jump.
  Single-pane view.
- **Call hierarchy (callers)** (Tools → Language Server → Call Hierarchy). Lists
  the incoming calls to the symbol under the cursor via LSP
  (`prepareCallHierarchy` → `incomingCalls`), shown in the references jump list.
- **Emmet expansion** (Edit → Emmet Expand). Expands the abbreviation before the
  cursor into HTML — child `>`, sibling `+`, multiply `*N`, `#id`, `.class`,
  `{text}`, and `$` numbering (e.g. `ul>li.item$*3`). Pure logic in `crate::emmet`.
- **Persistent undo.** The undo tree is saved per file on save and restored on
  reopen (under `<config>/undo/`), guarded by a content hash so it's only replayed
  when the file still matches. Setting `persistent_undo` (on by default).
- **Live Backlinks** (Org → Roam → Live Backlinks). A toggle that fills the bottom
  dock with the active node's linked + unlinked references and refreshes as you move
  between nodes.
- **Dailies calendar** (Org → Roam → Dailies → Calendar…). Opens the month grid;
  Enter on a day opens/creates that day's Org-roam daily note instead of inserting
  the date.
- **`[[` wiki-link autocomplete** for Org-roam/Node. Typing `[[` in an `.org` file
  opens node-title completion (also Org → Node → Complete Link…); accepting inserts
  the title and closes the link, without double-closing when auto-pair already
  added `]]`.
- **Rainbow brackets** (View → Rainbow Brackets, off by default). Colors `()[]{}`
  by nesting depth (6-color cycle) over the syntax colors; skipped for very large
  files. (Idea from the zee/editor feature.)
- **Sticky scroll** (View → Sticky Scroll, on by default). Pins the enclosing
  scope's header line (e.g. the `fn`/`class`/heading) at the top of the editor
  while you scroll through its body. Single-pane view. (Idea from the zee/VS Code
  feature.)
- **Indentation guides** — faint vertical bars at each indent level on the
  (non-wrapped) editor view, for space-indented buffers. (Idea from the zee/VS Code
  visual.)
- **Auto-save** (View → Auto Save, off by default). Periodically writes the active
  dirty file-backed buffer (every ~5s; plain save, no format-on-save churn).
- **Format on save** (View → Format on Save, off by default). On save it runs the
  language server's formatter and re-saves once the edits land; the plain save
  happens first, so nothing is lost if formatting is slow or unsupported.
- **Auto-reload on external change.** Files changed on disk by another process (a
  formatter, `git checkout`, a second editor) are detected (mtime poll, throttled
  to 1s): clean buffers reload automatically; a buffer with unsaved edits gets a
  one-time warning and keeps your edits.
- **Keymap renames + a new VSCode Windows keymap.** Display names dropped the `+`
  ("IntelliJ macOS"/"IntelliJ Windows") and reordered VS Code ("VSCode macOS");
  added a **VSCode Windows** keymap. Canonical ids are now full platform names
  (`vscode-macos`, `vscode-windows`, `intellij-macos`, `intellij-windows`); the
  old `vscode`/`intellij-mac`/`intellij-win` ids still load. Dropped the
  `jetbrains-*` aliases.
- **One syntax-highlight query per viewport.** The non-wrapped renderer now runs a
  single Tree-sitter highlight query over the whole visible region instead of one
  per line — cheaper while typing — and the highlight cache memoizes a single
  entry instead of one per visible line. (Idea from the zee editor.)
- **Undo tree (branch-preserving undo).** Editing after an undo no longer discards
  the redo history — it starts a new branch, so no state is ever lost. Undo/redo
  behave like a linear history by default; **Edit → Switch Undo Branch** cycles
  which branch redo follows, making every past state reachable. (Idea from the zee
  editor's edit-tree.)
- **Background (async) syntax parsing for large files.** Buffers ≥ 50 KB now
  reparse on a background thread after each edit instead of blocking the keystroke,
  with cancel-on-new-edit and generation-based stale-result rejection (the edited
  tree keeps rendering until the fresh one lands). Smaller files stay fully
  synchronous — identical behavior. (Idea from the zee editor's async parser.)
- **Org → Contacts ([org-contacts](https://github.com/doomelpa/org-contacts)).**
  Manage contacts stored as Org headlines: New Contact (skeleton with a property
  drawer), Find Contacts (name/email/phone table), Insert Field (Email/Phone/
  Address/Birthday/Nickname/Note), Birthdays, and Export to vCard 3.0. Pure logic
  in `crate::org_contacts`.
- **Help menu additions**: License (crate metadata from `Cargo.toml`), Report an
  Issue… (links the issue tracker), and Privacy Statement — each shown in a
  scrollable overlay.
- **Renamed the Debug menu to Run** (action ids `debug.*` → `run.*`) and moved it
  to just after the new Go menu (Alt+R).
- **Top-level Go menu** (promoted from Edit → Go, placed after View, Alt+N). Adds
  Go to Symbol / Declaration / Implementations / References, Next/Previous Issue
  (diagnostics) and Next/Previous Change (git hunks), and granularity submenus —
  Word, Sentence, Line, Paragraph, Section (each with Start/End/Next/Previous and
  go-to-Nth by number), plus File (Start/End). New LSP `textDocument/declaration`
  request and in-file diagnostic navigation.
- **Workspaces (multi-folder, saved to a file).** File → Open Workspace from
  File… / Save Workspace into File… / Add Folder to Workspace…. A workspace is a
  portable `.vix-workspace` (TOML) file listing project folders plus the files to
  reopen; the fuzzy finder and project index span every workspace folder.
- **base16 themes.** Ten well-known [base16](https://github.com/chriskempson/base16)
  color schemes (Tomorrow Night, Solarized Dark/Light, Gruvbox, Nord, Ocean,
  Monokai, Material, Dracula, …) generated from a compact palette table and
  merged into View → Theme. (Idea from the zee editor's base16 theme generation.)
- **`scripts/check` (and `make check`)**: a one-command local CI-parity gate —
  build + Clippy (pedantic, `-D warnings`) + full test suite. Plus Unicode/emoji
  stress fixtures in `test-data/` with a grapheme-width regression test. (Ideas
  from the zee editor.)
- **`.gitignore`-aware file finder.** The project file index now walks with the
  `ignore` crate (the engine ripgrep uses), so the fuzzy file finder honors
  `.gitignore`, `.ignore`, and git's global/excludes (even outside a git repo) and
  no longer surfaces build artifacts; `target`/`node_modules` are always pruned.
  (Idea from the zee editor.)
- **Sticky vertical "goal column."** Moving the caret up/down now remembers the
  column you started from, so passing through a short line no longer snaps the
  cursor inward (it returns to the original column on the next long line). Any
  horizontal move resets it. (Idea from the zee editor.)
- **Org checkbox & statistics cookies.** Parent checkboxes now reflect their
  children (all → `[X]`, none → `[ ]`, some → `[-]`), and `[/]`/`[%]` statistics
  cookies in headlines and list items are recomputed automatically after Toggle
  Checkbox / Cycle TODO (or on demand via **Org → Update Statistics**). Headline
  cookies count child checkboxes or child TODO headlines, honoring the
  `:COOKIE_DATA:` property (`checkbox`/`todo`, plus `recursive`). Pure logic in
  `org::update_statistics`.
- **Org → Node ([org-node](https://github.com/meedstrom/org-node) functionality).**
  A new submenu for fast, ID-based nodes where a node is a file *or* a subtree:
  nodeify the entry at the cursor (give it an `:ID:`), extract a subtree into its
  own file node (leaving an `[[id:…]]` link), insert a `#+transclude:` directive,
  list dead ID links, rename a file by its `#+title:`, and rebuild the node cache —
  alongside shared find / insert-link / random / backlinks. Pure helpers in
  `crate::roam`; see `spec/org/index.md`.
- **Org → Roam ([Org-roam](https://www.orgroam.com/) note-taking).** A new submenu
  for networked, Zettelkasten-style notes over a directory of `.org` files: find /
  capture / random node, insert `[[id:…]]` node links, a backlinks buffer (linked
  + unlinked references), dailies (today / capture / go-to-date), node metadata
  (tags / aliases / refs), a Mermaid node graph, and a stateless database sync that
  lists every node. Pure logic in `crate::roam`; see `spec/org/index.md`.
- **Tools → Draw (ditaa ASCII art).** A new submenu inserts ASCII-art shapes for
  [ditaa](https://ditaa.sourceforge.net/) diagrams: rectangles, rounded
  rectangles, document/storage shapes, horizontal/vertical (and dashed) lines,
  arrows (right/left/up/down), a point, a two-box flow, and a colored box. See
  `spec/tools/draw/index.md`.
- **PlantUML media type** (`text/plantuml`, `.plantuml`/`.puml`) plus a bundled
  snippet library (diagram skeleton, sequence/class/use-case/activity/state/
  component constructs, notes, skinparam) **and a 41-diagram example gallery**
  imported from `joelparkerhenderson/plantuml-examples` (activity, archimate,
  C4, class, deployment, ERD, gantt, mind-map, sequence, state, timing, …).
- **Graphviz media type** (`text/graphviz`, `.graphviz`/`.gv`/`.dot`) plus a DOT
  snippet library (directed/undirected graphs, nodes, edges, clusters, ranks,
  attribute defaults, record nodes).
- **Mermaid media type** (`text/mermaid`, `.mmd`/`.mermaid`) plus a snippet
  library covering flowcharts, sequence/class/state/ER diagrams, gantt, pie,
  mindmap, git graph, user journey, timeline, and quadrant charts.
- **Bundled example snippet libraries** under `config/media-types/<type>/snippets/examples.json`
  for ~38 media types — the originals (Python, Plain, TypeScript, Rust, Java, C++,
  C#, JavaScript, C, Ruby, JSON, YAML, SQL) plus HTML, CSS, Markdown, Go, TOML,
  Shell, PHP, Kotlin, Swift, Lua, Scala, Haskell, Elixir, Erlang, Clojure, Dart,
  R, Julia, Groovy, PowerShell, Objective-C, F#, OCaml, Perl, and XML — many
  common snippets each, modeled on TextMate/VS Code.
- **More programming-language media types** added to the table (Scala, Haskell,
  Elixir, Erlang, Clojure, Dart, R, Julia, Groovy, PowerShell, Objective-C, F#,
  OCaml), all using the clean `text/<lang>` form.
- **`Base` column in the media-type table** — each media type is classified
  `text` or `binary` (`MediaType::is_text()`); shown in the Media Types picker.
- **JSON snippet files (global / media-type / project).** Snippets can now be
  defined in JSON files (the VS Code shape: `name → {prefix, body, description}`)
  loaded from `~/.config/vix/global/snippets/snippets.json`, per-media-type
  (`…/media-types/<type>/snippets/snippets.json`), and a project file
  (`project_snippets`, default `config/snippets/snippets.json`). The Snippets
  picker (Tools → Snippets…) is now **searchable** and shows every in-scope
  snippet; typing a snippet **prefix** then **Tab** expands it. Bodies use the
  TextMate/VS Code tabstop syntax; interpolated shell code and `\u` are omitted.
  See `spec/snippets/index.md`.
- **Edit → Lines → Shuffle.** Randomly reorder the selected lines (or the whole
  buffer) with a Fisher–Yates shuffle (`edit.shuffle`).
- **Edit SQL mode.** A new **Edit → Mode → SQL** surface parses a `.sql` buffer
  into its statements (ignoring semicolons in strings/comments) and lists them by
  kind; reorder, delete, uppercase-format keywords, undo/redo, and save back.
  Backed by the unit-tested `edit_sql` module. See `spec/edit-sql/index.md`.
- **Media types.** A new **Tools → Media Types** picker lists common media types
  (MIME) with descriptions and extensions; type to filter, Enter to insert at the
  cursor. Opens pre-selected to the current file's type. Backed by the curated
  `spec/media-types/media-types.tsv` and the `media_type` module (with
  `for_extension` lookup). See `spec/media-types/index.md`.
- **Org → Clock In / Clock Out.** Clock In inserts an open `CLOCK: [now]` entry;
  Clock Out closes the most recent one with the end time and `=> H:MM` duration
  (local time via `jiff`). Feeds the Time Tracker report.
- **Org capture, agenda & time tracking.** **Org → Capture…** logs an idea/task
  as a `* TODO` headline via a quick prompt; **Agenda Tracker** compiles
  deadlines/scheduled items/TODOs from the project's `.org` files into a dated
  agenda; **Time Tracker** totals `CLOCK:` durations per headline into a report.
- **Org mode (basics).** A new top-level **Org** menu (`Alt+O`) brings headline
  structure editing (promote/demote and move subtree across siblings), fold
  cycling, TODO cycling (none → `TODO` → `DONE`), checkbox toggling, and export to
  Markdown / standalone HTML. Backed by the pure, unit-tested `org` module. See
  `spec/org/index.md`.
- **`affix` helpers** — `add` / `drop` / `toggle` a `prefix`/`suffix` pair around
  text (a conventional wrap: `add("alfa","bravo","charlie") == "bravoalfacharlie"`),
  for wrapping and unwrapping selections.
- **Tools → Insert → Org** now also hosts the inline **markers** (folded in from
  the former Markers submenu), with a new **Tag `:`** marker and a **Properties**
  (`:PROPERTIES: … :END:`) snippet.
- **Tools → Insert → Org / Markers / Begin-End** submenus — Org inserts Org-mode
  snippets (title/author, headlines, links/images, lists, table, TODO/DONE,
  planning lines, timestamps, drawer); Markers toggle inline emphasis (`*`/`/`/`_`
  /`+`/`~`/`=`) around the selection; Begin-End toggle `#+BEGIN_…/#+END_…` blocks.
- **Bracket / quote auto-pairing.** Typing `(`, `[`, `{`, `"`, `'`, `` ` ``
  inserts the matching closer (wrapping a selection when one exists), typing a
  closer steps over an auto-inserted one, and Backspace inside an empty pair
  deletes both. New `auto_pair` setting (on by default) and a **View → Editor →
  Auto-Pair Brackets** toggle.
- **Snippet tabstops.** Snippets (Tools → Snippets…) now support `$1`/`$2`/`$0`
  and `${1:placeholder}` fields: inserting one selects the first field and **Tab**
  walks the rest (Esc exits). Several bundled snippets gained fields. See
  `spec/snippets/index.md`.
- **Persistent macros.** Recorded keyboard macros can be saved by name
  (**Edit → Save Macro…**) to `macros.toml` and replayed in later sessions
  (**Edit → Play Saved Macro…**). See `spec/macros/index.md`.
- **View → Zoom** (In / Out / Reset) — best-effort terminal font zoom. A TUI
  can't portably resize the font, so Vix emits the font-resize escape for
  terminals that support one (xterm/urxvt) and tells you to use the terminal's
  own zoom otherwise.
- **Switch Project** (**File → Switch Project…**, `file.switch_project`) —
  re-root the editor at a recent workspace without relaunching: saves the current
  session, rebuilds explorer/Git/LSP, and restores the chosen project's session.
  See `spec/switch-project/index.md`.
- **IntelliJ & Eclipse keymaps** (**View → Keymap**) — three new keymaps
  mirroring IntelliJ (macOS and Windows defaults) and Eclipse (Windows). See
  `spec/keymaps/index.md`.
- **Spacemacs keymap** (**View → Keymap → Spacemacs**) — Vim-style modal editing
  plus a `Space` leader for menu-like command sequences (`SPC f f` open, `SPC g s`
  git, `SPC w /` split, `SPC q q` quit, …). See `spec/keymaps/index.md`.
- **Inline git blame** (**Git → Toggle Inline Blame**, `inline_blame` setting) —
  shows the cursor line's blame (`author, date · summary`) dimmed at the end of
  the line, following the cursor. See `spec/git-integration/index.md`.
- **Outline sidebar** (**View → Layout → Outline Sidebar**, `show_outline_dock`) —
  a persistent symbol list beside the editor that follows the cursor; click a row
  to jump. See `spec/outline-sidebar/index.md`.

- **Test runner.** **Tools → Run Tests** runs the configured `test_command`,
  parses the output (cargo libtest, pytest `-v`-style) into a pass/fail panel with
  ✓/✗/○ icons, jump-to-failure on click, and a summary notification. New
  `test_command` / `test_width` settings. See `spec/test-runner/index.md`.
- **Integrated debugger (DAP).** A new **Debug** menu drives an external debug
  adapter (configured via `debug_adapters`): breakpoints (gutter `●`), start/stop,
  continue, step over/into/out, pause, a call-stack + variables + watch panel, an
  evaluate REPL, and program output in the bottom dock. Built on a DAP client that
  reuses the LSP framing. See `spec/debugger/index.md`.

### Changed

- **Nested split panes (up to a 2x2 grid).** The editor split is now a binary
  tree: **View → Split → Vertical/Horizontal** split the *focused* pane, so
  repeated splits nest into a grid (up to four panes). Drag any divider to resize
  that split; Other Pane (`F6`) cycles focus; Unsplit collapses the focused pane.
  The layout persists in the session. See `spec/split-panes/index.md`.

## [0.4.0] - 2026-06-27

### Added

- **AI chat panel** (**AI → Chat…**, action `ai.chat`) — a persistent
  conversation surface for the configured assistant CLI. Each reply is remembered
  and fed back as context, so follow-up questions work; an editor selection seeds
  the input. `Alt+T` opens the last reply in a tab, `Alt+C` copies it. See
  `spec/vix-agent-panel/index.md` and `docs/agent-panel/index.md`.
- **AI edit review.** Annotate and Improve now open an accept/reject **diff
  review** (hunk by hunk) instead of overwriting the buffer immediately: `↑↓` move
  between hunks, `Space` toggles one, `a`/`r` accept/reject all, `Enter` applies,
  `Esc` discards. New `ai_diff_review` setting (on by default) controls it. See
  `spec/ai/index.md`.
- **Background results in the notification panel.** Run Command (and Git
  Pull/Push/Fetch) completions and AI menu outcomes now post to the right-dock
  notification feed (Info on success, Error on failure), giving a durable record
  beyond the transient status bar. See `spec/vix-notification-panel/index.md`.
- **Open buffers reload after a branch switch.** Switching branches now re-reads
  every clean open file from disk so it reflects the new branch; dirty buffers are
  left untouched and a notification reports the count.
- **Persistent spellcheck user dictionary.** Words added from the spell-suggest
  popup now persist across sessions in `user_dictionary.txt` (config directory)
  and are reloaded on launch, instead of being session-only.
- **Multi-line commit messages.** The Git commit prompt now accepts a multi-line
  message: `Alt+Enter` inserts a newline, `Enter` commits — so subject + body
  messages work.
- **Searchable keyboard-shortcut browser.** The Help → Shortcuts overlay (`F1`)
  now has a search field — type to filter shortcuts by key or description.
- **Tools → Insert → LaTeX** submenu — inserts Org/LaTeX markup snippets
  (headlines, link, bold/italic/underline, table, deadline/scheduled, timestamps,
  quote/verse/center blocks, drawer).
- **Tools → Insert → SQL** submenu — inserts ready-to-edit PostgreSQL snippets:
  Alter Role, Create Extension, Create Function, Create User, Grant Create, Grant
  Usage, and Create Table (with trigger + trigram index).
- **Diff gutter in soft-wrap mode.** The Git change bar in the line-number gutter
  now also renders when soft wrap is on (on a changed line's first visual row).
- **Richer session restore.** Reopening a workspace now also restores each file's
  **scroll position** and the **split-pane layout** (direction, ratio, focused
  side), not just open files, focus, and cursor. Older session files still load.
- **EditorConfig support.** Vix now reads `.editorconfig` files and applies their
  indent style/size and trim/final-newline rules per opened file, overriding the
  global settings. New `editorconfig` setting (on by default). See
  `spec/editorconfig/index.md`.
- **Task runner.** A workspace `tasks.toml` (or `.vix/tasks.toml`) defines named
  shell commands; **Tools → Tasks…** (action `tools.tasks`) lists and runs them
  through the async pipeline. See `spec/tasks/index.md`.
- **Compare With File.** **Tools → Compare With File…** (action `tools.diff`)
  shows a read-only unified diff between the active buffer and another file. See
  `spec/diff-view/index.md`.
- **Project-wide replace preview.** Workspace-wide replace now shows a
  preview/confirm overlay listing each affected file and its match count; nothing
  is written to disk until you confirm (`y`/`Enter`), with `n`/`Esc` to cancel.
- **Integrated terminal.** **Tools → Terminal** (action `tools.terminal`) opens a
  real interactive shell on a PTY (`portable-pty` + `vt100`) inside Vix —
  full-screen programs, colors, and the cursor all work. `Ctrl+]` closes it. See
  `spec/terminal/index.md`.

### Changed

- **AI menu is now CLI-agnostic.** The AI commands (Summarize, Explain, Define,
  Annotate, Improve) no longer hardcode `claude`; they run the new `ai_command`
  setting (default `claude -p "{prompt}"`). The template's `{prompt}` placeholder
  receives the instruction and the input text is piped on stdin (or substituted
  for `{file}`), so the menu can drive Codex, Mistral, a local `ollama` model, or
  any other assistant CLI. See `spec/ai/index.md` and the configuration docs.

## [0.2.0] - 2026-06-27

### Added

- **Edit surfaces (Tools menu)** — full-screen overlay editors for non-plain-text
  views of the active buffer: **Edit Table** (CSV/TSV spreadsheet, `edit_table`),
  **Edit Outline** (indented prose hierarchy with folding, `edit_outline`),
  **Edit JSON** / **Edit YAML** (foldable structured-value tree, `edit_value`),
  and **Edit Bytes** (hex/ASCII byte editor, `edit_bytes`).
- **Tools → Insert submenu** (renamed from "Generate") — inserts generated content
  at the cursor: UUID, ZID, **Markdown** and **HTML** snippets, **Lorem ipsum**
  (`lorem`), and **Date/Time** presets (ISO 8601, RFC 3339, Unix epoch).
- **QR Code generator** (Tools → QR Code) — encodes the selection or line into a
  scannable Unicode QR overlay (`qr_tool`, via the `qrcode` crate).
- **Git per-hunk unstage** (`git.unstage_hunk`) — completes interactive per-hunk
  staging alongside stage/revert; plus diff next/prev hunk navigation.
- **Select all occurrences** (`edit.select_all_occurrences`) — multi-cursor on
  every match of the selection — and **column / rectangular selection**
  (Alt+Shift+↑/↓, `edit.column_select_*`) with block editing.
- **Zen (focus) mode** (`view.zen`), an optional **breadcrumb bar**
  (`view.breadcrumbs`, `file ▸ symbol`), and **trim / final-newline on-save**
  toggles.
- **Release packaging** — `cargo-dist` (macOS arm64/x64, Windows, Linux MUSL) and
  a `Makefile` that tests then cross-builds the three targets.
- **Single-crate architecture (edition 2024).** Every former `vix-*` subcrate is
  now a module under `tui/src/`; the editor widget is the `editor_core` module
  (Tree-sitter highlight queries in `langs/`, gated behind `lang-*` features).
  The workspace has no members. See `agents/share/crate-map.md`.
- **`#![warn(clippy::pedantic)]` in every module**, with no blanket
  `#![allow(clippy::pedantic)]`/`#![allow(missing_docs)]` — findings fixed in
  code; only four targeted `struct_excessive_bools` allows remain.
- **Full Language Server Protocol support — all 25 methods** in
  `spec/lsp/language-server-protocol.tsv`: lifecycle + document sync,
  `publishDiagnostics` (colored underlines + diagnostics panel), `hover`,
  `completion` + `completionItem/resolve`, `definition`/`implementation`/
  `typeDefinition`, `references`, `documentHighlight`, `documentSymbol`,
  `workspace/symbol`, `signatureHelp`, `formatting`/`rangeFormatting`, `rename`,
  `codeAction`, `codeLens`, `selectionRange`, `foldingRange`, `inlayHint`,
  `linkedEditingRange`. Configured per language via `lsp_enabled`/`lsp_servers`
  (no built-in server); protocol core in `src/lsp_core/`, process IO in
  `src/lsp.rs`. See `spec/lsp`.
- **Editor features:** code folding (▾/▸ gutter), inline inlay hints, bookmarks
  (toggle/next/prev/list), keyboard macros, buffer-word autocomplete, overwrite
  mode, column ruler, multi-cursor up/down, line transforms (join/sort/sort-
  unique/reverse/dedupe/trim), and a shortcuts/key-menu overlay.
- **Tools:** MD5/CRC32 checksums, JWT decode, base conversion (dec/hex/bin/oct),
  a live regex tester, markdown preview, snippets, and a text-information panel.
- **Git:** stash / stash pop / commit amend, a merge-conflict resolver
  (keep ours/theirs/both, next conflict), and per-hunk stage/revert.
- **X11 Colors picker** (Tools → X11 Colors) — swatch + hex + name; inserts the
  hex. New `x11_color_picker` module.
- **HTML Characters picker** (Tools → HTML Characters) — glyph / entity / code
  point; clicking a cell inserts that cell's text. New `html_character_picker`
  module.
- **macOS VSCode keymap** — Quick Open (`Ctrl+P`), Command Palette
  (`Ctrl+Shift+P`), Go to Symbol, Go to Line, and the familiar VS Code chords.
- **Edit → Go submenu** — File Start/End, Line Number, and Line / Paragraph /
  Section Start & End cursor jumps.
- **Edit → Select** gains **Select Paragraph** and **Select Section**.
- **Scrollbars** on the file explorer and the character/color pickers.

### Changed

- **Renamed the Tools "Generate" submenu to "Insert"** (actions `tools.generate.*`
  → `tools.insert.*`); format/acronym menu labels route through `menu.name.*`
  locale keys.
- **`Alt+V` now opens the Vix menu** (the first menu); **View** moved to `Alt+I`.
- **Renamed "keyway" → "keymap"** throughout (setting, chooser module, menus, docs).
- **Renamed "project" → "workspace"** throughout — **Find In Workspace…**,
  **Workspace Dashboard**, workspace-wide search/replace, the `workspace_dashboard_panel`
  module, and the `workspace_search` module.
- **Renamed the picker crates** from `-palette`/`-panel` to `-picker`:
  `ascii_character_picker`, `html_character_picker`, `nerd_font_picker`,
  `x11_color_picker` (the ASCII one was `ascii_panel`).
- **Menu dropdowns open with nothing highlighted** — the user arrows, hovers, or
  types to pick an item (no auto-selected first row).
- **Removed the standalone Find & Replace menu item** — replace now lives inside
  the Find panel (`Ctrl+R` or `Tab` to the Replace field).
- **Per-crate specifications** moved into each crate's `spec/index.md`; the
  top-level `spec/` keeps the app-level specs.
- **Shortcut labels use spaces** instead of `+` (e.g. `Ctrl Shift Z` rather than
  `Ctrl+Shift+Z`) in menus and the keyboard-help overlay.
- **Recent-files count is configurable** via the `recent_files_max` setting
  (default 15), controlling how many entries **File → Open Recent…** keeps.
- **Spellcheck autodetects Hunspell dictionaries** from the platform's standard
  locations (`/usr/share/hunspell`, `/Library/Spelling`,
  `/opt/homebrew/share/hunspell`, `$XDG_DATA_HOME/hunspell`, and `hunspell -D`).
  The `dictionaries_dir` setting is replaced by `dictionary_path` (an extra
  directory to search; empty = autodetect only); both the standard
  `<name>.{aff,dic}` and wooorm `<name>/index.{aff,dic}` layouts are accepted.
- **Bottom-dock scrollback is configurable** via the `scrollback` setting
  (default 1000 lines, down from a hard-coded 5000); the oldest lines are dropped
  past the limit.
- **Redo shortcut is now `Ctrl+Shift+Z`** (was `Ctrl+Y`), matching the common
  undo/redo pairing.

### Added

- **Right-click context menu** in the editor: Cut / Copy / Paste, Select All /
  Select More / Select Less, and Find / Find Next / Find Previous.
- **File-explorer path filters**: filter the tree by **Include regex** and
  **Exclude regex** (command palette "Explorer: Include/Exclude Regex Filter").
  Files whose workspace-relative path fails the filter are hidden; directories
  stay visible. The explorer title shows `(filtered)` when active.
- **Outline panel** (`Ctrl+Shift+B`, or the palette "Outline"): a list of the
  active buffer's symbols (kind prefix + name); Enter or a click jumps to the
  symbol, and the cursor's enclosing symbol is selected on open. New internal
  `outline_panel` module.
- **Workspace Dashboard** (Tools → Workspace Dashboard): a live overlay showing the
  workspace folder name, disk usage (`du`), file count, and git commit count, each
  computed asynchronously and filled in as it completes. New internal
  `workspace_dashboard_panel` module.
- **Select More / Select Less** (Edit menu; `Ctrl+Shift+→` / `Ctrl+Shift+←`):
  grow or shrink the selection by a word. **Move Up / Move Down** (Edit menu;
  `Alt+↑` / `Alt+↓`) are now also surfaced in the menu.
- **Case transforms** (Edit → Case): convert the selection to Upper, Lower,
  Title, Kebab (`foo-bar`), Snake (`foo_bar`), Camel (`fooBar`), or Pascal
  (`FooBar`).
- **Workspace search path filters**: the workspace-wide search/replace panel gains
  **Include path** and **Exclude path** regex fields that narrow the searched
  files by their workspace-relative path (`Tab` cycles to them).
- **Git integration** via the new `git` module, shelling out to the `git`
  CLI. The status bar shows the current branch and a dirty dot; the file explorer
  shows colored M/A/?/D/R/U badges on changed files; the editor draws a colored
  diff gutter (added/modified/deleted) against HEAD. The **Git** menu offers a
  **Changes…** panel to stage/unstage files (`Space`/`s`/`u`) and commit (`c`),
  **Switch Branch…**, and **Pull / Push / Fetch** (streamed to the bottom dock).
- **Spell checking** (View → Editor → Toggle Spellcheck): underlines misspelled
  words in comments and string literals in red, using Hunspell dictionaries from
  the `dictionaries/<locale>/` directory (`dictionaries_dir` setting) via the new
  pure-Rust `spellcheck` module. The language follows the UI locale; code-like
  tokens (acronyms, camelCase identifiers) are skipped. Off by default. With the
  cursor on a misspelled word, **`Ctrl+;`** opens a suggestions popup with
  replace, add-to-dictionary, and ignore actions.
- **System Information panel** (Tools → System Information): a scrolling,
  read-only snapshot of the host — OS, CPU, memory, swap, disks, uptime, and
  environment (via the `sysinfo` crate). Enter or a click inserts the highlighted
  value into the editor; Esc closes. Lives in the new internal
  `system_information_panel` module.
- **Unsaved-changes prompt.** Closing a tab or quitting with unsaved changes now
  raises a modal asking to **(s)ave**, **(d)on't save**, or **(c)ancel**. Quit
  walks every dirty tab in turn before exiting. Vim `:q!` still force-quits
  without prompting.
- **ASCII panel** (Tools → ASCII): a scrolling overlay of the 128 ASCII codes
  showing each code's decimal, hexadecimal, and character representation. Arrow
  keys / PageUp / PageDown / Home / End move the highlight; Enter or a click
  inserts the highlighted character into the active editor; Esc closes. Lives in
  the new internal `ascii_character_picker` module.
- **View → Layout submenu.** The dock and status-bar toggles (Show/Hide Left
  Dock, Right Dock, Bottom Dock, Bottom Status) now live under a **Layout**
  submenu, alongside the existing **Editor** submenu.
- **Menu type-ahead.** With a menu open, typing a letter jumps to the next item
  whose label starts with it, cycling — e.g. in File, `S` → Save, `S` → Save As.
  Works inside an open submenu too.
- **Search in Workspace → Dock** (Edit → Find submenu, or the palette): search
  every workspace file for a term and list the hits in the bottom dock as
  `path:line:col` lines — each one click-to-jumps to the match. In the prompt,
  `Alt+C` toggles case-sensitivity and `Alt+R` toggles regex.
- **Run Command** (Tools → Run Command…, or the palette): prompt for a shell
  command, run it in the workspace root in a **background thread**, and **stream**
  its merged stdout/stderr into the bottom dock (shown automatically) line by
  line, with a `$ command` header and an `[exit N]` footer. The UI stays
  responsive; **Cancel Command** (Tools menu / palette) kills a running command.
- **Resizable bottom dock.** The bottom dock is pinned directly above the status
  bar, and its top edge is draggable to grow or shrink it (persisted in the
  `bottom_dock_height` setting), matching the draggable left/right docks.
- **Bottom-dock scrolling, focus & click-to-jump.** Click the bottom dock to
  focus it (its border brightens); then `↑`/`↓`, `PgUp`/`PgDn`, and `Home`/`End`
  scroll its buffer. The mouse wheel scrolls it any time; `Esc` returns focus to
  the editor. Clicking a line that names a `path:line[:col]` location (a build
  error, grep hit, …) opens that file there, making Run Command output
  actionable.
- **Bottom dock** (View → Show/Hide Bottom Dock, or the palette;
  `show_bottom_dock` setting, default off): a full-width scrollable line buffer at
  the bottom of the body for log messages, command/terminal output, data views,
  etc. State lives in the new `bottom_dock` module (line buffer + scroll).
- **Calendar month-nav arrows.** The calendar box's month header shows
  `◀ Month Year ▶`; the arrows are clickable (and mirror the `←`/`→` keys), and a
  bottom help line shows `◀ ▶ month   Esc close`.
- **Many more UI languages.** Added Italian, Korean, Turkish, Dutch, Vietnamese,
  Indonesian, Thai, Persian, Ukrainian, and Greek — plus **Klingon** (`tlh`) and
  **Sindarin** (`sjn`) — for 27 selectable languages. The full menu bar is
  translated into the 15 primary locales; other keys fall back to English.
- **Calendar click-to-insert.** In the calendar box, clicking one of the
  date-time lines (local date-time, UTC ISO instant, ISO week date) inserts that
  string into the editor; clicking a day in the month grid inserts that date
  formatted per the active locale. The box stays open for repeated inserts; a
  click outside closes it.
- **Nested submenus** in the menu bar. **View → Editor** groups the editor
  display toggles (line numbers, visible whitespace, scroll bar); **Edit → Find**
  groups the find-related items (Find, Find Next, Find Previous, Find Selection,
  Find & Replace). Arrow keys / clicks open and navigate submenus (Right or a
  click opens, Left or Esc backs out).
- **Show/Hide Editor Scroll Bar** (View → Editor, or the palette): toggle the
  editor's right-side scroll bar; the text reclaims the column when hidden.
  Persists in the `show_scrollbar` setting (default on).
- **Reopen Closed Tab** (`Ctrl+Shift+T`, File menu, or the palette): reopen the
  most recently closed file (remembers a stack of recently closed paths).
- **Close All Tabs** (File menu, after Close, or the palette): close every open
  buffer, leaving a single empty untitled buffer.
- **Find Next / Find Previous / Find Selection** in the Edit menu (after Find).
  Find Next (`Ctrl+G`) and Find Previous (`Ctrl+Shift+G`) repeat the last search
  — and now keep working **after the find box is closed** (the last pattern is
  remembered; `F3` / `Shift+F3` repeat it too). Find Selection jumps to the next
  occurrence of the selection (`Alt+N`).
- **Toggle Bottom Status** (View menu / palette / `view.status_bar`): show or
  hide the bottom status bar; the editor body reclaims the row when it is hidden.
  Persists in the `show_status_bar` setting (default on).
- **Editing comforts.** **Select All** (`Ctrl+A`, Edit menu, or the palette),
  **Duplicate Line** (`Ctrl+D` or the palette), **Move Line Up/Down**
  (`Alt+↑`/`Alt+↓` or the palette), and **Jump to Matching Bracket** (`Ctrl+]` or
  the palette). Auto-indent on Enter (carry the previous line's leading
  whitespace) was already present and is now covered by tests.
- **Nerd Font Palette** (Tools → Nerd Font Palette…, crate
  `nerd_font_picker`): a character picker showing a grid of curated Nerd
  Font glyphs. Browse with the arrow keys or the mouse; Enter or a click inserts
  the highlighted glyph into the active editor and leaves the palette open so
  several can be picked in a row; Esc closes it.
- **Menu separators.** Dropdowns group related items with non-selectable
  divider lines: in File (before Open, Close, Quit), Edit (before Cut, Toggle
  Comment, Find), and View (before Toggle Left Dock, Toggle Editor Line Numbers).
  Keyboard navigation, hover, and clicks skip separators.
- **Bracket matching.** When the cursor is on (or just after) a bracket
  `()[]{}`, its matching partner is highlighted. No auto-insertion of pairs.
- **Richer status bar.** The status bar now shows the language, line ending
  (LF/CRLF), encoding (UTF-8), and — when text is selected — the selected
  character and line count, alongside the existing line:column.
- **Fully-custom editor widget (`editor_core`) with soft wrap.** The editor was
  migrated from the vendored `editor_core` fork to an in-house widget:
  the Tree-sitter highlighting + buffer + undo/redo engine is reused, while the
  editor state, input, mouse, and renderer are owned by Vix. The renderer now
  supports **soft wrap** — toggle with **View → Toggle Soft Wrap** (or the
  palette); the `soft_wrap` setting persists. Long lines wrap across screen rows
  with cursor, scroll, and mouse all wrap-aware. (Also fixed a latent panic when
  jumping to a line past the end of the buffer.)

- **Internationalization** via `rust-i18n`. The entire UI is translatable; 15
  languages are selectable (English, Spanish, French, German, Welsh fully
  translated; Irish, Scottish Gaelic, Polish, Portuguese, Russian, Arabic, Hindi,
  Bengali, Chinese, Japanese with menu/theme coverage and English fallback).
  Language is chosen with `--locale`, the `locale` setting, or **View → Locale…**
  (a live chooser). English is the fallback. See `docs/i18n.md`.
- **Themes.** All themes are **JSON themes** with per-region RGB colors (menu
  bar, status bar, left/right dock, editor) and optional editor cursor and syntax
  colors. Dark and Light ship bundled; more are bundled too, and users can add
  their own in `~/.config/vix/themes/*.json`. Chosen live in **View → Theme…**.
  See `docs/themes.md`.
- **Configuration** via `confy`, stored as TOML in the platform config directory.
  New `theme` and `locale` settings. See `docs/configuration.md`.
- **Command-line interface** via `clap`: positional files (with optional
  `path:line:col`) and a `--locale` flag; `--help` / `--version`.
- **Vix menu** (first in the bar) with **About Vix** (shows `Vix <version>`),
  **Website**, and **Email** — each opens a modal dialog with an **Ok** button.
  The Website/Email dialogs show the text in a selectable text field (drag or
  arrow-select, `Ctrl+C` to copy).
- **Keymaps** (**View → Keymap…**, module `keymap_model`): choose the
  keyboard navigation style, which changes how keys are dispatched. The choice
  persists (`keymap` setting); Apple is the default.
  - **Apple** — modifier shortcuts (e.g. `Ctrl+O` open, `Ctrl+Q` quit).
  - **Emacs** — `Ctrl` chords and the `Ctrl+X` prefix: `Ctrl+X Ctrl+F` open,
    `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit, `Ctrl+X k` close; cursor motion
    with `Ctrl+F/B/N/P/A/E/V`, `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel.
  - **Vim** — modal: a Normal mode (`h/j/k/l`, `0`, `$`, `x`, `i/a/o/O` to enter
    Insert, `Esc` back to Normal) and a `:` command line (`:w`, `:q`, `:q!`,
    `:wq`/`:x`, `:Ex`). The status bar shows the current mode.
- **View menu** with theme, locale, and keymap choosers and the drawer/line-number
  toggles.
- **Indentation settings** — `indent_style` (`"spaces"` / `"tabs"`) and
  `tab_width` control what the Tab key inserts (default: 4 spaces), overriding the
  editor widget's per-language default.
- **Live go-to-line preview** — in the palette's `:` mode the cursor now follows
  the line number as you type (scrolling it into view); `Enter` commits (recording
  the original position in the jump history) and `Esc` reverts. (Also fixes a
  latent panic when jumping to a line past the end of the buffer.)
- **Find occurrence of selection** (`Alt+N` / `Alt+P`, or the palette): jump to
  the next/previous occurrence of the current selection — or the word under the
  cursor when there is no selection — without opening the search bar.
- **Smart Home** — `Home` jumps to the first non-blank character of the line;
  pressing it again jumps to column 0 (toggling between the two).
- **On-save normalization** — two settings (`trim_trailing_whitespace`,
  `ensure_final_newline`, both default on) strip trailing spaces/tabs from each
  line and append a final newline when saving. (Making the previously
  always-on final-newline behavior configurable.)
- **Toggle Comment** (`Ctrl+/`, the Edit menu, or the palette): comment or
  uncomment the cursor line or every line in the selection, using the language's
  comment token (`//`, `#`, `--`), as a single undoable edit. The editor widget's
  comment-token map gained TOML/YAML (`#`) and SQL (`--`).
- **Go to Symbol in File** — a new command-palette mode (`@` prefix, or the
  "Go to Symbol in File" command) listing the current file's declarations
  (functions, types, classes, traits, modules, `#define`s, …) to fuzzy-filter
  and jump to. A fast, offline, language-agnostic heuristic — no language server.
- **Open Recent** (`File → Open Recent…`, `Ctrl+Shift+O`, or the palette): a
  chooser of recently opened files. The list (most-recent first, de-duplicated,
  capped at 15) persists in the `recent_files` setting.
- **Toggle Editor Visible Whitespace** (View menu / palette / `view.whitespace`):
  render dim glyphs for space (`·`), tab (`→`), carriage return (`␍`), and line
  ending (`¶`). Off by default; persists in the `show_whitespace` setting.
- **Dock toggle icons** in the menu bar (clickable explorer/messages toggles;
  bright when open, dim when closed).
- A visible **block cursor** in the editor, themeable via a custom theme's
  `cursor` color.
- Custom themes can set per-region **`font-style`** (`normal`/`italic`) and
  **`font-weight`** (`normal`/`bold`); the editor also applies a custom theme's
  syntax token colors.
- **Editor scrollbar drag**: press and drag the scrollbar thumb/track to scroll.
- **Resizable docks**: drag the explorer's right edge or the message drawer's left
  edge to resize them. The widths persist (`explorer_width` / `messages_width`).
- A collection of themes **bundled into the binary** (Darker, Darkest, Lighter,
  Lightest, Matrix, Turbo, Solarized Dark/Light, Dracula, Nord, Gruvbox Dark,
  Monokai, One Dark, Tokyo Night) that appear in **View → Theme…** with no
  installation. A same-named theme in `~/.config/vix/themes/` overrides a
  bundled one.
- New modules: `theme_model`, `locale_model`,
  `keymap_model`, `keyboard_shortcut_panel`,
  `calendar_panel`, `nerd_font_picker`, and `find_panel`
  (the find / find-and-replace box state).
- New docs: `docs/themes.md`, `docs/i18n.md`, `docs/configuration.md`,
  `index.md`, `AGENTS.md` (+ `agents/`), and this changelog.

### Changed

- **Docks and status bar extracted to modules.** The left dock (file
  explorer) moved to `left_dock`, the right dock (message drawer) to
  `right_dock`, and the status-bar segment formatting to
  `status_bar_panel`. The app re-exports them; behavior is unchanged.
- The main panes use a lighter border frame: the left and right docks keep only
  their inner (top + side-facing-the-editor) borders, the center editor keeps
  only its top border, and the bottom status bar gains a full-width top border
  that separates it from the body.
- The editor widget module was renamed `ratatui-code-editor` →
  `editor_core` and made **theme-aware** (configurable text,
  line-number, selection, and cursor styles, and a settable syntax palette).
- The calendar logic moved into `calendar_panel` and gained
  month navigation (Left/Right while the calendar is open).
- **Every theme is now a JSON theme.** The hardcoded monochrome Dark/Light
  *modes* were removed; **Dark** and **Light** are now ordinary bundled themes
  (`tui/themes/dark.json` / `tui/themes/light.json`, soft `[215,215,215]` on `[40,40,40]`
  and its inverse) loaded like any other. The chooser lists every theme
  (including Dark and Light) sorted by name, and the persisted `theme` setting is
  the theme's name.
- Settings moved from hand-rolled JSON to `confy` TOML.
- All public items are documented (`#![deny(missing_docs)]`); the module forbids
  `unsafe`.

### Fixed

- Panel border lines now use each pane's own foreground color (via
  `region_title`) instead of the global editor foreground, so borders match the
  pane under themes whose regions use different colors.
- In the find / replace box, clicking the Find or Replace field now focuses it
  (previously the box swallowed all mouse input, so the Replace field was only
  reachable with `Tab`, which the hint never mentioned). The hint now states
  `Tab / click: switch field` in replace mode.
- In the file explorer, `←` (Left) no longer expands a collapsed folder. It now
  collapses an expanded folder, or jumps to the parent folder when the selection
  is already collapsed — it never opens a folder.
- Duplicating the last line of a buffer with no trailing newline (`Ctrl+D`) now
  produces a real second line instead of concatenating the copy onto the
  original. Line-boundary detection at end-of-buffer was off by one, which also
  affected `Ctrl+K` (delete line) and triple-click line selection on the last
  line.
- Menu dropdown items keep at least one space between the label and the
  right-aligned keyboard shortcut (the widest item used to let them touch).
- Keyboard-only modal overlays (the calendar box, find, query-replace, workspace
  search, confirm, paste-conflict) now swallow mouse clicks instead of letting
  them fall through to the editor underneath.
- Menu mouseover now moves the selection: with a dropdown open, hovering a row
  highlights it and hovering another top-level name switches menus (any-motion
  mouse tracking is enabled for this; other panes ignore button-less motion).
- The theme, locale, and keymap choosers now respond to the mouse: clicking a
  row highlights (and, for theme/locale, live-previews) that entry instead of
  being ignored.
- The active editor tab keeps the theme background (marked with an underline)
  instead of reversed video, which showed a light background under a dark theme.
- Search-hit marks render monochrome (underline) instead of a hard-coded color.
- Overlays paint the theme background so they read correctly in the light theme.
- The menu dropdown no longer shows its raw i18n key as a title.
- Clicking an item in an open menu dropdown now runs it (and clicking away
  closes the menu).
- The tab bar paints the editor's theme background instead of resetting to the
  terminal default (it no longer shows white under a dark theme).
- Removed the gray app-name label from the right of the menu bar.
