# Lsp Core

A pure LSP client core: JSON-RPC framing, message builders, parsers,
positions. LSP stands for Language Server Protocol.

This crate owns the *protocol*, not the *process*: JSON-RPC 2.0 message
framing ([`frame`]), request/notification builders and response parsers
([`message`]), and char↔encoding column maths ([`position`]). The host spawns
the language server, pumps its stdout bytes through [`frame::Decoder`], and
writes [`frame::encode`]d requests to its stdin — so everything here stays
synchronous and unit-testable with no IO.

## Completion (T612)

`message::parse_completion` returns a `CompletionList` (`items` +
`is_incomplete`); each `CompletionItem` carries its `textEdit` range,
`additionalTextEdits`, `filterText`, `sortText`, `kind`, and
`commitCharacters` alongside the label/insert text/detail/resolve data,
plus its `insertTextFormat` (T613; `CompletionItem::is_snippet` for format
2, which `initialize_params` advertises via `snippetSupport: true`).
`message::completion_params` builds a request with a `CompletionContext`
(`triggerKind` 1 invoked, 2 trigger character, 3 re-trigger for an
incomplete list) and `message::parse_completion_trigger_characters` reads
`capabilities.completionProvider.triggerCharacters` from an `initialize`
result. How the host filters, triggers, and applies them is specified in
the [lsp spec](../../vix-lsp/spec/index.md#completion-t612).

## Document sync and semantic token requests (T615)

`sync::SyncKind::from_initialize` reads `capabilities.textDocumentSync`
(`2` is incremental, anything else full). `sync::diff(old, new, encoding)`
returns the one `ContentChange` (an LSP `Range` of the old text plus its
replacement) that turns `old` into `new`, or `None` when they are equal:
the common prefix and suffix are kept, columns are in the given encoding,
every LSP line break counts, and a CRLF pair or a multi-byte character is
never split. `sync::apply` applies a change exactly as a server does (it is
the reference the diff is property-tested against), on top of
`sync::position_at` / `sync::offset_at`. `message::did_change_incremental_params`
builds the ranged `didChange`.

For semantic tokens, `message::parse_semantic_tokens_support` reads whether a
server offers `range` and `full.delta`; `semantic_tokens_range_params` and
`semantic_tokens_delta_params` build those requests;
`parse_semantic_tokens_reply` returns a reply's raw `data` or delta `edits`
with its `resultId`; `apply_semantic_tokens_edits` applies the edits to the
previous data; and `decode_semantic_tokens` decodes data against the legend.
How the host uses them is in the
[lsp spec](../../vix-lsp/spec/index.md#semantic-token-requests-t615).

## Inline completion (T618)

`inline_completion` builds the `textDocument/inlineCompletion` params
(`inline_completion::params`) and parses its reply
(`inline_completion::parse`): a bare `InlineCompletionItem[]` or an
`InlineCompletionList` with `items`. Each item's `insertText` is a string or a
`StringValue`; a snippet value is flattened to plain text (`$1`, `${1}` removed,
`${1:name}` becomes `name`). Items with empty text are dropped. The client
capability `textDocument.inlineCompletion` is declared in `initialize`.

## File operations and watchers (T623)

`file_ops` holds the pure side of `workspace/will{Create,Rename,Delete}Files`,
`workspace/did{Create,Rename,Delete}Files` and
`workspace/didChangeWatchedFiles`: `parse_file_operation` reads one
`capabilities.workspace.fileOperations.<op>` entry into `FileOpFilters` (glob
filters with `matches` file/folder, `ignoreCase`, `scheme` other than `file`
skipped; no filters means every path), `FileOpCaps::from_initialize` reads all
six, and `FileOpCaps::wanted(op, will, items, roots)` picks the `FileOpItem`s
(path, optional new path, and whether it is a folder, known by the caller
before a delete) a server's filters match -- a rename's request against the old
path, its notification against the new one. `FileOp` names the methods and
capability keys; `file_operation_params` builds the `files` array.
`parse_registrations`/`parse_unregistrations`/`parse_watchers` decode
`client/registerCapability` and `client/unregisterCapability`;
`Watcher::covers` says whether a file is one a watcher's glob names, and
`scan_watched`/`diff_snapshots` are the bounded recursive scan: it visits at
most `SCAN_MAX_ENTRIES` (10 000) directory entries, descends at most
`SCAN_MAX_DEPTH` (16) levels, reads each directory in sorted order, follows no
symlinks, skips `.git`, `.hg`, `.svn`, `target` and `node_modules`, and
remembers only covered files (mtime and length). `diff_snapshots` yields
created, changed and deleted files, reporting deletions only when the newer
walk finished within its budget. Globs use `globset` with `*` not crossing
`/`. `initialize` declares all six `workspace.fileOperations` entries and
`workspace.didChangeWatchedFiles.dynamicRegistration`.

## File URIs (T772)

`uri` builds `file://` URIs from paths and back, for `vix-lsp` and
`vix-mcp-server`. Each function takes the platform as a flag (`windows`), so
the Windows rules are tested on every OS; `file_uri_native` and
`uri_path_native` pass `cfg!(windows)`.

- On Windows a verbatim prefix is dropped (`\\?\C:\x` → `C:\x`,
  `\\?\UNC\srv\share` → `\\srv\share`, `strip_verbatim`), `\` is a
  separator, a drive path is `file:///C:/x`, and a UNC path keeps its server
  as the host (`file://srv/share/x`). Decoding reverses all of that; a path
  with no drive, such as a remote POSIX path, is kept as it is.
- Elsewhere a `\` is an ordinary file-name byte and is percent-encoded.
- Every byte but the unreserved ones, `/` and the drive's `:` is
  percent-encoded; decoding refuses bad percent-encoding and bad UTF-8.

`vix-lsp`'s `uri_to_path` also canonicalizes a decoded Windows drive path
that exists, so it matches the (verbatim) paths of open tabs.

## See also

- [lsp spec](../../vix-lsp/spec/) — shared LSP behavior

## Code action kinds

`message::code_action_params_only` adds `context.only` (a list of
`CodeActionKind`s) to a `textDocument/codeAction` request; the host uses
`["refactor"]` for the Refactor menu and `["source.organizeImports"]` for
Organize Imports. `code_action_params` is the unfiltered form.

## Progress and capabilities (T732)

`message::parse_progress_update` decodes a `$/progress` notification into a
`ProgressUpdate` — its token (a string, or an integer normalized to a
string), its `ProgressKind` (`Begin`/`Report`/`End`), and the optional
title, message and percentage — so the client can track several concurrent
tasks per server. `message::parse_progress` (T123d) still formats a single
report as one line. `message::parse_capability_names` lists the
capabilities in an `initialize` result: every key of `capabilities` whose
value is present and not `false`, with a trailing `Provider` dropped
(`hoverProvider` → `hover`), sorted.

## Document colors and links (T621)

`document_extras::parse_colors` reads a `textDocument/documentColor` result
(`ColorInformation[]`, channels as 0.0-1.0 floats) into `DocColor { range,
rgb, rgba }` with `rgb` clamped to bytes and `rgba` the server's unrounded
floats; `parse_color_presentations` reads a `textDocument/colorPresentation`
result into `ColorPresentation { label, edit, additional }` (`edits(color)`
resolves it to the edits to apply) and `color_presentation_params` builds the
request; `parse_links` reads a
`textDocument/documentLink` result into `DocLink { range, target }`, dropping
links with no `target`; `target_kind` classifies a target as a file (with an
optional line), an external URL, or unsupported. `initialize_params` declares
`textDocument.colorProvider` and `textDocument.documentLink`. How the host
draws and opens them is in the
[lsp spec](../../vix-lsp/spec/index.md#document-colors-and-links-t621).
