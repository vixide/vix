# Lsp Core

A pure Language Server Protocol (LSP) client core.

This crate owns the *protocol*, not the *process*: JSON-RPC 2.0 message
framing ([`frame`]), request/notification builders and response parsers
([`message`]), and char↔encoding column maths ([`position`]). The host spawns
the language server, pumps its stdout bytes through [`frame::Decoder`], and
writes [`frame::encode`]d requests to its stdin — so everything here stays
synchronous and unit-testable with no IO.

## See also

- [lsp spec](../../vix-lsp/spec/) — shared LSP behavior
