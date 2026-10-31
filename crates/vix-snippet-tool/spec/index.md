# Snippet Tool

A small library of reusable text snippets, plus the picker's selection state.

Vix's Tools → Snippets… picker lists each snippet by name; choosing one
inserts its body at the cursor. The set is curated and language-agnostic; the
host owns insertion.

## Tabstop parser

`parse` turns a snippet body into its literal text plus navigation-ordered
tabstops (`Stop { num, start, end }`, char offsets into the text, `$0`
last). It is the one tabstop engine behind the snippet picker, prefix and
postfix expansion, and LSP snippet completions. It follows the LSP/VS Code
snippet grammar: `$N`/`${N}`, `${N:placeholder}` with nesting (bounded
depth, so a hostile body cannot exhaust the stack), `${N|a,b|}` choices
(first option inserted), `\$`/`\}`/`\\` escapes, and a malformed construct
kept as literal text; a repeated number is navigated once, at its first
occurrence. `parse_with(body, Dialect::Lsp)` (`parse_lsp`, T613) also
resolves variables — `$NAME`, `${NAME}`, `${NAME:default}`, and
`${NAME/regex/format/}` — to their default text or to nothing; the default
`Dialect::Tabstops` leaves `$NAME` as literal text. The full user-facing
syntax is in the [snippets spec](../../vix-snippets/spec/index.md#tabstops-body-syntax).
