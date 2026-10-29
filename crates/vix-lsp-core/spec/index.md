# Lsp Core

A pure LSP client core: JSON-RPC framing, message builders, parsers,
positions. LSP stands for Language Server Protocol.

This crate owns the *protocol*, not the *process*: JSON-RPC 2.0 message
framing ([`frame`]), request/notification builders and response parsers
([`message`]), and char↔encoding column maths ([`position`]). The host spawns
the language server, pumps its stdout bytes through [`frame::Decoder`], and
writes [`frame::encode`]d requests to its stdin — so everything here stays
synchronous and unit-testable with no IO.

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
